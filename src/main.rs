use std::time::Instant;
use wu_repair_cycle::{pause, repair, resume, runner, ui, verify, winapi};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    ui::enable();

    // --source <path> / --source=<path>: point RestoreHealth at a local image
    // (install.wim/.esd/.swm or an ISO, mounted on demand) when Windows Update
    // has no repair content (0x800f0915).
    let mut explicit_source: Option<String> = None;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--help" || a == "-h" {
            println!("wu-repair-cycle [--dry-run | --check-only | --pause-only] [--source <wim|esd|swm|iso>]\n  (no flags = full elevated repair cycle)");
            std::process::exit(0);
        } else if a == "--source" {
            match it.next() {
                Some(v) => explicit_source = Some(v.clone()),
                None => {
                    eprintln!("--source needs a path; refusing to start a full run");
                    std::process::exit(2);
                }
            }
        } else if let Some(v) = a.strip_prefix("--source=") {
            explicit_source = Some(v.to_string());
        } else if a != "--check-only" && a != "--dry-run" && a != "--pause-only" {
            eprintln!("unknown argument '{}'; refusing to start a full run (try --help)", a);
            std::process::exit(2);
        }
    }

    if args.iter().any(|a| a == "--check-only") {
        ui::banner("WU-Repair-Cycle v2.1.0 — CHECK ONLY (read-only, nothing changes)");
        resume::check_only_report();
        std::process::exit(0);
    }

    if args.iter().any(|a| a == "--dry-run") {
        dry_run();
        std::process::exit(0);
    }

    // Safeguard mode: skip repairs, only guarantee the permanent pause. Used by
    // the supervisor as the last-resort end state when repair attempts die.
    if args.iter().any(|a| a == "--pause-only") {
        if !winapi::is_admin() {
            println!("Requesting administrator rights...");
            winapi::relaunch_elevated();
        }
        ui::banner("PAUSE ONLY — permanently pause Windows Update (no repairs)");
        runner::transcript_line("=== PAUSE-ONLY run ===");
        let steps = pause::run();
        let code = verify::gate(&steps);
        ui::summary(&steps, 0, false);
        runner::transcript_line(&format!(
            "FINAL gate={} steps={} failed={}",
            code,
            steps.len(),
            steps.iter().filter(|s| !s.ok).count()
        ));
        std::process::exit(code);
    }

    // Full cycle — self-elevate first.
    if !winapi::is_admin() {
        println!("Requesting administrator rights...");
        winapi::relaunch_elevated();
    }

    let t0 = Instant::now();
    runner::transcript_line("=== WU-Repair-Cycle v2.1.0 full run ===");
    if let Some(s) = &explicit_source {
        runner::transcript_line(&format!("explicit source: {}", s));
    }

    ui::banner("PHASE 1 / 3 — RESUME Windows Update (remove every pause mechanism)");
    runner::transcript_line("=== PHASE 1: RESUME ===");
    let mut steps = resume::run();

    ui::banner("PHASE 2 / 3 — REPAIR: every corruption command, zero-error gate");
    runner::transcript_line("=== PHASE 2: REPAIR ===");
    let (rsteps, reboot) = repair::run_all(explicit_source.as_deref());
    steps.extend(rsteps);

    ui::banner("PHASE 3 / 3 — PAUSE Windows Update PERMANENTLY (until 2099)");
    runner::transcript_line("=== PHASE 3: PAUSE ===");
    steps.extend(pause::run());

    let code = verify::gate(&steps);
    ui::summary(&steps, t0.elapsed().as_secs(), reboot);
    runner::transcript_line(&format!(
        "FINAL gate={} steps={} failed={}",
        code,
        steps.len(),
        steps.iter().filter(|s| !s.ok).count()
    ));
    println!("Transcript: {}", runner::transcript_name());
    if code == 0 {
        ui::ok("Windows updates permanently paused - returning to shell.");
        ui::ok("Your PC stays on: nothing in this tool restarts, shuts down, or logs off Windows - a reboot is only ever your own choice.");
    } else {
        ui::err("Some steps FAILED - see the table above and the transcript log.");
    }
    println!();
    println!("Closing in 30 seconds (read the table above; the full transcript stays at the path shown)...");
    for i in (1..=30).rev() {
        print!("\r{:>3}s ", i);
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    std::process::exit(code);
}

/// Print the complete matrix without executing anything. Proves coverage.
fn dry_run() {
    ui::banner("DRY RUN — the full command matrix, NOTHING is executed");
    let mut steps: Vec<verify::Step> = Vec::new();
    fn add(id: &str, label: &str, steps: &mut Vec<verify::Step>) {
        let s = verify::Step::new(id, label, true, "dry-run", 0.0);
        ui::cmd_done(&s);
        steps.push(s);
    }

    ui::step("PHASE 1 — RESUME (registry + services)");
    for v in resume::PAUSE_VALUES {
        add(
            &format!("resume-del-{}", v),
            &format!("delete HKLM pause value {}", v),
            &mut steps,
        );
    }
    for p in [
        "policy AU\\NoAutoUpdate",
        "policy AU\\UseWUServer",
        "policy WU\\WUServer",
        "policy WU\\WUStatusServer",
        "policy WU\\DoNotConnectToWindowsUpdateInternetLocations",
        "policy WU\\DisableWindowsUpdateAccess",
        "policy WU\\SetPolicyDrivenUpdateSource x4",
    ] {
        add(
            &format!("resume-pol-{}", p.replace('\\', "-")),
            p,
            &mut steps,
        );
    }
    add(
        "resume-svc-wuauserv-demand",
        "sc config wuauserv start= demand",
        &mut steps,
    );
    add(
        "resume-medic-manual",
        "registry WaaSMedicSvc Start=3",
        &mut steps,
    );
    for s in resume::SERVICES {
        add(
            &format!("resume-svc-{}", s),
            &format!("start + verify {}", s),
            &mut steps,
        );
    }

    ui::step("PHASE 2 — REPAIR MATRIX (R-pre, R0-R11)");
    add(
        "net-connectivity",
        "curl msftconnecttest + windowsupdate.com (repair-payload download path)",
        &mut steps,
    );
    add(
        "reset-components",
        "stop svcs, rename SoftwareDistribution + catroot2, restart svcs",
        &mut steps,
    );
    add(
        "dism-analyze",
        "dism /Online /Cleanup-Image /AnalyzeComponentStore",
        &mut steps,
    );
    add(
        "dism-checkhealth",
        "dism /Online /Cleanup-Image /CheckHealth",
        &mut steps,
    );
    add(
        "dism-scanhealth",
        "dism /Online /Cleanup-Image /ScanHealth",
        &mut steps,
    );
    add("dism-restore", "dism /Online /Cleanup-Image /RestoreHealth (3-attempt ladder, fresh store between attempts)", &mut steps);
    add(
        "dism-log-forensics",
        "parse CBS + DISM logs for the exact missing-payload identities",
        &mut steps,
    );
    add("dism-restore-src", "dism /RestoreHealth /Source (local image ladder: mounted media, auto-mounted ISOs, --source path)", &mut steps);
    add(
        "sfc-scannow",
        "sfc /scannow (+ extra restore+sfc pair if it cannot fix)",
        &mut steps,
    );
    add(
        "sfc-verifyonly",
        "sfc /verifyonly (falls back to /scannow only when CBS is pending)",
        &mut steps,
    );
    let sysdrive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    for d in winapi::fixed_drives() {
        let is_sys = d == sysdrive;
        add(
            &format!("chkdsk-{}-scan", d),
            &format!(
                "chkdsk {} /scan{}",
                d,
                if is_sys {
                    " (+ boot spotfix scheduling if errors found - never live dismount)"
                } else {
                    ""
                }
            ),
            &mut steps,
        );
    }
    add(
        "dll-reg",
        "regsvr32 /s x 32 Windows Update DLLs",
        &mut steps,
    );
    add(
        "dism-componentcleanup",
        "dism /Online /Cleanup-Image /StartComponentCleanup",
        &mut steps,
    );
    add(
        "dism-spsuperseded",
        "dism /Online /Cleanup-Image /SPSuperseded (no-op classified OK)",
        &mut steps,
    );
    add(
        "dism-checkhealth-final",
        "dism /Online /Cleanup-Image /CheckHealth (final health gate)",
        &mut steps,
    );
    add(
        "sfc-verifyonly-final",
        "sfc /verifyonly (final)",
        &mut steps,
    );

    ui::step("PHASE 3 — PAUSE PERMANENTLY (until 2099)");
    add(
        "pause-keys",
        "9 pause values (incl PauseUpdatesExpiryDate) + FlightSettingsMaxPauseDays=3650",
        &mut steps,
    );
    add(
        "pause-NoAutoUpdate",
        "HKLM policy AU\\NoAutoUpdate=1",
        &mut steps,
    );
    add(
        "pause-stop-services",
        "stop wuauserv, bits, cryptsvc, dosvc, UsoSvc (verified)",
        &mut steps,
    );
    add(
        "pause-disable-wuauserv",
        "sc config wuauserv start= disabled",
        &mut steps,
    );
    add(
        "pause-disable-medic",
        "registry WaaSMedicSvc Start=4",
        &mut steps,
    );

    ui::summary(&steps, 0, false);
    ui::info("Run without flags (elevates via UAC) for the real zero-error repair cycle.");
    ui::info("If DISM reports 0x800f0915 (repair content not found), add: --source <install.wim|.esd|.swm or Windows ISO>");
}
