#![cfg(windows)]
use rstpd::{
    core::{Search, SearchMode},
    editor::{self, Editor, Palette, sci::*},
    search_results,
    symbols::ShowSymbols,
};
use std::{
    ptr::{null, null_mut},
    sync::atomic::AtomicBool,
};
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

unsafe fn representation(editor: Editor, ch: char) -> String {
    let mut bytes = [0u8; 5];
    ch.encode_utf8(&mut bytes[..4]);
    let mut output = [0u8; 128];
    let len = unsafe {
        editor.send_raw(
            SCI_GETREPRESENTATION,
            bytes.as_ptr() as usize,
            output.as_mut_ptr() as isize,
        )
    };
    String::from_utf8(output[..len.max(0) as usize].to_vec()).unwrap()
}

#[test]
fn symbol_options_are_view_only_and_results_are_read_only() {
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
        let map = Editor::new(parent, instance, 104, false).unwrap();
        let results = Editor::new(parent, instance, 105, false).unwrap();
        let document = left.create_document().unwrap();
        for view in [left, right, map] {
            view.attach(&document);
        }
        let original = " \tspace\r\n\u{a0}\u{200b}\u{85}\u{2028}\u{2029}\0\u{1}\u{7f}\u{80}";
        left.set_text(original).unwrap();
        left.replace(left.length()..left.length(), "edited")
            .unwrap();
        let text = left.text().unwrap();
        left.select(2..7);
        let mut options = ShowSymbols::default();
        options.toggle_all();
        options.indent_guides = true;
        options.wrap_markers = true;
        for dark in [false, true] {
            for view in [left, right] {
                view.show_symbols(options, Palette::new(dark));
                assert_eq!(view.send(SCI_GETVIEWWS, 0, 0), 1);
                assert_eq!(view.send(SCI_GETVIEWEOL, 0, 0), 1);
                assert_eq!(view.send(SCI_GETINDENTATIONGUIDES, 0, 0), 3);
                assert_eq!(view.send(SCI_GETWRAPVISUALFLAGS, 0, 0), 1);
                for (ch, expected) in [
                    ('\0', "NUL"),
                    ('\u{a0}', "NBSP"),
                    ('\u{200b}', "ZWSP"),
                    ('\u{2028}', "LS"),
                    ('\u{85}', "NEL"),
                ] {
                    assert_eq!(representation(view, ch), expected);
                }
                assert_eq!(view.text().unwrap(), text);
                assert_ne!(view.send(SCI_GETMODIFY, 0, 0), 0);
            }
        }
        assert_eq!(left.selection(), 2..7);
        assert_eq!(map.send(SCI_GETVIEWWS, 0, 0), 0);
        assert_eq!(map.send(SCI_GETVIEWEOL, 0, 0), 0);
        assert_eq!(representation(map, '\u{a0}'), "");
        options.toggle_all();
        left.show_symbols(options, Palette::new(false));
        assert_eq!(representation(left, '\0'), " ");
        assert_eq!(representation(left, '\u{a0}'), "");
        assert_eq!(left.send(SCI_GETVIEWWS, 0, 0), 0);
        assert_eq!(left.send(SCI_GETVIEWEOL, 0, 0), 0);
        assert_eq!(left.send(SCI_GETINDENTATIONGUIDES, 0, 0), 3);
        options.non_printing = true;
        left.show_symbols(options, Palette::new(false));
        assert_eq!(representation(left, '\u{85}'), "NEL");
        assert_eq!(representation(left, '\u{2028}'), "LS");
        assert_eq!(representation(left, '\u{1}'), " ");
        left.send(SCI_UNDO, 0, 0);
        assert_eq!(left.text().unwrap(), original);
        assert_eq!(left.send(SCI_GETMODIFY, 0, 0), 0);

        let search = Search::new("space", SearchMode::Literal, true, false).unwrap();
        let data = search_results::find_all(
            &search,
            "space".into(),
            vec![search_results::Input {
                id: 1,
                revision: 0,
                title: "Untitled 1".into(),
                text: original.into(),
                tab_width: 4,
            }],
            &AtomicBool::new(false),
        )
        .unwrap();
        let rendered = data.render();
        results.set_read_only_text(&rendered.text).unwrap();
        results.highlight(&rendered.highlight).unwrap();
        for range in &rendered.emphasis {
            results
                .indicator(search_results::MATCH_INDICATOR, range.clone())
                .unwrap();
        }
        assert_eq!(results.send(SCI_GETREADONLY, 0, 0), 1);
        assert_ne!(
            results.send(
                SCI_INDICATORVALUEAT,
                search_results::MATCH_INDICATOR,
                rendered.emphasis[0].start as isize
            ),
            0
        );
        assert_ne!(results.send(SCI_GETFOLDLEVEL, 1, 0) & 0x2000, 0);
        results.send(SCI_CLEARALL, 0, 0);
        assert_eq!(results.text().unwrap(), rendered.text);
        results.set_read_only_text("No matches.\n").unwrap();
        assert_eq!(results.send(SCI_GETREADONLY, 0, 0), 1);
        DestroyWindow(parent);
    }
}
