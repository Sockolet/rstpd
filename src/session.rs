use crate::core::{EditorFont, Encoding, Eol, MAX_DOCUMENT_BYTES, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SESSION_VERSION: u32 = 2;
pub const MAX_RECOVERY_BYTES: usize = 512 * 1024 * 1024;

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
    #[serde(default)]
    pub pinned: bool,
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
    pub show_symbols: crate::symbols::ShowSymbols,
    #[serde(default = "monitor_enabled")]
    pub monitor_files: bool,
    #[serde(default)]
    pub compare_options: crate::comparison::CompareOptions,
    #[serde(default)]
    pub custom_languages: Vec<crate::udl::UserLanguage>,
    #[serde(default)]
    pub completion_api: Vec<crate::completion::Api>,
    #[serde(default)]
    pub pane_documents: [Vec<usize>; 2],
    #[serde(default)]
    pub pane_selected: [usize; 2],
    #[serde(default)]
    pub focused_pane: usize,
}

fn monitor_enabled() -> bool {
    true
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

fn empty_session() -> Session {
    Session {
        version: SESSION_VERSION,
        theme: "system".into(),
        monitor_files: true,
        ..Session::default()
    }
}

pub fn load(path: &Path) -> Result<Session> {
    if !path.exists() {
        return Ok(empty_session());
    }
    parse(&read_bounded(path, MAX_RECOVERY_BYTES)?)
}

fn parse(bytes: &[u8]) -> Result<Session> {
    let session: Session = serde_json::from_slice(bytes)
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
    session.compare_options.validate()?;
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

/// Loads recovery state for startup. Content that fails validation is renamed
/// (never edited or deleted) so the editor can start with an empty session.
/// Read errors such as access denied still fail startup, because renaming a
/// file we could not read would hide an environmental problem.
pub fn load_or_quarantine(path: &Path) -> Result<(Session, Option<String>)> {
    if !path.exists() {
        return Ok((empty_session(), None));
    }
    let bytes = read_bounded(path, MAX_RECOVERY_BYTES)?;
    match parse(&bytes) {
        Ok(session) => Ok((session, None)),
        Err(error) => {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            let moved =
                quarantine(path, stamp).map_err(|move_error| format!("{error}\n\n{move_error}"))?;
            Ok((
                empty_session(),
                Some(format!(
                    "The recovery file could not be used and was moved, unchanged, to:\n{}\n\nrstpd started with an empty session.\n\nDetails: {error}",
                    moved.display()
                )),
            ))
        }
    }
}

fn quarantine(path: &Path, stamp: u64) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or("The recovery file has no parent directory.")?;
    let stem = path.file_stem().map_or_else(
        || "session".into(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    for attempt in 0..100u32 {
        let name = if attempt == 0 {
            format!("{stem}.invalid-{stamp}.json")
        } else {
            format!("{stem}.invalid-{stamp}-{attempt}.json")
        };
        let target = parent.join(name);
        if target
            .try_exists()
            .map_err(|e| format!("Could not inspect {}: {e}", target.display()))?
        {
            continue;
        }
        fs::rename(path, &target)
            .map_err(|e| format!("The invalid recovery file could not be moved aside: {e}"))?;
        return Ok(target);
    }
    Err("Could not choose a name for the invalid recovery file.".into())
}

pub fn save(path: &Path, session: &Session) -> Result<()> {
    struct BoundedOutput(Vec<u8>);
    impl Write for BoundedOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len() + bytes.len() > MAX_RECOVERY_BYTES {
                return Err(std::io::Error::other(
                    "Recovery exceeds 512 MiB. Save and close some documents.",
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
    #[test]
    fn pane_metadata_round_trips_and_legacy_defaults() {
        let session = super::Session {
            pane_documents: [vec![2, 0], vec![1, 2]],
            pane_selected: [0, 2],
            focused_pane: 1,
            ..Default::default()
        };
        let mut json = serde_json::to_value(&session).unwrap();
        let restored: super::Session = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored.pane_documents, session.pane_documents);
        assert_eq!(restored.pane_selected, [0, 2]);
        assert_eq!(restored.focused_pane, 1);
        for field in ["pane_documents", "pane_selected", "focused_pane"] {
            json.as_object_mut().unwrap().remove(field);
        }
        let legacy: super::Session = serde_json::from_value(json).unwrap();
        assert!(legacy.pane_documents.iter().all(Vec::is_empty));
        assert_eq!(legacy.focused_pane, 0);
    }
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rstpd-{name}-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn invalid_recovery_is_moved_aside_unchanged() {
        for input in [
            b"{not json".to_vec(),
            serde_json::to_vec(&serde_json::json!({
                "version": 99,
                "documents": [],
                "active": 0,
                "theme": "system"
            }))
            .unwrap(),
        ] {
            let dir = test_directory("invalid-recovery");
            fs::create_dir(&dir).unwrap();
            let path = dir.join("session.json");
            fs::write(&path, &input).unwrap();
            let (session, warning) = load_or_quarantine(&path).unwrap();
            assert!(session.documents.is_empty());
            assert_eq!(session.version, SESSION_VERSION);
            assert_eq!(session.theme, "system");
            assert!(session.monitor_files);
            let warning = warning.unwrap();
            assert!(warning.contains("started with an empty session"));
            assert!(!path.exists());
            let entries: Vec<_> = fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap())
                .collect();
            assert_eq!(entries.len(), 1);
            let moved = entries[0].path();
            let name = moved.file_name().unwrap().to_string_lossy();
            assert!(name.starts_with("session.invalid-"));
            assert!(name.ends_with(".json"));
            assert_eq!(fs::read(&moved).unwrap(), input);
            assert!(warning.contains(&moved.display().to_string()));
            fs::remove_file(moved).unwrap();
            fs::remove_dir(&dir).unwrap();
        }
    }

    #[test]
    fn valid_and_missing_recovery_are_not_moved() {
        let dir = test_directory("valid-recovery");
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        let (missing, warning) = load_or_quarantine(&path).unwrap();
        assert_eq!(missing.version, SESSION_VERSION);
        assert!(warning.is_none());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);

        let session = Session {
            version: SESSION_VERSION,
            theme: "dark".into(),
            ..Session::default()
        };
        save(&path, &session).unwrap();
        let bytes = fs::read(&path).unwrap();
        let (restored, warning) = load_or_quarantine(&path).unwrap();
        assert_eq!(restored.theme, "dark");
        assert!(warning.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn quarantine_never_overwrites_an_earlier_copy() {
        let dir = test_directory("quarantine-collision");
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        let earlier = dir.join("session.invalid-42.json");
        fs::write(&path, b"new").unwrap();
        fs::write(&earlier, b"old").unwrap();
        let moved = quarantine(&path, 42).unwrap();
        assert_eq!(moved, dir.join("session.invalid-42-1.json"));
        assert_eq!(fs::read(&earlier).unwrap(), b"old");
        assert_eq!(fs::read(&moved).unwrap(), b"new");
        fs::remove_file(earlier).unwrap();
        fs::remove_file(moved).unwrap();
        fs::remove_dir(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn unreadable_recovery_fails_without_moving() {
        let dir = test_directory("unreadable-recovery");
        fs::create_dir(&dir).unwrap();
        let path = dir.join("session.json");
        fs::create_dir(&path).unwrap();
        assert!(load_or_quarantine(&path).is_err());
        assert!(path.is_dir());
        assert!(!fs::read_dir(&dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("session.invalid-")
        }));
        fs::remove_dir(&path).unwrap();
        fs::remove_dir(&dir).unwrap();
    }

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
            pinned: true,
        });
        session.monitor_files = true;
        session.compare_options.ignore_case = true;
        session.compare_options.ignore_regex = r"\d+".into();
        save(&path, &session).unwrap();
        assert_eq!(load(&path).unwrap().documents[0].text, "unsaved\0text");
        let restored = load(&path).unwrap();
        assert!(
            restored.documents[0].pinned
                && restored.monitor_files
                && restored.compare_options.ignore_case
        );
        assert_eq!(restored.compare_options.ignore_regex, r"\d+");
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
