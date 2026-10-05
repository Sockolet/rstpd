use rstpd::{
    completion,
    core::{self, Encoding, Eol},
    session::{self, DocumentSnapshot, Session},
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "rstpd-parity-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn pane_session() -> Session {
    Session {
        version: session::SESSION_VERSION,
        theme: "system".into(),
        active: 2,
        documents: (0..3)
            .map(|index| DocumentSnapshot {
                id: index + 1,
                title: format!("document {index}"),
                path: None,
                text: format!("unsaved {index}"),
                encoding: Encoding::Utf8,
                eol: Eol::Lf,
                language: "Plain text".into(),
                dirty: true,
                disk_hash: None,
                caret: 0,
                pinned: index == 2,
            })
            .collect(),
        pane_documents: [vec![2, 0], vec![2, 1]],
        pane_selected: [0, 2],
        focused_pane: 1,
        ..Session::default()
    }
}

#[test]
fn completion_hints_and_receiver_types_have_the_same_contract_on_both_platforms() {
    let text = "fn f(a: i32, b: i32, c: i32) {}\nf(g::<A, B>(), x < y, ";
    let tip = completion::call_tip(text, text.len(), "Rust", &[]).unwrap();
    assert_eq!(tip.signature[tip.parameter].trim(), "c: i32");
    let text = "let text: String = String::new();\nif text == \"\" {}\ntext.tr";
    let words = completion::suggestions(text, text.len(), "Rust", &[], &[], false).words;
    assert!(words.contains(&"trim".into()));
}

#[test]
fn utf16_hints_never_change_decoding_without_an_explicit_override() {
    for (bytes, encoding) in [
        (&b"a\0b\0c\0"[..], Encoding::Utf16Le),
        (&b"\0a\0b\0c"[..], Encoding::Utf16Be),
    ] {
        assert_eq!(core::bomless_utf16_hint(bytes), Some(encoding.clone()));
        let (text, detected) = core::decode(bytes, None).unwrap();
        assert_eq!(detected, Encoding::Utf8);
        assert!(text.contains('\0'));
        assert_eq!(core::decode(bytes, Some(&encoding)).unwrap().0, "abc");
    }
}

#[test]
fn pane_groups_clones_selections_and_focus_round_trip_with_legacy_defaults() {
    let workspace = Workspace::new();
    let path = workspace.0.join("session.json");
    let session = pane_session();
    session::save(&path, &session).unwrap();
    let restored = session::load(&path).unwrap();
    assert_eq!(restored.pane_documents, session.pane_documents);
    assert_eq!(restored.pane_selected, session.pane_selected);
    assert_eq!(restored.focused_pane, 1);
    assert_eq!(
        restored.documents.len(),
        3,
        "A clone is a view, not a copied document"
    );

    for version in [1, session::SESSION_VERSION] {
        let mut legacy = serde_json::to_value(&session).unwrap();
        for field in ["pane_documents", "pane_selected", "focused_pane"] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["version"] = version.into();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let restored = session::load(&path).unwrap();
        assert!(restored.pane_documents.iter().all(Vec::is_empty));
        assert_eq!(restored.pane_selected, [0, 0]);
        assert_eq!(restored.focused_pane, 0);
        assert_eq!(restored.documents.len(), 3);
    }
}

#[test]
fn invalid_recovery_is_quarantined_without_changing_original_bytes() {
    let workspace = Workspace::new();
    let path = workspace.0.join("session.json");
    let baseline = serde_json::to_value(pane_session()).unwrap();
    let mut unsupported = baseline.clone();
    unsupported["version"] = 99.into();
    let mut too_many = baseline.clone();
    too_many["documents"] = serde_json::Value::Array(vec![baseline["documents"][0].clone(); 257]);
    let mut bad_regex = baseline.clone();
    bad_regex["compare_options"]["ignore_regex"] = "[".into();
    let mut bad_font = baseline;
    bad_font["editor_font"]["size_hundredths"] = 0.into();
    for invalid in [unsupported, too_many, bad_regex, bad_font] {
        let bytes = serde_json::to_vec(&invalid).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(session::load(&path).is_err());
        let (empty, warning) = session::load_or_quarantine(&path).unwrap();
        assert!(empty.documents.is_empty());
        assert!(warning.unwrap().contains("moved, unchanged"));
        assert!(!path.exists());
        assert!(
            fs::read_dir(&workspace.0)
                .unwrap()
                .any(|entry| { fs::read(entry.unwrap().path()).unwrap() == bytes })
        );
    }
}

#[test]
fn unreadable_file_types_are_not_misreported_as_quarantined_recovery() {
    let workspace = Workspace::new();
    let path = workspace.0.join("session.json");
    fs::create_dir(&path).unwrap();
    assert!(session::load_or_quarantine(&path).is_err());
    assert!(path.is_dir());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_recovery_symlinks_and_hard_links_remain_untouched() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new();
    let original = workspace.0.join("original.json");
    let path = workspace.0.join("session.json");
    let bytes = b"invalid recovery that must not be moved through a link";
    fs::write(&original, bytes).unwrap();
    symlink(&original, &path).unwrap();
    assert!(session::load_or_quarantine(&path).is_err());
    assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
    fs::remove_file(&path).unwrap();
    fs::hard_link(&original, &path).unwrap();
    assert!(session::load_or_quarantine(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&original).unwrap(), bytes);
}
