#![cfg(windows)]
use rstpd::{
    core::{EditorFont, MAX_DOCUMENT_BYTES},
    editor::{self, Editor, Palette, sci::*},
    languages,
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

fn font_family(editor: Editor, style: usize) -> String {
    unsafe {
        let length = usize::try_from(editor.send_raw(SCI_STYLEGETFONT, style, 0)).unwrap();
        let mut bytes = vec![0; length + 1];
        editor.send_raw(SCI_STYLEGETFONT, style, bytes.as_mut_ptr() as isize);
        bytes.truncate(length);
        String::from_utf8(bytes).unwrap()
    }
}

#[test]
fn markdown_refresh_preserves_mapped_viewport_in_hidden_control() {
    unsafe {
        let instance = GetModuleHandleW(null());
        editor::register(instance).unwrap();
        // No top-level window is ever shown.
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
        let source_editor = Editor::new(parent, instance, 101, false).unwrap();
        let preview = Editor::new(parent, instance, 102, false).unwrap();
        let preview_hwnd = GetDlgItem(parent, 102);
        assert!(!preview_hwnd.is_null());
        MoveWindow(preview_hwnd, 0, 0, 320, 240, 0);
        assert_eq!(IsWindowVisible(parent), 0);
        assert_eq!(IsWindowVisible(preview_hwnd), 0);
        for margin in 0..5 {
            preview.send(SCI_SETMARGINWIDTHN, margin, 0);
        }
        preview.send(SCI_SETWRAPMODE, 1, 0);
        let old: String = (0..80).map(|i| format!("## Section {i}\n\nParagraph {i} has enough words to wrap across the narrow preview several times without changing source selection.\n\n")).collect();
        source_editor.set_text(&old).unwrap();
        source_editor.send(SCI_SETSEL, 20, 30);
        let (text, highlight, map) = rstpd::markdown_preview::render_mapped(&old).unwrap();
        preview.set_read_only_text(&text).unwrap();
        preview.highlight(&highlight).unwrap();
        preview.restore_viewport_position(text.find("Paragraph 40").unwrap() + 70);
        let anchor = map.source(preview.viewport_position());
        let saved_position = map.rendered(anchor);
        preview.set_read_only_text("Another document\n").unwrap();
        preview.restore_viewport_position(0);
        assert_eq!(preview.viewport_position(), 0);
        preview.set_read_only_text(&text).unwrap();
        preview.highlight(&highlight).unwrap();
        preview.restore_viewport_position(saved_position);
        assert_eq!(preview.viewport_position(), saved_position);
        for new in [
            format!("# Inserted\n\n{old}"),
            old.replace("## Section 3\n\n", ""),
            old.replace("Paragraph 40", "Extra words inserted before Paragraph 40"),
        ] {
            let (text, highlight, map) = rstpd::markdown_preview::render_mapped(&new).unwrap();
            let anchor = rstpd::markdown_preview::rebase(&old, &new, anchor);
            preview.set_read_only_text(&text).unwrap();
            preview.highlight(&highlight).unwrap();
            let position = map.rendered(anchor);
            preview.restore_viewport_position(position);
            let top_position = preview.viewport_position();
            assert!(top_position <= position);
            assert_eq!(
                preview.send(SCI_POINTYFROMPOSITION, 0, position as isize),
                preview.send(SCI_POINTYFROMPOSITION, 0, top_position as isize),
            );
            assert_eq!(source_editor.send(SCI_GETANCHOR, 0, 0), 20);
            assert_eq!(source_editor.send(SCI_GETCURRENTPOS, 0, 0), 30);
        }
        DestroyWindow(parent);
    }
}

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
        let preview = Editor::new(parent, instance, 105, false).unwrap();
        let markdown = catalog
            .iter()
            .find(|language| language.lexer == "markdown")
            .unwrap();
        preview.theme_with_font(markdown, Palette::new(true), &EditorFont::default());
        preview.send_raw(SCI_SETILEXER, 0, 0);
        let (rendered, styles) =
            rstpd::markdown_preview::render("# Héllo\n\n**bold** and `code`").unwrap();
        preview.set_read_only_text(&rendered).unwrap();
        preview.highlight(&styles).unwrap();
        assert_eq!(preview.send(SCI_GETREADONLY, 0, 0), 1);
        assert_eq!(preview.send(SCI_GETSTYLEAT, 0, 0), 6);
        assert_eq!(preview.text().unwrap(), rendered);
        preview.send_raw(SCI_INSERTTEXT, 0, c"not allowed".as_ptr() as isize);
        assert_eq!(preview.text().unwrap(), rendered);
        assert_eq!(left.text().unwrap(), "one\ntwo\n");
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
        let preferred_font = EditorFont::new("Segoe UI", 1250).unwrap();
        let original = left.text().unwrap();
        left.replace(left.length()..left.length(), "\n").unwrap();
        let edited = left.text().unwrap();
        for pane in [left, right] {
            pane.language_with_font(rust, Palette::new(false), &preferred_font)
                .unwrap();
            pane.send(SCI_SETZOOM, 3, 0);
            pane.theme_with_font(rust, Palette::new(true), &preferred_font);
            assert_eq!(font_family(pane, 32), "Segoe UI");
            assert_eq!(pane.send(SCI_STYLEGETSIZEFRACTIONAL, 32, 0), 1250);
            assert_eq!(pane.send(SCI_GETZOOM, 0, 0), 3);
            assert_eq!(pane.text().unwrap(), edited);
            assert_ne!(pane.send(SCI_GETMODIFY, 0, 0), 0);
            assert_ne!(pane.send(SCI_CANUNDO, 0, 0), 0);
            pane.send(SCI_SETZOOM, 0, 0);
            assert_eq!(pane.send(SCI_STYLEGETSIZEFRACTIONAL, 32, 0), 1250);
        }
        left.send(SCI_UNDO, 0, 0);
        assert_eq!(left.text().unwrap(), original);
        assert_eq!(right.text().unwrap(), original);
        assert_eq!(left.send(SCI_GETMODIFY, 0, 0), 0);
        left.theme(rust, Palette::new(true));
        assert_eq!(font_family(left, 32), EditorFont::default().family());
        assert_eq!(left.send(SCI_STYLEGETSIZEFRACTIONAL, 32, 0), 1100);
        assert_eq!(left.send(SCI_GETMODIFY, 0, 0), 0);
        assert_ne!(left.send(SCI_CANREDO, 0, 0), 0);
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

        let mut definition = rstpd::udl::import(include_str!("fixtures\\custom-language.xml"))
            .unwrap()
            .remove(0);
        definition.styles[4].font = Some("Consolas".into());
        definition.styles[4].font_size = Some(18);
        definition.styles[4].font_style = 7;
        let mut custom_catalog = languages::catalog(&available);
        let custom = languages::add_custom(&mut custom_catalog, definition.clone()).unwrap();
        left.language(&custom_catalog[custom], Palette::new(false))
            .unwrap();
        left.language_with_font(
            &custom_catalog[custom],
            Palette::new(false),
            &preferred_font,
        )
        .unwrap();
        assert_eq!(font_family(left, 32), "Segoe UI");
        assert_eq!(font_family(left, 14), "Segoe UI");
        assert_eq!(left.send(SCI_STYLEGETSIZEFRACTIONAL, 14, 0), 1250);
        assert_eq!(font_family(left, 4), "Consolas");
        assert_eq!(left.send(SCI_STYLEGETSIZEFRACTIONAL, 4, 0), 1800);
        for attribute in [SCI_STYLEGETBOLD, SCI_STYLEGETITALIC, SCI_STYLEGETUNDERLINE] {
            assert_ne!(left.send(attribute, 4, 0), 0);
        }
        let mut default_override = definition.clone();
        default_override.name = "Explicit default font".into();
        default_override.styles[0].font = Some("Lucida Console".into());
        default_override.styles[0].font_size = Some(16);
        let custom_default = languages::add_custom(&mut custom_catalog, default_override).unwrap();
        left.theme_with_font(
            &custom_catalog[custom_default],
            Palette::new(false),
            &preferred_font,
        );
        assert_eq!(font_family(left, 32), "Lucida Console");
        assert_eq!(font_family(left, 14), "Lucida Console");
        assert_eq!(left.send(SCI_STYLEGETSIZEFRACTIONAL, 14, 0), 1600);
        assert_eq!(font_family(left, 4), "Consolas");
        assert_eq!(left.send(SCI_STYLEGETSIZEFRACTIONAL, 4, 0), 1800);
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
        let text_before_padding = left.text().unwrap();
        let modified_before_padding = left.send(SCI_GETMODIFY, 0, 0);
        assert_eq!(
            left.apply_compare_padding(&[
                rstpd::comparison::Padding {
                    before: 0,
                    count: 2
                },
                rstpd::comparison::Padding {
                    before: 1,
                    count: 3
                },
            ])
            .unwrap(),
            2
        );
        assert_eq!(left.send(SCI_ANNOTATIONGETLINES, 0, 0), 3);
        assert_eq!(left.text().unwrap(), text_before_padding);
        assert_eq!(left.send(SCI_GETMODIFY, 0, 0), modified_before_padding);
        left.send(SCI_MARKERADD, 0, 22);
        assert_ne!(left.send(SCI_MARKERGET, 0, 0) & (1 << 22), 0);
        left.clear_diff();
        assert_eq!(left.send(SCI_ANNOTATIONGETLINES, 0, 0), 0);
        assert_eq!(left.send(SCI_MARKERGET, 0, 0) & (1 << 22), 0);
        let line = "x".repeat(1023) + "\n";
        let large_text = line.repeat(MAX_DOCUMENT_BYTES / line.len());
        left.set_text(&large_text).unwrap();
        assert_eq!(left.length(), MAX_DOCUMENT_BYTES);
        assert_eq!(right.length(), MAX_DOCUMENT_BYTES);
        assert!(left.replace(left.length()..left.length(), "x").is_err());
        left.set_text(&text_before_padding).unwrap();
        drop(large_text);
        left.clear_styles();
        assert_eq!(left.send(SCI_GETENDSTYLED, 0, 0) as usize, left.length());
        assert_eq!(left.send(SCI_GETSTYLEAT, text.find("say").unwrap(), 0), 0);

        let large_font = EditorFont::new("Consolas", 3200).unwrap();
        left.set_text("line numbers\n").unwrap();
        left.language_with_font(rust, Palette::new(false), &large_font)
            .unwrap();
        let two_digits = left.send_raw(SCI_TEXTWIDTH, 33, c"99".as_ptr() as isize);
        assert_eq!(
            left.send(SCI_GETMARGINWIDTHN, 0, 0),
            two_digits + (two_digits / 2).max(4)
        );
        left.set_text(&"\n".repeat(98)).unwrap();
        left.update_line_number_margin();
        let compact_width = left.send(SCI_GETMARGINWIDTHN, 0, 0);
        assert_eq!(compact_width, two_digits + (two_digits / 2).max(4));
        left.set_text(&"\n".repeat(99)).unwrap();
        left.update_line_number_margin();
        let three_digits = left.send_raw(SCI_TEXTWIDTH, 33, c"999".as_ptr() as isize);
        assert_eq!(
            left.send(SCI_GETMARGINWIDTHN, 0, 0),
            three_digits + (three_digits / 3).max(4)
        );
        assert!(left.send(SCI_GETMARGINWIDTHN, 0, 0) > compact_width);

        left.set_text(&"\n".repeat(9999)).unwrap();
        assert_eq!(left.send(SCI_GETLINECOUNT, 0, 0), 10_000);
        left.update_line_number_margin();
        let five_digits = left.send_raw(SCI_TEXTWIDTH, 33, c"99999".as_ptr() as isize);
        assert!(five_digits > three_digits);
        assert_eq!(
            left.send(SCI_GETMARGINWIDTHN, 0, 0),
            five_digits + (five_digits / 5).max(4)
        );
        left.send(SCI_SETZOOM, 4, 0);
        left.update_line_number_margin();
        let zoomed_digits = left.send_raw(SCI_TEXTWIDTH, 33, c"99999".as_ptr() as isize);
        assert!(zoomed_digits > five_digits);
        assert_eq!(
            left.send(SCI_GETMARGINWIDTHN, 0, 0),
            zoomed_digits + (zoomed_digits / 5).max(4)
        );
        right.theme_with_font(
            rust,
            Palette::new(false),
            &EditorFont::new("Consolas", 400).unwrap(),
        );
        right.update_line_number_margin();
        let small_digits = right.send_raw(SCI_TEXTWIDTH, 33, c"99999".as_ptr() as isize);
        assert_eq!(
            right.send(SCI_GETMARGINWIDTHN, 0, 0),
            small_digits + (small_digits / 5).max(4)
        );
        assert!(left.send(SCI_GETMARGINWIDTHN, 0, 0) > right.send(SCI_GETMARGINWIDTHN, 0, 0));
        left.set_text("").unwrap();
        left.send(SCI_SETZOOM, 0, 0);
        left.theme_with_font(rust, Palette::new(true), &EditorFont::default());
        let compact_digits = left.send_raw(SCI_TEXTWIDTH, 33, c"99".as_ptr() as isize);
        assert_eq!(
            left.send(SCI_GETMARGINWIDTHN, 0, 0),
            compact_digits + (compact_digits / 2).max(4)
        );
        assert!(left.send(SCI_GETMARGINWIDTHN, 0, 0) < 52);
        assert_eq!(left.send(SCI_GETMARGINWIDTHN, 2, 0), 16);
        assert_eq!(left.send(SCI_GETMODIFY, 0, 0), 0);
        assert_eq!(left.send(SCI_CANUNDO, 0, 0), 0);
        DestroyWindow(parent);
        drop(document);
    }
}
