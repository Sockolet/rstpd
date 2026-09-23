use crate::core::{Difference, MAX_TOOL_BYTES, Result, Search, SearchMode};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    ops::Range,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CompareOptions {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub ignore_empty_lines: bool,
    pub ignore_regex: String,
    pub detect_moves: bool,
    pub align: bool,
}
impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            ignore_whitespace: false,
            ignore_case: false,
            ignore_empty_lines: false,
            ignore_regex: String::new(),
            detect_moves: true,
            align: true,
        }
    }
}
impl CompareOptions {
    pub fn validate(&self) -> Result<()> {
        if self.ignore_regex.len() > 4096 {
            return Err("Compare ignore expressions are limited to 4 KiB.".into());
        }
        if !self.ignore_regex.is_empty() {
            Search::new(&self.ignore_regex, SearchMode::Regex, true, false)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Padding {
    pub before: usize,
    pub count: usize,
}
#[derive(Clone, Debug)]
pub struct MovedLine {
    pub left: usize,
    pub right: usize,
}
pub struct Comparison {
    pub differences: Vec<Difference>,
    pub left_padding: Vec<Padding>,
    pub right_padding: Vec<Padding>,
    pub moves: Vec<MovedLine>,
}
impl Comparison {
    pub fn offset(
        &mut self,
        left_byte: usize,
        left_line: usize,
        right_byte: usize,
        right_line: usize,
    ) {
        for difference in &mut self.differences {
            difference.left = left_line + difference.left.start..left_line + difference.left.end;
            difference.right =
                right_line + difference.right.start..right_line + difference.right.end;
            for range in &mut difference.left_inline {
                *range = left_byte + range.start..left_byte + range.end;
            }
            for range in &mut difference.right_inline {
                *range = right_byte + range.start..right_byte + range.end;
            }
        }
        for pad in &mut self.left_padding {
            pad.before += left_line;
        }
        for pad in &mut self.right_padding {
            pad.before += right_line;
        }
        for moved in &mut self.moves {
            moved.left += left_line;
            moved.right += right_line;
        }
        // Selection starts themselves must align even when they begin on different lines.
        if left_line < right_line {
            self.left_padding.push(Padding {
                before: left_line,
                count: right_line - left_line,
            });
        } else if right_line < left_line {
            self.right_padding.push(Padding {
                before: right_line,
                count: left_line - right_line,
            });
        }
    }
}
struct Line {
    index: usize,
    span: Range<usize>,
    key: String,
}
fn lines(
    text: &str,
    options: &CompareOptions,
    ignore: Option<&Search>,
    deadline: Instant,
) -> Result<(Vec<Line>, usize)> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut pos = 0;
    let mut index = 0;
    while start < text.len() {
        while pos < text.len() && !matches!(text.as_bytes()[pos], b'\r' | b'\n') {
            pos += 1;
        }
        let span = start..pos;
        let mut key = if let Some(ignore) = ignore {
            ignore.replace_all(&text[span.clone()], "")?.0
        } else {
            text[span.clone()].to_owned()
        };
        if options.ignore_whitespace {
            key.retain(|ch| !ch.is_whitespace());
        }
        if options.ignore_case {
            key = key.to_lowercase();
        }
        if !options.ignore_empty_lines || !key.trim().is_empty() {
            if pos < text.len() {
                key.push('\n');
            }
            result.push(Line { index, span, key });
        }
        if pos < text.len() {
            let cr = text.as_bytes()[pos] == b'\r';
            pos += 1;
            if cr && text.as_bytes().get(pos) == Some(&b'\n') {
                pos += 1;
            }
            index += 1;
        }
        start = pos;
        if Instant::now() > deadline {
            return Err("Comparison exceeded its processing budget.".into());
        }
    }
    if !options.ignore_empty_lines && (text.is_empty() || text.ends_with(['\r', '\n'])) {
        result.push(Line {
            index,
            span: text.len()..text.len(),
            key: String::new(),
        });
    }
    Ok((result, index + 1))
}
fn source_range(lines: &[Line], range: Range<usize>) -> Range<usize> {
    let start = lines.get(range.start).map_or_else(
        || lines.last().map_or(0, |line| line.index + 1),
        |line| line.index,
    );
    if range.is_empty() {
        start..start
    } else {
        start..lines[range.end - 1].index + 1
    }
}
fn inline(
    left: &str,
    right: &str,
    a: &Line,
    b: &Line,
    deadline: Instant,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    if a.span.len() + b.span.len() > 32 * 1024 || Instant::now() >= deadline {
        return (vec![a.span.clone()], vec![b.span.clone()]);
    }
    let old = &left[a.span.clone()];
    let new = &right[b.span.clone()];
    let ax: Vec<_> = old
        .char_indices()
        .map(|(i, _)| i)
        .chain(Some(old.len()))
        .collect();
    let bx: Vec<_> = new
        .char_indices()
        .map(|(i, _)| i)
        .chain(Some(new.len()))
        .collect();
    let diff = similar::TextDiff::configure()
        .timeout(deadline.saturating_duration_since(Instant::now()))
        .diff_chars(old, new);
    let mut x = Vec::new();
    let mut y = Vec::new();
    for op in diff
        .ops()
        .iter()
        .filter(|op| op.tag() != similar::DiffTag::Equal)
    {
        let old = op.old_range();
        let new = op.new_range();
        if !old.is_empty() {
            x.push(a.span.start + ax[old.start]..a.span.start + ax[old.end]);
        }
        if !new.is_empty() {
            y.push(b.span.start + bx[new.start]..b.span.start + bx[new.end]);
        }
    }
    (x, y)
}
pub fn compare(left: &str, right: &str, options: &CompareOptions) -> Result<Comparison> {
    options.validate()?;
    if left.len() + right.len() > MAX_TOOL_BYTES {
        return Err("Compare is limited to 16 MiB of combined text.".into());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let ignore = if options.ignore_regex.is_empty() {
        None
    } else {
        Some(Search::new(
            &options.ignore_regex,
            SearchMode::Regex,
            true,
            false,
        )?)
    };
    let (a, left_count) = lines(left, options, ignore.as_ref(), deadline)?;
    let (b, right_count) = lines(right, options, ignore.as_ref(), deadline)?;
    let ak: Vec<_> = a.iter().map(|line| line.key.as_str()).collect();
    let bk: Vec<_> = b.iter().map(|line| line.key.as_str()).collect();
    let diff = similar::TextDiff::configure()
        .timeout(Duration::from_millis(500))
        .diff_slices(&ak, &bk);
    let mut output = Comparison {
        differences: Vec::new(),
        left_padding: Vec::new(),
        right_padding: Vec::new(),
        moves: Vec::new(),
    };
    let mut anchors = Vec::new();
    let mut deleted = Vec::new();
    let mut added = Vec::new();
    for op in diff.ops() {
        let ar = op.old_range();
        let br = op.new_range();
        for offset in 0..ar.len().min(br.len()) {
            anchors.push((a[ar.start + offset].index, b[br.start + offset].index));
        }
        if op.tag() == similar::DiffTag::Equal {
            continue;
        }
        deleted.extend(ar.clone());
        added.extend(br.clone());
        let mut difference = Difference {
            left: source_range(&a, ar.clone()),
            right: source_range(&b, br.clone()),
            left_inline: Vec::new(),
            right_inline: Vec::new(),
        };
        for offset in 0..ar.len().max(br.len()) {
            match (
                a.get(ar.start + offset).filter(|_| offset < ar.len()),
                b.get(br.start + offset).filter(|_| offset < br.len()),
            ) {
                (Some(a), Some(b)) => {
                    let (x, y) = inline(left, right, a, b, deadline);
                    difference.left_inline.extend(x);
                    difference.right_inline.extend(y);
                }
                (Some(a), None) => difference.left_inline.push(a.span.clone()),
                (None, Some(b)) => difference.right_inline.push(b.span.clone()),
                _ => {}
            }
        }
        output.differences.push(difference);
    }
    if options.detect_moves {
        let mut candidates: HashMap<&str, VecDeque<usize>> = HashMap::new();
        for index in deleted {
            if !a[index].key.trim().is_empty() {
                candidates
                    .entry(a[index].key.trim_end_matches('\n'))
                    .or_default()
                    .push_back(a[index].index);
            }
        }
        for index in added {
            if let Some(from) = candidates
                .get_mut(b[index].key.trim_end_matches('\n'))
                .and_then(VecDeque::pop_front)
                && from != b[index].index
            {
                output.moves.push(MovedLine {
                    left: from,
                    right: b[index].index,
                });
            }
        }
    }
    if options.align {
        anchors.push((left_count, right_count));
        let (mut lp, mut rp) = (0usize, 0usize);
        let (mut left_pad, mut right_pad) = (BTreeMap::new(), BTreeMap::new());
        for (al, bl) in anchors {
            let x = al + lp;
            let y = bl + rp;
            if x < y {
                *left_pad.entry(al).or_insert(0) += y - x;
                lp += y - x;
            }
            if y < x {
                *right_pad.entry(bl).or_insert(0) += x - y;
                rp += x - y;
            }
        }
        if lp + rp > 20_000 {
            return Err(
                "Comparison alignment requires more than 20,000 spacer lines; disable Align panes."
                    .into(),
            );
        }
        output.left_padding = left_pad
            .into_iter()
            .map(|(before, count)| Padding { before, count })
            .collect();
        output.right_padding = right_pad
            .into_iter()
            .map(|(before, count)| Padding { before, count })
            .collect();
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_options_but_keeps_original_line_and_character_coordinates() {
        let options = CompareOptions {
            ignore_case: true,
            ignore_whitespace: true,
            ignore_empty_lines: true,
            ignore_regex: r"\d+".into(),
            ..Default::default()
        };
        assert!(
            compare("A 12\n\nb\n", "a\t99\nb\n", &options)
                .unwrap()
                .differences
                .is_empty()
        );
        let result = compare(
            "name = '\u{e9}';\r\n",
            "name = '\u{1f680}';\n",
            &CompareOptions::default(),
        )
        .unwrap();
        assert_eq!(
            &"name = '\u{e9}';\r\n"[result.differences[0].left_inline[0].clone()],
            "\u{e9}"
        );
    }
    #[test]
    fn moved_lines_and_alignment_do_not_edit_inputs() {
        let result = compare(
            "one\ntwo\nthree\n",
            "three\none\ntwo\n",
            &CompareOptions::default(),
        )
        .unwrap();
        assert!(result.moves.iter().any(|m| m.left == 2 && m.right == 0));
        assert!(
            result
                .left_padding
                .iter()
                .any(|p| p.before == 0 && p.count == 1)
        );
        let mut result = compare("a\nb\n", "a\nc\n", &CompareOptions::default()).unwrap();
        result.offset(10, 2, 20, 4);
        assert_eq!(result.differences[0].left, 3..4);
        assert!(
            CompareOptions {
                ignore_regex: "(".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn alignment_keeps_trailing_empty_lines_and_eof_coordinates() {
        let result = compare("a\n", "a\nb\n", &CompareOptions::default()).unwrap();
        assert_eq!(result.differences[0].left, 1..1);
        assert_eq!(result.differences[0].right, 1..2);
        assert!(
            result
                .left_padding
                .iter()
                .any(|pad| pad.before == 1 && pad.count == 1)
        );
        let result = compare("", "first\nsecond\n", &CompareOptions::default()).unwrap();
        assert_eq!(result.left_padding[0].before, 0);
        assert_eq!(result.left_padding[0].count, 2);
        let result = compare("a", "a\nb", &CompareOptions::default()).unwrap();
        assert!(
            result
                .differences
                .iter()
                .all(|change| change.left.end <= 1 && change.right.end <= 2)
        );
    }

    #[test]
    fn ignore_options_are_independent_and_moves_can_be_disabled() {
        let default = CompareOptions::default();
        assert!(
            !compare("alpha 1\n", "ALPHA 9\n", &default)
                .unwrap()
                .differences
                .is_empty()
        );
        assert!(
            compare(
                "alpha 1\n",
                "ALPHA 9\n",
                &CompareOptions {
                    ignore_case: true,
                    ignore_regex: r"\d".into(),
                    ..default.clone()
                }
            )
            .unwrap()
            .differences
            .is_empty()
        );
        assert!(
            !compare("a b\n", "ab\n", &default)
                .unwrap()
                .differences
                .is_empty()
        );
        assert!(
            compare(
                "a b\n",
                "ab\n",
                &CompareOptions {
                    ignore_whitespace: true,
                    ..default.clone()
                }
            )
            .unwrap()
            .differences
            .is_empty()
        );
        assert!(
            compare(
                "a\nb\nc\n",
                "c\na\nb\n",
                &CompareOptions {
                    detect_moves: false,
                    align: false,
                    ..default
                }
            )
            .unwrap()
            .moves
            .is_empty()
        );
    }
}
