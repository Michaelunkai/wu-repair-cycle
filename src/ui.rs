//! Beautiful console output — 24-bit ANSI with a plain-text fallback when VT is unavailable.

use crate::verify::Step;
use std::sync::atomic::{AtomicBool, Ordering};

static VT: AtomicBool = AtomicBool::new(false);

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
    let line: String = std::iter::repeat(ch).take(74).collect();
    println!("{}", cyan(&line));
}

pub fn banner(title: &str) {
    println!();
    rule("=");
    println!("  {}", cyan(&bold(title)));
    rule("=");
}

pub fn step(msg: &str) {
    println!("\n{} {}", bold("[*]"), msg);
}

pub fn info(msg: &str) {
    println!("    {}", dim(msg));
}

pub fn ok(msg: &str) {
    println!("    {} {}", green("[ok]"), msg);
}

pub fn warn(msg: &str) {
    println!("    {} {}", yellow("[!]"), msg);
}

pub fn err(msg: &str) {
    println!("    {} {}", red("[X]"), msg);
}

/// One line per executed command — the live heart of the output.
pub fn cmd_done(s: &Step) {
    let status = if s.ok {
        green(&format!("PASS",))
    } else {
        red("FAIL")
    };
    println!(
        "  {} {}{}  {}  {}  {}",
        status,
        bold(&s.id),
        dim(&format!("  {}", s.label)),
        wrap("2", &format!("{:6.1}s", s.secs)),
        if s.detail.is_empty() { "" } else { "—" },
        dim(&s.detail)
    );
}

/// The final zero-error table.
pub fn summary(steps: &[Step], elapsed_secs: u64, reboot: bool) {
    banner("DONE — ZERO-ERROR GATE");
    println!();
    let id_w = steps.iter().map(|s| s.id.len()).max().unwrap_or(4).max(4);
    for s in steps {
        let status = if s.ok {
            green("PASS")
        } else {
            red("FAIL")
        };
        println!(
            "  {} {:<id_w$}  {:>6.1}s  {}",
            status,
            s.id,
            s.secs,
            dim(&s.detail),
            id_w = id_w
        );
    }
    println!();
    rule("-");
    let failed = steps.iter().filter(|s| !s.ok).count();
    if failed == 0 {
        println!(
            "  {}",
            green(&bold(&format!(
                "ALL {} STEPS PASSED — ZERO ERRORS",
                steps.len()
            )))
        );
    } else {
        println!(
            "  {}",
            red(&bold(&format!(
                "{} OF {} STEPS FAILED",
                failed,
                steps.len()
            )))
        );
    }
    println!("  Runtime: {}m {:02}s", elapsed_secs / 60, elapsed_secs % 60);
    if reboot {
        println!("  {}", yellow("REBOOT RECOMMENDED — repairs completed that need it."));
    }
    println!();
}
