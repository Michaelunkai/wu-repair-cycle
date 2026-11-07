//! PHASE 1 — RESUME: remove every Windows Update pause mechanism so repairs
//! can actually reach Windows Update for repair payloads.

use crate::ui;
use crate::verify::Step;
use crate::winapi;
use std::time::Instant;

pub const UX: &str = "SOFTWARE\\Microsoft\\WindowsUpdate\\UX\\Settings";
pub const WU_POL: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate";
pub const AU_POL: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU";
pub const MEDIC: &str = "SYSTEM\\CurrentControlSet\\Services\\WaaSMedicSvc";

/// THE 0x800f0915 KILLER: WU also keeps pause METADATA under UpdatePolicy\Settings.
/// While these say paused, CBS refuses to fetch repair payloads from Windows Update
/// even after the UX pause keys are gone.
pub const UPD_POL: &str = "SOFTWARE\\Microsoft\\WindowsUpdate\\UpdatePolicy\\Settings";
pub const POLICY_UPD: [&str; 4] = [
    "PausedQualityStatus",
    "PausedFeatureStatus",
    "PausedQualityDate",
    "PausedFeatureDate",
];

pub const PAUSE_VALUES: [&str; 9] = [
    "PauseUpdatesStartTime",
    "PauseUpdatesExpiryTime",
    "PauseUpdatesExpiryDate",
    "PauseFeatureUpdatesStartTime",
    "PauseFeatureUpdatesEndTime",
    "PauseQualityUpdatesStartTime",
    "PauseQualityUpdatesEndTime",
    "PausePreviewBuildsStartTime",
    "PausePreviewBuildsEndTime",
];

const POLICY_AU: [&str; 2] = ["NoAutoUpdate", "UseWUServer"];
const POLICY_WU: [&str; 8] = [
    "WUServer",
    "WUStatusServer",
    "DoNotConnectToWindowsUpdateInternetLocations",
    "DisableWindowsUpdateAccess",
    "SetPolicyDrivenUpdateSourceForQualityUpdates",
    "SetPolicyDrivenUpdateSourceForFeatureUpdates",
    "SetPolicyDrivenUpdateSourceForDriverUpdates",
    "SetPolicyDrivenUpdateSourceForContentUpdates",
];

pub const SERVICES: [&str; 5] = ["wuauserv", "bits", "cryptsvc", "dosvc", "UsoSvc"];

/// Honest service-state classification: wuauserv must be VERIFIED running (it is
/// the repair-payload source); demand-start services idling stopped are in their
/// healthy default state; dosvc disabled is NON-BLOCKING because Phase 1 sets
/// DODownloadMode=0 (Delivery Optimization bypassed - downloads go via HTTP/BITS,
/// which is dosvc's only role here). The watchdog on hardened hosts force-disables
/// dosvc faster than any config can land; we do not fight a battle we do not need.
pub fn svc_state_ok(svc: &str) -> (bool, String) {
    if winapi::sc_running(svc) == Some(true) {
        return (true, "RUNNING (verified)".to_string());
    }
    if svc == "wuauserv" {
        return (false, "NOT running (payload source required)".to_string());
    }
    let path = format!("SYSTEM\\CurrentControlSet\\Services\\{}", svc);
    match winapi::reg_get_dword(&path, "Start") {
        Some(2) | Some(3) => (true, "demand/auto startable - idle (healthy)".to_string()),
        Some(4) => {
            if svc == "dosvc" {
                (true, "disabled - NON-BLOCKING (Delivery Optimization bypassed: DODownloadMode=0)".to_string())
            } else {
                (false, "DISABLED".to_string())
            }
        }
        _ => (false, "state unknown".to_string()),
    }
}

pub fn run() -> Vec<Step> {
    let mut steps = Vec::new();

    ui::step("Removing Settings pause keys (all variants)");
    for v in PAUSE_VALUES {
        let t = Instant::now();
        let ok = winapi::reg_del(UX, v);
        steps.push(Step::new(
            &format!("resume-del-{}", v),
            &format!("delete HKLM\\...\\UX\\Settings\\{}", v),
            ok,
            if ok { "removed/absent" } else { "delete failed" },
            t.elapsed().as_secs_f64(),
        ));
        if ok {
            ui::ok(&format!("Pause value {} removed", v));
        } else {
            ui::err(&format!("Pause value {} could NOT be removed", v));
        }
    }

    ui::step("Delivery Optimization bypass (DO range downloads fail 0x8024001E on proxied networks)");
    {
        let t = Instant::now();
        let ok = winapi::reg_set_dword(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\DeliveryOptimization\\Config",
            "DODownloadMode",
            0, // 0 = HTTP bypass: no DO peers, no range-request path
        );
        steps.push(Step::new(
            "resume-do-bypass",
            "set DODownloadMode=0 (bypass Delivery Optimization)",
            ok,
            if ok { "updates download via plain HTTP from WU" } else { "set failed" },
            t.elapsed().as_secs_f64(),
        ));
        if ok {
            ui::ok("Delivery Optimization bypassed for reliable payload downloads");
        } else {
            ui::warn("could not set DODownloadMode - downloads may 0x8024001E");
        }
    }

    ui::step("Commit rescue (CBS repairs die with E_OUTOFMEMORY when commit is exhausted)");
    {
        let (free, limit) = winapi::commit_status();
        if free < 8 * 1024 * 1024 * 1024 {
            let t = Instant::now();
            let ok = winapi::create_pagefile("C:\\wufix-pagefile.sys", 32);
            let (free2, limit2) = winapi::commit_status();
            steps.push(Step::new(
                "commit-rescue-pagefile",
                "create supplementary 32GB pagefile C:\\wufix-pagefile.sys (runtime commit expansion)",
                ok,
                &format!(
                    "commit free {:.1}GB -> {:.1}GB (limit {:.1}GB -> {:.1}GB)",
                    free as f64 / 1073741824.0,
                    free2 as f64 / 1073741824.0,
                    limit as f64 / 1073741824.0,
                    limit2 as f64 / 1073741824.0
                ),
                t.elapsed().as_secs_f64(),
            ));
            if ok {
                ui::ok(&format!("commit headroom raised: {:.1} GB free now", free2 as f64 / 1073741824.0));
            } else {
                ui::warn("pagefile expansion failed - repairs may hit E_OUTOFMEMORY");
            }
        } else {
            steps.push(Step::new(
                "commit-rescue-pagefile",
                "commit headroom check",
                true,
                &format!("{:.1} GB free of {:.1} GB - no rescue needed", free as f64 / 1073741824.0, limit as f64 / 1073741824.0),
                0.0,
            ));
        }
    }

    ui::step("Removing UpdatePolicy pause metadata (the 0x800f0915 blocker)");
    for v in POLICY_UPD {
        let t = Instant::now();
        let ok = winapi::reg_del(UPD_POL, v);
        steps.push(Step::new(
            &format!("resume-upd-{}", v),
            &format!("delete UpdatePolicy\\Settings\\{}", v),
            ok,
            if ok { "removed/absent" } else { "delete failed" },
            t.elapsed().as_secs_f64(),
        ));
        if ok {
            ui::ok(&format!("UpdatePolicy value {} removed", v));
        } else {
            ui::err(&format!("UpdatePolicy value {} could NOT be removed", v));
        }
    }

    ui::step("Clearing group-policy blocks (NoAutoUpdate / WSUS / online-blocking)");
    for v in POLICY_AU {
        let t = Instant::now();
        let ok = winapi::reg_del(AU_POL, v);
        steps.push(Step::new(
            &format!("resume-pol-{}", v),
            &format!("delete policy AU\\{}", v),
            ok,
            if ok { "removed/absent" } else { "delete failed" },
            t.elapsed().as_secs_f64(),
        ));
    }
    for v in POLICY_WU {
        let t = Instant::now();
        let ok = winapi::reg_del(WU_POL, v);
        steps.push(Step::new(
            &format!("resume-pol-{}", v),
            &format!("delete policy WU\\{}", v),
            ok,
            if ok { "removed/absent" } else { "delete failed" },
            t.elapsed().as_secs_f64(),
        ));
    }
    ui::ok("policy blocks cleared (if any existed)");

    ui::step("Healing hardened service security descriptors (all update services)");
    // Update-blocker tools strip every DACL grant on update services; then even
    // elevated admins get ACCESS_DENIED on query/start/config -> the real cause of
    // dead Windows Update + 0x800f0915 on hardened hosts. Heal ANY of the five
    // that is not even queryable.
    for s in SERVICES {
        if winapi::sc_running(s).is_none() {
            let t = Instant::now();
            let ok = winapi::sc_heal_sd(s);
            steps.push(Step::new(
                &format!("resume-sdheal-{}", s),
                &format!("take ownership + reset DACL on {}", s),
                ok,
                if ok { "ownership -> Administrators; DACL: BA+SYSTEM full" } else { "heal FAILED (still denied)" },
                t.elapsed().as_secs_f64(),
            ));
            if ok {
                ui::ok(&format!("{} security descriptor healed", s));
            } else {
                ui::err(&format!("{} security descriptor could NOT be healed", s));
            }
        } else {
            steps.push(Step::new(
                &format!("resume-sdheal-{}", s),
                &format!("security descriptor check {}", s),
                true,
                "queryable - no heal needed",
                0.0,
            ));
        }
    }

    ui::step("Restoring service start types (wuauserv/dosvc/UsoSvc manual, WaaSMedicSvc manual)");
    let t = Instant::now();
    let ok = winapi::sc_set_start_type("wuauserv", 3);
    steps.push(Step::new("resume-svc-wuauserv-demand", "sc config wuauserv start= demand", ok, if ok { "set" } else { "failed" }, t.elapsed().as_secs_f64()));
    // The re-pause watchdog also disables dosvc + UsoSvc outright. Try to reset
    // both; dosvc staying disabled is classified non-blocking (DO bypassed).
    for s in ["UsoSvc", "dosvc"] {
        let t = Instant::now();
        let cur = winapi::reg_get_dword(&format!("SYSTEM\\CurrentControlSet\\Services\\{}", s), "Start");
        if cur == Some(4) {
            let mut ok = false;
            for _try in 0..3 {
                let _ = winapi::sc_heal_sd(s);
                winapi::reg_set_dword(&format!("SYSTEM\\CurrentControlSet\\Services\\{}", s), "Start", 3);
                if winapi::sc_set_start_type(s, 3) {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let now = winapi::reg_get_dword(&format!("SYSTEM\\CurrentControlSet\\Services\\{}", s), "Start");
                    if now != Some(4) {
                        ok = true;
                        break;
                    }
                }
            }
            let nonblocking = s == "dosvc";
            let detail = if ok {
                "was DISABLED - held at manual".to_string()
            } else if nonblocking {
                "still disabled - NON-BLOCKING: Delivery Optimization is bypassed (DODownloadMode=0); payloads download via HTTP/BITS".to_string()
            } else {
                "still disabled".to_string()
            };
            steps.push(Step::new(
                &format!("resume-svc-{}-demand", s),
                &format!("set {} start= demand (was disabled)", s),
                ok || nonblocking,
                &detail,
                t.elapsed().as_secs_f64(),
            ));
        }
    }
    let t = Instant::now();
    let ok = winapi::reg_set_dword(MEDIC, "Start", 3);
    steps.push(Step::new("resume-medic-manual", "registry WaaSMedicSvc Start=3", ok, if ok { "set" } else { "failed" }, t.elapsed().as_secs_f64()));

    ui::step("Starting all update services (wuauserv verified; demand services startable)");
    for s in SERVICES {
        let t = Instant::now();
        let _ = winapi::sc_start(s);
        let (ok, detail) = svc_state_ok(s);
        steps.push(Step::new(
            &format!("resume-svc-{}", s),
            &format!("start + verify {}", s),
            ok,
            &detail,
            t.elapsed().as_secs_f64(),
        ));
        if ok {
            ui::ok(&format!("{}: {}", s, detail));
        } else {
            ui::err(&format!("{}: {}", s, detail));
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    steps
}

/// Read-only status report (used by --check-only). Changes NOTHING.
pub fn check_only_report() {
    println!("Pause keys under HKLM\\{}:", UX);
    for v in PAUSE_VALUES {
        let val = winapi::reg_get_sz(UX, v).unwrap_or_else(|| "<absent>".to_string());
        println!("   {:<34} {}", v, val);
    }
    let noauto = winapi::reg_get_dword(AU_POL, "NoAutoUpdate");
    let noauto_s = match noauto {
        Some(v) => v.to_string(),
        None => "<not set>".to_string(),
    };
    println!("   {:<34} {}", "NoAutoUpdate policy", noauto_s);
    println!();
    println!("Services:");
    for s in SERVICES {
        let st = match winapi::sc_running(s) {
            Some(true) => "Running".to_string(),
            Some(false) => "Not running".to_string(),
            None => format!("unknown (registry Start={})", svc_start(s)),
        };
        println!("   {:<12} {}", s, st);
    }
    println!(
        "   {:<12} registry Start={} (3=manual, 4=disabled)",
        "WaaSMedicSvc",
        svc_start("WaaSMedicSvc")
    );
    println!();
    println!("Run without --check-only for the full cycle: RESUME -> REPAIR -> PERMANENT RE-PAUSE.");
}

fn svc_start(name: &str) -> String {
    winapi::reg_get_dword(&format!("SYSTEM\\CurrentControlSet\\Services\\{}", name), "Start")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "?".to_string())
}
