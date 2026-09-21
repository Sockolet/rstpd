//! Deterministic fuzz guard. These parsers index bytes directly and are driven by
//! untrusted document content, so a panic here is a crash in the editor.
use rstpd::{core, json_tools, markdown, udl};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
    fn build(&mut self, pieces: &[&str], max: u64) -> String {
        let count = self.next() % max + 1;
        let mut text = String::new();
        for _ in 0..count {
            text.push_str(self.pick(pieces));
        }
        text
    }
}

#[test]
fn json_tree_never_panics() {
    let pieces = [
        "{",
        "}",
        "[",
        "]",
        ":",
        ",",
        "\"a\"",
        "'b'",
        "1",
        "-1",
        "+1",
        ".5",
        "0x1F",
        "Infinity",
        "NaN",
        "true",
        "false",
        "null",
        "//c\n",
        "/*c*/",
        " ",
        "\n",
        "\r\n",
        "\\",
        "\"\\",
        "a",
        "\u{e9}",
        "\u{1f680}",
        "\u{feff}",
        "\t",
        "\"\\u",
        "e",
        "'",
        "\"",
        "/*",
        "//",
        "*/",
        "$",
        "\"\\\u{e9}\"",
        "{a:",
        "1e5",
        "-",
        "_x",
        "\"\\\\\"",
    ];
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut accepted = 0u32;
    for _ in 0..400_000 {
        if core::json_tree(&rng.build(&pieces, 12)).is_ok() {
            accepted += 1;
        }
    }
    assert!(
        accepted > 1_000,
        "fuzz corpus produced too few valid documents: {accepted}"
    );
}

#[test]
fn json_format_never_panics() {
    let pieces = [
        "{",
        "}",
        "[",
        "]",
        ":",
        ",",
        "\"a\"",
        "'b'",
        "1",
        "-1",
        "0x1F",
        "Infinity",
        "NaN",
        "true",
        "null",
        "//c\n",
        "/*c*/",
        " ",
        "\n",
        "\\",
        "a",
        "\u{e9}",
        "\u{1f680}",
        "\u{feff}",
        "'",
        "\"",
        "/*",
        "//",
        "*/",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..200_000 {
        let text = rng.build(&pieces, 10);
        let _ = json_tools::format(&text, false);
        let _ = json_tools::format(&text, true);
    }
}

#[test]
fn udl_highlight_never_panics() {
    let language = udl::import(include_str!("fixtures\\custom-language.xml"))
        .unwrap()
        .remove(0);
    let pieces = [
        "begin",
        "end",
        "say",
        "\"",
        "\\",
        "//",
        "/*",
        "*/",
        "\r",
        "\n",
        "\r\n",
        "42",
        "1.5",
        "\u{e9}",
        "\u{1f680}",
        " ",
        "\t",
        "'",
        "{",
        "}",
        "_",
        "0x",
        "-",
        "+",
        "e",
    ];
    let mut rng = Rng(0xDEAD_BEEF_CAFE_BABE);
    for _ in 0..200_000 {
        let _ = udl::highlight(&language, &rng.build(&pieces, 14));
    }
}

#[test]
fn markdown_highlight_never_panics() {
    let pieces = [
        "#",
        "##",
        " ",
        "\n",
        "\r",
        "\r\n",
        "*",
        "**",
        "***",
        "~~",
        "`",
        "```",
        "> ",
        "- ",
        "1. ",
        "[a](b)",
        "![a](b)",
        "<div>",
        "|a|b|",
        "\u{e9}",
        "\u{1f680}",
        "text",
        "\t",
        "---",
        "\\",
        "[^1]",
        "- [ ] ",
    ];
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..100_000 {
        let _ = markdown::highlight(&rng.build(&pieces, 14));
    }
}
