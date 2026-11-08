//! Hostile-input guards for the repair-source parsers: garbage must never panic.
use wu_repair_cycle::source::{kind_for, parse_index_detail, parse_index_list};

#[test]
fn kind_for_handles_extensions_and_junk() {
    assert!(kind_for("D:\\x\\INSTALL.WIM").is_some());
    assert!(kind_for("a.esd").is_some());
    assert!(kind_for("w.ISO").is_some());
    assert!(kind_for("notes.txt").is_none());
    assert!(kind_for("").is_none());
}

#[test]
fn index_list_parses_and_ignores_garbage() {
    let out = "Index : 1\nName : Home\n\nIndex : 6\nIndex : abc\n\u{0}\u{ffff}junk\nIndex :\n";
    assert_eq!(parse_index_list(out), vec![1, 6]);
    assert!(parse_index_list("").is_empty());
}

#[test]
fn index_detail_never_panics() {
    for s in [
        "",
        "garbage",
        "Version : 10.0.\nEdition :",
        "Version : x.y.z\nServicePack Build : \u{0}",
    ] {
        let _ = parse_index_detail(s);
    }
}
