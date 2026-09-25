use rstpd::{
    core::{self, MAX_DOCUMENT_BYTES, MAX_SEARCH_BYTES, MAX_TOOL_BYTES, Search, SearchMode},
    folder_search::{self, FolderOptions},
    search_results::{self, Input, MAX_BATCH_BYTES},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Instant, SystemTime},
};

struct Fixtures(PathBuf);
impl Drop for Fixtures {
    fn drop(&mut self) {
        for name in ["large.log", "oversized.log"] {
            let path = self.0.join(name);
            if path.exists() {
                fs::remove_file(path).expect("remove large search fixture");
            }
        }
        fs::remove_dir(&self.0).expect("remove fixture directory");
    }
}

fn input(id: u64, text: String) -> Input {
    Input {
        id,
        revision: 0,
        title: format!("log-{id}"),
        text,
        tab_width: 4,
    }
}

#[test]
fn document_search_and_folder_boundaries_preserve_exact_results() {
    assert_eq!(MAX_DOCUMENT_BYTES, 256 * 1024 * 1024);
    assert_eq!(MAX_SEARCH_BYTES, 128 * 1024 * 1024);
    assert_eq!(MAX_BATCH_BYTES, 256 * 1024 * 1024);
    assert_eq!(MAX_TOOL_BYTES, 16 * 1024 * 1024);
    assert!(Search::validate_size(MAX_SEARCH_BYTES).is_ok());
    assert!(Search::validate_size(MAX_SEARCH_BYTES + 1).is_err());

    let mut bytes = vec![b'x'; MAX_DOCUMENT_BYTES + 1];
    assert!(core::decode(&bytes, None).is_err());
    bytes.pop();
    let (text, _) = core::decode(&bytes, None).unwrap();
    assert_eq!(text.len(), MAX_DOCUMENT_BYTES);
    drop(text);
    drop(bytes);

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "rstpd-large-search-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let fixtures = Fixtures(root);
    let path = fixtures.0.join("large.log");
    let oversized = fs::File::create(fixtures.0.join("oversized.log")).unwrap();
    oversized.set_len((MAX_SEARCH_BYTES + 1) as u64).unwrap();
    drop(oversized);
    let cancelled = AtomicBool::new(false);
    let query = "TARGET-12345";
    let search = Search::new(query, SearchMode::Literal, true, false).unwrap();
    for size in [50 * 1024 * 1024, MAX_SEARCH_BYTES] {
        let line = "ordinary log entry: operation completed successfully\n";
        let mut text = line.repeat(size / line.len() + 1);
        text.truncate(size);
        let positions = [0, size / 2, size - query.len()];
        for position in positions {
            text.replace_range(position..position + query.len(), query);
        }
        let started = Instant::now();
        assert_eq!(
            search.find(&text, 1).unwrap(),
            Some(positions[1]..positions[1] + query.len())
        );
        assert_eq!(
            search.find(&text, text.len()).unwrap(),
            Some(0..query.len())
        );
        let regex = Search::new(r"TARGET-\d{5}", SearchMode::Regex, true, false).unwrap();
        assert_eq!(regex.matches(&text).unwrap().len(), 3);
        let extended = Search::new(r"TARGET\x2d12345", SearchMode::Extended, true, false).unwrap();
        assert_eq!(extended.matches(&text).unwrap().len(), 3);
        eprintln!(
            "{} MiB: next/wrapped/regex/extended search {:?}",
            size / (1024 * 1024),
            started.elapsed()
        );

        let started = Instant::now();
        let found = search_results::find_all(
            &search,
            query.into(),
            vec![input(1, text.clone())],
            &cancelled,
        )
        .unwrap();
        assert_eq!(found.count(), 3);
        for (hit, position) in found.files[0].hits.iter().zip(positions) {
            assert_eq!(hit.range, position..position + query.len());
            assert_eq!(&text[hit.range.clone()], query);
        }
        eprintln!(
            "{} MiB: Find All with exact line/snippet positions {:?}",
            size / (1024 * 1024),
            started.elapsed()
        );
        let (changed, count) = search.replace_all(&text, "REPLACED").unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            changed.len(),
            text.len() - 3 * (query.len() - "REPLACED".len())
        );
        drop(changed);

        fs::write(&path, &text).unwrap();
        let started = Instant::now();
        let results = folder_search::find_in_files(
            &search,
            query.into(),
            FolderOptions {
                directory: fixtures.0.clone(),
                filters: "*.log".into(),
                recursive: false,
                hidden: false,
            },
            &cancelled,
        )
        .unwrap();
        assert_eq!(results.count(), 3);
        assert_eq!(results.skipped, 1);
        assert!(results.warnings[0].contains("128 MiB"));
        assert_eq!(
            folder_search::verify_source(results.files[0].source.as_ref().unwrap()).unwrap(),
            text
        );
        eprintln!(
            "{} MiB: directory search and source verification {:?}",
            size / (1024 * 1024),
            started.elapsed()
        );

        if size == MAX_SEARCH_BYTES {
            let found = search_results::find_all(
                &search,
                query.into(),
                vec![input(1, text.clone()), input(2, text.clone())],
                &cancelled,
            )
            .unwrap();
            assert_eq!(found.count(), 6);
            assert!(
                search_results::find_all(
                    &search,
                    query.into(),
                    vec![
                        input(1, text.clone()),
                        input(2, text.clone()),
                        input(3, "x".into())
                    ],
                    &cancelled
                )
                .is_err()
            );
            text.push('x');
            assert!(search.find(&text, 0).is_err());
            assert!(search.replace_all(&text, "replacement").is_err());
            assert!(
                search_results::find_all(&search, query.into(), vec![input(1, text)], &cancelled)
                    .is_err()
            );
        }
    }
}
