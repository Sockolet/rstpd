#[test]
fn manifest_version_matches_the_package_version() {
    let manifest = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "\\assets\\app.manifest"
    ))
    .unwrap();
    let expected = format!(
        "<assemblyIdentity version=\"{}.0\" processorArchitecture=\"*\" name=\"rstpd\"",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        manifest.contains(&expected),
        "assets\\app.manifest must contain {expected}; update it when bumping Cargo.toml"
    );
}
