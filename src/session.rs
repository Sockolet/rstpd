use crate::core::{Encoding, Eol, MAX_DOCUMENT_BYTES, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    hash::{Hash, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const SESSION_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    pub id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub text: String,
    pub encoding: Encoding,
    pub eol: Eol,
    pub language: String,
    pub dirty: bool,
    pub disk_hash: Option<u64>,
    pub caret: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub documents: Vec<DocumentSnapshot>,
    pub active: usize,
    pub theme: String,
    #[serde(default)]
    pub custom_languages: Vec<crate::udl::UserLanguage>,
    #[serde(default)]
    pub completion_api: Vec<crate::completion::Api>,
}

pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("{} exceeds the size limit.", path.display()));
    }
    Ok(bytes)
}

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or("Destination has no parent directory.")?;
    let tmp = parent.join(format!(
        ".rstpad-{}-{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let mut operation = || -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        created = true;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
            };
            let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let ok = unsafe {
                if path.exists() {
                    ReplaceFileW(
                        to.as_ptr(),
                        from.as_ptr(),
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        std::ptr::null(),
                    )
                } else {
                    MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH)
                }
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    };
    let result = operation();
    if let Err(error) = &result
        && created
        && tmp.exists()
        && let Err(cleanup) = fs::remove_file(&tmp)
    {
        return Err(format!("{error}; temporary-file cleanup failed: {cleanup}"));
    }
    result.map_err(|e| format!("Could not save {}: {e}", path.display()))
}

pub fn load(path: &Path) -> Result<Session> {
    if !path.exists() {
        return Ok(Session {
            version: SESSION_VERSION,
            theme: "system".into(),
            ..Session::default()
        });
    }
    let bytes = read_bounded(path, MAX_DOCUMENT_BYTES * 2)?;
    let session: Session = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Recovery file is invalid; it has not been changed: {e}"))?;
    if !matches!(session.version, 1 | SESSION_VERSION) {
        return Err("Unsupported recovery version; the file has not been changed.".into());
    }
    if session.documents.len() > 256
        || session
            .documents
            .iter()
            .any(|d| d.text.len() > MAX_DOCUMENT_BYTES)
    {
        return Err("Recovery file exceeds document limits; it has not been changed.".into());
    }
    if session.custom_languages.len() > 64 || session.completion_api.len() > 10_000 {
        return Err("Recovery language/completion definitions exceed their limits.".into());
    }
    for language in &session.custom_languages {
        language.validate()?;
    }
    for api in &session.completion_api {
        if api.language.len() > 128
            || api.receiver.len() > 256
            || api.name.len() > 256
            || api.signature.len() > 4096
            || api.name.contains('\0')
            || api.signature.contains('\0')
        {
            return Err("Recovery contains an invalid completion definition.".into());
        }
    }
    Ok(session)
}

pub fn save(path: &Path, session: &Session) -> Result<()> {
    struct BoundedOutput(Vec<u8>);
    impl Write for BoundedOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len() + bytes.len() > MAX_DOCUMENT_BYTES * 2 {
                return Err(std::io::Error::other(
                    "Recovery exceeds 256 MiB. Save and close some documents.",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = BoundedOutput(Vec::new());
    serde_json::to_writer(&mut output, session).map_err(|e| e.to_string())?;
    atomic_write(path, &output.0)
}

pub struct RecoveryWorker {
    tx: std::sync::mpsc::Sender<Option<(u64, Session)>>,
    pub rx: std::sync::mpsc::Receiver<(u64, Result<()>)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl RecoveryWorker {
    pub fn new(path: PathBuf) -> Self {
        let (tx, requests) = std::sync::mpsc::channel::<Option<(u64, Session)>>();
        let (results, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            while let Ok(Some((revision, session))) = requests.recv() {
                if results.send((revision, save(&path, &session))).is_err() {
                    break;
                }
            }
        });
        Self {
            tx,
            rx,
            thread: Some(thread),
        }
    }
    pub fn submit(&self, revision: u64, session: Session) -> Result<()> {
        self.tx
            .send(Some((revision, session)))
            .map_err(|_| "Recovery worker stopped.".into())
    }
    pub fn flush(&self, revision: u64, session: Session) -> Result<()> {
        self.submit(revision, session)?;
        loop {
            let (saved, result) = self.rx.recv().map_err(|_| "Recovery worker stopped.")?;
            if saved == revision {
                return result;
            }
        }
    }
}
impl Drop for RecoveryWorker {
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
    fn atomic_session_round_trip_and_replace() {
        let dir = std::env::temp_dir().join(format!(
            "rstpad-test-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        let mut session = Session {
            version: 1,
            theme: "dark".into(),
            ..Session::default()
        };
        save(&path, &session).unwrap();
        session.documents.push(DocumentSnapshot {
            id: 1,
            title: "Untitled".into(),
            path: None,
            text: "unsaved\0text".into(),
            encoding: Encoding::Utf8,
            eol: Eol::Lf,
            language: "Rust".into(),
            dirty: true,
            disk_hash: None,
            caret: 3,
        });
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap().documents[0].text, "unsaved\0text");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&dir).unwrap();
    }
}
