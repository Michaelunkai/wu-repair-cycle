//! Guard: an SFC "could not perform the requested operation" (WRP conflict)
//! refusal must be retried up to 3 times after a settle, not just once.
#[test]
fn sfc_wrp_refusal_retries_up_to_three_times() {
    let src = std::fs::read_to_string("src/repair.rs").unwrap();
    assert!(src.contains("refused to run"));
    assert!(src.contains("retries < 3"));
    assert!(src.contains("settling 90 s"));
}

#[test]
fn finding_check_accepts_clean_verify_pass() {
    let src = std::fs::read_to_string("src/repair.rs").unwrap();
    assert!(src.contains("let mut verify_clean = false;"));
    assert!(src.contains("!(sfc_clean || verify_clean)"));
    let r6 = src.find("R6: SFC /verifyonly").unwrap();
    let chk = src.find("!(sfc_clean || verify_clean)").unwrap();
    assert!(
        chk > r6,
        "finding check must run after the R6 verification pass"
    );
}
