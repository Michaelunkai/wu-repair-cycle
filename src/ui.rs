//! Beautiful console output — 24-bit ANSI with a plain-text fallback when VT is unavailable.

use crate::verify::Step;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static VT: AtomicBool = AtomicBool::new(false);

// Optional console mirror: when WUFIX_CONSOLE_MIRROR points at a file, every
// complete line the UI prints is appended there too (used for unattended
// verification that the console never goes silent).
static MIRROR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

fn mirror_file() -> &'static Option<std::path::PathBuf> {
    MIRROR.get_or_init(|| {
        std::env::var("WUFIX_CONSOLE_MIRROR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
    })
}

fn mirror_line(s: &str) {
    if let Some(p) = mirror_file() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            let _ = std::io::Write::write_all(&mut f, format!("{}\n", s).as_bytes());
        }
    }
}

/// The single console emission point: prints the line and mirrors it.
pub fn out(s: &str) {
    println!("{}", s);
    mirror_line(s);
}

pub fn enable() {
    let ok = crate::winapi::enable_vt();
    VT.store(ok, Ordering::Relaxed);
}

fn vt() -> bool {
    VT.load(Ordering::Relaxed)
}

fn wrap(pre: &str, s: &str) -> String {
    if vt() {
        format!("\x1b[{}m{}\x1b[0m", pre, s)
    } else {
        s.to_string()
    }
}

fn cyan(s: &str) -> String {
    wrap("38;2;66;165;245", s)
}
fn green(s: &str) -> String {
    wrap("38;2;76;175;80", s)
}
fn red(s: &str) -> String {
    wrap("38;2;239;83;80", s)
}
fn yellow(s: &str) -> String {
    wrap("38;2;255;193;7", s)
}
fn bold(s: &str) -> String {
    wrap("1", s)
}
fn dim(s: &str) -> String {
    wrap("2", s)
}

fn rule(ch: &str) {
    let line: String = ch.repeat(74);
    out(&cyan(&line));
}

pub fn banner(title: &str) {
    out("");
    rule("=");
    out(&format!("  {}", cyan(&bold(title))));
    rule("=");
}

pub fn step(msg: &str) {
    out(&format!("\n{} {}", bold("[*]"), msg));
}

pub fn info(msg: &str) {
    out(&format!("    {}", dim(msg)));
}

pub fn ok(msg: &str) {
    out(&format!("    {} {}", green("[ok]"), msg));
}

pub fn warn(msg: &str) {
    out(&format!("    {} {}", yellow("[!]"), msg));
}

pub fn err(msg: &str) {
    out(&format!("    {} {}", red("[X]"), msg));
}

/// Heartbeat: a dim liveness line. The console must never go silent for more
/// than 10 seconds — every long wait calls this on that cadence.
pub fn heartbeat(msg: &str) {
    out(&format!("  {} {}", cyan("[..]"), dim(msg)));
}

/// Wait `total_secs` one second at a time, emitting a heartbeat every 8 s so
/// the console never goes silent during planned waits.
pub fn wait_steps(total_secs: u64, label: &str) {
    let mut since = 0u64;
    for _ in 0..total_secs {
        std::thread::sleep(std::time::Duration::from_secs(1));
        since += 1;
        if since.is_multiple_of(8) {
            heartbeat(&format!("{} — {}s of {}s", label, since, total_secs));
        }
    }
}

/// One line per executed command — the live heart of the output.
pub fn cmd_done(s: &Step) {
    let status = if s.ok { green("PASS") } else { red("FAIL") };
    out(&format!(
        "  {} {}{}  {}  {}  {}",
        status,
        bold(&s.id),
        dim(&format!("  {}", s.label)),
        wrap("2", &format!("{:6.1}s", s.secs)),
        if s.detail.is_empty() { "" } else { "—" },
        dim(&s.detail)
    ));
}

/// The final zero-error table.
pub fn summary(steps: &[Step], elapsed_secs: u64, reboot: bool) {
    banner("DONE — ZERO-ERROR GATE");
    out("");
    let id_w = steps.iter().map(|s| s.id.len()).max().unwrap_or(4).max(4);
    for s in steps {
        let status = if s.ok { green("PASS") } else { red("FAIL") };
        out(&format!(
            "  {} {:<id_w$}  {:>6.1}s  {}",
            status,
            s.id,
            s.secs,
            dim(&s.detail),
            id_w = id_w
        ));
    }
    out("");
    rule("-");
    let failed = steps.iter().filter(|s| !s.ok).count();
    if failed == 0 {
        out(&format!(
            "  {}",
            green(&bold(&format!(
                "ALL {} STEPS PASSED — ZERO ERRORS",
                steps.len()
            )))
        ));
    } else {
        out(&format!(
            "  {}",
            red(&bold(&format!(
                "{} OF {} STEPS FAILED",
                failed,
                steps.len()
            )))
        ));
    }
    out(&format!(
        "  Runtime: {}m {:02}s",
        elapsed_secs / 60,
        elapsed_secs % 60
    ));
    if reboot {
        out(&format!(
            "  {}",
            yellow("REBOOT RECOMMENDED — repairs completed that need it.")
        ));
    }
    out("");
}
