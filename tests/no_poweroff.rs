//! Regression guard for the standing user law: this tool must NEVER shut down,
//! restart or log off Windows. Scans every source file for power-off APIs/commands.
use std::fs;

#[test]
fn source_never_calls_power_off_apis() {
    let banned = [
        "shutdown.exe",
        "shutdown /",
        "restart-computer",
        "stop-computer",
        "initiatesystemshutdown",
        "exitwindowsex",
        "ntshutdownsystem",
        "/forcerestart",
        "wmic os where",
        "rundll32 powrprof",
        "logoff",
    ];
    for entry in fs::read_dir("src").unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|e| e == "rs") {
            let text = fs::read_to_string(&p).unwrap().to_lowercase();
            for b in banned {
                // Allow the explanatory sentence in main.rs that says the tool never does this.
                let hits: Vec<_> = text.match_indices(b).collect();
                for (i, _) in hits {
                    let line = text[..i].lines().count();
                    let ctx: String = text
                        .lines()
                        .nth(line.saturating_sub(1))
                        .unwrap_or("")
                        .to_string();
                    assert!(
                        ctx.contains("never") || ctx.contains("nothing in this tool"),
                        "power-off token '{}' in {:?} line {}: {}",
                        b,
                        p,
                        line,
                        ctx
                    );
                }
            }
        }
    }
}
