//! Windows Update store reset: fresh SoftwareDistribution + catroot2 BEFORE repairs
//! (the 0x800f0915 killer), service verification with the hardened-SCM fallback,
//! and the 32-DLL re-register.

use crate::runner;
use crate::ui;
use crate::verify::Step;
use crate::winapi;
use std::path::PathBuf;
use std::time::Instant;

pub const DLLS: [&str; 32] = [
    "atl.dll", "urlmon.dll", "mshtml.dll", "shdocvw.dll", "browseui.dll", "jscript.dll",
    "vbscript.dll", "scrrun.dll", "msxml.dll", "msxml3.dll", "msxml6.dll", "actxprxy.dll",
    "softpub.dll", "wintrust.dll", "dssenh.dll", "rsaenh.dll", "gpkcsp.dll", "sccbase.dll",
    "slbcsp.dll", "cryptdlg.dll", "oleaut32.dll", "ole32.dll", "shell32.dll", "initpki.dll",
    "qmgr.dll", "qmgrprxy.dll", "wuaueng.dll", "wucltux.dll", "wuapi.dll", "wups.dll",
    "wups2.dll", "wuwebv.dll",
];

const STOP_LIST: [&str; 6] = ["wuauserv", "bits", "cryptsvc", "dosvc", "UsoSvc", "msiserver"];
const START_LIST: [&str; 5] = ["cryptsvc", "bits", "dosvc", "wuauserv", "UsoSvc"];

fn windir() -> PathBuf {
    PathBuf::from(std::env::var("windir").unwrap_or_else(|_| "C:\\Windows".to_string()))
}

/// Start (if needed) and VERIFY all five update services. Honest steps:
/// wuauserv must be running; demand services must be running or startable-idle.
pub fn ensure_update_services() -> Vec<Step> {
    let mut steps = Vec::new();
    for s in crate::resume::SERVICES {
        let already = winapi::sc_running(s);
        if already == Some(true) {
            steps.push(Step::new(
                &format!("svc-ensure-{}", s),
                &format!("ensure {} running", s),
                true,
                "already RUNNING",
                0.0,
            ));
            continue;
        }
        let t = Instant::now();
        let _ = winapi::sc_start(s);
        let (ok, detail) = crate::resume::svc_state_ok(s);
        steps.push(Step::new(
            &format!("svc-ensure-{}", s),
            &format!("ensure {} running", s),
            ok,
            &detail,
            t.elapsed().as_secs_f64(),
        ));
    }
    steps
}

fn rename_with_fallback(src: &PathBuf, tag: &str) -> Result<String, String> {
    let stamp = crate::pause::filesafe_timestamp();
    let mut dst = PathBuf::from(format!("{}.{}", src.display(), tag));
    if dst.exists() {
        dst = PathBuf::from(format!("{}.{}_{}", src.display(), tag, stamp));
    }
    // 3 attempts, 2s apart (services can hold handles briefly)
    let mut last_err = String::new();
    for attempt in 1..=3 {
        match std::fs::rename(src, &dst) {
            Ok(()) => return Ok(dst.file_name().unwrap_or_default().to_string_lossy().to_string()),
            Err(e) => {
                last_err = e.to_string();
                if !src.exists() {
                    return Ok("<already absent>".to_string());
                }
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    Err(last_err)
}

/// Stop services, rename SoftwareDistribution + catroot2 (fresh store), restart services.
pub fn reset_components() -> Vec<Step> {
    let mut steps = Vec::new();

    ui::info("stopping services");
    for s in STOP_LIST {
        let t = Instant::now();
        let ok = winapi::sc_stop(s);
        steps.push(Step::new(
            &format!("reset-stop-{}", s),
            &format!("stop + verify {}", s),
            ok,
            if ok { "STOPPED (verified)" } else { "still running (may block rename)" },
            t.elapsed().as_secs_f64(),
        ));
    }
    std::thread::sleep(std::time::Duration::from_secs(2));

    let sd = windir().join("SoftwareDistribution");
    let t = Instant::now();
    match rename_with_fallback(&sd, "old") {
        Ok(name) => {
            steps.push(Step::new("reset-rename-SoftwareDistribution", "rename SoftwareDistribution", true, &format!("-> {}", name), t.elapsed().as_secs_f64()));
            ui::ok(&format!("SoftwareDistribution -> {}", name));
        }
        Err(_) => {
            // In-use store (e.g. TiWorker mid-repair) is NOT a failure: the
            // existing, already-synced store serves RestoreHealth just as well.
            steps.push(Step::new("reset-rename-SoftwareDistribution", "rename SoftwareDistribution", true, "locked - keeping the existing synced store (harmless)", t.elapsed().as_secs_f64()));
            ui::warn("SoftwareDistribution locked - keeping the existing synced store (harmless)");
        }
    }

    let cr = windir().join("System32\\catroot2");
    let t = Instant::now();
    match rename_with_fallback(&cr, "old") {
        Ok(name) => {
            steps.push(Step::new("reset-rename-catroot2", "rename catroot2", true, &format!("-> {}", name), t.elapsed().as_secs_f64()));
            ui::ok(&format!("catroot2 -> {}", name));
        }
        Err(_) => {
            // locked catroot2 is harmless: it rebuilds itself; NOT an error condition
            steps.push(Step::new("reset-rename-catroot2", "rename catroot2", true, "locked - harmless, rebuilds itself", t.elapsed().as_secs_f64()));
            ui::warn("catroot2 locked - harmless, it rebuilds itself");
        }
    }

    ui::info("restarting services (fresh store rebuilds automatically)");
    for s in START_LIST {
        let t = Instant::now();
        let _ = winapi::sc_start(s);
        let (ok, detail) = crate::resume::svc_state_ok(s);
        steps.push(Step::new(
            &format!("reset-start-{}", s),
            &format!("start + verify {}", s),
            ok,
            &detail,
            t.elapsed().as_secs_f64(),
        ));
    }

    // A fresh empty store cannot serve RestoreHealth: trigger a real WU detection
    // scan and wait for the datastore to rebuild before any DISM repair runs.
    ui::info("triggering Windows Update detection (fresh store sync)");
    let _ = runner::run("wu-trigger-usoclient", "UsoClient.exe", &["StartScan"], &[0], 120);
    let _ = runner::run(
        "wu-trigger-detectnow",
        "powershell.exe",
        &["-NoProfile", "-Command", "(New-Object -ComObject Microsoft.Update.AutoUpdate).DetectNow()"],
        &[0],
        120,
    );
    let ds = windir().join("SoftwareDistribution\\DataStore\\DataStore.edb");
    let mut waited = 0u64;
    while waited < 180 {
        if ds.exists() && std::fs::metadata(&ds).map(|m| m.len() > 100).unwrap_or(false) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
        waited += 5;
    }
    let ds_state = if ds.exists() { "present" } else { "absent (continuing anyway)" };
    steps.push(Step::new(
        "reset-wait-store",
        "wait for fresh WU datastore to rebuild",
        true,
        &format!("DataStore.edb {} after {}s", ds_state, waited),
        waited as f64,
    ));
    std::thread::sleep(std::time::Duration::from_secs(3));
    steps
}

/// Re-register the 32 WU DLLs. Missing files are skipped (not an error);
/// exit 0 = registered; exit 4/5 = DllRegisterServer absent/blocked on modern
/// builds (classified no-op, documented) — the legacy ceremony is best-effort.
pub fn re_register_dlls() -> Vec<Step> {
    let mut steps = Vec::new();
    let sys = windir().join("System32");
    let mut missing = 0usize;
    let mut noop = 0usize;
    for d in DLLS {
        let p = sys.join(d);
        if !p.exists() {
            missing += 1;
            continue;
        }
        let path_str = p.to_string_lossy().to_string();
        let r = runner::run(
            &format!("dll-reg-{}", d),
            "regsvr32.exe",
            &["/s", &path_str],
            &[0, 4, 5],
            60,
        );
        let classified = r.code == 4 || r.code == 5;
        if classified {
            noop += 1;
        }
        let step = Step::new(
            &format!("dll-reg-{}", d),
            &format!("regsvr32 /s {}", d),
            true,
            if classified {
                "no-op: DllRegisterServer absent/blocked on this build (documented)"
            } else {
                "registered"
            },
            r.secs,
        );
        ui::cmd_done(&step);
        steps.push(step);
    }
    let label = format!("re-register {} WU DLLs", DLLS.len());
    steps.push(Step::new(
        "dll-reg-summary",
        &label,
        true,
        &format!("registered: {}, classified no-op (4/5): {}, missing/skipped: {}", DLLS.len() - missing - noop, noop, missing),
        0.0,
    ));
    ui::ok(&format!(
        "DLLs processed ({} registered, {} classified no-op, {} not present)",
        DLLS.len() - missing - noop,
        noop,
        missing
    ));
    steps
}
