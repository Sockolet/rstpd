//! Data-only Markdown rendering for the native read-only preview.
use crate::core::{Highlight, Result};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::time::{Duration, Instant};

pub const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;

pub fn render(source: &str) -> Result<(String, Highlight)> {
    let (text, highlight, _) = render_mapped(source)?;
    Ok((text, highlight))
}

#[derive(Default)]
pub struct SourceMap(Vec<(std::ops::Range<usize>, std::ops::Range<usize>, bool)>);

impl SourceMap {
    fn translate(&self, position: usize, reverse: bool) -> usize {
        let mut nearest = (0, 0);
        let mut containing = None;
        for (source, rendered, exact) in &self.0 {
            let (from, to) = if reverse {
                (rendered, source)
            } else {
                (source, rendered)
            };
            if from.start <= position && from.start >= nearest.0 {
                nearest = (from.start, to.start);
            }
            if from.contains(&position) {
                let value = to.start + if *exact { position - from.start } else { 0 };
                if containing.is_none_or(|(size, _)| from.len() < size) {
                    containing = Some((from.len(), value));
                }
            }
        }
        containing.map_or(nearest.1, |(_, value)| value)
    }
    pub fn source(&self, rendered: usize) -> usize {
        self.translate(rendered, true)
    }
    pub fn rendered(&self, source: usize) -> usize {
        self.translate(source, false)
    }
}

/// Right affinity keeps an existing anchor attached to text inserted before it.
pub fn rebase(old: &str, new: &str, position: usize) -> usize {
    if old == new {
        return position.min(new.len());
    }
    let diff = similar::TextDiff::configure()
        .timeout(Duration::from_millis(20))
        .diff_chars(old, new);
    let (mut before, mut after) = (0, 0);
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            similar::ChangeTag::Insert => after += len,
            similar::ChangeTag::Delete => {
                before += len;
            }
            similar::ChangeTag::Equal => {
                if position < before + len {
                    return after + position.saturating_sub(before);
                }
                before += len;
                after += len;
            }
        }
    }
    after
}

pub fn render_mapped(source: &str) -> Result<(String, Highlight, SourceMap)> {
    if source.len() > MAX_PREVIEW_BYTES {
        return Err("Markdown preview is limited to 2 MiB. The source remains editable.".into());
    }
    let deadline = Instant::now() + Duration::from_millis(250);
    let mut text = String::new();
    let mut styles = Vec::new();
    let mut stack = vec![0u8];
    let mut lists = Vec::new();
    let mut strikes = Vec::new();
    let mut strike_start = Vec::new();
    let mut links = Vec::new();
    let mut mapping = SourceMap::default();
    fn append(text: &mut String, styles: &mut Vec<u8>, value: &str, style: u8) {
        text.push_str(value);
        styles.resize(text.len(), style);
    }
    for (i, event) in Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_FOOTNOTES,
    )
    .into_offset_iter()
    .enumerate()
    {
        let (event, range) = event;
        let start = text.len();
        let literal = matches!(
            event,
            Event::Text(_)
                | Event::Code(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_)
        );
        let ending = matches!(event, Event::End(_));
        if i % 1024 == 0 && Instant::now() > deadline {
            return Err("Markdown preview exceeded its time budget.".into());
        }
        let style = *stack.last().unwrap_or(&0);
        match event {
            Event::Start(tag) => {
                if stack.len() >= 256 {
                    return Err("Markdown preview nesting exceeds 256 levels.".into());
                }
                let next = match &tag {
                    Tag::Heading { level, .. } => 6 + *level as u8 - 1,
                    Tag::Strong => {
                        if style == 4 {
                            22
                        } else {
                            2
                        }
                    }
                    Tag::Emphasis => {
                        if style == 2 {
                            22
                        } else {
                            4
                        }
                    }
                    Tag::CodeBlock(_) => 21,
                    Tag::BlockQuote(_) => 15,
                    Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                        links.push(dest_url.to_string());
                        18
                    }
                    Tag::List(start) => {
                        lists.push(*start);
                        style
                    }
                    Tag::Item => {
                        if !text.ends_with('\n') && !text.is_empty() {
                            append(&mut text, &mut styles, "\n", 0);
                        }
                        append(
                            &mut text,
                            &mut styles,
                            &"  ".repeat(lists.len().saturating_sub(1)),
                            0,
                        );
                        let marker = if let Some(Some(n)) = lists.last_mut() {
                            let marker = format!("{n}. ");
                            *n += 1;
                            marker
                        } else {
                            "• ".into()
                        };
                        append(&mut text, &mut styles, &marker, 13);
                        style
                    }
                    Tag::Strikethrough => {
                        strike_start.push(text.len());
                        style
                    }
                    Tag::FootnoteDefinition(label) => {
                        append(&mut text, &mut styles, &format!("[{label}] "), 18);
                        style
                    }
                    _ => style,
                };
                stack.push(next);
            }
            Event::End(tag) => {
                stack.pop();
                match tag {
                    TagEnd::Link | TagEnd::Image => {
                        if let Some(url) = links.pop() {
                            append(&mut text, &mut styles, &format!(" ({url})"), 18);
                        }
                    }
                    TagEnd::Strikethrough => {
                        if let Some(start) = strike_start.pop() {
                            strikes.push(start..text.len());
                        }
                    }
                    TagEnd::List(_) => {
                        lists.pop();
                    }
                    TagEnd::TableCell => append(&mut text, &mut styles, "   |   ", 16),
                    TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::CodeBlock
                    | TagEnd::BlockQuote(_)
                    | TagEnd::Table
                    | TagEnd::FootnoteDefinition => {
                        append(&mut text, &mut styles, "\n\n", 0);
                    }
                    TagEnd::Item | TagEnd::TableRow | TagEnd::TableHead => {
                        append(&mut text, &mut styles, "\n", 0);
                    }
                    _ => {}
                }
            }
            Event::Text(value) => append(&mut text, &mut styles, &value, style),
            Event::Code(value) => append(&mut text, &mut styles, &value, 19),
            Event::Html(value) | Event::InlineHtml(value) => {
                append(&mut text, &mut styles, &value, 16);
            }
            Event::SoftBreak => append(&mut text, &mut styles, " ", style),
            Event::HardBreak => append(&mut text, &mut styles, "\n", style),
            Event::Rule => append(&mut text, &mut styles, "\n────────────────────────\n\n", 16),
            Event::TaskListMarker(done) => {
                append(&mut text, &mut styles, if done { "☑ " } else { "☐ " }, 13)
            }
            Event::FootnoteReference(label) => {
                append(&mut text, &mut styles, &format!("[{label}]"), 18)
            }
            Event::InlineMath(value) | Event::DisplayMath(value) => {
                append(&mut text, &mut styles, &value, 19)
            }
        }
        if text.len() > start {
            let output = &text[start..];
            let literal_range = literal
                .then(|| source[range.clone()].find(output))
                .flatten()
                .map(|offset| range.start + offset..range.start + offset + output.len());
            let exact = literal_range.is_some();
            let range = literal_range.unwrap_or_else(|| {
                if literal {
                    range.clone()
                } else if ending {
                    range.end..range.end
                } else {
                    range.start..range.start
                }
            });
            mapping.0.push((range, start..text.len(), exact));
        }
        if text.len() > MAX_PREVIEW_BYTES * 4 {
            return Err("Rendered Markdown exceeds the preview size limit.".into());
        }
    }
    Ok((
        text,
        Highlight {
            styles,
            folds: Vec::new(),
            strikes,
        },
        mapping,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_anchor_follows_edits_before_it() {
        let old = "# Heading\n\n- first\n- Héllo **visible**\n\nlast";
        let (text, _, map) = render_mapped(old).unwrap();
        let anchor = map.source(text.find("visible").unwrap());
        for new in [
            format!("## Added\n\n{old}"),
            old.replace("- first\n", ""),
            old.replace("Héllo", "Much longer 🦀 greeting"),
            old.replace("# Heading", "# H")
                .replace("last", "a longer last paragraph"),
        ] {
            let (text, _, map) = render_mapped(&new).unwrap();
            assert_eq!(
                map.rendered(rebase(old, &new, anchor)),
                text.find("visible").unwrap()
            );
        }
    }
    #[test]
    fn deleted_anchor_clamps_to_surviving_text() {
        assert_eq!(rebase("before visible after", "before after", 10), 7);
        assert_eq!(rebase("abc", "Xabc", 0), 1);
        assert_eq!(rebase("abc", "", 2), 0);
    }
    #[test]
    fn mappings_preserve_utf8_and_inline_offsets() {
        let source = "# Title\n\n- Héllo **world** &amp; `code`\n\nlast";
        let (text, _, map) = render_mapped(source).unwrap();
        for value in ["Héllo", "world", "code", "last"] {
            let rendered = text.find(value).unwrap();
            let original = source.find(value).unwrap();
            assert_eq!(map.source(rendered), original);
            assert_eq!(map.rendered(original), rendered);
        }
        let offset = source.find("world").unwrap() + 3;
        assert_eq!(map.source(map.rendered(offset)), offset);
    }
    #[test]
    fn renders_structures_and_utf8_styles() {
        let (text, highlight) = render("# Héllo\n\n**bold** *italic* ~~gone~~ `code`\n\n- [x] task\n- item\n\n[link](https://example.org)\n\n```rust\nlet x = 1;\n```\n").unwrap();
        assert!(text.starts_with("Héllo\n\n"));
        assert!(text.contains("bold italic gone code"));
        assert!(text.contains("• ☑ task"));
        assert!(text.contains("link (https://example.org)"));
        assert!(text.contains("let x = 1;"));
        assert_eq!(text.len(), highlight.styles.len());
        assert_eq!(&text[highlight.strikes[0].clone()], "gone");
        assert_eq!(highlight.styles[0], 6);
    }
    #[test]
    fn treats_html_and_images_as_inert_text() {
        let (text, _) =
            render("<script>alert(1)</script>\n\n![alt](file:///secret)\n\n1. first\n2. second")
                .unwrap();
        assert!(text.contains("<script>"));
        assert!(text.contains("alt (file:///secret)"));
        assert!(text.contains("1. first"));
        assert!(text.contains("2. second"));
        assert!(render(&"a".repeat(MAX_PREVIEW_BYTES + 1)).is_err());
    }
}
