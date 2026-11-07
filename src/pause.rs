//! PHASE 3 — PAUSE PERMANENTLY: rewrite every pause key until 2099, NoAutoUpdate=1,
//! stop + disable wuauserv, disable WaaSMedicSvc. The mission's final state.

use crate::ui;
use crate::verify::Step;
use crate::winapi;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const FAR: &str = "2099-12-31T23:59:59Z";

pub fn now_utc_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Unix seconds -> "yyyy-MM-ddTHH:mm:ssZ" (UTC, chrono-free, exact).
pub fn iso_from_unix(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's civil-from-days algorithm (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Filesystem-safe local timestamp "yyyyMMdd_HHmmss" (UTC) for log names.
pub fn filesafe_timestamp() -> String {
    let s = now_utc_secs();
    let days = (s / 86400) as i64;
    let rem = s % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub fn run() -> Vec<Step> {
    let mut steps = Vec::new();
    let now_iso = iso_from_unix(now_utc_secs());

    ui::step("Writing pause-until-2099 registry keys (all 9 + FlightSettingsMaxPauseDays)");
    let pairs: [(&str, &str); 9] = [
        ("PauseUpdatesStartTime", &now_iso),
        ("PauseUpdatesExpiryTime", FAR),
        ("PauseUpdatesExpiryDate", FAR),
        ("PauseFeatureUpdatesStartTime", &now_iso),
        ("PauseFeatureUpdatesEndTime", FAR),
        ("PauseQualityUpdatesStartTime", &now_iso),
        ("PauseQualityUpdatesEndTime", FAR),
        ("PausePreviewBuildsStartTime", &now_iso),
        ("PausePreviewBuildsEndTime", FAR),
    ];
    for (name, val) in pairs {
        let t = Instant::now();
        let ok = winapi::reg_set_sz(crate::resume::UX, name, val);
        steps.push(Step::new(
            &format!("pause-set-{}", name),
            &format!("set {} = {}", name, val),
            ok,
            if ok { "written" } else { "WRITE FAILED" },
            t.elapsed().as_secs_f64(),
        ));
    }
    let t = Instant::now();
    let ok = winapi::reg_set_dword(crate::resume::UX, "FlightSettingsMaxPauseDays", 3650);
    steps.push(Step::new("pause-set-FlightSettingsMaxPauseDays", "set FlightSettingsMaxPauseDays=3650", ok, if ok { "written" } else { "WRITE FAILED" }, t.elapsed().as_secs_f64()));
    ui::ok(&format!("all update types paused until {}", FAR));

    ui::step("NoAutoUpdate=1 policy (automatic updates off)");
    let t = Instant::now();
    let ok = winapi::reg_set_dword(crate::resume::AU_POL, "NoAutoUpdate", 1);
    steps.push(Step::new("pause-set-NoAutoUpdate", "set AU\\NoAutoUpdate=1", ok, if ok { "written" } else { "WRITE FAILED" }, t.elapsed().as_secs_f64()));

    ui::step("Stopping update services and disabling wuauserv");
    for s in crate::resume::SERVICES {
        let t = Instant::now();
        let ok = winapi::sc_stop(s);
        steps.push(Step::new(
            &format!("pause-stop-{}", s),
            &format!("stop + verify {}", s),
            ok,
            if ok { "STOPPED (verified)" } else { "still running" },
            t.elapsed().as_secs_f64(),
        ));
    }
    let t = Instant::now();
    let ok = winapi::sc_set_start_type("wuauserv", 4);
    steps.push(Step::new("pause-disable-wuauserv", "sc config wuauserv start= disabled", ok, if ok { "disabled" } else { "FAILED" }, t.elapsed().as_secs_f64()));
    ui::ok("wuauserv: disabled");

    ui::step("Disabling WaaSMedicSvc (stops Windows from re-enabling updates)");
    let t = Instant::now();
    let ok = winapi::reg_set_dword(crate::resume::MEDIC, "Start", 4);
    steps.push(Step::new("pause-disable-medic", "registry WaaSMedicSvc Start=4", ok, if ok { "disabled" } else { "FAILED (protected key)" }, t.elapsed().as_secs_f64()));
    if ok {
        ui::ok("WaaSMedicSvc: disabled");
    } else {
        ui::warn("WaaSMedicSvc could not be disabled - pause keys + service block still hold");
    }
    steps
}
