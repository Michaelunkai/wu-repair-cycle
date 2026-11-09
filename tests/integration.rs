use std::process::Command;
use wu_repair_cycle::pause::iso_from_unix;
use wu_repair_cycle::source::{parse_index_detail, parse_index_list, rank_indexes, WimIndex};
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
    assert!(
        out.status.success(),
        "dry-run must exit 0, got {:?}",
        out.status.code()
    );
    assert!(s.contains("dism-restore"), "matrix must list RestoreHealth");
    assert!(s.contains("sfc-scannow"), "matrix must list SFC");
    assert!(
        s.contains("verifyonly"),
        "matrix must use the valid /verifyonly switch"
    );
    assert!(
        !s.contains("verifynow"),
        "the invalid /verifynow switch must be gone"
    );
    assert!(
        s.contains("net-connectivity"),
        "matrix must list the connectivity probe"
    );
    assert!(
        s.contains("dism-restore-src"),
        "matrix must list the local-source ladder"
    );
    assert!(s.contains("chkdsk"), "matrix must list chkdsk");
    assert!(
        s.contains("ZERO ERRORS"),
        "dry-run summary must show zero errors"
    );
}

#[test]
fn check_only_zero_exit_and_readonly_hint() {
    let exe = env!("CARGO_BIN_EXE_wu-repair-cycle");
    let out = Command::new(exe).arg("--check-only").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(
        s.contains("Pause keys"),
        "check-only must report pause keys"
    );
}

// ---------- source ladder (v2.1.0) ----------

const WIM_LISTING: &str = "\
Details for image : D:\\sources\\install.wim

Index : 1
Name : Windows 11 Home

Index : 6
Name : Windows 11 Pro

Index : 7
Name : Windows 11 Pro N
";

const WIM_DETAIL: &str = "\
Details for image : D:\\sources\\install.wim

Index : 6
Name : Windows 11 Pro
Description : Windows 11 Pro
Size : 24,860,631,795 bytes
WIM Bootable : No
Architecture : x64
Hal : <undefined>
Version : 10.0.26200
ServicePack Build : 8037
ServicePack Level : 0
Edition : Professional
Installation : Client
";

#[test]
fn wim_index_parsing() {
    let idxs = parse_index_list(WIM_LISTING);
    assert_eq!(idxs, vec![1, 6, 7]);
    let d = parse_index_detail(WIM_DETAIL).expect("detail must parse");
    assert_eq!(d.index, 6);
    assert_eq!(d.build, 26200);
    assert_eq!(d.revision, 8037);
    assert_eq!(d.edition, "Professional");
    assert_eq!(d.name, "Windows 11 Pro");
}

#[test]
fn rank_prefers_same_build_closest_revision_same_edition() {
    let mk = |i: u32, name: &str, ed: &str, build: u32, rev: u32| WimIndex {
        index: i,
        name: name.to_string(),
        edition: ed.to_string(),
        build,
        revision: rev,
    };
    let ix = vec![
        mk(1, "Windows 11 Home", "Core", 26200, 8037),
        mk(2, "Windows 11 Pro", "Professional", 26200, 8037),
        mk(3, "Windows 11 Pro", "Professional", 26200, 9400),
        mk(4, "Windows 11 Pro 24H2", "Professional", 26100, 4000), // older line
        mk(5, "Windows 11 Pro", "Professional", 26200, 9999),      // newer than OS
    ];
    // OS is 26200.9457 Professional: closest revision <= UBR first, then older
    // revisions of the same build+edition, then same-build-newer-rev, then other
    // editions of the same build; the older build line is dead last.
    let ranked = rank_indexes(&ix, 26200, 9457, "Professional");
    assert_eq!(
        ranked[0], 3,
        "index 3 (rev 9400, closest <= 9457) must rank first"
    );
    assert_eq!(
        ranked[1], 2,
        "index 2 (older rev 8037, same build+edition) must rank second"
    );
    assert_eq!(
        ranked[2], 5,
        "index 5 (same build, newer rev) must rank third"
    );
    assert_eq!(
        *ranked.last().unwrap(),
        4,
        "the older build line must rank last"
    );
}

#[test]
fn index_detail_uses_image_version_not_dism_header() {
    let out = "Deployment Image Servicing and Management tool\nVersion: 10.0.26100.8972\n\nDetails for image : install.esd\n\nIndex : 6\nName : Windows 11 Pro\nDescription : Windows 11 Pro\nSize : 1 bytes\n\nArchitecture : x64\nHAL : <undefined>\nVersion : 10.0.26200\nServicePack Build : 8246\nServicePack Level : 0\nEdition : Professional\n";
    let w = parse_index_detail(out).unwrap();
    assert_eq!(w.build, 26200);
    assert_eq!(w.revision, 8246);
    assert_eq!(w.index, 6);
}
