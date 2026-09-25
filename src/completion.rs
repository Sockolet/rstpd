use crate::core::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeSet, ops::Range, rc::Rc, sync::OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Api {
    pub language: String,
    pub receiver: String,
    pub name: String,
    pub signature: String,
}

#[derive(Debug)]
pub struct Suggestions {
    pub entered: usize,
    pub words: Vec<String>,
}
#[derive(Debug)]
pub struct CallTip {
    pub anchor: usize,
    pub signature: String,
    pub parameter: Range<usize>,
}

fn language_key(language: &str) -> &str {
    match language {
        "Rust" => "rust",
        "Python" => "python",
        "JavaScript" | "TypeScript" | "ActionScript" => "javascript",
        "C++" => "cpp",
        "C" => "c",
        "C#" => "csharp",
        "Java" => "java",
        "Go" => "go",
        "Lua" => "lua",
        "PHP" => "php",
        "Ruby" => "ruby",
        "SQL" | "MS SQL" => "sql",
        other => other,
    }
}
fn builtins() -> &'static [Api] {
    static APIS: OnceLock<Vec<Api>> = OnceLock::new();
    APIS.get_or_init(|| {
        include_str!("..\\assets\\completion.api")
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '|');
                let language = parts.next()?;
                let receiver = parts.next()?;
                let signature = parts.next()?;
                Some(Api {
                    language: language.into(),
                    receiver: receiver.into(),
                    name: signature.split('(').next().unwrap_or(signature).into(),
                    signature: signature.into(),
                })
            })
            .collect()
    })
}
fn identifier(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Everything derived from one editor context window. Completion and call-tips are
/// raised back-to-back for the same caret, so this is computed once and reused.
struct Analysis {
    masked: String,
    scopes: Vec<(String, Range<usize>)>,
    functions: Vec<Api>,
}

fn analysis(text: &str, language: &str) -> Rc<Analysis> {
    thread_local! {
        static CACHE: RefCell<Option<(String, String, Rc<Analysis>)>> = const { RefCell::new(None) };
    }
    let cached = CACHE.with_borrow(|cache| {
        cache
            .as_ref()
            .filter(|(cached_text, cached_language, _)| {
                cached_text == text && cached_language == language
            })
            .map(|(_, _, analysis)| analysis.clone())
    });
    if let Some(analysis) = cached {
        return analysis;
    }
    let masked = code_mask(text, language);
    let scopes = class_scopes(&masked, language);
    let functions = functions_in(text, &masked, &scopes, language);
    let analysis = Rc::new(Analysis {
        masked,
        scopes,
        functions,
    });
    CACHE.with_borrow_mut(|cache| {
        *cache = Some((text.to_owned(), language.to_owned(), analysis.clone()));
    });
    analysis
}

// Keep byte positions unchanged so the lexer, signatures and UTF-8 editor use the same offsets.
fn code_mask(text: &str, language: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = bytes.to_vec();
    let mut i = 0;
    let hash_comment = matches!(
        language_key(language),
        "python" | "ruby" | "Bash" | "PowerShell" | "Perl"
    );
    while i < bytes.len() {
        let start = i;
        if bytes[i..].starts_with(b"//") || (hash_comment && bytes[i] == b'#') {
            while i < bytes.len() && !b"\r\n".contains(&bytes[i]) {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if matches!(bytes[i], b'"' | b'\'' | b'`') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == quote {
                    i += 1;
                    break;
                } else if b"\r\n".contains(&bytes[i]) && quote != b'`' {
                    break;
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
            continue;
        }
        for b in &mut result[start..i] {
            if !b"\r\n".contains(b) {
                *b = b' ';
            }
        }
    }
    String::from_utf8(result).expect("ASCII masks preserve UTF-8 boundaries")
}

fn functions_in(
    text: &str,
    masked: &str,
    owners: &[(String, Range<usize>)],
    language: &str,
) -> Vec<Api> {
    static NAMES: OnceLock<Regex> = OnceLock::new();
    let pattern=NAMES.get_or_init(||Regex::new(r"([\p{XID_Start}_$][\p{XID_Continue}$]*(?:[.:]{1,2}[\p{XID_Start}_$][\p{XID_Continue}$]*)*)\s*(?:<[^>\n]{0,200}>)?\s*\(").unwrap());
    let mut result = Vec::new();
    for captures in pattern.captures_iter(masked).take(2000) {
        let matched = captures.get(0).unwrap();
        let full_name = captures.get(1).unwrap().as_str();
        let name = full_name.rsplit(['.', ':']).next().unwrap();
        if matches!(
            name,
            "if" | "for" | "while" | "switch" | "match" | "catch" | "sizeof" | "return"
        ) {
            continue;
        }
        let open = matched.end() - 1;
        let Some(close) = matching_close(masked, open) else {
            continue;
        };
        if close - open > 2048 {
            continue;
        }
        let line_start = masked[..matched.start()].rfind('\n').map_or(0, |p| p + 1);
        let before = masked[line_start..matched.start()].trim();
        let after = masked[close + 1..].trim_start();
        let declared = before.split_whitespace().any(|word| {
            matches!(
                word,
                "fn" | "def" | "function" | "func" | "sub" | "Sub" | "Function"
            )
        }) || (!before.contains('=')
            && !before.starts_with("return")
            && (after.starts_with('{')
                || after.starts_with("=>")
                || after.starts_with("->")
                || (!before.is_empty() && after.starts_with(';'))));
        if !declared {
            continue;
        }
        let signature = format!("{name}{}", &text[open..=close])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace('\0', " ");
        let receiver = full_name
            .strip_suffix(name)
            .unwrap_or("")
            .trim_end_matches(['.', ':']);
        let receiver = if receiver.is_empty() {
            owners
                .iter()
                .filter(|(_, range)| range.contains(&matched.start()))
                .min_by_key(|(_, range)| range.len())
                .map(|(name, _)| name.as_str())
                .unwrap_or("")
        } else {
            receiver
        };
        result.push(Api {
            language: language_key(language).into(),
            receiver: receiver.into(),
            name: name.into(),
            signature,
        });
    }
    if matches!(language_key(language), "javascript") {
        static ARROWS: OnceLock<Regex> = OnceLock::new();
        let arrows = ARROWS.get_or_init(|| {
            Regex::new(
                r"\b(?:const|let|var)\s+([\w$]+)\s*=\s*(?:async\s*)?\(([^)\n]{0,2048})\)\s*=>",
            )
            .unwrap()
        });
        for captures in arrows.captures_iter(masked) {
            let args = captures.get(2).unwrap();
            result.push(Api {
                language: language_key(language).into(),
                receiver: String::new(),
                name: captures[1].into(),
                signature: format!("{}({})", &captures[1], &text[args.range()]),
            });
        }
    }
    result
}

fn class_scopes(masked: &str, language: &str) -> Vec<(String, Range<usize>)> {
    static CLASSES: OnceLock<Regex> = OnceLock::new();
    let pattern = CLASSES.get_or_init(|| {
        Regex::new(
            r"\b(class|struct|impl)\b\s*(?:<[^>\n]+>\s*)?([\p{XID_Start}_][\p{XID_Continue}]*)",
        )
        .unwrap()
    });
    let mut result = Vec::new();
    for captures in pattern.captures_iter(masked) {
        let matched = captures.get(0).unwrap();
        let mut name = captures[2].to_owned();
        if language_key(language) == "python" {
            let line_start = masked[..matched.start()].rfind('\n').map_or(0, |p| p + 1);
            let indent = masked[line_start..matched.start()].len();
            let begin = masked[matched.end()..]
                .find('\n')
                .map_or(masked.len(), |i| matched.end() + i + 1);
            let mut end = masked.len();
            let mut pos = begin;
            for line in masked[begin..].split_inclusive('\n') {
                if !line.trim().is_empty() && line.len() - line.trim_start().len() <= indent {
                    end = pos;
                    break;
                }
                pos += line.len();
            }
            result.push((name, begin..end));
        } else if let Some(offset) = masked[matched.end()..].find('{') {
            let open = matched.end() + offset;
            let header = &masked[matched.end()..open];
            if header.contains(';')
                || header
                    .split_whitespace()
                    .any(|word| matches!(word, "class" | "struct" | "impl"))
            {
                continue;
            }
            if captures.get(1).unwrap().as_str() == "impl"
                && let Some((_, tail)) = header.split_once(" for ")
            {
                name = tail
                    .trim()
                    .chars()
                    .take_while(|ch| identifier(*ch))
                    .collect();
            }
            let mut depth = 0;
            let mut end = masked.len();
            for (offset, ch) in masked[open..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end = open + offset;
                        break;
                    }
                }
            }
            result.push((name, open + 1..end));
        }
    }
    result
}

fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn receiver_type(text: &str, analysis: &Analysis, receiver: &str, caret: usize) -> Option<String> {
    if matches!(receiver, "self" | "Self" | "this") {
        return analysis
            .scopes
            .iter()
            .filter(|(_, range)| range.contains(&caret))
            .min_by_key(|(_, range)| range.len())
            .map(|(name, _)| name.clone());
    }
    if receiver.ends_with(['"', '\'']) {
        return Some("string".into());
    }
    if receiver.ends_with(']') {
        return Some("array".into());
    }
    if receiver.is_empty() || receiver.len() > 256 {
        return None;
    }
    let variable = regex::escape(receiver);
    let assignment = Regex::new(&format!(
        r"(?m)\b{variable}[ \t]*(?::[^=\n]+)?=(?:[^=>\n]|$)"
    ))
    .expect("escaped bounded identifier");
    let annotation_pattern = Regex::new(&format!(
        r"\b{variable}[ \t]*:[ \t]*&?(?:mut[ \t]+)?([\w]+)|\b([\w]+)(?:<[^>]+>)?[ \t]+{variable}\b"
    ))
    .expect("escaped identifier forms a valid annotation expression");
    let normalize = |kind: &str| match kind {
        "String" | "string" | "str" | "QString" => "string".into(),
        "Vec" | "vector" | "List" | "Array" | "ArrayList" | "list" | "slice" => "array".into(),
        "HashMap" | "Dictionary" | "dict" | "Map" => "map".into(),
        other => other.to_owned(),
    };
    let binding = assignment.find_iter(&analysis.masked[..caret]).last();
    let annotation = annotation_pattern
        .captures_iter(&analysis.masked[..caret])
        .filter(|caps| {
            let kind = caps.get(1).or(caps.get(2)).unwrap().as_str();
            !matches!(kind, "let" | "const" | "var" | "mut" | "return" | "if")
        })
        .last();
    if let Some(caps) = annotation
        && binding
            .as_ref()
            .is_none_or(|binding| caps.get(0).unwrap().end() >= binding.start())
    {
        let kind = caps.get(1).or(caps.get(2)).unwrap().as_str();
        return Some(normalize(kind));
    }
    if let Some(binding) = binding {
        let equals = analysis.masked[..binding.end()]
            .rfind('=')
            .map_or(binding.end(), |offset| offset + 1);
        let rhs = text[equals..caret].trim_start().trim_start_matches("new ");
        if rhs.starts_with(['"', '\'', '`']) {
            return Some("string".into());
        }
        if rhs.starts_with('[') || rhs.starts_with("vec![") {
            return Some("array".into());
        }
        if rhs.starts_with('{') {
            return Some("map".into());
        }
        let kind = rhs.split(|ch: char| !identifier(ch)).next().unwrap_or("");
        if !kind.is_empty() {
            return Some(normalize(kind));
        }
    }
    None
}

fn prefix(text: &str, caret: usize) -> (usize, Option<String>) {
    let start = text[..caret]
        .char_indices()
        .rev()
        .find(|(_, ch)| !identifier(*ch))
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    let before = text[..start].trim_end();
    let receiver = before
        .strip_suffix('.')
        .or_else(|| before.strip_suffix("::"))
        .or_else(|| before.strip_suffix("->"))
        .map(|s| {
            let s = s.trim_end();
            if s.ends_with(['"', '\'', '`']) {
                return "\"".into();
            }
            if s.ends_with(']') {
                return "]".into();
            }
            s.rsplit(|ch: char| !identifier(ch) && !matches!(ch, '.' | ':'))
                .next()
                .unwrap_or("")
                .to_owned()
        });
    (start, receiver)
}

fn receiver_matches(api: &Api, receiver: Option<&str>, kind: Option<&str>) -> bool {
    match receiver {
        None => api.receiver.is_empty(),
        Some(name) => {
            !api.receiver.is_empty()
                && (api.receiver == name
                    || kind == Some(api.receiver.as_str())
                    || name.rsplit(['.', ':']).next() == Some(api.receiver.as_str()))
        }
    }
}

pub fn suggestions(
    text: &str,
    caret: usize,
    language: &str,
    keywords: &[String],
    extra: &[Api],
    manual: bool,
) -> Suggestions {
    let caret = caret.min(text.len());
    if !text.is_char_boundary(caret) {
        return Suggestions {
            entered: 0,
            words: Vec::new(),
        };
    }
    let (start, receiver) = prefix(text, caret);
    let entered = &text[start..caret];
    if !manual && entered.chars().count() < 2 {
        return Suggestions {
            entered: entered.len(),
            words: Vec::new(),
        };
    }
    let lower = entered.to_lowercase();
    let mut words = BTreeSet::new();
    let mut add = |word: &str| {
        if word.len() > entered.len()
            && word.len() < 128
            && word.to_lowercase().starts_with(&lower)
            && word.chars().all(|ch| identifier(ch) || ch == '!')
            && words.len() < 300
        {
            words.insert(word.to_owned());
        }
    };
    let analysis = analysis(text, language);
    let kind = receiver
        .as_deref()
        .and_then(|name| receiver_type(text, &analysis, name, caret));
    for api in analysis.functions.iter().chain(extra).chain(builtins()) {
        if api.language != language_key(language) && api.language != language {
            continue;
        }
        let applicable = receiver_matches(api, receiver.as_deref(), kind.as_deref());
        if applicable {
            add(&api.name);
        }
    }
    if let Some(receiver) = receiver {
        let member = Regex::new(&format!(
            r"\b{}\s*(?:\.|->|::)\s*([\p{{XID_Start}}_$][\p{{XID_Continue}}$]*)",
            regex::escape(&receiver)
        ))
        .unwrap();
        for captures in member.captures_iter(&analysis.masked) {
            add(&captures[1]);
        }
    } else {
        for word in keywords.iter().flat_map(|set| set.split_whitespace()) {
            add(word);
        }
        for word in analysis.masked.split(|ch: char| !identifier(ch)) {
            add(word);
        }
    }
    Suggestions {
        entered: entered.len(),
        words: words.into_iter().collect(),
    }
}

pub fn call_tip(text: &str, caret: usize, language: &str, extra: &[Api]) -> Option<CallTip> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let prefix_mask = code_mask(&text[..caret], language);
    let mut stack = Vec::new();
    for (pos, ch) in prefix_mask.char_indices() {
        match ch {
            '(' => stack.push(pos),
            ')' => {
                stack.pop();
            }
            _ => {}
        }
    }
    let open = *stack.last()?;
    let analysis = analysis(text, language);
    let name_end = text[..open].trim_end().len();
    let identifier_end = if text[..name_end].ends_with('!') {
        name_end - 1
    } else {
        name_end
    };
    let (start, receiver) = prefix(text, identifier_end);
    let name = text[start..name_end].trim();
    if name.is_empty() {
        return None;
    }
    let kind = receiver
        .as_deref()
        .and_then(|name| receiver_type(text, &analysis, name, caret));
    let api = analysis
        .functions
        .iter()
        .chain(extra)
        .chain(builtins())
        .find(|api| {
            api.name == name
                && (api.language == language_key(language) || api.language == language)
                && receiver_matches(api, receiver.as_deref(), kind.as_deref())
        })?;
    let mut argument = 0;
    let mut depth = 0;
    let mut generics = 0usize;
    let mut previous = ['\0'; 2];
    for ch in prefix_mask[open + 1..].chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            // Only turbofish `::<` opens generics; a bare `<` is usually a comparison.
            '<' if previous == [':', ':'] => generics += 1,
            '>' if generics > 0 => generics -= 1,
            ',' if depth == 0 && generics == 0 => argument += 1,
            _ => {}
        }
        previous = [previous[1], ch];
    }
    let signature_mask = code_mask(&api.signature, language);
    let begin = api.signature.find('(')? + 1;
    let end = matching_close(&signature_mask, begin - 1)?;
    let mut spans = Vec::new();
    let mut part = begin;
    depth = 0;
    for (offset, ch) in signature_mask[begin..end].char_indices() {
        let pos = begin + offset;
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => {
                spans.push(part..pos);
                part = pos + 1;
            }
            _ => {}
        }
    }
    spans.push(part..end);
    let parameter = spans
        .get(argument)
        .or_else(|| spans.last())
        .cloned()
        .unwrap_or(begin..end);
    Some(CallTip {
        anchor: start,
        signature: api.signature.clone(),
        parameter,
    })
}

pub fn import_api(xml: &str, language: &str) -> Result<Vec<Api>> {
    if xml.len() > 1024 * 1024 {
        return Err("Completion definitions are limited to 1 MiB.".into());
    }
    let document = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            nodes_limit: 25_000,
            ..Default::default()
        },
    )
    .map_err(|error| format!("Invalid completion XML: {error}"))?;
    let mut result = Vec::new();
    for keyword in document
        .descendants()
        .filter(|node| node.has_tag_name("KeyWord"))
    {
        let name = keyword
            .attribute("name")
            .ok_or("Completion keyword is missing its name.")?;
        if name.len() > 256 || name.contains('\0') {
            return Err("Invalid completion name.".into());
        }
        let overloads: Vec<_> = keyword
            .children()
            .filter(|node| node.has_tag_name("Overload"))
            .collect();
        for overload in overloads {
            let params: Vec<_> = overload
                .children()
                .filter(|node| node.has_tag_name("Param"))
                .filter_map(|node| node.attribute("name"))
                .collect();
            let (receiver, short) = name
                .rsplit_once('.')
                .or_else(|| name.rsplit_once("::"))
                .unwrap_or(("", name));
            let signature = format!(
                "{}({}) -> {}",
                short,
                params.join(", "),
                overload.attribute("retVal").unwrap_or("")
            );
            if signature.len() > 4096 {
                return Err("Completion signature exceeds 4 KiB.".into());
            }
            result.push(Api {
                language: language_key(language).into(),
                receiver: receiver.into(),
                name: short.into(),
                signature,
            });
        }
    }
    if result.is_empty() {
        return Err("No function overloads found in the completion XML.".into());
    }
    if result.len() > 10_000 {
        return Err("Completion definitions are limited to 10,000 overloads.".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signatures_and_context_sensitive_members() {
        let text = "def greet(name, count=1):\n    pass\n\ngre";
        assert!(
            suggestions(text, text.len(), "Python", &[], &[], false)
                .words
                .contains(&"greet".into())
        );
        let text = "text = 'hello'\ntext.sp";
        let words = suggestions(text, text.len(), "Python", &[], &[], false).words;
        assert!(words.contains(&"split".into()));
        assert!(!words.contains(&"append".into()));
        let text = "values = []\nvalues.ap";
        assert!(
            suggestions(text, text.len(), "Python", &[], &[], false)
                .words
                .contains(&"append".into())
        );
        let text = "fn greet(name: &str, count: usize) {}\ngreet(\"x,y\", ";
        let tip = call_tip(text, text.len(), "Rust", &[]).unwrap();
        assert_eq!(tip.signature[tip.parameter].trim(), "count: usize");
    }
    #[test]
    fn excludes_strings_and_imports_data_without_entities() {
        let text = "// pretend secretToken\n\"anotherSecret\"\nsec";
        assert!(
            !suggestions(text, text.len(), "Rust", &[], &[], false)
                .words
                .contains(&"secretToken".into())
        );
        assert!(import_api("<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///x'>]><x/>", "Rust").is_err());
        let api=import_api("<NotepadPlus><AutoComplete><KeyWord name='launch'><Overload retVal='bool'><Param name='target'/></Overload></KeyWord></AutoComplete></NotepadPlus>","Rust").unwrap();
        let tip = call_tip("launch(", 7, "Rust", &api).unwrap();
        assert_eq!(tip.signature, "launch(target) -> bool");
    }
    #[test]
    fn local_class_methods_arrow_functions_and_macro_hints() {
        let rust = "struct Task {}\nimpl Task { fn finish(&self, force: bool) {} }\nlet task: Task = Task {};\ntask.fi";
        assert!(
            suggestions(rust, rust.len(), "Rust", &[], &[], false)
                .words
                .contains(&"finish".into())
        );
        let python = "class Task:\n    def finish(self, force=False):\n        pass\n\ntask = Task()\ntask.fi";
        assert!(
            suggestions(python, python.len(), "Python", &[], &[], false)
                .words
                .contains(&"finish".into())
        );
        let javascript = "const multiply = (left, right) => left * right;\nmultiply (1, ";
        assert_eq!(
            call_tip(javascript, javascript.len(), "JavaScript", &[])
                .unwrap()
                .signature,
            "multiply(left, right)"
        );
        assert!(call_tip("println!(\"{}\", ", 15, "Rust", &[]).is_some());
    }

    #[test]
    fn bundled_completion_api_is_well_formed() {
        let raw = include_str!("..\\assets\\completion.api");
        for (number, line) in raw.lines().enumerate() {
            assert_eq!(
                line.split('|').count().min(3),
                3,
                "assets\\completion.api line {}: expected 'language|receiver|signature'",
                number + 1
            );
            assert!(
                line.splitn(3, '|').nth(2).is_some_and(|s| s.contains('(')),
                "assets\\completion.api line {}: signature has no argument list",
                number + 1
            );
        }
        assert_eq!(builtins().len(), raw.lines().count());
    }

    #[test]
    fn analysis_cache_does_not_leak_between_contexts() {
        let first = "fn alpha_one(x: u32) {}\nal";
        let second = "fn beta_two(x: u32) {}\nbe";
        for _ in 0..3 {
            let a = suggestions(first, first.len(), "Rust", &[], &[], false).words;
            assert!(a.contains(&"alpha_one".into()), "{a:?}");
            assert!(!a.contains(&"beta_two".into()), "{a:?}");
            let b = suggestions(second, second.len(), "Rust", &[], &[], false).words;
            assert!(b.contains(&"beta_two".into()), "{b:?}");
            assert!(!b.contains(&"alpha_one".into()), "{b:?}");
        }
        let once = suggestions(first, first.len(), "Rust", &[], &[], false).words;
        let twice = suggestions(first, first.len(), "Rust", &[], &[], false).words;
        assert_eq!(once, twice);
    }

    #[test]
    fn analysis_cache_is_keyed_by_language() {
        // Identical text, different language: `strip` is a Python string method and
        // `trim` is a Rust one, so a cache keyed only on the text would leak between them.
        let text = "s = \"abc\"\ns.";
        for _ in 0..3 {
            let python = suggestions(text, text.len(), "Python", &[], &[], true).words;
            assert!(python.contains(&"strip".into()), "{python:?}");
            assert!(!python.contains(&"trim".into()), "{python:?}");
            let rust = suggestions(text, text.len(), "Rust", &[], &[], true).words;
            assert!(rust.contains(&"trim".into()), "{rust:?}");
            assert!(!rust.contains(&"strip".into()), "{rust:?}");
        }
    }

    #[test]
    fn turbofish_commas_and_comparisons_do_not_shift_call_tip_arguments() {
        let text = "fn f(a: i32, b: i32, c: i32) {}\nf(g::<A, B>(), ";
        let tip = call_tip(text, text.len(), "Rust", &[]).unwrap();
        assert_eq!(tip.signature[tip.parameter].trim(), "b: i32");
        let text = "fn f(a: i32, b: i32, c: i32) {}\nf(x < y, ";
        let tip = call_tip(text, text.len(), "Rust", &[]).unwrap();
        assert_eq!(tip.signature[tip.parameter].trim(), "b: i32");
    }

    #[test]
    fn comparisons_do_not_replace_annotated_receiver_types() {
        let text = "let text: String = String::new();\nif text == \"\" {}\ntext.tr";
        let words = suggestions(text, text.len(), "Rust", &[], &[], false).words;
        assert!(words.contains(&"trim".into()), "{words:?}");
    }
}
