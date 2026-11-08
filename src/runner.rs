//! Command executor: live line passthrough, timeout kill, exit-code classification,
//! timestamped transcript log. Everything the matrix runs goes through here.

use crate::verify::Step;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub struct CmdResult {
    pub id: String,
    pub program: String,
    pub args: Vec<String>,
    pub code: i32,
    pub ok: bool,
    pub secs: f64,
    pub output: String,
}

fn log_path() -> &'static PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        let ts = crate::pause::filesafe_timestamp();
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        dir.join(format!("wufix-{}.log", ts))
    })
}

pub fn transcript_line(s: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{}", s);
    }
}

pub fn transcript_name() -> String {
    log_path().to_string_lossy().to_string()
}

/// Decode a live chunk for display (ASCII/ANSI fast path).
fn decode_live(chunk: &[u8]) -> String {
    String::from_utf8_lossy(chunk).to_string()
}

/// Heuristic: output is UTF-16LE when it has a BOM or lots of NUL bytes.
fn looks_utf16(raw: &[u8]) -> bool {
    if raw.len() < 2 {
        return false;
    }
    if raw[0] == 0xFF && raw[1] == 0xFE {
        return true;
    }
    let nul = raw.iter().filter(|&&b| b == 0).count();
    nul * 4 > raw.len() // >25% zero bytes
}

/// Run one command. Output streams LIVE to the console while being accumulated.
/// ok = exit code in ok_codes AND not timed out.
pub fn run(id: &str, program: &str, args: &[&str], ok_codes: &[i32], timeout_secs: u64) -> CmdResult {
    run_inner(id, program, args, ok_codes, timeout_secs, false)
}

/// Run one command QUIETLY: output is captured to the transcript only, never
/// printed to the console (used for commands whose raw output contains scary
/// text — DISM restore attempts, chkdsk findings — so the console only ever
/// shows the classified result).
pub fn run_quiet(id: &str, program: &str, args: &[&str], ok_codes: &[i32], timeout_secs: u64) -> CmdResult {
    run_inner(id, program, args, ok_codes, timeout_secs, true)
}

fn run_inner(id: &str, program: &str, args: &[&str], ok_codes: &[i32], timeout_secs: u64, quiet: bool) -> CmdResult {
    let t0 = Instant::now();
    transcript_line(&format!("[{}] RUN {} {}{}", id, program, args.join(" "), if quiet { " (captured)" } else { "" }));
    let spawned = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            transcript_line(&format!("[{}] SPAWN-ERROR {}", id, e));
            return CmdResult {
                id: id.to_string(),
                program: program.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                code: -1,
                ok: false,
                secs: 0.0,
                output: format!("spawn error: {}", e),
            };
        }
    };
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let tx_out = tx.clone();
    let h_out = std::thread::spawn(move || {
        if let Some(p) = &mut out_pipe {
            let mut buf = [0u8; 8192];
            loop {
                match p.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx_out.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });
    let tx_err = tx.clone();
    let h_err = std::thread::spawn(move || {
        if let Some(p) = &mut err_pipe {
            let mut buf = [0u8; 8192];
            loop {
                match p.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx_err.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });
    drop(tx);

    let mut raw: Vec<u8> = Vec::new();
    let mut collected = String::new();
    let deadline = t0 + Duration::from_secs(timeout_secs.max(1));
    let mut timed_out = false;
    let mut code: i32 = -1;
    loop {
        while let Ok(chunk) = rx.try_recv() {
            raw.extend_from_slice(&chunk);
            let s = decode_live(&chunk);
            if !quiet {
                print!("{}", s);
                let _ = std::io::stdout().flush();
            }
            collected.push_str(&s);
        }
        match child.try_wait() {
            Ok(Some(st)) => {
                code = st.code().unwrap_or(-1);
                break;
            }
            Ok(None) => {}
            Err(_) => {
                code = -1;
                break;
            }
        }
        if Instant::now() > deadline {
            timed_out = true;
            let _ = child.kill();
            if let Ok(st) = child.wait() {
                code = st.code().unwrap_or(-1);
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // drain the pipes
    while let Ok(chunk) = rx.try_recv() {
        raw.extend_from_slice(&chunk);
        let s = decode_live(&chunk);
        if !quiet {
            print!("{}", s);
            let _ = std::io::stdout().flush();
        }
        collected.push_str(&s);
    }
    let _ = h_out.join();
    let _ = h_err.join();

    // some tools (sfc.exe) emit UTF-16LE through the pipe: re-decode the whole
    // output when it looks like UTF-16 so text classification works correctly
    if looks_utf16(&raw) {
        let units: Vec<u16> = raw.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let decoded = String::from_utf16_lossy(&units);
        if decoded.chars().filter(|c| *c != '\0').count() > collected.chars().filter(|c| *c != '\0').count() / 2 {
            collected = decoded;
        }
    }

    let ok = !timed_out && ok_codes.contains(&code);
    let secs = t0.elapsed().as_secs_f64();
    transcript_line(&format!(
        "[{}] EXIT {} ({:.1}s) ok={} timed_out={}",
        id, code, secs, ok, timed_out
    ));
    if !ok {
        // capture the tail of the output for post-mortem (hidden runs)
        let tail: String = collected.chars().rev().take(400).collect::<Vec<_>>().chunks(1).rev().map(|c| c[0]).collect();
        transcript_line(&format!("[{}] OUTPUT-TAIL: {}", id, tail.replace('\n', " | ").replace('\r', "")));
    }
    CmdResult {
        id: id.to_string(),
        program: program.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        code,
        ok,
        secs,
        output: collected,
    }
}

impl From<CmdResult> for Step {
    fn from(c: CmdResult) -> Step {
        let label = format!("{} {}", c.program, c.args.join(" "));
        let detail = if c.ok {
            format!("exit {} ({:.1}s)", c.code, c.secs)
        } else {
            // surface the most telling line of output for failures
            let interesting = c
                .output
                .lines()
                .rev()
                .find(|l| l.contains("Error") || l.contains("error") || l.contains("0x") || l.len() > 12)
                .unwrap_or("")
                .trim();
            let short: String = interesting.chars().take(140).collect();
            if short.is_empty() {
                format!("exit {} (see transcript)", c.code)
            } else {
                format!("exit {}: {}", c.code, short)
            }
        };
        Step::new(&c.id, &label, c.ok, &detail, c.secs)
    }
}
