#![cfg(windows)]
use rstpd::{
    editor::{self, Editor, Palette, sci::*},
    languages,
};
use std::{
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

fn color_at(editor: Editor, text: &str, needle: &str) -> isize {
    let position = text.find(needle).unwrap();
    let style = editor.send(SCI_GETSTYLEAT, position, 0) as usize;
    editor.send(SCI_STYLEGETFORE, style, 0)
}

#[test]
fn markdown_has_visible_syntax_in_both_themes() {
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
            900,
            700,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        assert!(!parent.is_null());
        let editor = Editor::new(parent, instance, 101, false).unwrap();
        let document = editor.create_document().unwrap();
        editor.attach(&document);
        let catalog = languages::catalog(&editor::available_lexers());
        let language = &catalog[languages::detect(Path::new("highlighting.md"), &catalog)];
        assert_eq!(language.name, "Markdown");
        let text = include_str!("fixtures\\highlighting.md");
        editor.set_text(text).unwrap();
        for dark in [false, true] {
            editor.language(language, Palette::new(dark)).unwrap();
            editor.send(SCI_COLOURISE, 0, -1);
            if language.uses_container() {
                editor
                    .highlight(&language.highlight(text).unwrap())
                    .unwrap();
            }
            assert_ne!(
                editor.send(SCI_GETSTYLEAT, 0, 0),
                editor.send(SCI_GETSTYLEAT, text.find("Plain body").unwrap(), 0),
                "The Markdown lexer should distinguish headings from body text."
            );
            assert_ne!(
                color_at(editor, text, "# A"),
                color_at(editor, text, "Plain body"),
                "Recognized Markdown headings must not render in the same color as body text."
            );
            assert_ne!(
                color_at(editor, text, "visible heading"),
                color_at(editor, text, "Plain body"),
                "Heading text, not just the # marker, must be highlighted."
            );
            let bold = editor.send(SCI_GETSTYLEAT, text.find("bold words").unwrap(), 0) as usize;
            assert_ne!(editor.send(SCI_STYLEGETBOLD, bold, 0), 0);
            let italic =
                editor.send(SCI_GETSTYLEAT, text.find("italic words").unwrap(), 0) as usize;
            assert_ne!(editor.send(SCI_STYLEGETITALIC, italic, 0), 0);
            assert_ne!(
                color_at(editor, text, "inline_code"),
                color_at(editor, text, "Plain body")
            );
            assert_ne!(
                color_at(editor, text, "fn example"),
                color_at(editor, text, "Plain body")
            );
            assert_ne!(
                editor.send(
                    SCI_INDICATORVALUEAT,
                    rstpd::markdown::STRIKE_INDICATOR,
                    text.find("removed words").unwrap() as isize
                ),
                0
            );
        }
        for (path, text, needles) in [
            (
                "example.py",
                "class Widget:\n    def render(self):\n        return \"hello\" # comment\n",
                vec!["class", "Widget", "render", "\"hello\"", "# comment"],
            ),
            (
                "example.html",
                "<section title=\"hello\"><?php echo 'embedded'; ?></section>",
                vec!["section", "title", "\"hello\""],
            ),
            (
                "example.css",
                "article { color: red; margin: 10px; }",
                vec!["article", "color", "red"],
            ),
        ] {
            let language = &catalog[languages::detect(Path::new(path), &catalog)];
            editor.set_text(text).unwrap();
            for dark in [false, true] {
                let palette = Palette::new(dark);
                editor.language(language, palette).unwrap();
                editor.send(SCI_COLOURISE, 0, -1);
                let mut colors = std::collections::HashSet::new();
                for needle in &needles {
                    let color = color_at(editor, text, needle);
                    assert_ne!(
                        color, palette.background as isize,
                        "Invisible token {needle} in {path}"
                    );
                    assert_ne!(
                        color, palette.text as isize,
                        "Unstyled token {needle} in {path}"
                    );
                    colors.insert(color);
                }
                assert!(
                    colors.len() >= 3,
                    "Token categories collapse to the same color in {path}"
                );
            }
        }
        DestroyWindow(parent);
    }
}
