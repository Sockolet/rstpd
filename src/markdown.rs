use crate::{
    core::{Highlight, MAX_DOCUMENT_BYTES, Result},
    syntax::{Role, Style},
};
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::time::{Duration, Instant};

pub const HEADING: u8 = 6;
pub const CODE_BLOCK: u8 = 21;
pub const STRIKE_INDICATOR: usize = 22;

pub fn styles() -> [Style; 256] {
    let mut styles = [Style::default(); 256];
    let make = |role, bold, italic, underline, shaded| Style {
        role,
        bold,
        italic,
        underline,
        shaded,
    };
    for id in [2, 3] {
        styles[id] = make(Role::Strong, true, false, false, false);
    }
    for id in [4, 5] {
        styles[id] = make(Role::Emphasis, false, true, false, false);
    }
    styles[6..=11].fill(make(Role::Heading, true, false, false, false));
    for id in [13, 14, 17, 26] {
        styles[id] = make(Role::Marker, true, false, false, false);
    }
    styles[15] = make(Role::Quote, false, true, false, false);
    styles[16] = make(Role::Muted, false, false, false, false);
    styles[18] = make(Role::Link, false, false, true, false);
    for id in [19, 20, 21] {
        styles[id] = make(Role::Code, false, false, false, true);
    }
    styles[22] = make(Role::Strong, true, true, false, false);
    styles[23] = make(Role::Heading, true, true, false, false);
    styles[24] = make(Role::Link, true, false, true, false);
    styles[25] = make(Role::Link, false, true, true, false);
    styles[27] = make(Role::Tag, false, false, false, false);
    styles
}

pub fn highlight(text: &str) -> Result<Highlight> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err("Markdown exceeds the document size limit.".into());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut styles = vec![0; text.len()];
    let mut strikes = Vec::new();
    let mut stack: Vec<(u8, bool, bool)> = Vec::new();
    let mut line_offsets = vec![0usize];
    let mut offset = 0;
    while offset < text.len() {
        if text.as_bytes()[offset] == b'\r' {
            offset += 1;
            if text.as_bytes().get(offset) == Some(&b'\n') {
                offset += 1;
            }
            line_offsets.push(offset);
        } else if text.as_bytes()[offset] == b'\n' {
            offset += 1;
            line_offsets.push(offset);
        } else {
            offset += 1;
        }
    }
    let mut fold_changes = vec![0i32; line_offsets.len()];
    let mut heading_stack: Vec<(usize, usize)> = Vec::new();
    for (index, (event, range)) in Parser::new_ext(text, options)
        .into_offset_iter()
        .enumerate()
    {
        if index.is_multiple_of(1024) && Instant::now() > deadline {
            return Err("Markdown highlighting exceeded its time budget.".into());
        }
        let inherited = stack.last().copied().unwrap_or((0, false, false));
        match event {
            Event::Start(tag) => {
                if stack.len() >= 256 {
                    return Err("Markdown nesting exceeds 256 levels.".into());
                }
                let mut state = inherited;
                let mut paint = true;
                match tag {
                    Tag::Heading { level, .. } => {
                        let level = level as usize;
                        state = (HEADING + level as u8 - 1, true, false);
                        let line = line_offsets.partition_point(|start| *start <= range.start) - 1;
                        while heading_stack
                            .last()
                            .is_some_and(|(depth, _)| *depth >= level)
                        {
                            heading_stack.pop();
                            fold_changes[line] -= 1;
                        }
                        heading_stack.push((level, line));
                        if line + 1 < fold_changes.len() {
                            fold_changes[line + 1] += 1;
                        }
                    }
                    Tag::Strong => {
                        state.1 = true;
                        state.0 = if matches!(inherited.0, 6..=11 | 23) {
                            inherited.0
                        } else if inherited.0 == 18 {
                            24
                        } else if inherited.2 {
                            22
                        } else {
                            2
                        };
                    }
                    Tag::Emphasis => {
                        state.2 = true;
                        state.0 = if matches!(inherited.0, 6..=11 | 23) {
                            23
                        } else if inherited.0 == 18 {
                            25
                        } else if inherited.1 {
                            22
                        } else {
                            4
                        };
                    }
                    Tag::Strikethrough => {
                        state.0 = 16;
                        strikes.push(range.clone());
                    }
                    Tag::Link { .. } | Tag::Image { .. } => {
                        state.0 = if inherited.1 {
                            24
                        } else if inherited.2 {
                            25
                        } else {
                            18
                        }
                    }
                    Tag::CodeBlock(_) => {
                        state = (CODE_BLOCK, false, false);
                        let begin = line_offsets.partition_point(|start| *start <= range.start) - 1;
                        let end = line_offsets
                            .partition_point(|start| *start < range.end)
                            .saturating_sub(1);
                        if end > begin {
                            fold_changes[begin + 1] += 1;
                            if end + 1 < fold_changes.len() {
                                fold_changes[end + 1] -= 1;
                            }
                        }
                    }
                    Tag::BlockQuote(_) => state = (15, false, true),
                    Tag::Item => {
                        let marker = text[range.clone()].find(char::is_whitespace).unwrap_or(0);
                        if marker > 0 {
                            let end = (range.start + marker).min(range.end);
                            styles[range.start..end].fill(13);
                        }
                        paint = false;
                    }
                    Tag::TableHead => state = (2, true, false),
                    Tag::HtmlBlock => state = (27, false, false),
                    _ => paint = false,
                }
                if paint {
                    styles[range.clone()].fill(state.0);
                }
                stack.push(state);
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Code(_) => styles[range].fill(19),
            Event::Html(_) | Event::InlineHtml(_) => styles[range].fill(27),
            Event::Rule => styles[range].fill(17),
            Event::TaskListMarker(_) => styles[range].fill(26),
            Event::FootnoteReference(_) => styles[range].fill(18),
            _ => {}
        }
    }
    let mut level = 0i32;
    let mut folds = Vec::with_capacity(fold_changes.len());
    for (line, change) in fold_changes.iter().enumerate() {
        level = (level + change).max(0);
        let next = fold_changes.get(line + 1).copied().unwrap_or(0);
        folds.push((0x400 + level.min(0xbff) as usize) | if next > 0 { 0x2000 } else { 0 });
    }
    if Instant::now() > deadline {
        return Err("Markdown highlighting exceeded its time budget.".into());
    }
    Ok(Highlight {
        styles,
        folds,
        strikes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_markdown_constructs_have_semantic_styles() {
        let text = include_str!("..\\tests\\fixtures\\highlighting.md");
        let output = highlight(text).unwrap();
        for (needle, style) in [
            ("visible heading", 6),
            ("bold words", 2),
            ("italic words", 4),
            ("the documentation", 18),
            ("inline_code", 19),
            ("fn example", 21),
            ("tilde-fenced", 21),
            ("quoted paragraph", 15),
        ] {
            assert_eq!(output.styles[text.find(needle).unwrap()], style, "{needle}");
        }
        assert_eq!(output.styles[text.find("Plain body").unwrap()], 0);
        assert!(!output.strikes.is_empty());
        assert_ne!(output.folds[0] & 0x2000, 0);
    }
    #[test]
    fn nested_emphasis_and_unfinished_fences_are_safe() {
        let text = "## *Heading*\n\n***combined***\n\n```rs\nlet x = 1;\n";
        let output = highlight(text).unwrap();
        assert_eq!(output.styles[text.find("Heading").unwrap()], 23);
        assert_eq!(output.styles[text.find("combined").unwrap()], 22);
        assert_eq!(output.styles[text.find("let x").unwrap()], 21);
        assert_eq!(output.styles.len(), text.len());
    }
    #[test]
    fn unicode_cr_line_endings_and_plain_edits_preserve_byte_spans() {
        let text = "# Caf\u{e9}\r\r**\u{1f680}** and `\u{4e2d}`\r";
        let output = highlight(text).unwrap();
        assert_eq!(output.folds.len(), 4);
        assert_eq!(output.styles[text.find('\u{1f680}').unwrap()], 2);
        assert_eq!(output.styles[text.find('\u{4e2d}').unwrap()], 19);
        let plain = highlight("No formatting now").unwrap();
        assert!(plain.styles.iter().all(|style| *style == 0));
        assert!(plain.strikes.is_empty());
    }
}
