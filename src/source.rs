//! Repair-source discovery (v2.1.0). When Windows Update no longer offers the
//! repair payloads (CBS_E_REPAIR_CONTENT_MISSING = 0x800f0915 on retired or
//! mis-targeted content), DISM's own error text says it: use the "Source"
//! option. This module:
//!   1. discovers candidate images on every drive: already-mounted media and
//!      extracted folders (X:\sources\install.*), plus Windows ISO files
//!      anywhere shallow on the fixed drives,
//!   2. mounts ISOs on demand (Mount-DiskImage) and dismounts ONLY the ones
//!      it mounted itself,
//!   3. probes WIM/ESD indexes, ranks them against the running OS build
//!      (same build, same edition, closest revision <= UBR first), and
//!   4. hands DISM /RestoreHealth the exact /Source:wim|esd:path:index arg.
//! CBS pins exact file revisions - a same-build-line but older-revision ISO
//! is attempted anyway (it repairs everything whose payloads did not change)
//! and its outcome is reported honestly.

use crate::runner;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImgKind {
    Wim,
    Esd,
    Swm,
    Iso,
}

#[derive(Clone, Debug)]
pub struct ImageSource {
    pub path: String,
    pub kind: ImgKind,
    /// ISOs this module mounted itself - dismount them when done.
    pub mounted_by_us: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WimIndex {
    pub index: u32,
    pub name: String,
    pub edition: String,
    pub build: u32,
    pub revision: u32,
}

pub fn kind_for(path: &str) -> Option<ImgKind> {
    let low = path.to_lowercase();
    if low.ends_with(".wim") {
        Some(ImgKind::Wim)
    } else if low.ends_with(".esd") {
        Some(ImgKind::Esd)
    } else if low.ends_with(".swm") {
        Some(ImgKind::Swm)
    } else if low.ends_with(".iso") {
        Some(ImgKind::Iso)
    } else {
        None
    }
}

/// Running OS identity: (build, revision/UBR, EditionID).
pub fn os_identity() -> (u32, u32, String) {
    let cv = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let build = crate::winapi::reg_get_sz(cv, "CurrentBuildNumber")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let ubr = crate::winapi::reg_get_dword(cv, "UBR").unwrap_or(0);
    let edition = crate::winapi::reg_get_sz(cv, "EditionID").unwrap_or_default();
    (build, ubr, edition)
}

// ---------- parsing (pure, unit-tested) ----------

fn field(out: &str, key: &str) -> Option<String> {
    for line in out.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix(':') {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Parse the "Index : N" lines of a /Get-WimInfo listing (no /Index arg).
pub fn parse_index_list(out: &str) -> Vec<u32> {
    out.lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("Index")?;
            let rest = rest.trim_start().strip_prefix(':')?;
            rest.trim().parse::<u32>().ok()
        })
        .collect()
}

/// Parse one detailed /Get-WimInfo /Index:N output.
/// DISM prints "Version : 10.0.26200" (build) and "ServicePack Build : 9457"
/// (= revision/UBR) and "Edition : Professional".
pub fn parse_index_detail(out: &str) -> Option<WimIndex> {
    let index = field(out, "Index")?.parse::<u32>().ok()?;
    let name = field(out, "Name").unwrap_or_default();
    let edition = field(out, "Edition").unwrap_or_default();
    let build = field(out, "Version")
        .and_then(|v| v.split('.').nth(2).map(|s| s.trim().to_string()))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let revision = field(out, "ServicePack Build")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    Some(WimIndex {
        index,
        name,
        edition,
        build,
        revision,
    })
}

/// Edition match that knows the Windows family aliases: Professional <-> pro,
/// Core <-> home, ProfessionalN <-> pro n.
pub fn edition_matches(edition: &str, name: &str, os_edition: &str) -> bool {
    if os_edition.is_empty() {
        return false;
    }
    let norm = |s: &str| {
        s.to_lowercase()
            .replace("windows 11 ", "")
            .replace("windows 10 ", "")
            .trim()
            .to_string()
    };
    let alias = |s: &str| match s {
        "professional" => "pro".to_string(),
        "core" => "home".to_string(),
        "corecountryspecific" => "home single language".to_string(),
        "coresinglelanguage" => "home single language".to_string(),
        "professionaln" => "pro n".to_string(),
        _ => s.to_string(),
    };
    let target = alias(&norm(os_edition));
    if target.is_empty() {
        return false;
    }
    alias(&norm(edition)) == target || alias(&norm(name)) == target
}

/// Rank indexes best-first for a running OS (build, ubr, edition):
///   score 0: same build, same edition, revision <= UBR (closest to UBR first)
///   score 1: same build, same edition, any revision
///   score 2: same build, other editions
///   score 3: newer build, same edition
///   score 4: newer build, any edition
/// Older build lines are never tried (CBS payload pinning).
pub fn rank_indexes(ix: &[WimIndex], os_build: u32, os_ubr: u32, os_edition: &str) -> Vec<u32> {
    let mut scored: Vec<(i64, u32)> = ix
        .iter()
        .map(|w| {
            let same_build = w.build == os_build && os_build > 0;
            let newer = w.build > os_build && os_build > 0;
            let ed = edition_matches(&w.edition, &w.name, os_edition);
            let rev_pen: i64 = if same_build
                && w.revision > 0
                && w.revision <= os_ubr
            {
                (os_ubr - w.revision) as i64
            } else {
                1_000_000
            };
            let score: i64 = if same_build && ed {
                0
            } else if same_build {
                1
            } else if ed && newer {
                2
            } else if newer {
                3
            } else if same_build {
                1 // unknown build (ESD detail parse failed) -> same tier as same-build
            } else {
                9
            };
            (score * 10_000_000 + rev_pen, w.index)
        })
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    scored.into_iter().map(|(_, i)| i).collect()
}

// ---------- discovery ----------

const ISO_SKIP_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "node_modules",
    "sources",
];

fn iso_score(path: &str) -> i32 {
    let low = path.to_lowercase();
    let mut s = 0;
    if low.contains("windows") || low.contains("win11") || low.contains("win10") {
        s += 2;
    }
    s
}

fn walk_isos(dir: &str, depth: usize, max_depth: usize, out: &mut Vec<(u64, String)>) {
    if depth > max_depth || out.len() >= 10 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if p.is_dir() {
            if ISO_SKIP_DIRS.iter().any(|s| name == *s) {
                continue;
            }
            walk_isos(&p.to_string_lossy(), depth + 1, max_depth, out);
        } else if name.ends_with(".iso") {
            if let Ok(m) = e.metadata() {
                // real Windows ISOs are hundreds of MB; filter junk
                if m.len() >= 400 * 1024 * 1024 {
                    out.push((m.len(), p.to_string_lossy().to_string()));
                }
            }
        }
    }
}

/// All candidate image sources, best first: explicit --source path, then
/// mounted media (X:\sources\install.* on every drive incl. CD-ROM), then
/// Windows ISO files found shallow on the fixed drives + the user's Downloads.
pub fn discover(explicit: Option<&str>) -> Vec<ImageSource> {
    let mut out: Vec<ImageSource> = Vec::new();
    let mut push = |path: String, kind: ImgKind, by_us: bool| {
        if !out.iter().any(|s| s.path.eq_ignore_ascii_case(&path)) && Path::new(&path).is_file() {
            out.push(ImageSource {
                path,
                kind,
                mounted_by_us: by_us,
            });
        }
    };

    if let Some(p) = explicit {
        if let Some(k) = kind_for(p) {
            push(p.to_string(), k, false);
        }
    }
    // 1) already-mounted media / extracted folders
    for (letter, _t) in crate::winapi::drive_types() {
        for name in ["install.wim", "install.esd", "install.swm"] {
            let p = format!("{}\\sources\\{}", letter, name);
            if let Some(k) = kind_for(&name) {
                push(p, k, false);
            }
        }
    }
    // 2) ISO files: shallow walk of every fixed drive + Downloads
    let mut isos: Vec<(u64, String)> = Vec::new();
    for (letter, t) in crate::winapi::drive_types() {
        if t != 3 {
            continue; // walk fixed drives only
        }
        walk_isos(&format!("{}\\", letter), 0, 2, &mut isos);
        if isos.len() >= 10 {
            break;
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let dl = Path::new(&home).join("Downloads");
        if dl.is_dir() {
            walk_isos(&dl.to_string_lossy(), 1, 2, &mut isos);
        }
    }
    isos.sort_by(|a, b| iso_score(&b.1).cmp(&iso_score(&a.1)).then(b.0.cmp(&a.0)));
    for (_sz, p) in isos.into_iter().take(8) {
        if let Some(k) = kind_for(&p) {
            push(p, k, true);
        }
    }
    out
}

// ---------- mount / probe / source arg ----------

/// Mount an ISO via Mount-DiskImage; returns its drive letter as "X:".
pub fn mount_iso(iso: &str) -> Option<String> {
    let esc = iso.replace('\'', "''");
    let script = format!(
        "$d=(Mount-DiskImage -ImagePath '{}' -PassThru | Get-Volume).DriveLetter; if ($d) {{ Write-Output ('WULETTER=' + $d) }} else {{ exit 3 }}",
        esc
    );
    let r = runner::run_quiet(
        "src-mount-iso",
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script],
        &[0],
        240,
    );
    for line in r.output.lines() {
        if let Some(l) = line.trim().strip_prefix("WULETTER=") {
            let l = l.trim();
            if !l.is_empty() {
                return Some(format!("{}:", l));
            }
        }
    }
    None
}

/// Dismount an ISO previously mounted by mount_iso (same image path).
pub fn dismount_iso(iso: &str) -> bool {
    let esc = iso.replace('\'', "''");
    let script = format!("Dismount-DiskImage -ImagePath '{}' | Out-Null", esc);
    runner::run_quiet(
        "src-dismount-iso",
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script],
        &[0],
        180,
    )
    .ok
}

/// Enumerate the image's indexes. Detail probing uses dism /Get-WimInfo;
/// some ESDs reject /Index detail calls (DISM Error 3) - those indexes are
/// still usable as sources, so a fallback WimIndex with unknown fields is
/// produced from the listing when detail parsing fails.
pub fn probe_indexes(img: &str) -> Vec<WimIndex> {
    let mut out = Vec::new();
    let list = runner::run_quiet(
        "src-wiminfo-list",
        "dism.exe",
        &["/Get-WimInfo", &format!("/WimFile:{}", img)],
        &[0],
        300,
    );
    if !list.ok {
        return out;
    }
    for idx in parse_index_list(&list.output).into_iter().take(12) {
        let d = runner::run_quiet(
            &format!("src-wiminfo-{}", idx),
            "dism.exe",
            &[
                "/Get-WimInfo",
                &format!("/WimFile:{}", img),
                &format!("/Index:{}", idx),
            ],
            &[0],
            300,
        );
        match parse_index_detail(&d.output) {
            Some(w) => out.push(w),
            None => out.push(WimIndex {
                index: idx,
                name: String::new(),
                edition: String::new(),
                build: 0,
                revision: 0,
            }),
        }
    }
    out
}

/// The /Source argument for one image + index.
pub fn source_arg(kind: ImgKind, path: &str, index: u32) -> String {
    match kind {
        ImgKind::Esd => format!("/Source:esd:{}:{}", path, index),
        _ => format!("/Source:wim:{}:{}", path, index), // wim + swm
    }
}
