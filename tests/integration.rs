use std::process::Command;
use wu_repair_cycle::pause::iso_from_unix;
use wu_repair_cycle::verify::{gate, Step};
use wu_repair_cycle::wu_reset::DLLS;

fn step(ok: bool) -> Step {
    Step::new("t", "t", ok, "", 0.0)
}

#[test]
fn gate_zero_iff_all_ok() {
    assert_eq!(gate(&[step(true), step(true)]), 0);
    assert_eq!(gate(&[step(true), step(false)]), 1);
    assert_eq!(gate(&[]), 0);
}

#[test]
fn iso_known_epochs() {
    assert_eq!(iso_from_unix(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso_from_unix(946_684_800), "2000-01-01T00:00:00Z");
    assert_eq!(iso_from_unix(4_102_444_799), "2099-12-31T23:59:59Z");
}

#[test]
fn dll_table_has_32() {
    assert_eq!(DLLS.len(), 32);
    assert!(DLLS.contains(&"wuaueng.dll"));
}

#[test]
fn dry_run_zero_exit_all_pass() {
    let exe = env!("CARGO_BIN_EXE_wu-repair-cycle");
    let out = Command::new(exe).arg("--dry-run").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "dry-run must exit 0, got {:?}", out.status.code());
    assert!(s.contains("dism-restore"), "matrix must list RestoreHealth");
    assert!(s.contains("sfc-scannow"), "matrix must list SFC");
    assert!(s.contains("chkdsk"), "matrix must list chkdsk");
    assert!(s.contains("ZERO ERRORS"), "dry-run summary must show zero errors");
}

#[test]
fn check_only_zero_exit_and_readonly_hint() {
    let exe = env!("CARGO_BIN_EXE_wu-repair-cycle");
    let out = Command::new(exe).arg("--check-only").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(s.contains("Pause keys"), "check-only must report pause keys");
}
