#![cfg(windows)]
use rstpd::editor::{self, CharacterCounts, Editor, sci::*};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

#[test]
fn native_character_counts_cover_unicode_selections_and_edits() {
    unsafe {
        let instance = GetModuleHandleW(null());
        editor::register(instance).unwrap();
        let class: Vec<_> = "STATIC\0".encode_utf16().collect();
        let parent = CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            WS_OVERLAPPED,
            0,
            0,
            800,
            600,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        assert!(!parent.is_null());
        let left = Editor::new(parent, instance, 101, false).unwrap();
        let right = Editor::new(parent, instance, 102, false).unwrap();
        let first = left.create_document().unwrap();
        left.attach(&first);
        right.attach(&first);
        assert_eq!(
            left.character_counts().unwrap(),
            CharacterCounts {
                total: 0,
                selected: 0
            }
        );
        let text = "A\u{e9}\u{1f680}\r\ne\u{301}\t\0\u{4e2d}";
        left.set_text(text).unwrap();
        assert_eq!(
            left.character_counts().unwrap(),
            CharacterCounts {
                total: 10,
                selected: 0
            }
        );
        assert_eq!(left.send(SCI_GETMODIFY, 0, 0), 0);
        assert_eq!(left.send(SCI_CANUNDO, 0, 0), 0);
        assert_ne!(left.send(SCI_GETLINECHARACTERINDEX, 0, 0) & 1, 0);
        left.select(0..3);
        assert_eq!(left.character_counts().unwrap().selected, 2);
        left.send(SCI_SETSEL, 7, 3);
        assert_eq!(left.character_counts().unwrap().selected, 1);
        left.select(7..9);
        assert_eq!(
            left.character_counts().unwrap().selected,
            2,
            "CR and LF are separate characters"
        );
        left.select(9..12);
        assert_eq!(
            left.character_counts().unwrap().selected,
            2,
            "Combining marks count as code points"
        );
        left.send(SCI_SETSELECTION, 3, 1);
        left.send(SCI_ADDSELECTION, 7, 3);
        left.send(SCI_ADDSELECTION, text.len(), text.len() as isize);
        assert_eq!(left.character_counts().unwrap().selected, 2);
        assert_eq!(right.character_counts().unwrap().selected, 0);
        left.select(0..left.length());
        assert_eq!(left.character_counts().unwrap().selected, 10);
        left.select(0..0);
        left.replace(0..3, "\u{3bb}").unwrap();
        assert_eq!(left.character_counts().unwrap().total, 9);
        assert_eq!(right.character_counts().unwrap().total, 9);
        left.send(SCI_UNDO, 0, 0);
        assert_eq!(left.character_counts().unwrap().total, 10);
        left.send(SCI_REDO, 0, 0);
        assert_eq!(left.character_counts().unwrap().total, 9);

        left.set_text("\u{e9}x\n\u{1f680}y\n").unwrap();
        left.send(SCI_SETRECTANGULARSELECTIONANCHOR, 0, 0);
        left.send(SCI_SETRECTANGULARSELECTIONCARET, 8, 0);
        assert_eq!(left.send(SCI_GETSELECTIONS, 0, 0), 2);
        let rectangular_text = left.text().unwrap();
        let selected: usize = (0..2)
            .map(|index| {
                let start = left.send(SCI_GETSELECTIONNSTART, index, 0) as usize;
                let end = left.send(SCI_GETSELECTIONNEND, index, 0) as usize;
                rectangular_text[start..end].chars().count()
            })
            .sum();
        assert_eq!(
            left.character_counts().unwrap(),
            CharacterCounts { total: 6, selected }
        );
        left.send(SCI_SETRECTANGULARSELECTIONANCHOR, 3, 0);
        left.send(SCI_SETRECTANGULARSELECTIONCARET, 9, 0);
        left.send(SCI_SETRECTANGULARSELECTIONANCHORVIRTUALSPACE, 2, 0);
        left.send(SCI_SETRECTANGULARSELECTIONCARETVIRTUALSPACE, 5, 0);
        assert_eq!(
            left.character_counts().unwrap().selected,
            0,
            "Virtual space is not stored document text"
        );
        let second = left.create_document().unwrap();
        left.attach(&second);
        left.set_text("Other tab").unwrap();
        assert_eq!(left.character_counts().unwrap().total, 9);
        left.attach(&first);
        assert_eq!(left.character_counts().unwrap().total, 6);

        let multiline = "A\u{1f680}\r\n".repeat(5000);
        left.set_text(&multiline).unwrap();
        assert_eq!(left.character_counts().unwrap().total, 20_000);
        left.select(1..left.length() - 1);
        assert_eq!(left.character_counts().unwrap().selected, 19_998);
        left.set_text("a".repeat(100_000).as_str()).unwrap();
        left.select(0..left.length());
        assert_eq!(
            left.character_counts().unwrap(),
            CharacterCounts {
                total: 100_000,
                selected: 100_000
            }
        );
        left.set_text("").unwrap();
        assert_eq!(
            left.character_counts().unwrap(),
            CharacterCounts {
                total: 0,
                selected: 0
            }
        );
        DestroyWindow(parent);
    }
}

#[test]
fn status_count_wording_matches_the_requested_format() {
    assert_eq!(
        CharacterCounts {
            total: 4995,
            selected: 852
        }
        .to_string(),
        "852 of 4995 characters"
    );
    assert_eq!(
        CharacterCounts {
            total: 4995,
            selected: 0
        }
        .to_string(),
        "4995 characters"
    );
    assert_eq!(
        CharacterCounts {
            total: 1,
            selected: 0
        }
        .to_string(),
        "1 character"
    );
    assert_eq!(
        CharacterCounts {
            total: 1,
            selected: 1
        }
        .to_string(),
        "1 of 1 character"
    );
    assert_eq!(
        CharacterCounts {
            total: 0,
            selected: 0
        }
        .to_string(),
        "0 characters"
    );
}
