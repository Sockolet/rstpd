#![cfg(windows)]
use rstpd::{
    editor::{self, Editor, Palette, sci::*},
    languages,
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

#[test]
fn native_editing_unicode_split_selection_highlighting_and_undo() {
    unsafe {
        let instance = GetModuleHandleW(null());
        editor::register(instance).unwrap();
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
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
            null_mut(),
        );
        assert!(!parent.is_null());
        let left = Editor::new(parent, instance, 101, false).unwrap();
        let right = Editor::new(parent, instance, 102, false).unwrap();
        let document = left.create_document().unwrap();
        left.attach(&document);
        right.attach(&document);
        left.set_text("caf\u{e9}\0\u{1f680}\r\nsecond\r\n").unwrap();
        assert_eq!(left.text().unwrap(), "caf\u{e9}\0\u{1f680}\r\nsecond\r\n");
        assert!(left.range(4..5).is_err());
        assert!(left.replace(0..4, "bad").is_err());
        assert!(left.replace(100..101, "bad").is_err());
        assert_eq!(left.send(SCI_GETCODEPAGE, 0, 0), 65001);
        assert_eq!(left.send(SCI_POSITIONAFTER, 3, 0), 5);
        assert_eq!(left.send(SCI_POSITIONAFTER, 6, 0), 10);
        left.replace(0..5, "tea").unwrap();
        assert_eq!(right.text().unwrap(), "tea\0\u{1f680}\r\nsecond\r\n");
        right.send(SCI_UNDO, 0, 0);
        assert!(left.text().unwrap().starts_with("caf\u{e9}"));

        left.set_text("one\ntwo\n").unwrap();
        left.send(SCI_SETSELECTION, 3, 0);
        left.send(SCI_ADDSELECTION, 7, 4);
        assert_eq!(left.send(SCI_GETSELECTIONS, 0, 0), 2);
        left.transform_selections(str::to_uppercase).unwrap();
        assert_eq!(right.text().unwrap(), "ONE\nTWO\n");
        left.send(SCI_UNDO, 0, 0);
        assert_eq!(left.text().unwrap(), "one\ntwo\n");
        left.send(SCI_SETRECTANGULARSELECTIONANCHOR, 1, 0);
        left.send(SCI_SETRECTANGULARSELECTIONCARET, 6, 0);
        assert_eq!(left.send(SCI_SELECTIONISRECTANGLE, 0, 0), 1);
        assert_eq!(left.send(SCI_GETSELECTIONS, 0, 0), 2);

        let available = editor::available_lexers();
        let catalog = languages::catalog(&available);
        for language in &catalog {
            left.language(language, Palette::new(false)).unwrap();
            if language.uses_container() {
                assert_eq!(left.send(SCI_GETLEXER, 0, 0), 0);
                continue;
            }
            let len = left.send_raw(SCI_GETLEXERLANGUAGE, 0, 0) as usize;
            let mut name = vec![0u8; len + 1];
            left.send_raw(SCI_GETLEXERLANGUAGE, 0, name.as_mut_ptr() as isize);
            assert!(
                name[..len].eq_ignore_ascii_case(language.lexer.as_bytes()),
                "{}",
                language.name
            );
        }
        let rust = &catalog[languages::detect(std::path::Path::new("main.rs"), &catalog)];
        left.language(rust, Palette::new(true)).unwrap();
        left.set_text("fn main() { let x = 123; } // comment")
            .unwrap();
        left.send(SCI_COLOURISE, 0, -1);
        assert_ne!(
            left.send(SCI_GETSTYLEAT, 0, 0),
            left.send(SCI_GETSTYLEAT, 2, 0)
        );
        assert_ne!(
            left.send(SCI_GETSTYLEAT, 0, 0),
            left.send(SCI_GETSTYLEAT, 19, 0)
        );
        let position = 12;
        left.send(SCI_GOTOPOS, position, 0);
        left.attach(&document);
        assert_eq!(left.position(), position);

        left.set_text("let text = String::from(\"hello\");\ntext.tr")
            .unwrap();
        left.send(SCI_GOTOPOS, left.length(), 0);
        left.complete(rust, &[], true).unwrap();
        assert_ne!(left.send(SCI_AUTOCACTIVE, 0, 0), 0);
        left.send(SCI_AUTOCCANCEL, 0, 0);
        left.set_text(include_str!("fixtures\\functions.rs"))
            .unwrap();
        left.send(SCI_GOTOPOS, left.length(), 0);
        left.call_tip(rust, &[]).unwrap();
        assert_ne!(left.send(SCI_CALLTIPACTIVE, 0, 0), 0);
        left.send(SCI_CALLTIPCANCEL, 0, 0);

        let definition = rstpd::udl::import(include_str!("fixtures\\custom-language.xml"))
            .unwrap()
            .remove(0);
        let mut custom_catalog = languages::catalog(&available);
        let custom = languages::add_custom(&mut custom_catalog, definition.clone()).unwrap();
        left.language(&custom_catalog[custom], Palette::new(false))
            .unwrap();
        let text = include_str!("fixtures\\sample.rstlang");
        left.set_text(text).unwrap();
        left.highlight(&rstpd::udl::highlight(&definition, text).unwrap())
            .unwrap();
        assert_eq!(left.send(SCI_GETSTYLEAT, text.find("say").unwrap(), 0), 4);
        assert_ne!(left.send(SCI_GETFOLDLEVEL, 0, 0) & 0x2000, 0);
        left.indicator(20, 0..5).unwrap();
        assert_ne!(left.send(SCI_INDICATORVALUEAT, 20, 1), 0);
        left.clear_diff();
        assert_eq!(left.send(SCI_INDICATORVALUEAT, 20, 1), 0);
        left.clear_styles();
        assert_eq!(left.send(SCI_GETENDSTYLED, 0, 0) as usize, left.length());
        assert_eq!(left.send(SCI_GETSTYLEAT, text.find("say").unwrap(), 0), 0);
        DestroyWindow(parent);
        drop(document);
    }
}
