//! Guard: the console must never go silent >10 s (user's 10-second law).
use std::fs;

#[test]
fn runner_heartbeat_cadence_is_at_most_10s() {
    let t = fs::read_to_string("src/runner.rs").unwrap();
    assert!(
        t.contains("ui::heartbeat"),
        "runner lost its heartbeat call"
    );
    let line = t
        .lines()
        .find(|l| l.contains("last_beat.elapsed()"))
        .expect("no heartbeat cadence check");
    let n: u64 = line
        .split("from_secs(")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .and_then(|s| s.trim().parse().ok())
        .expect("cadence not parseable");
    assert!(n <= 10, "heartbeat cadence {n}s exceeds 10s law");
}
