//! Guard: the final phase must permanently pause updates (2099 expiry, NoAutoUpdate=1,
//! wuauserv + WaaSMedicSvc disabled) -- the binding end state of the whole tool.
use std::fs;

#[test]
fn pause_phase_sets_permanent_end_state() {
    let t = fs::read_to_string("src/pause.rs").unwrap();
    for needle in ["2099-12-31T23:59:59Z", "NoAutoUpdate", "FlightSettingsMaxPauseDays", "WaaSMedicSvc", "wuauserv", "disabled"] {
        assert!(t.contains(needle), "pause.rs lost required end-state token {needle}");
    }
}
