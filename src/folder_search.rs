use crate::{
    core::{self, MAX_SEARCH_BYTES, Result, Search},
    search_results::{self, Input, MAX_HITS, Results},
    session,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct DiskSource {
    pub path: PathBuf,
    pub hash: u64,
    pub text_hash: u64,
}
#[derive(Clone, Debug)]
pub struct FolderOptions {
    pub directory: PathBuf,
    pub filters: String,
    pub recursive: bool,
    pub hidden: bool,
}
impl FolderOptions {
    pub fn validate(&self) -> Result<()> {
        if !self.directory.is_dir() {
            return Err("Choose an existing directory to search.".into());
        }
        patterns(&self.filters)?;
        Ok(())
    }
}
struct Filter {
    include: Vec<regex::Regex>,
    exclude: Vec<regex::Regex>,
}
fn patterns(filters: &str) -> Result<Filter> {
    if filters.len() > 8192 {
        return Err("File filters are limited to 8 KiB.".into());
    }
    let mut filter = Filter {
        include: Vec::new(),
        exclude: Vec::new(),
    };
    for (index, mask) in filters
        .split([';', ' '])
        .filter(|mask| !mask.is_empty())
        .enumerate()
    {
        if index >= 128 {
            return Err("At most 128 file filters are supported.".into());
        }
        let (exclude, mask) = mask
            .strip_prefix('!')
            .map_or((false, mask), |mask| (true, mask));
        if mask.is_empty() {
            return Err("An exclusion needs a filename pattern after !.".into());
        }
        let mut regex = String::from("^");
        for ch in mask.chars() {
            match ch {
                '*' => regex.push_str(".*"),
                '?' => regex.push('.'),
                _ => regex.push_str(&regex::escape(&ch.to_string())),
            }
        }
        regex.push('$');
        let regex = regex::RegexBuilder::new(&regex)
            .case_insensitive(cfg!(windows))
            .build()
            .map_err(|e| e.to_string())?;
        if exclude {
            filter.exclude.push(regex);
        } else {
            filter.include.push(regex);
        }
    }
    Ok(filter)
}
fn text_file(path: &Path) -> Result<(Vec<u8>, String)> {
    let bytes = session::read_bounded(path, MAX_SEARCH_BYTES)?;
    let unicode = bytes.starts_with(b"\xff\xfe")
        || bytes.starts_with(b"\xfe\xff")
        || bytes.starts_with(b"\0\0\xfe\xff");
    if !unicode && bytes.contains(&0) {
        return Err("binary file (NUL bytes without a Unicode BOM)".into());
    }
    let (text, _) = core::decode(&bytes, None)?;
    Search::validate_size(text.len())?;
    if text
        .chars()
        .take(8192)
        .filter(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t' | '\u{c}'))
        .count()
        > 32
    {
        return Err("binary/control-heavy file".into());
    }
    Ok((bytes, text))
}
pub fn verify_source(source: &DiskSource) -> Result<String> {
    let (bytes, text) = text_file(&source.path)?;
    if session::fingerprint(&bytes) != source.hash
        || session::fingerprint(text.as_bytes()) != source.text_hash
    {
        return Err("This file changed since the search. Run Find in Files again.".into());
    }
    Ok(text)
}
pub fn find_in_files(
    search: &Search,
    query: String,
    options: FolderOptions,
    cancelled: &AtomicBool,
) -> Result<Results> {
    options.validate()?;
    let filters = patterns(&options.filters)?;
    let root = fs::canonicalize(&options.directory).map_err(|e| e.to_string())?;
    let mut directories = vec![root.clone()];
    let mut results = Results {
        query,
        files: Vec::new(),
        searched: 0,
        truncated: false,
        skipped: 0,
        warnings: Vec::new(),
    };
    let mut visited = 0usize;
    let mut bytes = 0usize;
    let deadline = Instant::now() + Duration::from_secs(30);
    let warn = |results: &mut Results, path: &Path, message: &str| {
        results.skipped += 1;
        if results.warnings.len() < 100 {
            results
                .warnings
                .push(format!("{}: {message}", path.display()));
        }
    };
    while let Some(directory) = directories.pop() {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Search cancelled.".into());
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                warn(&mut results, &directory, &error.to_string());
                continue;
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Search cancelled.".into());
            }
            visited += 1;
            if visited > 100_000 || Instant::now() > deadline {
                return Err("Folder search exceeded its entry/time budget. Choose a smaller directory or tighter filters.".into());
            }
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(error) => warn(&mut results, &directory, &error.to_string()),
            }
        }
        paths.sort();
        for path in paths {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Search cancelled.".into());
            }
            if Instant::now() > deadline {
                return Err(
                    "Folder search exceeded its 30-second budget; choose a smaller scope.".into(),
                );
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !options.hidden && name.starts_with('.') {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    warn(&mut results, &path, &error.to_string());
                    continue;
                }
            };
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if !options.hidden && metadata.file_attributes() & 2 != 0 {
                    continue;
                }
                if metadata.file_attributes() & 0x400 != 0 {
                    warn(&mut results, &path, "reparse point skipped");
                    continue;
                }
            }
            if metadata.file_type().is_symlink() {
                warn(&mut results, &path, "symbolic link skipped");
                continue;
            }
            if metadata.is_dir() {
                if options.recursive {
                    directories.push(path);
                }
                continue;
            }
            if !metadata.is_file() {
                warn(&mut results, &path, "not a regular file");
                continue;
            }
            if (!filters.include.is_empty()
                && !filters.include.iter().any(|filter| filter.is_match(&name)))
                || filters.exclude.iter().any(|filter| filter.is_match(&name))
            {
                continue;
            }
            if metadata.len() > MAX_SEARCH_BYTES as u64 {
                warn(&mut results, &path, "exceeds the 128 MiB search file limit");
                continue;
            }
            if results.searched >= 10_000 {
                return Err("Folder search is limited to 10,000 files; narrow the scope.".into());
            }
            let canonical = match fs::canonicalize(&path) {
                Ok(path) => path,
                Err(error) => {
                    warn(&mut results, &path, &error.to_string());
                    continue;
                }
            };
            if !canonical.starts_with(&root) {
                warn(&mut results, &path, "resolved outside the selected folder");
                continue;
            }
            let (raw, text) = match text_file(&canonical) {
                Ok(data) => data,
                Err(error) => {
                    warn(&mut results, &path, &error);
                    continue;
                }
            };
            bytes += raw.len();
            if bytes > 512 * 1024 * 1024 {
                return Err(
                    "Folder search is limited to 512 MiB of input; narrow the scope.".into(),
                );
            }
            let hash = session::fingerprint(&raw);
            let text_hash = session::fingerprint(text.as_bytes());
            let found = search_results::find_all(
                search,
                results.query.clone(),
                vec![Input {
                    id: 0,
                    revision: 0,
                    title: canonical.display().to_string(),
                    text,
                    tab_width: 4,
                }],
                cancelled,
            )?;
            results.searched += 1;
            for mut file in found.files {
                let available = MAX_HITS - results.count();
                if file.hits.len() > available {
                    file.hits.truncate(available);
                    results.truncated = true;
                }
                file.source = Some(DiskSource {
                    path: canonical.clone(),
                    hash,
                    text_hash,
                });
                if !file.hits.is_empty() {
                    results.files.push(file);
                }
            }
            results.truncated |= found.truncated;
            if results.count() == MAX_HITS || results.truncated {
                results.truncated = true;
                return Ok(results);
            }
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("Search cancelled.".into());
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_filters_recursion_binary_skips_and_stale_results() {
        let root = std::env::temp_dir().join(format!(
            "rstpd-folder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        for (name, data) in [
            ("a.txt", "needle"),
            ("ignore.txt", "needle"),
            (".hidden.txt", "needle"),
            ("nested/b.txt", "needle"),
            ("c.rs", "needle"),
        ] {
            fs::write(root.join(name), data).unwrap();
        }
        fs::write(root.join("binary.txt"), b"needle\0\xff").unwrap();
        let options = FolderOptions {
            directory: root.clone(),
            filters: "*.txt;!ignore*".into(),
            recursive: true,
            hidden: false,
        };
        let search = Search::new("needle", core::SearchMode::Literal, true, false).unwrap();
        let found = find_in_files(
            &search,
            "needle".into(),
            options.clone(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(found.count(), 2);
        assert_eq!(found.skipped, 1);
        let source = found.files[0].source.as_ref().unwrap();
        assert_eq!(verify_source(source).unwrap(), "needle");
        fs::write(&source.path, "changed").unwrap();
        assert!(verify_source(source).is_err());
        fs::write(&source.path, "needle").unwrap();
        let flat = find_in_files(
            &search,
            "needle".into(),
            FolderOptions {
                recursive: false,
                hidden: true,
                ..options
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(flat.count(), 2);
        for name in [
            "a.txt",
            "ignore.txt",
            ".hidden.txt",
            "nested/b.txt",
            "c.rs",
            "binary.txt",
        ] {
            fs::remove_file(root.join(name)).unwrap();
        }
        fs::remove_dir(root.join("nested")).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn invalid_filters_and_cancellation_are_explicit_errors() {
        assert!(patterns("!").is_err());
        assert!(patterns(&"a".repeat(8193)).is_err());
        let filter = patterns("*.rs;?.txt;!generated*").unwrap();
        assert!(
            filter
                .include
                .iter()
                .any(|pattern| pattern.is_match("a.txt"))
        );
        assert!(
            !filter
                .include
                .iter()
                .any(|pattern| pattern.is_match("long.txt"))
        );
        assert!(
            filter
                .exclude
                .iter()
                .any(|pattern| pattern.is_match("generated.rs"))
        );
        let search = Search::new("x", core::SearchMode::Literal, true, false).unwrap();
        let options = FolderOptions {
            directory: std::env::temp_dir(),
            filters: "*".into(),
            recursive: true,
            hidden: false,
        };
        assert!(
            find_in_files(&search, "x".into(), options, &AtomicBool::new(true))
                .err()
                .expect("cancelled search")
                .contains("cancelled")
        );
    }
}
