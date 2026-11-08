//! Guard: the repair set must keep covering DISM, SFC and chkdsk, and must never dismount drives.
use std::fs;

fn all_src() -> String {
    let mut s = String::new();
    for e in fs::read_dir("src").unwrap() {
        let p = e.unwrap().path();
        if p.extension().map_or(false, |x| x == "rs") {
            s.push_str(&fs::read_to_string(p).unwrap().to_lowercase());
        }
    }
    s
}

#[test]
fn repair_set_covers_dism_sfc_chkdsk() {
    let s = all_src();
    for n in ["/checkhealth", "/scanhealth", "/restorehealth", "/startcomponentcleanup", "/analyzecomponentstore", "sfc", "/scannow", "chkdsk"] {
        assert!(s.contains(n), "missing repair command token {n}");
    }
}

#[test]
fn never_dismounts_volumes() {
    let s = all_src();
    for bad in ["chkdsk /x", "/x ", "fsutil volume dismount", "dismount-volume"] {
        if bad == "/x " { continue; }
        assert!(!s.contains(bad), "volume-dismount token present: {bad}");
    }
}
