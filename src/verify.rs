/// One executed unit of work (a command, a registry write, a service action).
/// Every phase produces these; the final gate counts them.
pub struct Step {
    pub id: String,
    pub label: String,
    pub ok: bool,
    pub detail: String,
    pub secs: f64,
}

impl Step {
    pub fn new(id: &str, label: &str, ok: bool, detail: &str, secs: f64) -> Self {
        // every step, everywhere, lands in the transcript — no blind phases
        crate::runner::transcript_line(&format!(
            "{} {} | {} | {:.1}s | {}",
            if ok { "PASS" } else { "FAIL" },
            id,
            label,
            secs,
            detail
        ));
        Step {
            id: id.to_string(),
            label: label.to_string(),
            ok,
            detail: detail.to_string(),
            secs,
        }
    }
}

/// The ZERO-ERROR GATE: 0 if and only if every single step passed.
pub fn gate(steps: &[Step]) -> i32 {
    let failed = steps.iter().filter(|s| !s.ok).count();
    if failed == 0 {
        0
    } else {
        1
    }
}
