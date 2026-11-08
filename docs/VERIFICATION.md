# Verification guards

`cargo test` runs these regression guards in addition to the unit tests:

| Test file | Guards |
|---|---|
| `tests/no_poweroff.rs` | Source never calls shutdown / restart / logoff APIs or commands |
| `tests/pause_endstate.rs` | Final phase keeps the 2099 pause expiry, `NoAutoUpdate`, disabled `wuauserv` / `WaaSMedicSvc` |
| `tests/command_coverage.rs` | DISM (check/scan/restore/cleanup), SFC and chkdsk stay in the repair set; no volume dismount |
| `tests/ten_second_law.rs` | Console heartbeat cadence stays at 10 s or less |
| `tests/zero_deps.rs` | Cargo manifest stays dependency-free (single static exe) |

Safe flags for manual checks: `--check-only` (read-only status) and `--dry-run` (full plan, nothing executed).
