use crate::core::{MAX_DOCUMENT_BYTES, MAX_TOOL_BYTES, Result};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use std::{collections::HashSet, fmt, ops::Range};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open(char),
    Close,
    Comma,
    Colon,
    Literal,
    LineComment,
    BlockComment,
}
struct Token {
    kind: Kind,
    span: Range<usize>,
}

fn tokens(text: &str) -> Result<Vec<Token>> {
    if text.len() > MAX_TOOL_BYTES {
        return Err("JSON tools are limited to 16 MiB.".into());
    }
    let mut result = Vec::new();
    let mut stack = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let start = pos;
        let ch = text[pos..].chars().next().unwrap();
        if ch.is_whitespace() || ch == '\u{feff}' {
            pos += ch.len_utf8();
            continue;
        }
        let kind = match ch {
            '{' | '[' => {
                if stack.len() == 128 {
                    return Err("JSON nesting exceeds 128 levels.".into());
                }
                stack.push(ch);
                pos += 1;
                Kind::Open(ch)
            }
            '}' | ']' => {
                if stack.pop() != Some(if ch == '}' { '{' } else { '[' }) {
                    return Err(format!("Mismatched JSON delimiter at byte {pos}."));
                }
                pos += 1;
                Kind::Close
            }
            ',' => {
                pos += 1;
                Kind::Comma
            }
            ':' => {
                pos += 1;
                Kind::Colon
            }
            '"' | '\'' => {
                pos += 1;
                let mut closed = false;
                while pos < text.len() {
                    let next = text.as_bytes()[pos];
                    pos += 1;
                    if next == b'\\' {
                        pos = (pos + 1).min(text.len());
                    } else if next == ch as u8 {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err("Unclosed JSON string.".into());
                }
                Kind::Literal
            }
            '/' if text[pos..].starts_with("//") => {
                pos += 2;
                while pos < text.len() && !b"\r\n".contains(&text.as_bytes()[pos]) {
                    pos += 1;
                }
                Kind::LineComment
            }
            '/' if text[pos..].starts_with("/*") => {
                let end = text[pos + 2..]
                    .find("*/")
                    .ok_or("Unclosed JSON block comment.")?;
                pos += end + 4;
                Kind::BlockComment
            }
            _ => {
                pos += ch.len_utf8();
                while pos < text.len() {
                    let next = text[pos..].chars().next().unwrap();
                    if next.is_whitespace()
                        || "{}[],:".contains(next)
                        || text[pos..].starts_with("//")
                        || text[pos..].starts_with("/*")
                    {
                        break;
                    }
                    pos += next.len_utf8();
                }
                Kind::Literal
            }
        };
        result.push(Token {
            kind,
            span: start..pos,
        });
        if result.len() > 500_000 {
            return Err("JSON exceeds the 500,000-token processing limit.".into());
        }
    }
    if !stack.is_empty() {
        return Err("Unclosed JSON container.".into());
    }
    Ok(result)
}

struct UniqueValue;
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Check;
        impl<'de> Visitor<'de> for Check {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a JSON value with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<UniqueValue, E> {
                Ok(UniqueValue)
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<UniqueValue, E> {
                Ok(UniqueValue)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<UniqueValue, E> {
                Ok(UniqueValue)
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<UniqueValue, E> {
                Ok(UniqueValue)
            }
            fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<UniqueValue, E> {
                Ok(UniqueValue)
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<UniqueValue, E> {
                Ok(UniqueValue)
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut values: A,
            ) -> std::result::Result<UniqueValue, A::Error> {
                while values.next_element::<UniqueValue>()?.is_some() {}
                Ok(UniqueValue)
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut values: A,
            ) -> std::result::Result<UniqueValue, A::Error> {
                let mut keys = HashSet::new();
                while let Some(key) = values.next_key::<String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(de::Error::custom(format!(
                            "Duplicate JSON key '{}'.",
                            key.chars().take(80).collect::<String>()
                        )));
                    }
                    values.next_value::<UniqueValue>()?;
                }
                Ok(UniqueValue)
            }
        }
        deserializer.deserialize_any(Check)
    }
}

/// Returns true for JSON5 and false for strict JSON. Values are validated, never evaluated.
pub fn validate(text: &str) -> Result<bool> {
    let parts = tokens(text)?;
    if serde_json::from_str::<UniqueValue>(text).is_ok() {
        return Ok(false);
    }
    json5::Deserializer::from_str(text).map_err(|e| format!("Invalid JSON/JSON5: {e}"))?;
    // The JSON5 deserializer narrows integers. Validate keys using a grammar-checked
    // copy with neutral number values; formatting and source spans retain the original lexemes.
    let mut validation = text.as_bytes().to_vec();
    for (index, token) in parts.iter().enumerate() {
        let raw = &text[token.span.clone()];
        let key = parts[index + 1..]
            .iter()
            .find(|next| !matches!(next.kind, Kind::LineComment | Kind::BlockComment))
            .is_some_and(|next| next.kind == Kind::Colon);
        if token.kind == Kind::Literal
            && !key
            && (raw.starts_with(['+', '-', '.'])
                || raw.as_bytes().first().is_some_and(u8::is_ascii_digit)
                || matches!(raw, "Infinity" | "NaN"))
        {
            validation[token.span.clone()].fill(b' ');
            validation[token.span.start] = b'0';
        }
    }
    let validation =
        String::from_utf8(validation).expect("Only ASCII numeric lexemes are replaced");
    json5::from_str::<UniqueValue>(&validation).map_err(|e| format!("Invalid JSON/JSON5: {e}"))?;
    Ok(true)
}

pub fn format(text: &str, compact: bool) -> Result<String> {
    validate(text)?;
    let tokens = tokens(text)?;
    let mut out = String::new();
    let mut depth = 0;
    let newline = |out: &mut String, depth: usize| {
        let tail = out.rfind('\n').map_or(0, |p| p + 1);
        if out[tail..].chars().all(char::is_whitespace) {
            out.truncate(tail);
        } else if !out.ends_with('\n') {
            out.push('\n');
        }
        if !compact {
            out.push_str(&"  ".repeat(depth));
        }
    };
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            Kind::Open(_) => {
                out.push_str(&text[token.span.clone()]);
                depth += 1;
                if !compact && tokens.get(index + 1).is_some_and(|t| t.kind != Kind::Close) {
                    newline(&mut out, depth);
                }
            }
            Kind::Close => {
                depth = depth.saturating_sub(1);
                if !compact && index > 0 && !matches!(tokens[index - 1].kind, Kind::Open(_)) {
                    newline(&mut out, depth);
                }
                out.push_str(&text[token.span.clone()]);
            }
            Kind::Comma => {
                out.push(',');
                if !compact {
                    newline(&mut out, depth);
                }
            }
            Kind::Colon => out.push_str(if compact { ":" } else { ": " }),
            Kind::LineComment | Kind::BlockComment => {
                if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                    out.push(' ');
                }
                out.push_str(&text[token.span.clone()]);
                if token.kind == Kind::LineComment || !compact {
                    newline(&mut out, depth);
                } else {
                    out.push(' ');
                }
            }
            Kind::Literal => out.push_str(&text[token.span.clone()]),
        }
        if out.len() > MAX_DOCUMENT_BYTES {
            return Err("Formatted JSON exceeds the document size limit.".into());
        }
    }
    Ok(out.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json5_formatting_keeps_comments_big_numbers_and_literals() {
        let text = "{/* keep */ name:'hello', value:123456789012345678901234567890, hex:0xFF, missing:NaN, list:[1,2,],}";
        let formatted = format(text, false).unwrap();
        for literal in [
            "/* keep */",
            "'hello'",
            "123456789012345678901234567890",
            "0xFF",
            "NaN",
        ] {
            assert!(formatted.contains(literal));
        }
        assert!(validate(&formatted).unwrap());
        assert!(validate("{name:1,name:2}").is_err());
        assert!(validate(&format!("{}0{}", "[".repeat(129), "]".repeat(129))).is_err());
    }
    #[test]
    fn formatting_is_not_limited_by_tree_node_count() {
        let text = format!("[{}]", vec!["0"; 25_000].join(","));
        assert_eq!(format(&text, true).unwrap(), text);
        let comments = "{x:1,// keep\n y:2}";
        assert!(validate(&format(comments, true).unwrap()).is_ok());
    }
}
