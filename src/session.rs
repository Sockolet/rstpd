use crate::core::{EditorFont, Encoding, Eol, MAX_DOCUMENT_BYTES, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const SESSION_VERSION: u32 = 2;

pub fn default_directory(local_app_data: &Path) -> Result<PathBuf> {
    let current = local_app_data.join("rstpd");
    let legacy = local_app_data.join("RSTPad");
    for directory in [&current, &legacy] {
        for name in ["session.json", "session.lock"] {
            let path = directory.join(name);
            if path.try_exists().map_err(|error| {
                format!(
                    "Could not inspect recovery state {}: {error}",
                    path.display()
                )
            })? {
                return Ok(directory.clone());
            }
        }
    }
    Ok(current)
}

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
    pub editor_font: EditorFont,
    #[serde(default)]
    pub custom_languages: Vec<crate::udl::UserLanguage>,
    #[serde(default)]
    pub completion_api: Vec<crate::completion::Api>,
}

/// FNV-1a (64-bit). Pinned on purpose: this value is written to `session.json` and
/// compared after a restart, so it must not change when the Rust toolchain changes.
/// `DefaultHasher` gives no such guarantee. Not cryptographic; used only to detect
/// that a file changed outside the editor.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
        ".rstpd-{}-{}.tmp",
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
    fn editor_font_preferences_round_trip_without_a_schema_bump() {
        let dir = std::env::temp_dir().join(format!(
            "rstpd-font-session-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        assert_eq!(load(&path).unwrap().editor_font, EditorFont::default());
        let session = Session {
            version: SESSION_VERSION,
            theme: "dark".into(),
            editor_font: EditorFont::new("Example Sans", 1250).unwrap(),
            ..Session::default()
        };
        save(&path, &session).unwrap();
        let restored = load(&path).unwrap();
        assert_eq!(restored.version, 2);
        assert_eq!(restored.editor_font, session.editor_font);
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            stored["editor_font"],
            serde_json::json!({"family": "Example Sans", "size_hundredths": 1250})
        );
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn old_sessions_default_the_font_without_rewriting_recovery() {
        let dir = std::env::temp_dir().join(format!(
            "rstpd-font-legacy-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        for version in [1, 2] {
            let bytes = serde_json::to_vec(&serde_json::json!({
                "version": version, "documents": [], "active": 0, "theme": "system"
            }))
            .unwrap();
            fs::write(&path, &bytes).unwrap();
            assert_eq!(load(&path).unwrap().editor_font, EditorFont::default());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn invalid_stored_fonts_fail_without_changing_recovery_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "rstpd-font-invalid-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        for font in [
            serde_json::json!({"family": " ", "size_hundredths": 1100}),
            serde_json::json!({"family": "Consolas\u{0000}", "size_hundredths": 1100}),
            serde_json::json!({"family": "a".repeat(129), "size_hundredths": 1100}),
            serde_json::json!({"family": "Consolas", "size_hundredths": 399}),
            serde_json::json!({"family": "Consolas", "size_hundredths": 7201}),
            serde_json::Value::Null,
        ] {
            let bytes = serde_json::to_vec(&serde_json::json!({
                "version": 2, "documents": [], "active": 0, "theme": "system",
                "editor_font": font
            }))
            .unwrap();
            fs::write(&path, &bytes).unwrap();
            let error = load(&path).unwrap_err();
            assert!(error.contains("has not been changed"), "{error}");
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        }
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn renamed_app_preserves_legacy_recovery_and_lock_locations() {
        let root = std::env::temp_dir().join(format!(
            "rstpd-rename-test-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let current = root.join("rstpd");
        let legacy = root.join("RSTPad");
        assert_eq!(default_directory(&root).unwrap(), current);
        fs::create_dir(&legacy).unwrap();
        fs::write(legacy.join("session.lock"), b"").unwrap();
        assert_eq!(default_directory(&root).unwrap(), legacy);
        fs::create_dir(&current).unwrap();
        assert_eq!(default_directory(&root).unwrap(), legacy);
        fs::write(legacy.join("session.json"), b"legacy recovery").unwrap();
        assert_eq!(default_directory(&root).unwrap(), legacy);
        fs::write(current.join("session.json"), b"current recovery").unwrap();
        assert_eq!(default_directory(&root).unwrap(), current);
        assert_eq!(
            fs::read(legacy.join("session.json")).unwrap(),
            b"legacy recovery"
        );
        for file in [
            current.join("session.json"),
            legacy.join("session.json"),
            legacy.join("session.lock"),
        ] {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(current).unwrap();
        fs::remove_dir(legacy).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn atomic_session_round_trip_and_replace() {
        let dir = std::env::temp_dir().join(format!(
            "rstpd-test-{}-{}",
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

    #[test]
    fn fingerprint_is_pinned_across_toolchain_releases() {
        assert_eq!(fingerprint(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fingerprint(b"abc"), 0xe71f_a219_0541_574b);
        assert_ne!(fingerprint(b"abc"), fingerprint(b"abd"));
        assert_ne!(fingerprint(b"ab"), fingerprint(b"abc"));
    }
}
