use crate::languages::{STYLE_HINTS, STYLE_SYMBOLS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Text,
    Comment,
    String,
    Keyword,
    Number,
    Directive,
    Operator,
    Type,
    Function,
    Attribute,
    Tag,
    Variable,
    Constant,
    Error,
    Heading,
    Strong,
    Emphasis,
    Link,
    Code,
    Quote,
    Marker,
    Muted,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    pub role: Role,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub shaded: bool,
}

pub fn role(description: &str) -> Option<Role> {
    let lower = description.to_ascii_lowercase();
    let words: Vec<_> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let has = |names: &[&str]| words.iter().any(|word| names.contains(word));
    if has(&[
        "comment",
        "comments",
        "commentline",
        "commentblock",
        "commentdoc",
        "commentlinedoc",
        "commentblockdoc",
    ]) {
        Some(Role::Comment)
    } else if has(&["error", "errors", "stringeol", "lexerror"]) {
        Some(Role::Error)
    } else if words
        .iter()
        .any(|word| word.starts_with("header") || word.starts_with("heading"))
    {
        Some(Role::Heading)
    } else if has(&["strong", "strong1", "strong2", "bold"]) {
        Some(Role::Strong)
    } else if has(&["emphasis", "italic", "italics", "em1", "em2"]) {
        Some(Role::Emphasis)
    } else if has(&["link", "url", "uri"]) {
        Some(Role::Link)
    } else if has(&["code", "code2", "codebk", "codeblock", "codespan"]) {
        Some(Role::Code)
    } else if has(&["blockquote", "quote"]) {
        Some(Role::Quote)
    } else if has(&["hrule", "ulist", "olist", "bullet", "listitem"]) {
        Some(Role::Marker)
    } else if has(&["strikeout", "strikethrough"]) {
        Some(Role::Muted)
    } else if has(&[
        "string",
        "strings",
        "character",
        "char",
        "stringraw",
        "rawstring",
        "verbatim",
        "regex",
        "triple",
        "tripledouble",
        "fstring",
        "fcharacter",
        "ftriple",
        "ftripledouble",
    ]) || words
        .iter()
        .any(|word| word.ends_with("string") || word.ends_with("quotedstring"))
    {
        Some(Role::String)
    } else if has(&["preprocessor", "preproc", "macro", "decorator", "directive"]) {
        Some(Role::Directive)
    } else if has(&[
        "classname",
        "class",
        "type",
        "typename",
        "typeword",
        "types",
    ]) {
        Some(Role::Type)
    } else if has(&["defname", "function", "functionname", "method", "command"]) {
        Some(Role::Function)
    } else if has(&[
        "property",
        "propertyname",
        "attribute",
        "attributes",
        "attr",
    ]) {
        Some(Role::Attribute)
    } else if has(&["tag", "tagend", "tagunknown", "entity"]) {
        Some(Role::Tag)
    } else if has(&["number", "numbers", "numeric", "hexnumber", "binnumber"]) {
        Some(Role::Number)
    } else if words.iter().any(|word| {
        word.starts_with("keyword")
            || *word == "reserved"
            || *word == "word"
            || matches!(
                *word,
                "word2" | "word3" | "word4" | "word5" | "word6" | "word7" | "word8"
            )
    }) {
        Some(Role::Keyword)
    } else if has(&[
        "operator",
        "operators",
        "operator2",
        "symbol",
        "punctuation",
    ]) {
        Some(Role::Operator)
    } else if has(&[
        "variable",
        "scalar",
        "array",
        "hash",
        "global",
        "instancevar",
        "classvar",
    ]) {
        Some(Role::Variable)
    } else if has(&["constant", "label", "labelname", "value"]) {
        Some(Role::Constant)
    } else {
        None
    }
}

pub fn styles(lexer: &str) -> [Style; 256] {
    let mut styles = [Style::default(); 256];
    for (name, id, description, properties) in STYLE_HINTS {
        if !name.eq_ignore_ascii_case(lexer) {
            continue;
        }
        let style = &mut styles[*id];
        style.role = role(description)
            .or_else(|| role(properties))
            .unwrap_or_default();
        for property in properties.split(',').map(str::trim) {
            match property {
                "bold" => style.bold = true,
                "notbold" => style.bold = false,
                "italics" => style.italic = true,
                "notitalics" => style.italic = false,
                "underlined" => style.underline = true,
                "notunderlined" => style.underline = false,
                _ => {}
            }
        }
    }
    for (name, id, symbol) in STYLE_SYMBOLS {
        if *id >= 256 || (32..=39).contains(id) || !name.eq_ignore_ascii_case(lexer) {
            continue;
        }
        if let Some(role) = role(symbol) {
            styles[*id].role = role;
        }
    }
    if lexer == "css" {
        for id in [6, 7, 15, 17, 19] {
            styles[id].role = Role::Attribute;
        }
        for id in [3, 4, 16, 20] {
            styles[id].role = Role::Directive;
        }
    }
    if lexer == "rust" {
        styles[7].role = Role::Type;
    }
    styles
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_do_not_erase_richer_semantics() {
        assert_eq!(role("identifier"), None);
        assert_eq!(styles("python")[8].role, Role::Type);
        assert_eq!(styles("python")[9].role, Role::Function);
        assert_eq!(styles("json")[4].role, Role::Attribute);
        assert_eq!(styles("css")[6].role, Role::Attribute);
        assert_eq!(styles("hypertext")[120].role, Role::String);
    }
}
