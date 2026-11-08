//! Guard: two DISM operations must never run at once - the runner must wait
//! for a foreign dism.exe before launching its own.
#[test]
fn runner_waits_for_foreign_dism() {
    let src = std::fs::read_to_string("src/runner.rs").unwrap();
    assert!(src.contains("fn wait_foreign_dism"));
    assert!(src.contains("wait_foreign_dism();"));
    assert!(src.contains("IMAGENAME eq dism.exe"));
}
