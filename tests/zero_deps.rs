//! Guard: the tool ships as a single dependency-free executable.
use std::fs;

#[test]
fn cargo_manifest_has_no_dependencies() {
    let t = fs::read_to_string("Cargo.toml").unwrap();
    let mut in_deps = false;
    for l in t.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_deps = l.contains("dependencies");
            continue;
        }
        assert!(
            !(in_deps && !l.is_empty() && !l.starts_with('#')),
            "dependency found: {l}"
        );
    }
}
