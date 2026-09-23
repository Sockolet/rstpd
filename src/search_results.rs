use crate::core::{Highlight, Result, Search};
use std::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub const MAX_HITS: usize = 10_000;
pub const MAX_BATCH_BYTES: usize = 256 * 1024 * 1024;
pub const MATCH_INDICATOR: usize = 24;

pub struct Input {
    pub id: u64,
    pub revision: u64,
    pub title: String,
    pub text: String,
    pub tab_width: usize,
}
#[derive(Debug)]
pub struct Hit {
    pub range: Range<usize>,
    pub line: usize,
    pub column: usize,
    pub preview: String,
    pub emphasis: Range<usize>,
}
pub struct FileResults {
    pub id: u64,
    pub revision: u64,
    pub title: String,
    pub hits: Vec<Hit>,
    pub source: Option<crate::folder_search::DiskSource>,
}
pub struct Results {
    pub query: String,
    pub files: Vec<FileResults>,
    pub searched: usize,
    pub truncated: bool,
    pub skipped: usize,
    pub warnings: Vec<String>,
}
pub struct Link {
    pub file: usize,
    pub hit: usize,
    pub row: usize,
}
pub struct Rendered {
    pub text: String,
    pub highlight: Highlight,
    pub emphasis: Vec<Range<usize>>,
    pub links: Vec<Link>,
}

pub fn single_line(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for ch in text.chars().take(limit) {
        match ch {
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            ch if ch.is_control()
                || matches!(ch, '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                use std::fmt::Write;
                write!(&mut out, "\\u{{{:x}}}", ch as u32).expect("writing to a String");
            }
            ch => out.push(ch),
        }
    }
    if text.chars().nth(limit).is_some() {
        out.push_str("...");
    }
    out
}

struct LineCursor {
    position: usize,
    start: usize,
    end: usize,
    line: usize,
    column: usize,
    tab_width: usize,
}
impl LineCursor {
    fn new(text: &str, tab_width: usize) -> Self {
        Self {
            position: 0,
            start: 0,
            end: text.find(['\r', '\n']).unwrap_or(text.len()),
            line: 1,
            column: 1,
            tab_width: tab_width.max(1),
        }
    }
    fn advance(&mut self, text: &str, target: usize) {
        while self.position < target {
            let ch = text[self.position..]
                .chars()
                .next()
                .expect("match boundary is in the document");
            self.position += ch.len_utf8();
            if ch == '\n' || (ch == '\r' && text.as_bytes().get(self.position) != Some(&b'\n')) {
                self.line += 1;
                self.column = 1;
                self.start = self.position;
            } else if ch == '\t' {
                self.column += self.tab_width - (self.column - 1) % self.tab_width;
            } else {
                self.column += 1;
            }
        }
        if self.end < self.start {
            self.end = self.start
                + text[self.start..]
                    .find(['\r', '\n'])
                    .unwrap_or(text.len() - self.start);
        }
    }
    fn hit(&self, text: &str, range: Range<usize>) -> Hit {
        let before_end = range.start.min(self.end);
        let start = text[self.start..before_end]
            .char_indices()
            .rev()
            .nth(79)
            .map_or(self.start, |(offset, _)| self.start + offset);
        let end = text[before_end..self.end]
            .char_indices()
            .nth(160)
            .map_or(self.end, |(offset, _)| before_end + offset);
        let mut preview = if start > self.start {
            "...".into()
        } else {
            String::new()
        };
        preview.push_str(&single_line(&text[start..before_end], 80));
        let emphasis_start = preview.len();
        let visible_end = range.end.min(end).max(before_end);
        preview.push_str(&single_line(&text[before_end..visible_end], 160));
        let emphasis_end = preview.len();
        preview.push_str(&single_line(&text[visible_end..end], 160));
        if end < self.end {
            preview.push_str("...");
        }
        if range.is_empty() {
            preview.push_str(" [zero-width match]");
        } else if range.end > self.end {
            preview.push_str(" [continues on next line]");
        }
        Hit {
            range,
            line: self.line,
            column: self.column,
            preview,
            emphasis: emphasis_start..emphasis_end,
        }
    }
}

pub fn find_all(
    search: &Search,
    query: String,
    inputs: Vec<Input>,
    cancelled: &AtomicBool,
) -> Result<Results> {
    let Some(bytes) = inputs
        .iter()
        .try_fold(0usize, |total, input| total.checked_add(input.text.len()))
        .filter(|total| *total <= MAX_BATCH_BYTES)
    else {
        return Err("Find All is limited to 256 MiB across the selected open documents.".into());
    };
    let mut result = Results {
        query,
        files: Vec::new(),
        searched: 0,
        truncated: false,
        skipped: 0,
        warnings: Vec::new(),
    };
    let mut total = 0usize;
    let deadline =
        Instant::now() + Duration::from_secs(10 * bytes.div_ceil(64 * 1024 * 1024).max(1) as u64);
    for input in inputs {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Search cancelled.".into());
        }
        if Instant::now() > deadline {
            return Err("Find All exceeded its processing budget. Narrow the search.".into());
        }
        Search::validate_size(input.text.len())
            .map_err(|error| format!("{}: {error}", input.title))?;
        let mut file = FileResults {
            id: input.id,
            revision: input.revision,
            title: input.title,
            hits: Vec::new(),
            source: None,
        };
        let mut cursor = LineCursor::new(&input.text, input.tab_width);
        search.visit_matches(&input.text, |range| {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Search cancelled.".into());
            }
            if Instant::now() > deadline {
                return Err("Find All exceeded its processing budget. Narrow the search.".into());
            }
            if total == MAX_HITS {
                result.truncated = true;
                return Ok(false);
            }
            cursor.advance(&input.text, range.start);
            file.hits.push(cursor.hit(&input.text, range));
            total += 1;
            Ok(true)
        })?;
        result.searched += 1;
        if !file.hits.is_empty() {
            result.files.push(file);
        }
        if result.truncated {
            break;
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("Search cancelled.".into());
    }
    Ok(result)
}

impl Results {
    pub fn count(&self) -> usize {
        self.files.iter().map(|file| file.hits.len()).sum()
    }
    pub fn summary(&self) -> String {
        let count = self.count();
        let summary = if self.truncated {
            format!(
                "Showing the first {} matches; more results exist. Narrow the search.",
                count
            )
        } else {
            format!(
                "{} {} in {} {} ({} searched)",
                count,
                if count == 1 { "match" } else { "matches" },
                self.files.len(),
                if self.files.len() == 1 {
                    "document"
                } else {
                    "documents"
                },
                self.searched
            )
        };
        if self.skipped == 0 {
            summary
        } else {
            format!("{summary}; {} skipped (see warnings)", self.skipped)
        }
    }
    pub fn render(&self) -> Rendered {
        let mut text = String::new();
        let mut styles = Vec::new();
        let mut folds = Vec::new();
        let mut emphasis = Vec::new();
        let mut links = Vec::new();
        fn line(
            text: &mut String,
            styles: &mut Vec<u8>,
            folds: &mut Vec<usize>,
            content: &str,
            style: u8,
            fold: usize,
        ) {
            text.push_str(content);
            text.push('\n');
            styles.resize(text.len(), style);
            folds.push(fold);
        }
        line(
            &mut text,
            &mut styles,
            &mut folds,
            &format!(
                "Find \"{}\" - {}",
                single_line(&self.query, 120),
                self.summary()
            ),
            1,
            0x400,
        );
        for warning in &self.warnings {
            line(
                &mut text,
                &mut styles,
                &mut folds,
                &format!("Skipped: {}", single_line(warning, 240)),
                0,
                0x400,
            );
        }
        for (index, file) in self.files.iter().enumerate() {
            line(
                &mut text,
                &mut styles,
                &mut folds,
                &format!(
                    "{} ({} {})",
                    single_line(&file.title, 512),
                    file.hits.len(),
                    if file.hits.len() == 1 {
                        "match"
                    } else {
                        "matches"
                    }
                ),
                2,
                0x400 | 0x2000,
            );
            for (number, hit) in file.hits.iter().enumerate() {
                let prefix = format!("  Line {}, Col {}: ", hit.line, hit.column);
                let base = text.len() + prefix.len();
                links.push(Link {
                    file: index,
                    hit: number,
                    row: folds.len(),
                });
                line(
                    &mut text,
                    &mut styles,
                    &mut folds,
                    &(prefix + &hit.preview),
                    0,
                    0x401,
                );
                if !hit.emphasis.is_empty() {
                    emphasis.push(base + hit.emphasis.start..base + hit.emphasis.end);
                }
            }
        }
        folds.push(0x400);
        Rendered {
            text,
            highlight: Highlight {
                styles,
                folds,
                strikes: Vec::new(),
            },
            emphasis,
            links,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SearchMode;
    fn input(id: u64, text: &str) -> Input {
        Input {
            id,
            revision: 3,
            title: format!("Untitled {id}"),
            text: text.into(),
            tab_width: 4,
        }
    }
    #[test]
    fn current_and_multi_document_results_have_exact_unicode_ranges() {
        let search = Search::new("cat", SearchMode::Literal, false, true).unwrap();
        let inputs = vec![
            input(1, "\u{e9} cat\r\ncat\rcat\n"),
            input(2, "no match"),
            input(3, "CAT cat"),
        ];
        let results = find_all(&search, "cat".into(), inputs, &AtomicBool::new(false)).unwrap();
        assert_eq!(results.count(), 5);
        assert_eq!(results.searched, 3);
        assert_eq!(results.files.len(), 2);
        assert_eq!(results.files[0].hits[0].range, 3..6);
        assert_eq!(
            (
                results.files[0].hits[0].line,
                results.files[0].hits[0].column
            ),
            (1, 3)
        );
        assert_eq!(results.files[0].hits[1].line, 2);
        assert_eq!(results.files[0].hits[2].line, 3);
        let rendered = results.render();
        assert_eq!(rendered.links.len(), 5);
        assert_eq!(rendered.text.len(), rendered.highlight.styles.len());
        assert_eq!(&rendered.text[rendered.emphasis[0].clone()], "cat");
    }
    #[test]
    fn regex_extended_empty_and_multiline_matches_remain_navigable() {
        let search = Search::new(r"(?<=x)a\r\nb", SearchMode::Regex, true, false).unwrap();
        let results = find_all(
            &search,
            "multiline".into(),
            vec![input(1, "xa\r\nb")],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(results.files[0].hits[0].range, 1..5);
        assert!(results.files[0].hits[0].preview.contains("continues"));
        let search = Search::new("^", SearchMode::Regex, true, false).unwrap();
        let results = find_all(
            &search,
            "^".into(),
            vec![input(1, "a\nb")],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(results.count(), 2);
        assert_eq!(results.files[0].hits[1].line, 2);
        let search = Search::new(r"\t", SearchMode::Extended, true, false).unwrap();
        let results = find_all(
            &search,
            r"\t".into(),
            vec![input(1, "a\tb")],
            &AtomicBool::new(false),
        )
        .unwrap();
        let rendered = results.render();
        assert_eq!(&rendered.text[rendered.emphasis[0].clone()], "\\t");
    }
    #[test]
    fn result_limits_and_cancellation_are_explicit() {
        let search = Search::new("a", SearchMode::Literal, true, false).unwrap();
        let results = find_all(
            &search,
            "a".into(),
            vec![input(1, &"a".repeat(MAX_HITS + 1))],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(results.count(), MAX_HITS);
        assert!(results.truncated);
        assert!(results.files[0].hits.last().unwrap().preview.len() < 256);
        assert!(
            find_all(
                &search,
                "a".into(),
                vec![input(1, "a")],
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert_eq!(
            find_all(
                &search,
                "a".into(),
                vec![input(1, "")],
                &AtomicBool::new(false)
            )
            .unwrap()
            .count(),
            0
        );
    }
    #[test]
    fn columns_respect_tabs_and_end_of_line_snippets_are_safe() {
        let search = Search::new("cat", SearchMode::Literal, true, false).unwrap();
        let results = find_all(
            &search,
            "cat".into(),
            vec![input(1, "\tcat \u{e9}\tcat")],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(results.files[0].hits[0].column, 5);
        assert_eq!(results.files[0].hits[1].column, 13);
        let search = Search::new(r"\r\n", SearchMode::Extended, true, false).unwrap();
        let result = find_all(
            &search,
            "EOL".into(),
            vec![input(1, "\r\n\r\n")],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.count(), 2);
        assert_eq!(result.files[0].hits[1].line, 2);
        let search = Search::new(r"$", SearchMode::Regex, true, false).unwrap();
        let result = find_all(
            &search,
            "end".into(),
            vec![input(1, "cat")],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.files[0].hits[0].range, 3..3);
    }
}
