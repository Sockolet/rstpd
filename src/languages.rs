use crate::udl::UserLanguage;
use std::{path::Path, sync::Arc};

include!(concat!(env!("OUT_DIR"), "/language_data.rs"));
pub const COVERAGE: &str = include_str!("..\\assets\\language-coverage.psv");

#[derive(Clone, Debug)]
pub struct Language {
    pub name: String,
    pub lexer: String,
    pub extensions: String,
    pub sample: String,
    pub custom: Option<Arc<UserLanguage>>,
}

pub struct MenuGroup {
    pub label: &'static str,
    pub indices: Vec<usize>,
}

pub fn menu_groups(languages: &[Language]) -> Vec<MenuGroup> {
    let mut groups: Vec<_> = [
        "&A-C",
        "&D-F",
        "&G-I",
        "&J-L",
        "&M-O",
        "&P-R",
        "&S-U",
        "&V-Z",
        "&Other",
        "&User-defined",
    ]
    .into_iter()
    .map(|label| MenuGroup {
        label,
        indices: Vec::new(),
    })
    .collect();
    for (index, language) in languages.iter().enumerate() {
        if language.name == "Plain text" && language.custom.is_none() {
            continue;
        }
        let group = if language.custom.is_some() {
            9
        } else {
            match language
                .name
                .chars()
                .next()
                .map(|ch| ch.to_ascii_uppercase())
            {
                Some('A'..='C') => 0,
                Some('D'..='F') => 1,
                Some('G'..='I') => 2,
                Some('J'..='L') => 3,
                Some('M'..='O') => 4,
                Some('P'..='R') => 5,
                Some('S'..='U') => 6,
                Some('V'..='Z') => 7,
                _ => 8,
            }
        };
        groups[group].indices.push(index);
    }
    for group in &mut groups {
        group
            .indices
            .sort_by_cached_key(|index| languages[*index].name.to_lowercase());
    }
    groups.retain(|group| !group.indices.is_empty());
    groups
}

impl Language {
    pub fn uses_container(&self) -> bool {
        self.custom.is_some() || self.lexer == "markdown"
    }
    pub fn highlight(&self, text: &str) -> crate::core::Result<crate::core::Highlight> {
        if let Some(definition) = &self.custom {
            crate::udl::highlight(definition, text)
        } else if self.lexer == "markdown" {
            crate::markdown::highlight(text)
        } else {
            Err("This language is styled by its native lexer.".into())
        }
    }
}

// Language aliases are app-owned data; all engine implementations stay upstream.
pub const CATALOG: &[(&str, &str, &str)] = &[
    ("Plain text", "null", "txt log"),
    ("ANSI escape sequences", "escseq", "ans"),
    ("Ada", "ada", "ads adb ada"),
    ("ActionScript", "cpp", "as"),
    ("ASN.1", "asn1", "mib asn asn1"),
    ("ASP", "hypertext", "asp aspx"),
    ("Assembly", "asm", "asm s"),
    ("Assembly (Motorola)", "a68k", "a68"),
    ("AutoIt", "au3", "au3"),
    ("AviSynth", "avs", "avs avsi"),
    ("BaanC", "baan", "bc cln"),
    ("Bash", "bash", "sh bash zsh"),
    ("Batch", "batch", "bat cmd"),
    ("BlitzBasic", "blitzbasic", "bb"),
    ("C", "cpp", "c h"),
    ("C++", "cpp", "cpp cxx cc hpp hxx hh ino"),
    ("C#", "cpp", "cs"),
    ("Caml / OCaml", "caml", "ml mli"),
    ("CMake", "cmake", "cmake"),
    ("COBOL", "COBOL", "cob cbl cbd cdb cdc cpy copy"),
    ("CoffeeScript", "coffeescript", "coffee"),
    ("CSS", "css", "css"),
    ("Csound", "csound", "orc sco csd"),
    ("D", "d", "d"),
    ("Dart", "dart", "dart"),
    ("Diff", "diff", "diff patch"),
    ("Erlang", "erlang", "erl hrl"),
    ("Error list", "errorlist", "err"),
    ("EScript", "escript", "em src"),
    ("F#", "fsharp", "fs fsx fsi"),
    ("Forth", "forth", "forth fth"),
    ("Fortran", "fortran", "f90 f95 f03 f08"),
    ("Fortran 77", "f77", "f for"),
    ("FreeBasic", "freebasic", "bas"),
    ("GDScript", "gdscript", "gd"),
    ("Go", "cpp", "go"),
    ("Gui4Cli", "gui4cli", "gui gc"),
    ("Haskell", "haskell", "hs lhs"),
    ("Hollywood", "hollywood", "hws"),
    ("HTML", "hypertext", "html htm shtml"),
    ("INI", "props", "ini properties cfg inf"),
    ("Inno Setup", "inno", "iss"),
    ("Intel HEX", "ihex", "hex ihex"),
    ("Java", "cpp", "java"),
    ("JavaScript", "cpp", "js mjs cjs jsx"),
    ("JSON", "json", "json"),
    ("JSON5 / JSONC", "cpp", "json5 jsonc"),
    ("JSP", "hypertext", "jsp"),
    ("Julia", "julia", "jl"),
    ("KiXtart", "kix", "kix"),
    ("Kotlin", "cpp", "kt kts"),
    ("LaTeX", "latex", "tex sty cls"),
    ("LISP", "lisp", "lisp lsp cl el"),
    ("Lua", "lua", "lua"),
    ("Makefile", "makefile", "mak mk"),
    ("Markdown", "markdown", "md markdown"),
    ("MATLAB", "matlab", "m"),
    ("MMIXAL", "mmixal", "mms"),
    ("MS SQL", "mssql", "tsql"),
    ("MS-DOS Style", "null", "nfo"),
    ("Nim", "nim", "nim nims"),
    ("Nix", "nix", "nix"),
    ("nnCron", "nncrontab", "tab spf"),
    ("NSIS", "nsis", "nsi nsh"),
    ("Objective-C", "cpp", "mm"),
    ("OScript", "oscript", "osx os"),
    ("Pascal", "pascal", "pas pp lpr"),
    ("Perl", "perl", "pl pm"),
    ("PHP", "hypertext", "php php3 phtml"),
    ("PostScript", "ps", "ps eps"),
    ("PowerShell", "powershell", "ps1 psm1 psd1"),
    ("Properties", "props", "conf cnf"),
    ("PureBasic", "purebasic", "pb pbi"),
    ("Python", "python", "py pyw pyi"),
    ("R", "r", "r rscript"),
    ("Raku", "raku", "raku rakumod"),
    ("REBOL", "rebol", "reb r3"),
    ("Registry", "registry", "reg"),
    ("Resource", "cpp", "rc rc2"),
    ("Ruby", "ruby", "rb rbw rake gemspec"),
    ("Rust", "rust", "rs"),
    ("SAS", "sas", "sas"),
    ("Scheme", "lisp", "scm ss"),
    ("Smalltalk", "smalltalk", "st"),
    ("Spice", "spice", "sp cir"),
    ("SQL", "sql", "sql"),
    ("S-Record", "srec", "srec mot"),
    ("Structured Text", "fcST", "stt"),
    ("Swift", "cpp", "swift"),
    ("Tcl", "tcl", "tcl tk"),
    ("TEHex", "tehex", "tek"),
    ("TeX", "tex", "texinfo"),
    ("TOML", "toml", "toml"),
    ("txt2tags", "txt2tags", "t2t"),
    ("TypeScript", "cpp", "ts tsx"),
    ("VB / Visual Basic", "vb", "vb vba"),
    ("VBScript", "vbscript", "vbs"),
    ("Verilog / SystemVerilog", "verilog", "v vh sv svh"),
    ("VHDL", "vhdl", "vhd vhdl"),
    ("Visual Prolog", "visualprolog", "pro"),
    ("XML", "xml", "xml xsl xslt xsd svg xaml csproj"),
    ("YAML", "yaml", "yaml yml"),
    ("Zig", "zig", "zig"),
];

pub fn catalog(available: &[String]) -> Vec<Language> {
    let mut result: Vec<_> = CATALOG
        .iter()
        .map(|(name, lexer, extensions)| Language {
            name: (*name).into(),
            lexer: (*lexer).into(),
            extensions: (*extensions).into(),
            sample: format!(
                "file.{}",
                extensions.split_whitespace().next().unwrap_or("txt")
            ),
            custom: None,
        })
        .collect();
    for row in COVERAGE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let mut fields = row.split('|').skip(1);
        let (Some(name), Some(extensions)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Some(language) = result.iter_mut().find(|language| language.name == name) else {
            continue;
        };
        for extension in extensions.split_whitespace() {
            if !language
                .extensions
                .split_whitespace()
                .any(|existing| existing == extension)
            {
                language.extensions.push(' ');
                language.extensions.push_str(extension);
            }
        }
    }
    for lexer in available {
        if !result.iter().any(|entry| entry.lexer == *lexer) {
            result.push(Language {
                name: format!("{lexer} (Lexilla)"),
                lexer: lexer.clone(),
                extensions: String::new(),
                sample: String::new(),
                custom: None,
            });
        }
    }
    result[1..].sort_by_key(|l| l.name.to_lowercase());
    result
}

pub fn add_custom(
    languages: &mut Vec<Language>,
    definition: UserLanguage,
) -> crate::core::Result<usize> {
    definition.validate()?;
    let existing = languages
        .iter()
        .position(|language| language.name.eq_ignore_ascii_case(&definition.name));
    if let Some(index) = existing {
        if languages[index].custom.is_none() {
            return Err("A user-defined language cannot replace a built-in language.".into());
        }
    } else if languages
        .iter()
        .filter(|language| language.custom.is_some())
        .count()
        >= 64
    {
        return Err("At most 64 user-defined languages may be installed.".into());
    }
    let language = Language {
        name: definition.name.clone(),
        lexer: "container".into(),
        extensions: definition.extensions.join(" "),
        sample: String::new(),
        custom: Some(Arc::new(definition)),
    };
    if let Some(index) = existing {
        languages[index] = language;
        Ok(index)
    } else {
        languages.push(language);
        Ok(languages.len() - 1)
    }
}

pub fn detect(path: &Path, languages: &[Language]) -> usize {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let special = match name.as_str() {
        "makefile" | "gnumakefile" => Some("Makefile"),
        "cmakelists.txt" => Some("CMake"),
        "dockerfile" => Some("Bash"),
        ".bashrc" | ".bash_profile" | ".profile" => Some("Bash"),
        ".gitconfig" | ".gitattributes" | ".gitmodules" | ".editorconfig" => Some("Properties"),
        _ => None,
    };
    if let Some(special) = special {
        return languages
            .iter()
            .position(|l| l.name == special)
            .unwrap_or(0);
    }
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if let Some(index) = languages.iter().position(|language| {
        language.custom.is_some()
            && language
                .extensions
                .split_whitespace()
                .any(|extension| name.ends_with(&format!(".{extension}")))
    }) {
        return index;
    }
    let preferred = match ext.as_str() {
        "h" => Some("C++"),
        "f" | "for" => Some("Fortran"),
        "vbs" => Some("VBScript"),
        _ => None,
    };
    if let Some(name) = preferred {
        return languages
            .iter()
            .position(|language| language.name == name)
            .unwrap_or(0);
    }
    languages
        .iter()
        .position(|l| l.extensions.split_whitespace().any(|x| x == ext))
        .unwrap_or(0)
}

fn matches(pattern: &str, sample: &str) -> bool {
    pattern.split(';').any(|p| {
        let p = p.trim();
        if let Some(suffix) = p.strip_prefix('*') {
            !suffix.contains('*') && sample.ends_with(suffix)
        } else {
            p.eq_ignore_ascii_case(sample)
        }
    })
}

pub fn keywords(language: &Language) -> [String; 9] {
    let mut sets: [String; 9] = Default::default();
    if let Some(custom) = &language.custom {
        for (index, group) in custom.keywords.iter().enumerate() {
            sets[index] = group.join(" ");
        }
        return sets;
    }
    for (pattern, index, value) in KEYWORDS {
        if matches(pattern, &language.sample) {
            sets[*index].push(' ');
            sets[*index].push_str(value);
        }
    }
    if sets.iter().all(|s| s.trim().is_empty()) {
        for (lexer, index, value) in FALLBACK_KEYWORDS {
            if lexer.eq_ignore_ascii_case(&language.lexer) {
                sets[*index].push(' ');
                sets[*index].push_str(value);
            }
        }
    }
    match language.name.as_str() {
        "CoffeeScript" => sets[0] = "and break by catch class continue debugger delete do else extends false finally for if in instanceof is isnt loop new no not null of off on or return super switch then this throw true try typeof undefined unless until when while with yes yield".into(),
        "Assembly (Motorola)" => {
            sets[0] = "abcd add adda addi addq addx and andi asl asr bcc bchg bclr bcs beq bge bgt bhi ble bls blt bmi bne bpl bra bset bsr btst bvc bvs chk clr cmp cmpa cmpi cmpm dbcc dbra divs divu eor eori exg ext illegal jmp jsr lea link lsl lsr move movea movem movep moveq muls mulu nbcd neg negx nop not or ori pea reset rol ror roxl roxr rte rtr rts sbcd scc stop sub suba subi subq subx swap tas trap trapv tst unlk".into();
            sets[1] = "d0 d1 d2 d3 d4 d5 d6 d7 a0 a1 a2 a3 a4 a5 a6 a7 sp pc sr ccr usp".into();
        }
        "Structured Text" => sets[0] = "action and array at bool by byte case configuration constant date dint do dword else elsif end_action end_case end_configuration end_for end_function end_function_block end_if end_program end_repeat end_resource end_step end_struct end_transition end_type end_var end_while exit false for from function function_block if initial_step int not of on or program real repeat resource retain return sbyte sint step string struct task then time time_of_day to transition true type udint uint until usint var var_access var_config var_external var_global var_in_out var_input var_output var_temp while with word xor".into(),
        "JSON5 / JSONC" => {
            sets = Default::default();
            sets[0] = "true false null Infinity NaN".into();
        }
        "Rust" => sets[0].push_str(" async await dyn try union macro_rules move"),
        "Kotlin" => sets[0] = "abstract actual annotation as break by catch class companion const constructor continue crossinline data delegate do dynamic else enum expect external false field file final finally for fun get if import in infix init inline inner interface internal is lateinit noinline null object open operator out override package param private property protected public receiver reified return sealed set setparam super suspend tailrec this throw true try typealias typeof val var vararg when where while".into(),
        "TypeScript" => sets[0].push_str(" abstract any as asserts bigint boolean constructor declare enum implements infer interface is keyof module namespace never number object private protected public readonly require satisfies string symbol type undefined unique unknown"),
        _ => {}
    }
    sets
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_menu_uses_predictable_letter_ranges_and_preserves_indices() {
        let languages = catalog(&["abap".into(), "zzz".into(), "9example".into()]);
        let groups = menu_groups(&languages);
        for (name, label) in [
            ("C++", "&A-C"),
            ("Markdown", "&M-O"),
            ("Python", "&P-R"),
            ("Rust", "&P-R"),
            ("YAML", "&V-Z"),
            ("9example (Lexilla)", "&Other"),
        ] {
            let index = languages
                .iter()
                .position(|language| language.name == name)
                .unwrap();
            assert!(
                groups
                    .iter()
                    .find(|group| group.label == label)
                    .unwrap()
                    .indices
                    .contains(&index),
                "{name}"
            );
        }
        let mut indices: Vec<_> = groups
            .iter()
            .flat_map(|group| group.indices.iter().copied())
            .collect();
        indices.sort_unstable();
        assert_eq!(indices, (1..languages.len()).collect::<Vec<_>>());
        for group in &groups {
            let names: Vec<_> = group
                .indices
                .iter()
                .map(|index| languages[*index].name.to_lowercase())
                .collect();
            assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
        }
    }
    #[test]
    fn imported_languages_do_not_change_built_in_menu_categories() {
        let mut languages = catalog(&[]);
        let before: Vec<_> = menu_groups(&languages)
            .into_iter()
            .map(|group| (group.label, group.indices))
            .collect();
        let definition =
            crate::udl::import(include_str!("..\\tests\\fixtures\\custom-language.xml"))
                .unwrap()
                .remove(0);
        let first = add_custom(&mut languages, definition.clone()).unwrap();
        let mut second = definition;
        second.name = "Aardvark custom".into();
        let last = add_custom(&mut languages, second).unwrap();
        let mut groups = menu_groups(&languages);
        let custom = groups.pop().unwrap();
        assert_eq!(custom.label, "&User-defined");
        assert_eq!(custom.indices, vec![last, first]);
        assert_eq!(
            groups
                .into_iter()
                .map(|group| (group.label, group.indices))
                .collect::<Vec<_>>(),
            before
        );
    }
    #[test]
    fn language_detection_and_keywords() {
        let languages = catalog(&[]);
        for (path, name) in [
            ("main.rs", "Rust"),
            ("main.cpp", "C++"),
            ("CMakeLists.txt", "CMake"),
            ("file.json", "JSON"),
            ("Makefile", "Makefile"),
            ("unknown.unknown", "Plain text"),
        ] {
            assert_eq!(languages[detect(Path::new(path), &languages)].name, name);
        }
        let rust = &languages[detect(Path::new("x.rs"), &languages)];
        assert!(keywords(rust)[0].split_whitespace().any(|w| w == "fn"));
        assert!(keywords(rust)[0].split_whitespace().any(|w| w == "async"));
    }

    #[test]
    fn keyword_languages_have_bundled_vocabulary() {
        let token_only = [
            "null",
            "escseq",
            "avs",
            "diff",
            "errorlist",
            "props",
            "ihex",
            "json",
            "makefile",
            "markdown",
            "registry",
            "srec",
            "tehex",
            "tex",
            "toml",
            "txt2tags",
            "xml",
            "yaml",
        ];
        let missing: Vec<_> = catalog(&[])
            .into_iter()
            .filter(|language| !token_only.contains(&language.lexer.as_str()))
            .filter(|language| keywords(language).iter().all(|s| s.trim().is_empty()))
            .map(|language| language.name)
            .collect();
        assert!(
            missing.is_empty(),
            "Missing keyword vocabularies: {missing:?}"
        );
    }

    #[test]
    fn upstream_inventory_modes_and_extensions_are_all_mapped() {
        let languages = catalog(&[]);
        let mut count = 0;
        for row in COVERAGE
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let fields: Vec<_> = row.split('|').collect();
            let language = languages
                .iter()
                .find(|language| language.name == fields[1])
                .unwrap();
            for extension in fields[2].split_whitespace() {
                assert!(
                    language
                        .extensions
                        .split_whitespace()
                        .any(|ext| ext == extension),
                    "{row}"
                );
                if fields[0] != "normal" {
                    assert_ne!(
                        detect(Path::new(&format!("example.{extension}")), &languages),
                        0,
                        "{row}"
                    );
                }
            }
            count += 1;
        }
        assert_eq!(count, 94);
    }

    #[test]
    fn bundled_coverage_rows_all_resolve() {
        let languages = catalog(&[]);
        for (number, row) in COVERAGE.lines().enumerate() {
            if row.starts_with('#') || row.is_empty() {
                continue;
            }
            let fields: Vec<_> = row.split('|').collect();
            assert!(
                fields.len() >= 3,
                "coverage line {}: expected 'upstream|mode|extensions'",
                number + 1
            );
            assert!(
                languages.iter().any(|language| language.name == fields[1]),
                "coverage line {}: unknown mode {:?}",
                number + 1,
                fields[1]
            );
        }
    }
}
