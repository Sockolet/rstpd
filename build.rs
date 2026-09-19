use std::{env, fs, path::Path};

fn language_data() {
    use std::collections::BTreeMap;
    let mut properties = BTreeMap::new();
    let mut styles = BTreeMap::new();
    let mut files: Vec<_> = fs::read_dir("vendor/language-data")
        .expect("bootstrap language data")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "properties"))
        .collect();
    files.sort();
    for path in files {
        let bytes = fs::read(&path).expect("read language data");
        let data = match String::from_utf8(bytes) {
            Ok(data) => data,
            Err(error) => {
                println!(
                    "cargo:warning=Reading legacy Latin-1 language data: {}",
                    path.display()
                );
                error.into_bytes().into_iter().map(char::from).collect()
            }
        };
        let mut logical = String::new();
        let mut comment = String::new();
        for line in data.lines() {
            let line = line.trim();
            if let Some(c) = line.strip_prefix('#') {
                comment = c.to_lowercase();
                continue;
            }
            if let Some(part) = line.strip_suffix('\\') {
                logical.push_str(part);
                continue;
            }
            logical.push_str(line);
            if let Some((key, value)) = logical.split_once('=') {
                let key = key.trim().to_owned();
                if key.starts_with("style.") {
                    styles.insert(key.clone(), (comment.clone(), value.to_owned()));
                }
                properties.insert(key, value.trim().to_owned());
            }
            logical.clear();
        }
    }
    fn expand(text: &str, props: &BTreeMap<String, String>, depth: u8) -> String {
        if depth > 16 {
            return String::new();
        }
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("$(") {
            out.push_str(&rest[..start]);
            let Some(end) = rest[start + 2..].find(')') else {
                break;
            };
            let name = &rest[start + 2..start + 2 + end];
            if let Some(value) = props.get(name) {
                out.push_str(&expand(value, props, depth + 1));
            }
            rest = &rest[start + 3 + end..];
        }
        out.push_str(rest);
        out
    }
    let mut out = String::from("pub const KEYWORDS: &[(&str, usize, &str)] = &[\n");
    for (key, value) in &properties {
        if let Some(tail) = key.strip_prefix("keywords") {
            let Some((index, pattern)) = tail.split_once('.') else {
                continue;
            };
            let index = if index.is_empty() {
                0
            } else {
                let Ok(index) = index.parse::<usize>() else {
                    continue;
                };
                if !(2..=9).contains(&index) {
                    continue;
                }
                index - 1
            };
            out.push_str(&format!(
                "({:?},{},{:?}),\n",
                expand(pattern, &properties, 0),
                index,
                expand(value, &properties, 0)
            ));
        }
    }
    out.push_str("];\npub const STYLE_HINTS: &[(&str,usize,&str,&str)] = &[\n");
    for (key, (description, value)) in styles {
        let Some((lexer, id)) = key[6..].rsplit_once('.') else {
            continue;
        };
        let Ok(id) = id.parse::<usize>() else {
            continue;
        };
        if id >= 256 || (32..=39).contains(&id) {
            continue;
        }
        out.push_str(&format!("({lexer:?},{id},{description:?},{value:?}),\n"));
    }
    out.push_str("];\npub const FALLBACK_KEYWORDS: &[(&str,usize,&str)] = &[\n");
    let mut examples: Vec<_> = fs::read_dir("vendor/lexilla/test/examples")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    examples.sort();
    for directory in examples {
        let path = directory.join("SciTE.properties");
        if !path.is_file() {
            continue;
        }
        let data = fs::read_to_string(path)
            .expect("read lexer vocabulary")
            .replace("\\\r\n", "")
            .replace("\\\n", "");
        let lexer = directory.file_name().unwrap().to_string_lossy();
        for line in data.lines() {
            let Some((key, value)) = line.trim().split_once('=') else {
                continue;
            };
            let Some(tail) = key.strip_prefix("keywords") else {
                continue;
            };
            let Some((index, _)) = tail.split_once('.') else {
                continue;
            };
            let index = if index.is_empty() {
                0
            } else {
                let Ok(index) = index.parse::<usize>() else {
                    continue;
                };
                if !(2..=9).contains(&index) {
                    continue;
                }
                index - 1
            };
            out.push_str(&format!(
                "({lexer:?},{index},{:?}),\n",
                expand(value.trim(), &properties, 0)
            ));
        }
    }
    out.push_str("];\npub const STYLE_SYMBOLS: &[(&str,usize,&str)] = &[\n");
    let header = fs::read_to_string("vendor/lexilla/include/SciLexer.h").unwrap();
    let mut style_ids = BTreeMap::new();
    for line in header.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() == 3
            && parts[0] == "#define"
            && parts[1].starts_with("SCE_")
            && let Ok(id) = parts[2].parse::<usize>()
        {
            style_ids.insert(parts[1], id);
        }
    }
    let mut sources: Vec<_> = fs::read_dir("vendor/lexilla/lexers")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cxx"))
        .collect();
    sources.sort();
    for path in sources {
        let bytes = fs::read(path).unwrap();
        let source = String::from_utf8_lossy(&bytes);
        let mut roles = BTreeMap::new();
        for word in source.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if let Some(id) = style_ids.get(word) {
                roles.insert(id, word);
            }
        }
        for declaration in source.split("extern const LexerModule ").skip(1) {
            let Some(statement) = declaration.split(';').next() else {
                continue;
            };
            let Some(lexer) = statement.split('"').nth(1) else {
                continue;
            };
            for (id, symbol) in &roles {
                out.push_str(&format!("({lexer:?},{id},{symbol:?}),\n"));
            }
        }
    }
    out.push_str("];\n");
    fs::write(
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("language_data.rs"),
        out,
    )
    .unwrap();
    println!("cargo:rerun-if-changed=vendor/language-data");
    println!("cargo:rerun-if-changed=vendor/lexilla/test/examples");
    println!("cargo:rerun-if-changed=vendor/lexilla/include/SciLexer.h");
}

fn sources(build: &mut cc::Build, directory: &str, exclude: &str) {
    let mut files: Vec<_> = fs::read_dir(directory)
        .unwrap_or_else(|e| panic!("{directory}: {e}. Run .\\scripts\\bootstrap.ps1 first."))
        .map(|e| e.expect("source entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cxx"))
        .filter(|p| p.file_name().is_some_and(|n| n != exclude))
        .collect();
    files.sort();
    build.files(files);
    println!("cargo:rerun-if-changed={directory}");
}

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        panic!("rstpd requires Windows and the MSVC toolchain.");
    }
    language_data();
    let header = fs::read_to_string("vendor/scintilla/include/Scintilla.h")
        .expect("Run .\\scripts\\bootstrap.ps1 to unpack the pinned editor sources.");
    let mut constants = String::from("// Generated from the vendored Scintilla public header.\n");
    for line in header.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.len() == 3
            && words[0] == "#define"
            && (words[1].starts_with("SCI_") || words[1].starts_with("SCN_"))
            && words[2].parse::<u32>().is_ok()
        {
            constants.push_str(&format!("pub const {}: u32 = {};\n", words[1], words[2]));
        }
    }
    let interface = fs::read_to_string("vendor/scintilla/include/Scintilla.iface").unwrap();
    let mut scalar_types: Vec<&str> = vec![
        "void",
        "int",
        "bool",
        "position",
        "line",
        "colour",
        "colouralpha",
        "keymod",
    ];
    scalar_types.extend(
        interface
            .lines()
            .filter_map(|line| line.strip_prefix("enu "))
            .filter_map(|line| line.split_once('=').map(|(name, _)| name)),
    );
    let mut scalar_messages = Vec::new();
    for line in interface.lines().filter(|line| {
        line.starts_with("fun ") || line.starts_with("get ") || line.starts_with("set ")
    }) {
        let result = line.split_whitespace().nth(1).unwrap();
        let Some((_, tail)) = line.split_once('=') else {
            continue;
        };
        let Some((id, parameters)) = tail.split_once('(') else {
            continue;
        };
        let parameters = parameters.split(')').next().unwrap();
        if scalar_types.contains(&result)
            && parameters.split(',').all(|p| {
                p.trim().is_empty() || scalar_types.contains(&p.split_whitespace().next().unwrap())
            })
        {
            scalar_messages.push(id.parse::<u32>().unwrap());
        }
    }
    scalar_messages.sort_unstable();
    constants.push_str(&format!(
        "pub const SCALAR_MESSAGES: &[u32] = &{scalar_messages:?};\n"
    ));
    fs::write(
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("scintilla.rs"),
        constants,
    )
    .expect("write Scintilla constants");
    println!("cargo:rerun-if-changed=vendor/scintilla/include/Scintilla.h");
    println!("cargo:rerun-if-changed=vendor/scintilla/include/Scintilla.iface");
    println!("cargo:rerun-if-changed=src/native_bridge.cxx");

    let mut sci = cc::Build::new();
    sci.cpp(true)
        .std("c++17")
        .static_crt(true)
        .warnings(false)
        .flag("/utf-8")
        .flag("/EHsc")
        .flag("/guard:cf")
        .define("NDEBUG", None)
        .define("NO_CXX11_REGEX", None)
        .file("src/native_bridge.cxx")
        .include("vendor/scintilla/include")
        .include("vendor/scintilla/src");
    sources(&mut sci, "vendor/scintilla/src", "");
    sources(&mut sci, "vendor/scintilla/win32", "ScintillaDLL.cxx");
    sci.compile("scintilla");

    let mut lex = cc::Build::new();
    lex.cpp(true)
        .std("c++17")
        .static_crt(true)
        .warnings(false)
        .flag("/utf-8")
        .flag("/EHsc")
        .flag("/guard:cf")
        .define("LEXILLA_NO_EXPORT", None)
        .define("NDEBUG", None)
        .include("vendor/scintilla/include")
        .include("vendor/lexilla/include")
        .include("vendor/lexilla/lexlib")
        .file("vendor/lexilla/src/Lexilla.cxx");
    sources(&mut lex, "vendor/lexilla/lexlib", "");
    sources(&mut lex, "vendor/lexilla/lexers", "");
    lex.compile("lexilla");
    for lib in ["imm32", "ole32", "oleaut32", "gdi32", "user32", "advapi32"] {
        println!("cargo:rustc-link-lib={lib}");
    }
    embed_resource::compile("assets/app.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile application manifest");
    println!("cargo:rerun-if-changed=assets");
}
