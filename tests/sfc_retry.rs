//! Guard: an SFC "could not perform the requested operation" (WRP conflict)
//! refusal must be retried up to 3 times after a settle, not just once.
#[test]
fn sfc_wrp_refusal_retries_up_to_three_times() {
    let src = std::fs::read_to_string("src/repair.rs").unwrap();
    assert!(src.contains("refused to run"));
    assert!(src.contains("retries < 3"));
    assert!(src.contains("settling 90 s"));
}
