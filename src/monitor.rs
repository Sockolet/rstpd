use crate::{
    core::{self, Encoding, Eol, MAX_DOCUMENT_BYTES, Result},
    session,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

/// When size and modification time are unchanged, the file is re-hashed at most
/// this often. Re-hashing every one-second poll would re-read up to 256 MiB per
/// open file; the cost is that an edit that preserves both size and timestamp is
/// reported up to this long after it happens.
const UNCHANGED_STAMP_RECHECK: Duration = Duration::from_secs(5);

pub struct Request {
    pub id: u64,
    pub path: PathBuf,
    pub known_hash: Option<u64>,
    pub encoding: Encoding,
}
pub struct Snapshot {
    pub hash: u64,
    pub text: String,
    pub encoding: Encoding,
    pub eol: Eol,
}
pub struct Change {
    pub id: u64,
    pub path: PathBuf,
    pub baseline_hash: Option<u64>,
    pub result: Result<Snapshot>,
}
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: SystemTime,
}
fn stamp(path: &std::path::Path) -> Result<Stamp> {
    let metadata = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is no longer a regular file.", path.display()));
    }
    Ok(Stamp {
        length: metadata.len(),
        modified: metadata.modified().map_err(|e| e.to_string())?,
    })
}
struct Observed {
    path: PathBuf,
    baseline: Option<u64>,
    stamp: Option<Stamp>,
    hash: Option<u64>,
    error: Option<String>,
    checked: Instant,
}
#[derive(Default)]
struct Scanner {
    files: HashMap<u64, Observed>,
}
impl Scanner {
    fn scan(&mut self, requests: Vec<Request>) -> Vec<Change> {
        let keep: HashSet<_> = requests.iter().map(|r| r.id).collect();
        self.files.retain(|id, _| keep.contains(id));
        let mut changes = Vec::new();
        for request in requests {
            let current = stamp(&request.path);
            if let Some(old) = self.files.get(&request.id)
                && old.path == request.path
                && old.baseline == request.known_hash
            {
                match &current {
                    Ok(stamp)
                        if old.stamp.as_ref() == Some(stamp)
                            && old.error.is_none()
                            && old.checked.elapsed() < UNCHANGED_STAMP_RECHECK =>
                    {
                        continue;
                    }
                    Err(error) if old.error.as_ref() == Some(error) => continue,
                    _ => {}
                }
            }
            let result = (|| -> Result<Snapshot> {
                let before = current.as_ref().map_err(Clone::clone)?;
                let bytes = session::read_bounded(&request.path, MAX_DOCUMENT_BYTES)?;
                if stamp(&request.path)? != *before {
                    return Err(format!(
                        "{} changed while being read; it will be checked again.",
                        request.path.display()
                    ));
                }
                let hash = session::fingerprint(&bytes);
                let unicode_bom = bytes.starts_with(b"\xef\xbb\xbf")
                    || bytes.starts_with(b"\xff\xfe")
                    || bytes.starts_with(b"\xfe\xff")
                    || bytes.starts_with(b"\0\0\xfe\xff");
                let override_encoding =
                    if unicode_bom || !matches!(request.encoding, Encoding::Legacy(_)) {
                        None
                    } else {
                        Some(&request.encoding)
                    };
                let (text, encoding) = core::decode(&bytes, override_encoding)?;
                let eol = Eol::detect(&text);
                Ok(Snapshot {
                    hash,
                    text,
                    encoding,
                    eol,
                })
            })();
            let (hash, error) = match &result {
                Ok(snapshot) => (Some(snapshot.hash), None),
                Err(error) => (None, Some(error.clone())),
            };
            let duplicate = self.files.get(&request.id).is_some_and(|old| {
                old.path == request.path
                    && old.baseline == request.known_hash
                    && old.hash == hash
                    && old.error == error
            });
            self.files.insert(
                request.id,
                Observed {
                    path: request.path.clone(),
                    baseline: request.known_hash,
                    stamp: current.ok(),
                    hash,
                    error,
                    checked: Instant::now(),
                },
            );
            if !duplicate && (result.is_err() || hash != request.known_hash) {
                changes.push(Change {
                    id: request.id,
                    path: request.path,
                    baseline_hash: request.known_hash,
                    result,
                });
            }
        }
        changes
    }
}

pub struct Monitor {
    tx: mpsc::SyncSender<Option<Vec<Request>>>,
    reset: Arc<AtomicBool>,
    pub rx: mpsc::Receiver<Vec<Change>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}
impl Monitor {
    pub fn new() -> Self {
        let (tx, requests) = mpsc::sync_channel(1);
        let (results, rx) = mpsc::channel();
        let reset = Arc::new(AtomicBool::new(false));
        let reset_requested = reset.clone();
        let thread = thread::spawn(move || {
            let mut scanner = Scanner::default();
            while let Ok(Some(request)) = requests.recv() {
                if reset_requested.swap(false, Ordering::AcqRel) {
                    scanner = Scanner::default();
                }
                if results.send(scanner.scan(request)).is_err() {
                    break;
                }
            }
        });
        Self {
            tx,
            reset,
            rx,
            thread: Some(thread),
        }
    }
    pub fn submit(&self, requests: Vec<Request>) -> Result<()> {
        self.tx
            .try_send(Some(requests))
            .map_err(|e| format!("File monitoring could not start: {e}"))
    }
    pub fn reset(&self) {
        self.reset.store(true, Ordering::Release);
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.tx.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitors_changes_without_repeating_or_treating_saves_as_external() {
        let path = std::env::temp_dir().join(format!(
            "rstpd-monitor-{}-{}.txt",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"old").unwrap();
        let initial = session::fingerprint(b"old");
        let request = |hash| {
            vec![Request {
                id: 1,
                path: path.clone(),
                known_hash: hash,
                encoding: Encoding::Utf8,
            }]
        };
        let mut scanner = Scanner::default();
        assert!(scanner.scan(request(Some(initial))).is_empty());
        fs::write(&path, b"updated text").unwrap();
        let changes = scanner.scan(request(Some(initial)));
        assert_eq!(changes.len(), 1);
        let snapshot = changes.into_iter().next().unwrap().result.unwrap();
        assert_eq!(snapshot.text, "updated text");
        assert!(scanner.scan(request(Some(initial))).is_empty());
        assert!(scanner.scan(request(Some(snapshot.hash))).is_empty());
        let previous_time = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, b"same length!").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(previous_time))
            .unwrap();
        assert!(
            scanner.scan(request(Some(snapshot.hash))).is_empty(),
            "same-stamp edits are deferred until the recheck window elapses"
        );
        scanner.files.get_mut(&1).unwrap().checked =
            Instant::now() - (UNCHANGED_STAMP_RECHECK + Duration::from_secs(1));
        let changes = scanner.scan(request(Some(snapshot.hash)));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].baseline_hash, Some(snapshot.hash));
        assert_eq!(changes[0].result.as_ref().unwrap().text, "same length!");
        assert!(scanner.scan(request(Some(snapshot.hash))).is_empty());
        fs::remove_file(&path).unwrap();
        assert!(
            scanner.scan(request(Some(snapshot.hash)))[0]
                .result
                .is_err()
        );
        assert!(scanner.scan(request(Some(snapshot.hash))).is_empty());
        fs::write(&path, b"recreated").unwrap();
        assert_eq!(scanner.scan(request(Some(snapshot.hash))).len(), 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn resuming_monitoring_rechecks_a_previously_discarded_change() {
        let path = std::env::temp_dir().join(format!(
            "rstpd-monitor-reset-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"external").unwrap();
        let monitor = Monitor::new();
        let request = || {
            vec![Request {
                id: 1,
                path: path.clone(),
                known_hash: Some(session::fingerprint(b"old")),
                encoding: Encoding::Utf8,
            }]
        };
        monitor.submit(request()).unwrap();
        assert_eq!(
            monitor
                .rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .len(),
            1
        );
        monitor.submit(request()).unwrap();
        assert!(
            monitor
                .rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .is_empty()
        );
        monitor.reset();
        monitor.submit(request()).unwrap();
        assert_eq!(
            monitor
                .rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .len(),
            1
        );
        drop(monitor);
        fs::remove_file(path).unwrap();
    }
}
