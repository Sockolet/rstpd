use fancy_regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashSet,
    ops::Range,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

pub type Result<T> = std::result::Result<T, String>;
pub const MAX_DOCUMENT_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_TOOL_BYTES: usize = 16 * 1024 * 1024;

pub struct Highlight {
    pub styles: Vec<u8>,
    pub folds: Vec<usize>,
    pub strikes: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
    Legacy(String),
}

impl Encoding {
    pub fn label(&self) -> &str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Utf32Le => "UTF-32 LE",
            Self::Utf32Be => "UTF-32 BE",
            Self::Legacy(label) => label,
        }
    }

    pub fn encode(&self, text: &str) -> Result<Vec<u8>> {
        match self {
            Self::Utf8 => Ok(text.as_bytes().to_vec()),
            Self::Utf8Bom => Ok([b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat()),
            Self::Utf16Le | Self::Utf16Be => {
                let little = *self == Self::Utf16Le;
                let mut out = if little {
                    vec![255, 254]
                } else {
                    vec![254, 255]
                };
                for ch in text.encode_utf16() {
                    out.extend(if little {
                        ch.to_le_bytes()
                    } else {
                        ch.to_be_bytes()
                    });
                }
                Ok(out)
            }
            Self::Utf32Le | Self::Utf32Be => {
                let little = *self == Self::Utf32Le;
                let mut bytes = if little {
                    vec![255, 254, 0, 0]
                } else {
                    vec![0, 0, 254, 255]
                };
                for ch in text.chars() {
                    bytes.extend(if little {
                        (ch as u32).to_le_bytes()
                    } else {
                        (ch as u32).to_be_bytes()
                    });
                }
                Ok(bytes)
            }
            Self::Legacy(label) => {
                if let Some(codepage) = code_page(label) {
                    return encode_code_page(text, codepage);
                }
                let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
                    .ok_or_else(|| format!("Unknown encoding: {label}"))?;
                let (bytes, _, errors) = encoding.encode(text);
                if errors {
                    Err(format!(
                        "Some characters cannot be represented in {label}. Use UTF-8 to avoid data loss."
                    ))
                } else {
                    Ok(bytes.into_owned())
                }
            }
        }
    }
}

pub fn decode(bytes: &[u8], override_encoding: Option<&Encoding>) -> Result<(String, Encoding)> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("Files over 128 MiB are not supported.".into());
    }
    let encoding = override_encoding.cloned().unwrap_or_else(|| {
        if bytes.starts_with(b"\xff\xfe\0\0") {
            Encoding::Utf32Le
        } else if bytes.starts_with(b"\0\0\xfe\xff") {
            Encoding::Utf32Be
        } else if bytes.starts_with(b"\xef\xbb\xbf") {
            Encoding::Utf8Bom
        } else if bytes.starts_with(b"\xff\xfe") {
            Encoding::Utf16Le
        } else if bytes.starts_with(b"\xfe\xff") {
            Encoding::Utf16Be
        } else if std::str::from_utf8(bytes).is_ok() {
            Encoding::Utf8
        } else {
            Encoding::Legacy("windows-1252".into())
        }
    });
    let text = match &encoding {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            let content = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
            std::str::from_utf8(content)
                .map_err(|e| format!("Invalid UTF-8: {e}"))?
                .to_owned()
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let little = encoding == Encoding::Utf16Le;
            let bom: &[u8] = if little { b"\xff\xfe" } else { b"\xfe\xff" };
            let content = bytes.strip_prefix(bom).unwrap_or(bytes);
            if !content.len().is_multiple_of(2) {
                return Err("Invalid UTF-16: incomplete code unit.".into());
            }
            let units: Vec<u16> = content
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| {
                    if little {
                        u16::from_le_bytes([v[0], v[1]])
                    } else {
                        u16::from_be_bytes([v[0], v[1]])
                    }
                })
                .collect();
            String::from_utf16(&units).map_err(|e| format!("Invalid UTF-16: {e}"))?
        }
        Encoding::Utf32Le | Encoding::Utf32Be => {
            let little = encoding == Encoding::Utf32Le;
            let bom: &[u8] = if little {
                b"\xff\xfe\0\0"
            } else {
                b"\0\0\xfe\xff"
            };
            let content = bytes.strip_prefix(bom).unwrap_or(bytes);
            if !content.len().is_multiple_of(4) {
                return Err("Invalid UTF-32: incomplete code point.".into());
            }
            content
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| {
                    let value = if little {
                        u32::from_le_bytes(*chunk)
                    } else {
                        u32::from_be_bytes(*chunk)
                    };
                    char::from_u32(value).ok_or_else(|| "Invalid UTF-32 code point.".to_owned())
                })
                .collect::<Result<String>>()?
        }
        Encoding::Legacy(label) if code_page(label).is_some() => {
            decode_code_page(bytes, code_page(label).unwrap())?
        }
        Encoding::Legacy(label) => {
            let enc = encoding_rs::Encoding::for_label(label.as_bytes())
                .ok_or_else(|| format!("Unknown encoding: {label}"))?;
            enc.decode_without_bom_handling_and_without_replacement(bytes)
                .ok_or_else(|| format!("Invalid bytes for {label}."))?
                .into_owned()
        }
    };
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err("Decoded text exceeds the 128 MiB document limit.".into());
    }
    Ok((text, encoding))
}

pub fn encoding_options() -> Vec<Encoding> {
    let mut encodings = vec![
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
        Encoding::Utf32Le,
        Encoding::Utf32Be,
    ];
    for label in [
        "windows-1250",
        "windows-1251",
        "windows-1252",
        "windows-1253",
        "windows-1254",
        "windows-1255",
        "windows-1256",
        "windows-1257",
        "windows-1258",
        "windows-874",
        "iso-8859-1",
        "iso-8859-2",
        "iso-8859-3",
        "iso-8859-4",
        "iso-8859-5",
        "iso-8859-6",
        "iso-8859-7",
        "iso-8859-8",
        "iso-8859-10",
        "iso-8859-13",
        "iso-8859-14",
        "iso-8859-15",
        "iso-8859-16",
        "koi8-r",
        "koi8-u",
        "macintosh",
        "x-mac-cyrillic",
        "IBM437",
        "IBM850",
        "IBM852",
        "IBM866",
        "shift_jis",
        "euc-jp",
        "iso-2022-jp",
        "gbk",
        "gb18030",
        "big5",
        "euc-kr",
    ] {
        encodings.push(Encoding::Legacy(label.into()));
    }
    encodings
}
fn code_page(label: &str) -> Option<u32> {
    match label {
        "IBM437" => Some(437),
        "IBM850" => Some(850),
        "IBM852" => Some(852),
        "IBM866" => Some(866),
        "iso-8859-1" => Some(28591),
        _ => None,
    }
}
fn decode_code_page(bytes: &[u8], codepage: u32) -> Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    use windows_sys::Win32::Globalization::{MB_ERR_INVALID_CHARS, MultiByteToWideChar};
    let len = i32::try_from(bytes.len()).map_err(|_| "Encoded input is too large.")?;
    unsafe {
        let count = MultiByteToWideChar(
            codepage,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            len,
            std::ptr::null_mut(),
            0,
        );
        if count == 0 {
            return Err(format!(
                "Cannot decode code page {codepage}: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut chars = vec![0u16; count as usize];
        if MultiByteToWideChar(
            codepage,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            len,
            chars.as_mut_ptr(),
            count,
        ) != count
        {
            return Err(format!(
                "Code-page decoding failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        String::from_utf16(&chars).map_err(|e| format!("Decoded text is invalid: {e}"))
    }
}
fn encode_code_page(text: &str, codepage: u32) -> Result<Vec<u8>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    use windows_sys::Win32::Globalization::{WC_NO_BEST_FIT_CHARS, WideCharToMultiByte};
    let chars: Vec<_> = text.encode_utf16().collect();
    let len = i32::try_from(chars.len()).map_err(|_| "Text is too large.")?;
    unsafe {
        let mut substituted = 0;
        let count = WideCharToMultiByte(
            codepage,
            WC_NO_BEST_FIT_CHARS,
            chars.as_ptr(),
            len,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            &mut substituted,
        );
        if count == 0 {
            return Err(format!(
                "Cannot encode code page {codepage}: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut bytes = vec![0; count as usize];
        if WideCharToMultiByte(
            codepage,
            WC_NO_BEST_FIT_CHARS,
            chars.as_ptr(),
            len,
            bytes.as_mut_ptr(),
            count,
            std::ptr::null(),
            &mut substituted,
        ) != count
        {
            return Err(format!(
                "Code-page encoding failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if substituted != 0 {
            return Err(
                "This encoding cannot represent every character. Use Unicode to avoid data loss."
                    .into(),
            );
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Eol {
    #[default]
    CrLf,
    Lf,
    Cr,
}

impl Eol {
    pub fn detect(text: &str) -> Self {
        match text.bytes().position(|c| c == b'\r' || c == b'\n') {
            Some(i) if text.as_bytes()[i] == b'\n' => Self::Lf,
            Some(i) if text.as_bytes().get(i + 1) == Some(&b'\n') => Self::CrLf,
            Some(_) => Self::Cr,
            None => Self::CrLf,
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::CrLf => "CRLF",
            Self::Lf => "LF",
            Self::Cr => "CR",
        }
    }
    pub fn convert(self, text: &str) -> String {
        text.replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', self.text())
    }
    pub fn scintilla(self) -> usize {
        match self {
            Self::CrLf => 0,
            Self::Cr => 1,
            Self::Lf => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Literal,
    Extended,
    Regex,
}

pub fn unescape(text: &str) -> Result<String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        out.push(
            match chars
                .next()
                .ok_or("Trailing backslash in extended search.")?
            {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '0' => '\0',
                '\\' => '\\',
                kind @ ('x' | 'u') => {
                    let n = if kind == 'x' { 2 } else { 4 };
                    let hex: String = chars.by_ref().take(n).collect();
                    if hex.len() != n || !hex.is_ascii() {
                        return Err("Incomplete hexadecimal escape.".into());
                    }
                    let value =
                        u32::from_str_radix(&hex, 16).map_err(|_| "Invalid hexadecimal escape.")?;
                    char::from_u32(value).ok_or("Invalid Unicode scalar value.")?
                }
                other => return Err(format!("Unknown escape: \\{other}")),
            },
        );
    }
    Ok(out)
}

pub struct Search {
    regex: Regex,
    mode: SearchMode,
}

fn expand_bounded(
    captures: &fancy_regex::Captures<'_, str>,
    replacement: &str,
    limit: usize,
) -> Result<String> {
    let mut out = String::new();
    let mut rest = replacement;
    while !rest.is_empty() {
        let end = if rest.starts_with("$$") {
            2
        } else if rest.starts_with("${") {
            rest.find('}').map(|i| i + 1).unwrap_or(1)
        } else if rest.starts_with('$') {
            1 + rest.as_bytes()[1..]
                .iter()
                .take_while(|ch| ch.is_ascii_alphanumeric() || **ch == b'_')
                .count()
        } else {
            rest.find('$').unwrap_or(rest.len())
        };
        let mut piece = String::new();
        // Expand one reference at a time so repeated captures cannot allocate unbounded output.
        captures.expand(&rest[..end], &mut piece);
        if piece.len() > limit.saturating_sub(out.len()) {
            return Err("Capture expansion exceeds the document size limit.".into());
        }
        out.push_str(&piece);
        rest = &rest[end..];
    }
    Ok(out)
}

impl Search {
    pub fn new(
        query: &str,
        mode: SearchMode,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Result<Self> {
        if query.is_empty() {
            return Err("Enter text to find.".into());
        }
        if query.len() > 32 * 1024 {
            return Err("Search expressions are limited to 32 KiB.".into());
        }
        let pattern = match mode {
            SearchMode::Literal => regex::escape(query),
            SearchMode::Extended => regex::escape(&unescape(query)?),
            SearchMode::Regex => query.to_owned(),
        };
        let pattern = if whole_word {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .multi_line(true)
            .crlf(true)
            .ignore_numbered_groups_when_named_groups_exist(false)
            .backtrack_limit(500_000)
            .delegate_size_limit(8 * 1024 * 1024)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { regex, mode })
    }
    pub fn find(&self, text: &str, start: usize) -> Result<Option<Range<usize>>> {
        let start = start.min(text.len());
        let mut boundary = start;
        while !text.is_char_boundary(boundary) {
            boundary += 1;
        }
        let found = self
            .regex
            .find_from_pos(text, boundary)
            .map_err(search_error)?;
        let found = if found.is_some() || boundary == 0 {
            found
        } else {
            self.regex.find(text).map_err(search_error)?
        };
        Ok(found.map(|m| m.range()))
    }
    pub fn matches(&self, text: &str) -> Result<Vec<Range<usize>>> {
        let mut out = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        for m in self.regex.find_iter(text) {
            if out.len() == 100_000 {
                return Err("More than 100,000 matches; narrow the search.".into());
            }
            if Instant::now() > deadline {
                return Err(
                    "Search exceeded its time budget. Narrow the expression or selection.".into(),
                );
            }
            out.push(m.map_err(search_error)?.range());
        }
        Ok(out)
    }
    pub fn replacement(
        &self,
        text: &str,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<String> {
        if text.len() > MAX_TOOL_BYTES || replacement.len() > 32 * 1024 {
            return Err(
                "Replacement is limited to 16 MiB of text and a 32 KiB replacement expression."
                    .into(),
            );
        }
        let replacement = if self.mode == SearchMode::Extended {
            unescape(replacement)?
        } else {
            replacement.into()
        };
        if self.mode != SearchMode::Regex {
            return Ok(replacement);
        }
        let captures = self
            .regex
            .captures_from_pos(text, range.start)
            .map_err(search_error)?
            .ok_or("The selection no longer matches.")?;
        if captures.get(0).unwrap().range() != range {
            return Err("The selection no longer matches.".into());
        }
        expand_bounded(&captures, &replacement, MAX_DOCUMENT_BYTES)
    }
    pub fn replace_all(&self, text: &str, replacement: &str) -> Result<(String, usize)> {
        let matches = self.matches(text)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut out = String::new();
        let mut previous = 0;
        for range in &matches {
            if Instant::now() > deadline {
                return Err(
                    "Replacement exceeded its time budget; the document was not changed.".into(),
                );
            }
            let piece = self.replacement(text, range.clone(), replacement)?;
            if out.len() + range.start - previous + piece.len() > MAX_DOCUMENT_BYTES {
                return Err("Replacement would exceed the 128 MiB document limit.".into());
            }
            out.push_str(&text[previous..range.start]);
            out.push_str(&piece);
            previous = range.end;
        }
        if out.len() + text.len() - previous > MAX_DOCUMENT_BYTES {
            return Err("Replacement is too large.".into());
        }
        out.push_str(&text[previous..]);
        Ok((out, matches.len()))
    }
}

fn search_error(error: fancy_regex::Error) -> String {
    format!(
        "Regex evaluation failed: {error}. Simplify the expression if its backtracking limit was reached."
    )
}

#[derive(Clone, Copy)]
pub enum CaseOp {
    Upper,
    Lower,
    Title,
    Sentence,
    Invert,
}

pub fn change_case(text: &str, operation: CaseOp) -> String {
    match operation {
        CaseOp::Upper => text.to_uppercase(),
        CaseOp::Lower => text.to_lowercase(),
        CaseOp::Invert => text
            .chars()
            .flat_map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<Vec<_>>()
                } else if c.is_lowercase() {
                    c.to_uppercase().collect()
                } else {
                    vec![c]
                }
            })
            .collect(),
        CaseOp::Title => text
            .split_word_bounds()
            .map(|word| {
                let mut first = true;
                word.to_lowercase()
                    .chars()
                    .flat_map(|ch| {
                        if first && ch.is_alphabetic() {
                            first = false;
                            ch.to_uppercase().collect::<Vec<_>>()
                        } else {
                            vec![ch]
                        }
                    })
                    .collect::<String>()
            })
            .collect(),
        CaseOp::Sentence => {
            let mut start = true;
            let mut previous = '\0';
            let lower = text.to_lowercase();
            let mut chars = lower.chars().peekable();
            let mut result = String::new();
            while let Some(ch) = chars.next() {
                if ch.is_alphabetic() {
                    if start {
                        result.extend(ch.to_uppercase());
                    } else {
                        result.push(ch);
                    }
                    start = false;
                } else {
                    result.push(ch);
                    if ch.is_numeric() {
                        start = false;
                    }
                }
                if matches!(ch, '!' | '?' | '\r' | '\n')
                    || (ch == '.'
                        && !(previous.is_ascii_digit()
                            && chars.peek().is_some_and(char::is_ascii_digit)))
                {
                    start = true;
                }
                previous = ch;
            }
            result
        }
    }
}

#[derive(Clone, Copy)]
pub enum LineOp {
    Sort,
    SortDescending,
    Unique,
    Trim,
    RemoveEmpty,
    SortIgnoreCase,
    SortDescendingIgnoreCase,
    SortNatural,
    SortNumeric,
    SortNumericDescending,
    SortNumericComma,
    Reverse,
    UniqueAdjacent,
    TrimStart,
    TrimBoth,
    RemoveEmptyOnly,
    Join,
}

fn natural_compare(mut left: &str, mut right: &str) -> Ordering {
    while !left.is_empty() && !right.is_empty() {
        if left.as_bytes()[0].is_ascii_digit() && right.as_bytes()[0].is_ascii_digit() {
            let a = left.bytes().take_while(u8::is_ascii_digit).count();
            let b = right.bytes().take_while(u8::is_ascii_digit).count();
            let x = left[..a].trim_start_matches('0');
            let y = right[..b].trim_start_matches('0');
            let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y)).then(a.cmp(&b));
            if order != Ordering::Equal {
                return order;
            }
            left = &left[a..];
            right = &right[b..];
        } else {
            let a = left.chars().next().unwrap();
            let b = right.chars().next().unwrap();
            if a != b {
                return a.cmp(&b);
            }
            left = &left[a.len_utf8()..];
            right = &right[b.len_utf8()..];
        }
    }
    left.len().cmp(&right.len())
}

fn numeric_key(text: &str, comma: bool) -> Result<(bool, &str, &str)> {
    let text = text.trim();
    let negative = text.starts_with('-');
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (whole, fraction) = digits
        .split_once(if comma { ',' } else { '.' })
        .unwrap_or((digits, ""));
    if (whole.is_empty() && fraction.is_empty())
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("Numeric sort requires one decimal number per line.".into());
    }
    let whole = whole.trim_start_matches('0');
    let fraction = fraction.trim_end_matches('0');
    Ok((
        negative && (!whole.is_empty() || !fraction.is_empty()),
        whole,
        fraction,
    ))
}

pub fn lines(text: &str, operation: LineOp, eol: Eol) -> Result<String> {
    let normalized = Eol::Lf.convert(text);
    let trailing = normalized.ends_with('\n');
    let mut lines: Vec<&str> = normalized
        .strip_suffix('\n')
        .unwrap_or(&normalized)
        .split('\n')
        .collect();
    match operation {
        LineOp::Sort => lines.sort(),
        LineOp::SortDescending => lines.sort_by(|a, b| b.cmp(a)),
        LineOp::SortIgnoreCase => lines.sort_by_cached_key(|s| s.to_lowercase()),
        LineOp::SortDescendingIgnoreCase => {
            lines.sort_by_cached_key(|s| std::cmp::Reverse(s.to_lowercase()))
        }
        LineOp::SortNatural => lines.sort_by(|a, b| natural_compare(a, b)),
        LineOp::SortNumeric | LineOp::SortNumericDescending | LineOp::SortNumericComma => {
            let mut numbers = lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    numeric_key(line, matches!(operation, LineOp::SortNumericComma))
                        .map(|key| (key, *line))
                        .map_err(|error| format!("Line {}: {error}", i + 1))
                })
                .collect::<Result<Vec<_>>>()?;
            numbers.sort_by(|((an, aw, af), _), ((bn, bw, bf), _)| {
                let absolute = aw
                    .len()
                    .cmp(&bw.len())
                    .then_with(|| aw.cmp(bw))
                    .then_with(|| af.cmp(bf));
                let order = bn
                    .cmp(an)
                    .then_with(|| if *an { absolute.reverse() } else { absolute });
                if matches!(operation, LineOp::SortNumericDescending) {
                    order.reverse()
                } else {
                    order
                }
            });
            lines = numbers.into_iter().map(|(_, line)| line).collect();
        }
        LineOp::Reverse => lines.reverse(),
        LineOp::UniqueAdjacent => lines.dedup(),
        LineOp::Unique => {
            let mut seen = HashSet::new();
            lines.retain(|line| seen.insert(*line));
        }
        LineOp::Trim => lines.iter_mut().for_each(|line| *line = line.trim_end()),
        LineOp::TrimStart => lines.iter_mut().for_each(|line| *line = line.trim_start()),
        LineOp::TrimBoth => lines.iter_mut().for_each(|line| *line = line.trim()),
        LineOp::RemoveEmpty => lines.retain(|line| !line.trim().is_empty()),
        LineOp::RemoveEmptyOnly => lines.retain(|line| !line.is_empty()),
        LineOp::Join => {}
    }
    let mut out = lines.join(if matches!(operation, LineOp::Join) {
        " "
    } else {
        eol.text()
    });
    if trailing && !lines.is_empty() {
        out.push_str(eol.text());
    }
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct Difference {
    pub left: Range<usize>,
    pub right: Range<usize>,
    pub left_inline: Vec<Range<usize>>,
    pub right_inline: Vec<Range<usize>>,
}

fn line_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut pos = 0;
    while pos < text.len() {
        match text.as_bytes()[pos] {
            b'\r' | b'\n' => {
                spans.push(start..pos);
                if text.as_bytes()[pos] == b'\r' && text.as_bytes().get(pos + 1) == Some(&b'\n') {
                    pos += 1;
                }
                start = pos + 1;
            }
            _ => {}
        }
        pos += 1;
    }
    if start < text.len() {
        spans.push(start..text.len());
    }
    spans
}

pub fn compare(left: &str, right: &str) -> Result<Vec<Difference>> {
    if left.len() + right.len() > MAX_TOOL_BYTES {
        return Err("Compare is limited to 16 MiB of combined text.".into());
    }
    let deadline = Instant::now() + Duration::from_millis(750);
    let left_spans = line_spans(left);
    let right_spans = line_spans(right);
    let left_normal = Eol::Lf.convert(left);
    let right_normal = Eol::Lf.convert(right);
    let diff = similar::TextDiff::configure()
        .timeout(Duration::from_millis(500))
        .diff_lines(&left_normal, &right_normal);
    Ok(diff
        .ops()
        .iter()
        .filter(|op| op.tag() != similar::DiffTag::Equal)
        .map(|op| {
            let mut result = Difference {
                left: op.old_range(),
                right: op.new_range(),
                left_inline: Vec::new(),
                right_inline: Vec::new(),
            };
            for offset in 0..result.left.len().max(result.right.len()) {
                let a = if offset < result.left.len() {
                    left_spans.get(result.left.start + offset)
                } else {
                    None
                };
                let b = if offset < result.right.len() {
                    right_spans.get(result.right.start + offset)
                } else {
                    None
                };
                match (a, b) {
                    (Some(a), Some(b))
                        if a.len() + b.len() <= 32 * 1024 && Instant::now() < deadline =>
                    {
                        let old = &left[a.clone()];
                        let new = &right[b.clone()];
                        let old_offsets: Vec<_> = old
                            .char_indices()
                            .map(|(i, _)| i)
                            .chain(Some(old.len()))
                            .collect();
                        let new_offsets: Vec<_> = new
                            .char_indices()
                            .map(|(i, _)| i)
                            .chain(Some(new.len()))
                            .collect();
                        let inline = similar::TextDiff::configure()
                            .timeout(deadline.saturating_duration_since(Instant::now()))
                            .diff_chars(old, new);
                        for operation in inline
                            .ops()
                            .iter()
                            .filter(|op| op.tag() != similar::DiffTag::Equal)
                        {
                            let x = operation.old_range();
                            let y = operation.new_range();
                            if !x.is_empty() {
                                result.left_inline.push(
                                    a.start + old_offsets[x.start]..a.start + old_offsets[x.end],
                                );
                            }
                            if !y.is_empty() {
                                result.right_inline.push(
                                    b.start + new_offsets[y.start]..b.start + new_offsets[y.end],
                                );
                            }
                        }
                    }
                    _ => {
                        if let Some(a) = a
                            && !a.is_empty()
                        {
                            result.left_inline.push(a.clone());
                        }
                        if let Some(b) = b
                            && !b.is_empty()
                        {
                            result.right_inline.push(b.clone());
                        }
                    }
                }
            }
            result
        })
        .collect())
}

pub fn corresponding_line(differences: &[Difference], line: usize, from_right: bool) -> usize {
    let mut offset = 0isize;
    for difference in differences {
        let (from, to) = if from_right {
            (&difference.right, &difference.left)
        } else {
            (&difference.left, &difference.right)
        };
        if line < from.start {
            break;
        }
        if line < from.end || (from.is_empty() && line == from.start) {
            return to.start + (line - from.start).min(to.len().saturating_sub(1));
        }
        offset = to.end as isize - from.end as isize;
    }
    line.saturating_add_signed(offset)
}

#[derive(Clone, Debug)]
pub struct JsonNode {
    pub parent: Option<usize>,
    pub label: String,
    pub pointer: String,
    pub span: Range<usize>,
}

pub fn json_tree(text: &str) -> Result<Vec<JsonNode>> {
    if text.len() > MAX_TOOL_BYTES {
        return Err("JSON tools are limited to 16 MiB.".into());
    }
    let json5 = crate::json_tools::validate(text)?;
    struct Parser<'a> {
        text: &'a str,
        pos: usize,
        nodes: Vec<JsonNode>,
        metadata_bytes: usize,
        json5: bool,
    }
    impl Parser<'_> {
        fn skip(&mut self) {
            loop {
                while self.pos < self.text.len()
                    && self.text[self.pos..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_whitespace() || c == '\u{feff}')
                {
                    self.pos += self.text[self.pos..].chars().next().unwrap().len_utf8();
                }
                if self.json5 && self.text[self.pos..].starts_with("//") {
                    self.pos += self.text[self.pos..]
                        .find(['\r', '\n'])
                        .unwrap_or(self.text.len() - self.pos);
                } else if self.json5 && self.text[self.pos..].starts_with("/*") {
                    self.pos += self.text[self.pos + 2..]
                        .find("*/")
                        .expect("validated comment")
                        + 4;
                } else {
                    break;
                }
            }
        }
        fn string(&mut self) -> Result<String> {
            let start = self.pos;
            let quote = self.text.as_bytes()[self.pos];
            self.pos += 1;
            while self.pos < self.text.len() {
                match self.text.as_bytes()[self.pos] {
                    b'\\' => self.pos += 2,
                    byte if byte == quote => {
                        self.pos += 1;
                        break;
                    }
                    _ => self.pos += 1,
                }
            }
            if self.json5 {
                json5::from_str(&self.text[start..self.pos]).map_err(|e| e.to_string())
            } else {
                serde_json::from_str(&self.text[start..self.pos]).map_err(|e| e.to_string())
            }
        }
        fn value(&mut self, parent: Option<usize>, name: &str, pointer: String) -> Result<()> {
            self.skip();
            if self.nodes.len() >= 20_000 {
                return Err("JSON tree is limited to 20,000 nodes.".into());
            }
            if pointer.len() > 64 * 1024 {
                return Err("JSON tree pointers are limited to 64 KiB.".into());
            }
            let display_name: String = name.chars().take(80).collect();
            self.metadata_bytes += pointer.len() + display_name.len();
            if self.metadata_bytes > 8 * 1024 * 1024 {
                return Err("JSON tree metadata exceeds 8 MiB.".into());
            }
            let start = self.pos;
            let index = self.nodes.len();
            self.nodes.push(JsonNode {
                parent,
                label: display_name.clone(),
                pointer: pointer.clone(),
                span: start..start,
            });
            match self.text.as_bytes()[self.pos] {
                open @ (b'{' | b'[') => {
                    let object = open == b'{';
                    let close = if object { b'}' } else { b']' };
                    self.pos += 1;
                    self.skip();
                    let mut count = 0;
                    let mut keys = HashSet::new();
                    while self.text.as_bytes()[self.pos] != close {
                        let key = if object {
                            let key = if matches!(self.text.as_bytes()[self.pos], b'"' | b'\'') {
                                self.string()?
                            } else {
                                let start = self.pos;
                                while self.pos < self.text.len()
                                    && !self.text[self.pos..].starts_with(':')
                                    && !self.text[self.pos..]
                                        .chars()
                                        .next()
                                        .unwrap()
                                        .is_whitespace()
                                    && !self.text[self.pos..].starts_with("/*")
                                    && !self.text[self.pos..].starts_with("//")
                                {
                                    self.pos +=
                                        self.text[self.pos..].chars().next().unwrap().len_utf8();
                                }
                                json5::from_str::<String>(&format!(
                                    "\"{}\"",
                                    &self.text[start..self.pos]
                                ))
                                .map_err(|e| e.to_string())?
                            };
                            if !keys.insert(key.clone()) {
                                return Err(format!(
                                    "Duplicate JSON key '{}'; formatting would lose data.",
                                    key.chars().take(80).collect::<String>()
                                ));
                            }
                            self.skip();
                            self.pos += 1;
                            key
                        } else {
                            count.to_string()
                        };
                        let segment = key.replace('~', "~0").replace('/', "~1");
                        self.value(Some(index), &key, format!("{pointer}/{segment}"))?;
                        count += 1;
                        self.skip();
                        if self.text.as_bytes()[self.pos] == b',' {
                            self.pos += 1;
                            self.skip();
                        }
                    }
                    self.pos += 1;
                    self.nodes[index].label = format!(
                        "{display_name}  {}{count}{}",
                        if object { "{" } else { "[" },
                        if object { "}" } else { "]" }
                    );
                }
                b'"' | b'\'' => {
                    let value = self.string()?;
                    self.nodes[index].label = format!(
                        "{display_name}: \"{}\"",
                        value.chars().take(100).collect::<String>()
                    );
                }
                _ => {
                    while self.pos < self.text.len()
                        && self.text[self.pos..]
                            .chars()
                            .next()
                            .is_some_and(|ch| !",}]".contains(ch) && !ch.is_whitespace())
                        && !self.text[self.pos..].starts_with("//")
                        && !self.text[self.pos..].starts_with("/*")
                    {
                        self.pos += self.text[self.pos..].chars().next().unwrap().len_utf8();
                    }
                    self.nodes[index].label = format!(
                        "{display_name}: {}",
                        self.text[start..self.pos]
                            .chars()
                            .take(100)
                            .collect::<String>()
                    );
                }
            }
            self.nodes[index].label = self.nodes[index]
                .label
                .replace(['\r', '\n', '\t', '\0'], " ");
            self.nodes[index].span = start..self.pos;
            self.metadata_bytes += self.nodes[index]
                .label
                .len()
                .saturating_sub(display_name.len());
            if self.metadata_bytes > 8 * 1024 * 1024 {
                return Err("JSON tree metadata exceeds 8 MiB.".into());
            }
            Ok(())
        }
    }
    let mut parser = Parser {
        text,
        pos: 0,
        nodes: Vec::new(),
        metadata_bytes: 0,
        json5,
    };
    parser.value(None, "$", String::new())?;
    Ok(parser.nodes)
}

pub fn format_json(text: &str, compact: bool, eol: Eol) -> Result<String> {
    let formatted = crate::json_tools::format(text, compact)?;
    Ok(eol.convert(&formatted))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_encodings_round_trip() {
        for encoding in [
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
            Encoding::Utf32Le,
            Encoding::Utf32Be,
        ] {
            let text = "A\0\u{1f680}\u{4e2d}\r\n";
            assert_eq!(
                decode(&encoding.encode(text).unwrap(), None).unwrap(),
                (text.into(), encoding)
            );
        }
    }
    #[test]
    fn malformed_and_lossy_encoding_is_rejected() {
        assert!(decode(&[255, 254, 0], None).is_err());
        assert!(decode(&[255, 254, 0, 216], None).is_err());
        assert!(
            Encoding::Legacy("windows-1252".into())
                .encode("\u{1f680}")
                .is_err()
        );
        assert_eq!(decode(&[0x80], None).unwrap().0, "\u{20ac}");
    }
    #[test]
    fn all_line_endings_convert_without_doubling() {
        assert_eq!(Eol::CrLf.convert("a\r\nb\rc\n"), "a\r\nb\r\nc\r\n");
        assert_eq!(Eol::detect("a\rb"), Eol::Cr);
        assert_eq!(
            lines("b\r\na\r\nb\r\n", LineOp::Unique, Eol::CrLf).unwrap(),
            "b\r\na\r\n"
        );
        assert_eq!(lines("b\nA\n", LineOp::Sort, Eol::Lf).unwrap(), "A\nb\n");
    }
    #[test]
    fn extended_search_and_literal_dollars() {
        let search = Search::new(r"\r\n", SearchMode::Extended, true, false).unwrap();
        assert_eq!(
            search.replace_all("a\r\nb", r"\t$1").unwrap(),
            ("a\t$1b".into(), 1)
        );
        assert!(unescape(r"\z").is_err());
        assert!(unescape(r"\uD800").is_err());
        assert_eq!(unescape(r"\x41\u0042\\").unwrap(), "AB\\");
    }
    #[test]
    fn regex_replacements_are_unicode_safe_and_zero_width_terminates() {
        let search = Search::new(r"(?P<word>\w+)", SearchMode::Regex, true, false).unwrap();
        assert_eq!(
            search.replace_all("caf\u{e9}", "${word}!").unwrap().0,
            "caf\u{e9}!"
        );
        let empty = Search::new("^", SearchMode::Regex, true, false).unwrap();
        assert_eq!(
            empty.replace_all("a\nb", ">").unwrap(),
            (">a\n>b".into(), 2)
        );
        assert!(Search::new("(", SearchMode::Regex, true, false).is_err());
        assert_eq!(
            Search::new(r"(?<=x)(a)\1(?=z)", SearchMode::Regex, true, false)
                .unwrap()
                .matches("xaaz")
                .unwrap(),
            vec![1..3]
        );
    }
    #[test]
    fn json_spans_point_to_original_values() {
        let text = r#"{"a/b":[12,{"~x":"h\"i"}],"z":true}"#;
        let nodes = json_tree(text).unwrap();
        let node = nodes.iter().find(|n| n.pointer == "/a~1b/1/~0x").unwrap();
        assert_eq!(&text[node.span.clone()], r#""h\"i""#);
        assert_eq!(nodes[0].span, 0..text.len());
        assert!(json_tree(r#"{"x":1,"x":2}"#).is_err());
        assert!(json_tree("{").is_err());
    }
    #[test]
    fn json_keeps_big_numbers_and_key_order() {
        let text = r#"{"z":123456789012345678901234567890,"a":1.234567890123456789}"#;
        assert_eq!(format_json(text, true, Eol::Lf).unwrap(), text);
    }
    #[test]
    fn compare_tracks_insert_delete_and_replace() {
        let diff = compare("a\nb\n", "a\nc\nd\n").unwrap();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].left, 1..2);
        assert_eq!(diff[0].right, 1..3);
        assert!(compare("", "").unwrap().is_empty());
        assert_eq!(compare("", "x").unwrap()[0].left, 0..0);
        assert_eq!(compare("a\rb\r", "a\rc\r").unwrap()[0].left, 1..2);
        assert!(compare("a\r\nb\r\n", "a\nb\n").unwrap().is_empty());
        let inserted = compare("a\nb\nc\n", "a\nnew\nb\nc\n").unwrap();
        assert_eq!(corresponding_line(&inserted, 2, false), 3);
        assert_eq!(corresponding_line(&inserted, 3, true), 2);
    }
    #[test]
    fn encoding_overrides_and_crlf_regex_anchors() {
        assert_eq!(
            decode(b"\xef\xbb\xbftext", Some(&Encoding::Utf8))
                .unwrap()
                .0,
            "text"
        );
        assert_eq!(decode(b"\xff\xfe\0\0x\0\0\0", None).unwrap().0, "x");
        let search = Search::new(r"^x$", SearchMode::Regex, true, false).unwrap();
        assert_eq!(search.matches("x\r\nx\n").unwrap(), vec![0..1, 3..4]);
    }

    #[test]
    fn bounded_replacements_preserve_regex_interpolation() {
        let regex = Regex::new(r"(?P<word>\w+)").unwrap();
        let captures = regex.captures("hello").unwrap().unwrap();
        for replacement in [
            "$0",
            "$1x",
            "${word}!",
            "$$$1",
            "$missing",
            "${missing}",
            "$",
            "${",
            "${unclosed$1",
            "$-",
            "$word$word",
            "$0_${word}",
        ] {
            let mut expected = String::new();
            captures.expand(replacement, &mut expected);
            assert_eq!(
                expand_bounded(&captures, replacement, 1024).unwrap(),
                expected,
                "{replacement}"
            );
        }
        assert_eq!(expand_bounded(&captures, "$1$1", 10).unwrap(), "hellohello");
        assert!(expand_bounded(&captures, "$1$1", 9).is_err());
    }

    #[test]
    fn json_tree_enforces_actual_metadata_and_node_limits() {
        let allowed = format!("[{}]", vec!["0"; 19_999].join(","));
        assert_eq!(json_tree(&allowed).unwrap().len(), 20_000);
        let excessive = format!("[{}]", vec!["0"; 20_000].join(","));
        assert!(json_tree(&excessive).unwrap_err().contains("20,000"));
        let long_key = format!("{{\"{}\":0}}", "x".repeat(64 * 1024));
        assert!(json_tree(&long_key).unwrap_err().contains("64 KiB"));
        let shared_prefix = format!(
            "{{\"{}\":[{}]}}",
            "x".repeat(512),
            vec!["0"; 19_000].join(",")
        );
        assert!(json_tree(&shared_prefix).unwrap_err().contains("metadata"));
    }

    #[test]
    fn additional_case_and_line_operations_are_unicode_and_number_safe() {
        assert_eq!(change_case("hELLO wORLD", CaseOp::Title), "Hello World");
        assert_eq!(
            change_case("hELLO. tHERE! 1.25 IS A NUMBER.", CaseOp::Sentence),
            "Hello. There! 1.25 is a number."
        );
        assert_eq!(change_case("AbC", CaseOp::Invert), "aBc");
        assert_eq!(
            lines("file10\nfile2\nfile1\n", LineOp::SortNatural, Eol::Lf).unwrap(),
            "file1\nfile2\nfile10\n"
        );
        assert_eq!(
            lines(
                "9007199254740993\n9007199254740992\n-10\n-2.5\n",
                LineOp::SortNumeric,
                Eol::Lf
            )
            .unwrap(),
            "-10\n-2.5\n9007199254740992\n9007199254740993\n"
        );
        assert_eq!(
            lines("1,2\n1,11\n", LineOp::SortNumericComma, Eol::Lf).unwrap(),
            "1,11\n1,2\n"
        );
        assert!(lines("word\n1\n", LineOp::SortNumeric, Eol::Lf).is_err());
        assert_eq!(
            lines("a\na\nb\na\n", LineOp::UniqueAdjacent, Eol::Lf).unwrap(),
            "a\nb\na\n"
        );
        assert_eq!(
            lines("a\nb\n", LineOp::Reverse, Eol::CrLf).unwrap(),
            "b\r\na\r\n"
        );
    }
    #[test]
    fn advanced_regex_replacements_retain_context_and_report_resource_failures() {
        let search = Search::new(
            r"(?<=prefix:)(\w+)\s+\1(?!x)",
            SearchMode::Regex,
            true,
            false,
        )
        .unwrap();
        assert_eq!(
            search.replace_all("prefix:cat cat", "<$1>").unwrap(),
            ("prefix:<cat>".into(), 1)
        );
        let regex = RegexBuilder::new(r"^(a|aa)+\1$")
            .backtrack_limit(50)
            .build()
            .unwrap();
        let search = Search {
            regex,
            mode: SearchMode::Regex,
        };
        assert!(search.matches(&format!("{}b", "a".repeat(64))).is_err());
    }
    #[test]
    fn inline_differences_use_original_utf8_byte_offsets() {
        let left = "name = '\u{e9}';\r\nnext\r\n";
        let right = "name = '\u{1f680}';\nnext\n";
        let difference = compare(left, right).unwrap().remove(0);
        assert_eq!(&left[difference.left_inline[0].clone()], "\u{e9}");
        assert_eq!(&right[difference.right_inline[0].clone()], "\u{1f680}");
    }
    #[test]
    fn every_encoding_choice_round_trips_and_rejects_loss() {
        for encoding in encoding_options() {
            let text = "text\0\r\n";
            let bytes = encoding
                .encode(text)
                .unwrap_or_else(|error| panic!("{}: {error}", encoding.label()));
            assert_eq!(
                decode(&bytes, Some(&encoding)).unwrap().0,
                text,
                "{}",
                encoding.label()
            );
        }
        let oem = Encoding::Legacy("IBM437".into());
        assert_eq!(decode(&[0xdb], Some(&oem)).unwrap().0, "\u{2588}");
        let latin = Encoding::Legacy("iso-8859-1".into());
        assert!(latin.encode("\u{20ac}").is_err());
        assert!(decode(&[255, 254, 0, 0, 0, 216, 0, 0], None).is_err());
    }
    #[test]
    fn json5_tree_keeps_original_spans_with_unicode_whitespace_and_comments() {
        let text = "{ /*header*/ unquoted\u{a0}: 0xFF\u{a0}, 'list': [1,2,], }";
        let nodes = json_tree(text).unwrap();
        let value = nodes
            .iter()
            .find(|node| node.pointer == "/unquoted")
            .unwrap();
        assert_eq!(&text[value.span.clone()], "0xFF");
        assert!(nodes.iter().any(|node| node.pointer == "/list/1"));
    }
}
