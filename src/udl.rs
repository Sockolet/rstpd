use crate::core::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub foreground: Option<u32>,
    pub background: Option<u32>,
    pub font_style: u8,
    pub nesting: u32,
    pub font: Option<String>,
    pub font_size: Option<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub open: String,
    pub close: Vec<String>,
    pub escape: Vec<String>,
    pub style: u8,
    pub category: u32,
    pub line_comment: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserLanguage {
    pub name: String,
    pub extensions: Vec<String>,
    pub ignore_case: bool,
    pub prefix: [bool; 8],
    pub keywords: [Vec<String>; 8],
    pub operators: [Vec<String>; 2],
    pub regions: Vec<Region>,
    pub styles: Vec<Style>,
    pub folders: [Vec<String>; 9],
    pub numbers: [Vec<String>; 7],
    pub line_position: u8,
    pub decimal_separator: u8,
    pub fold_comments: bool,
    pub fold_compact: bool,
}
pub use crate::core::Highlight;

fn words(text: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        let mut value = String::new();
        if ch == '"' && chars.clone().any(|c| c == '"') {
            for next in chars.by_ref() {
                if next == '"' {
                    break;
                }
                value.push(next);
            }
        } else {
            value.push(ch);
            while chars.peek().is_some_and(|c| !c.is_whitespace()) {
                value.push(chars.next().unwrap());
            }
        }
        if !value.is_empty() {
            result.push(value);
        }
    }
    if result.len() > 10_000 || result.iter().any(|word| word.len() > 1024) {
        return Err(
            "Language definitions allow at most 10,000 words per list and 1 KiB per token.".into(),
        );
    }
    Ok(result)
}

fn indexed(text: &str, count: usize) -> Result<Vec<Vec<Vec<String>>>> {
    let mut output = vec![Vec::new(); count];
    let mut rest = text.trim();
    while !rest.is_empty() {
        let bytes = rest.as_bytes();
        if bytes.len() < 2 || !bytes[..2].iter().all(u8::is_ascii_digit) {
            return Err("Invalid UDL delimiter/comment field.".into());
        }
        let index = ((bytes[0] - b'0') * 10 + bytes[1] - b'0') as usize;
        if index >= count {
            return Err("UDL field index is outside the supported definition.".into());
        }
        rest = &rest[2..];
        let value;
        if let Some(group) = rest.strip_prefix("((") {
            let end = group.find("))").ok_or("Unclosed UDL alternative group.")?;
            value = group[..end]
                .split_whitespace()
                .map(|part| {
                    if part == "EOL" {
                        "\n".into()
                    } else {
                        part.into()
                    }
                })
                .collect();
            rest = &group[end + 2..];
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            value = if end == 0 {
                Vec::new()
            } else {
                vec![rest[..end].to_owned()]
            };
            rest = &rest[end..];
        }
        output[index].push(value);
        rest = rest.trim_start();
    }
    Ok(output)
}

fn style_id(name: &str) -> Option<usize> {
    match name {
        "DEFAULT" => Some(0),
        "COMMENTS" => Some(1),
        "LINE COMMENTS" => Some(2),
        "NUMBERS" => Some(3),
        "OPERATORS" => Some(12),
        "FOLDER IN CODE1" => Some(22),
        "FOLDER IN CODE2" => Some(23),
        "FOLDER IN COMMENT" => Some(24),
        name if name.starts_with("KEYWORDS") => name[8..]
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=8).contains(n))
            .map(|n| n + 3),
        name if name.starts_with("DELIMITERS") => name[10..]
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=8).contains(n))
            .map(|n| n + 13),
        _ => None,
    }
}
fn boolean(value: Option<&str>) -> Result<bool> {
    match value {
        Some("yes" | "true" | "1") => Ok(true),
        None | Some("no" | "false" | "0") => Ok(false),
        _ => Err("Invalid boolean in language definition.".into()),
    }
}
fn color(value: Option<&str>) -> Result<Option<u32>> {
    value
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.len() != 6 {
                return Err("UDL colors must use six hexadecimal digits.".into());
            }
            u32::from_str_radix(s, 16).map_err(|_| "Invalid UDL color.".into())
        })
        .transpose()
}

pub fn import(xml: &str) -> Result<Vec<UserLanguage>> {
    if xml.len() > 1024 * 1024 {
        return Err("Language XML is limited to 1 MiB.".into());
    }
    let document = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            nodes_limit: 25_000,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Invalid language XML: {e}"))?;
    let mut output = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("UserLang"))
    {
        if !matches!(node.attribute("udlVersion"), Some("2.0" | "2.1")) {
            return Err("Only UDL 2.0 and 2.1 definitions are supported.".into());
        }
        let global = node.descendants().find(|node| node.has_tag_name("Global"));
        let pref = node.descendants().find(|node| node.has_tag_name("Prefix"));
        let attr = |name| global.and_then(|node| node.attribute(name));
        let mut language = UserLanguage {
            name: node
                .attribute("name")
                .ok_or("Language name is missing.")?
                .into(),
            extensions: node
                .attribute("ext")
                .unwrap_or("")
                .split_whitespace()
                .map(|ext| ext.trim_start_matches("*.").to_lowercase())
                .collect(),
            ignore_case: boolean(attr("caseIgnored"))?,
            prefix: [false; 8],
            keywords: Default::default(),
            operators: Default::default(),
            regions: Vec::new(),
            styles: vec![Style::default(); 25],
            folders: Default::default(),
            numbers: Default::default(),
            line_position: attr("forcePureLC")
                .unwrap_or("0")
                .parse()
                .map_err(|_| "Invalid line-comment position.")?,
            decimal_separator: attr("decimalSeparator")
                .unwrap_or("0")
                .parse()
                .map_err(|_| "Invalid decimal separator.")?,
            fold_comments: boolean(attr("allowFoldOfComments"))?,
            fold_compact: boolean(attr("foldCompact"))?,
        };
        for index in 0..8 {
            language.prefix[index] =
                boolean(pref.and_then(|p| p.attribute(format!("Keywords{}", index + 1).as_str())))?;
        }
        let lists: HashMap<_, _> = node
            .descendants()
            .filter(|node| node.has_tag_name("Keywords"))
            .filter_map(|node| {
                node.attribute("name")
                    .map(|name| (name, node.text().unwrap_or("")))
            })
            .collect();
        for (index, group) in language.keywords.iter_mut().enumerate() {
            *group = words(
                lists
                    .get(format!("Keywords{}", index + 1).as_str())
                    .copied()
                    .unwrap_or(""),
            )?;
        }
        for index in 0..2 {
            language.operators[index] = words(
                lists
                    .get(format!("Operators{}", index + 1).as_str())
                    .copied()
                    .unwrap_or(""),
            )?;
        }
        for (index, name) in [
            "Folders in code1, open",
            "Folders in code1, middle",
            "Folders in code1, close",
            "Folders in code2, open",
            "Folders in code2, middle",
            "Folders in code2, close",
            "Folders in comment, open",
            "Folders in comment, middle",
            "Folders in comment, close",
        ]
        .iter()
        .enumerate()
        {
            language.folders[index] = words(lists.get(name).copied().unwrap_or(""))?;
        }
        for (index, name) in [
            "Numbers, prefix1",
            "Numbers, prefix2",
            "Numbers, extras1",
            "Numbers, extras2",
            "Numbers, suffix1",
            "Numbers, suffix2",
            "Numbers, range",
        ]
        .iter()
        .enumerate()
        {
            language.numbers[index] = words(lists.get(name).copied().unwrap_or(""))?;
        }
        let comments = indexed(lists.get("Comments").copied().unwrap_or(""), 5)?;
        let delimiters = indexed(lists.get("Delimiters").copied().unwrap_or(""), 24)?;
        let mut add_regions = |opens: &Vec<Vec<String>>,
                               escapes: &Vec<Vec<String>>,
                               closes: &Vec<Vec<String>>,
                               style,
                               category,
                               line_comment| {
            for (index, group) in opens.iter().enumerate() {
                for open in group {
                    language.regions.push(Region {
                        open: open.clone(),
                        close: closes
                            .get(index)
                            .or_else(|| closes.last())
                            .cloned()
                            .unwrap_or_default(),
                        escape: escapes
                            .get(index)
                            .or_else(|| escapes.last())
                            .cloned()
                            .unwrap_or_default(),
                        style,
                        category,
                        line_comment,
                    });
                }
            }
        };
        add_regions(&comments[0], &comments[1], &comments[2], 2, 512, true);
        add_regions(&comments[3], &Vec::new(), &comments[4], 1, 256, false);
        for index in 0..8 {
            add_regions(
                &delimiters[index * 3],
                &delimiters[index * 3 + 1],
                &delimiters[index * 3 + 2],
                14 + index as u8,
                1 << index,
                false,
            );
        }
        for node in node
            .descendants()
            .filter(|node| node.has_tag_name("WordsStyle"))
        {
            let id =
                style_id(node.attribute("name").unwrap_or("")).ok_or("Unknown UDL style name.")?;
            let mask = node
                .attribute("colorStyle")
                .unwrap_or("3")
                .parse::<u8>()
                .map_err(|_| "Invalid color style.")?;
            language.styles[id] = Style {
                foreground: if mask & 1 != 0 {
                    color(node.attribute("fgColor"))?
                } else {
                    None
                },
                background: if mask & 2 != 0 {
                    color(node.attribute("bgColor"))?
                } else {
                    None
                },
                font_style: node
                    .attribute("fontStyle")
                    .unwrap_or("0")
                    .parse()
                    .map_err(|_| "Invalid UDL font style.")?,
                nesting: node
                    .attribute("nesting")
                    .unwrap_or("0")
                    .parse()
                    .map_err(|_| "Invalid UDL nesting mask.")?,
                font: node
                    .attribute("fontName")
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned),
                font_size: match node.attribute("fontSize") {
                    None | Some("" | "0" | "-1") => None,
                    Some(size) => Some(size.parse().map_err(|_| "Invalid UDL font size.")?),
                },
            };
        }
        language.validate()?;
        output.push(language);
    }
    if output.is_empty() {
        return Err("No UserLang definitions were found.".into());
    }
    if output.len() > 64 {
        return Err("At most 64 user-defined languages may be imported at once.".into());
    }
    Ok(output)
}

impl UserLanguage {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
        {
            return Err("Invalid custom-language name.".into());
        }
        if self.styles.len() != 25
            || self.regions.len() > 256
            || self.line_position > 2
            || self.decimal_separator > 2
            || self.extensions.len() > 128
            || self.styles.iter().any(|s| {
                s.font_style > 7
                    || s.font_size.is_some_and(|size| !(4..=72).contains(&size))
                    || s.font
                        .as_ref()
                        .is_some_and(|font| font.len() > 128 || font.chars().any(char::is_control))
            })
        {
            return Err("Language definition exceeds supported limits.".into());
        }
        for region in &self.regions {
            if region.open.is_empty()
                || region.open.len() > 1024
                || region.style as usize >= self.styles.len()
                || region.close.len() > 64
                || region.escape.len() > 64
                || region
                    .close
                    .iter()
                    .chain(&region.escape)
                    .any(|token| token.len() > 1024 || token.contains('\0'))
                || region.open.contains('\0')
            {
                return Err("Invalid custom-language region.".into());
            }
        }
        for tokens in self
            .keywords
            .iter()
            .chain(&self.operators)
            .chain(&self.folders)
            .chain(&self.numbers)
        {
            if tokens.len() > 10_000
                || tokens
                    .iter()
                    .any(|token| token.len() > 1024 || token.contains('\0'))
            {
                return Err("Language token limits exceeded.".into());
            }
        }
        Ok(())
    }
}

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
fn matched(text: &str, pos: usize, token: &str, ignore_case: bool) -> Option<usize> {
    if token == "\n" {
        return if text[pos..].starts_with("\r\n") {
            Some(2)
        } else if text[pos..].starts_with(['\r', '\n']) {
            Some(1)
        } else {
            None
        };
    }
    if token.is_empty() {
        return None;
    }
    if !ignore_case {
        return text[pos..].starts_with(token).then_some(token.len());
    }
    let end = text[pos..]
        .char_indices()
        .nth(token.chars().count())
        .map_or(text.len(), |(offset, _)| pos + offset);
    (text[pos..end].to_lowercase() == token.to_lowercase()).then_some(end - pos)
}
fn longest(text: &str, pos: usize, tokens: &[String], ignore: bool) -> Option<usize> {
    tokens
        .iter()
        .filter_map(|token| matched(text, pos, token, ignore))
        .max()
}

pub fn highlight(language: &UserLanguage, text: &str) -> Result<Highlight> {
    language.validate()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut styles = vec![0u8; text.len()];
    let mut folds = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut pos = 0;
    let mut level = 0usize;
    let mut minimum = 0usize;
    let mut line_start = 0;
    let keywords: Vec<HashSet<String>> = language
        .keywords
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|s| {
                    if language.ignore_case {
                        s.to_lowercase()
                    } else {
                        s.clone()
                    }
                })
                .collect()
        })
        .collect();
    let mut steps = 0usize;
    while pos < text.len() {
        steps += 1;
        if steps.is_multiple_of(256) && Instant::now() > deadline {
            return Err("Custom highlighting exceeded its time budget.".into());
        }
        let start = pos;
        let parent = stack.last().copied();
        let mut style = parent.map_or(0, |i| language.regions[i].style);
        let allowed = parent.map_or(u32::MAX, |i| {
            language.styles[language.regions[i].style as usize].nesting
        });
        let ch = text[pos..].chars().next().unwrap();
        let mut consumed = None;
        if let Some(index) = parent {
            let region = &language.regions[index];
            if let Some(length) = longest(text, pos, &region.escape, language.ignore_case) {
                let next = pos + length;
                let extra = if next < text.len() {
                    if text[next..].starts_with("\r\n") {
                        2
                    } else {
                        text[next..].chars().next().unwrap().len_utf8()
                    }
                } else {
                    0
                };
                consumed = Some(length + extra);
            } else if let Some(length) = longest(text, pos, &region.close, language.ignore_case) {
                consumed = Some(length);
                stack.pop();
                if language.fold_comments && region.style == 1 {
                    level = level.saturating_sub(1);
                    minimum = minimum.min(level);
                }
            } else if region.line_comment && matches!(ch, '\r' | '\n') {
                stack.pop();
                continue;
            }
        }
        if consumed.is_none() {
            let region = language
                .regions
                .iter()
                .enumerate()
                .filter(|(_, region)| allowed & region.category != 0)
                .filter(|(_, region)| {
                    !region.line_comment
                        || language.line_position == 0
                        || (language.line_position == 1 && pos == line_start)
                        || (language.line_position == 2 && text[line_start..pos].trim().is_empty())
                })
                .filter_map(|(i, region)| {
                    matched(text, pos, &region.open, language.ignore_case).map(|length| (i, length))
                })
                .max_by_key(|(_, len)| *len);
            if let Some((index, length)) = region {
                if stack.len() >= 128 {
                    return Err("Custom-language nesting exceeds 128 levels.".into());
                }
                stack.push(index);
                style = language.regions[index].style;
                consumed = Some(length);
                if language.fold_comments && style == 1 {
                    level += 1;
                }
            }
        }
        if consumed.is_none() {
            let in_comment = parent.is_some_and(|i| matches!(language.regions[i].style, 1 | 2));
            let groups = if in_comment { 6..9 } else { 0..6 };
            for index in groups {
                if !in_comment && allowed == 0 {
                    continue;
                }
                if let Some(length) =
                    longest(text, pos, &language.folders[index], language.ignore_case)
                {
                    let end = pos + length;
                    if index >= 3
                        && (text[..pos].chars().next_back().is_some_and(word_char)
                            || text[end..].chars().next().is_some_and(word_char))
                    {
                        continue;
                    }
                    style = 22 + (index / 3) as u8;
                    consumed = Some(length);
                    match index % 3 {
                        0 => level += 1,
                        1 => {
                            minimum = minimum.min(level.saturating_sub(1));
                        }
                        _ => {
                            level = level.saturating_sub(1);
                            minimum = minimum.min(level);
                        }
                    }
                    break;
                }
            }
        }
        if consumed.is_none() {
            for index in 0..2 {
                if allowed & (1 << (24 + index)) == 0 {
                    continue;
                }
                if let Some(length) =
                    longest(text, pos, &language.operators[index], language.ignore_case)
                {
                    if index == 1
                        && (pos > 0 && !text[..pos].chars().next_back().unwrap().is_whitespace()
                            || pos + length < text.len()
                                && !text[pos + length..].chars().next().unwrap().is_whitespace())
                    {
                        continue;
                    }
                    style = 12;
                    consumed = Some(length);
                    break;
                }
            }
        }
        if consumed.is_none() && allowed & (1 << 26) != 0 {
            let prefix = longest(text, pos, &language.numbers[0], language.ignore_case)
                .or_else(|| longest(text, pos, &language.numbers[1], language.ignore_case));
            if ch.is_ascii_digit() || prefix.is_some() {
                let mut end = pos + prefix.unwrap_or(0);
                while end < text.len() {
                    let c = text[end..].chars().next().unwrap();
                    if c.is_ascii_digit()
                        || (c == '.' && language.decimal_separator != 1)
                        || (c == ',' && language.decimal_separator != 0)
                    {
                        end += c.len_utf8();
                    } else if let Some(length) =
                        longest(text, end, &language.numbers[2], language.ignore_case)
                            .or_else(|| {
                                longest(text, end, &language.numbers[3], language.ignore_case)
                            })
                            .or_else(|| {
                                longest(text, end, &language.numbers[6], language.ignore_case)
                            })
                    {
                        end += length;
                    } else {
                        break;
                    }
                }
                if let Some(length) = longest(text, end, &language.numbers[4], language.ignore_case)
                    .or_else(|| longest(text, end, &language.numbers[5], language.ignore_case))
                {
                    end += length;
                }
                if end > pos {
                    consumed = Some(end - pos);
                    style = 3;
                }
            }
        }
        if consumed.is_none() && word_char(ch) {
            let mut length = text[pos..]
                .char_indices()
                .find(|(_, c)| !word_char(*c))
                .map_or(text.len() - pos, |(i, _)| i);
            if let Some(parent) = parent {
                let region = &language.regions[parent];
                if let Some(stop) =
                    text[pos..pos + length]
                        .char_indices()
                        .skip(1)
                        .find_map(|(offset, _)| {
                            (longest(text, pos + offset, &region.close, language.ignore_case)
                                .is_some()
                                || longest(
                                    text,
                                    pos + offset,
                                    &region.escape,
                                    language.ignore_case,
                                )
                                .is_some())
                            .then_some(offset)
                        })
                {
                    length = stop;
                }
            }
            let word = &text[pos..pos + length];
            let key = if language.ignore_case {
                word.to_lowercase()
            } else {
                word.into()
            };
            for (index, group) in keywords.iter().enumerate() {
                if allowed & (1 << (10 + index)) == 0 {
                    continue;
                }
                if group.contains(&key)
                    || (language.prefix[index]
                        && group.iter().any(|keyword| key.starts_with(keyword)))
                {
                    style = 4 + index as u8;
                    consumed = Some(length);
                    break;
                }
                if let Some(phrase) = language.keywords[index]
                    .iter()
                    .filter(|k| k.contains(char::is_whitespace))
                    .filter_map(|k| matched(text, pos, k, language.ignore_case))
                    .max()
                {
                    style = 4 + index as u8;
                    consumed = Some(phrase);
                    break;
                }
            }
            if consumed.is_none() {
                consumed = Some(length);
            }
        }
        pos += consumed.unwrap_or(ch.len_utf8());
        styles[start..pos].fill(style);
        for offset in start..pos {
            let byte = text.as_bytes()[offset];
            if byte == b'\r'
                || (byte == b'\n' && (offset == 0 || text.as_bytes()[offset - 1] != b'\r'))
            {
                let blank = language.fold_compact && text[line_start..offset].trim().is_empty();
                folds.push(
                    (0x400 + minimum.min(0xbff))
                        | if level > minimum { 0x2000 } else { 0 }
                        | if blank { 0x1000 } else { 0 },
                );
                minimum = level;
                line_start = offset + 1;
            } else if byte == b'\n' {
                line_start = offset + 1;
            }
        }
    }
    folds.push((0x400 + minimum.min(0xbff)) | if level > minimum { 0x2000 } else { 0 });
    if Instant::now() > deadline {
        return Err("Custom highlighting exceeded its time budget.".into());
    }
    Ok(Highlight {
        styles,
        folds,
        strikes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const XML: &str = include_str!("..\\tests\\fixtures\\custom-language.xml");
    #[test]
    fn imports_and_styles_keywords_comments_strings_and_folds() {
        let language = import(XML).unwrap().remove(0);
        let text = "begin\nsay \"hello\\\" world\" // comment\n42\nend\n";
        let styled = highlight(&language, text).unwrap();
        assert_eq!(styled.styles[6], 4);
        assert_eq!(styled.styles[11], 14);
        assert_eq!(styled.styles[text.find("//").unwrap()], 2);
        assert_eq!(styled.styles[text.find("42").unwrap()], 3);
        assert_ne!(styled.folds[0] & 0x2000, 0);
        assert_eq!(styled.folds.len(), 5);
    }
    #[test]
    fn data_only_parser_rejects_entities_and_bad_configuration() {
        assert!(import("<!DOCTYPE x [<!ENTITY x SYSTEM 'file:///secret'>]><x/>").is_err());
        assert!(import(&XML.replace("udlVersion=\"2.1\"", "udlVersion=\"9.9\"")).is_err());
        assert!(indexed("990", 24).is_err());
    }
}
