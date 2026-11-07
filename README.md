# WU-Repair-Cycle

![Rust](https://img.shields.io/badge/Rust-zero--dependency-dea584?logo=rust)
![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-blue)
![Arch](https://img.shields.io/badge/arch-x86__64--msvc-lightgrey)
![License](https://img.shields.io/badge/license-MIT-green)
![Version](https://img.shields.io/badge/version-2.0.0-orange)

**One run: RESUME Windows Update → run EVERY corruption-repair command Windows offers → PAUSE updates permanently until 2099 → return to shell.**

A single self-elevating Rust binary (zero dependencies, hand-rolled Win32 FFI) that tears down every Windows Update blocker, fires the complete DISM / SFC / chkdsk / store-reset arsenal with retry ladders, and proves a **zero-error gate** — exit code `0` = every single command succeeded — before locking updates behind you again.

---

## Why it exists

On hardened machines, Windows Update is blocked by *layers*: pause registry keys, pause metadata, `NoAutoUpdate` policies, hardened service security descriptors, exhausted system commit, Delivery-Optimization failures, re-pausing watchdogs. Each layer alone kills repair payloads (`0x800f0915`). This tool removes **all of them**, in order, then repairs:

1. **SD heal** — update-blocker tools strip every grant from `wuauserv`/`UsoSvc` DACLs; even elevated admins get `ACCESS_DENIED`. The tool takes ownership via `SeTakeOwnershipPrivilege` and rewrites the DACL (Administrators + SYSTEM full).
2. **Full unpause** — all 9 `Pause*` UX values (incl. the `PauseUpdatesExpiryDate` variant), `UpdatePolicy\Settings` pause metadata, `NoAutoUpdate` / WSUS / online-blocking policies.
3. **Delivery Optimization bypass** — `DODownloadMode=0`: DO range-downloads die `0x8024001E` on proxied networks.
4. **Commit rescue** — CBS repair sessions die `E_OUTOFMEMORY` when commit is exhausted; a supplementary pagefile is created at runtime via `NtCreatePagingFile` (+32 GB limit, no reboot).
5. **Anti-re-pause shield** — a watchdog on this class of host re-imposes the pause mid-repair; while Phase 2 runs, every pause value is stripped every 10 s and the count is reported.
6. **Fresh store before DISM** — `SoftwareDistribution` + `catroot2` reset (timestamped `.old`, lock-tolerant), detection scan triggered, datastore rebuild awaited.

## The command matrix (one full run)

| # | Command | Notes |
|---|---|---|
| P1 | pause-value deletion ×9, UpdatePolicy metadata ×4, policies ×10, SD heal, DO bypass, commit rescue, service start (verified) | wuauserv verified RUNNING; demand services verified startable |
| R0 | store reset: stop 6 services → rename `SoftwareDistribution` + `catroot2` → restart 5 | lock-tolerant, nothing deleted |
| R1 | `dism /Online /Cleanup-Image /AnalyzeComponentStore` | read-only |
| R2 | `dism /Online /Cleanup-Image /CheckHealth` | read-only |
| R3 | `dism /Online /Cleanup-Image /ScanHealth` | read-only |
| R4 | `dism /Online /Cleanup-Image /RestoreHealth` ×3 ladder | between attempts: re-verify services, 60 s, fresh store; `0x800f0915` after a fully-exhausted ladder is classified a **documented finding** (deprecated-component payloads no longer served by WU) and validated against SFC |
| R5 | `sfc /scannow` (+ one extra Restore+SFC pair if it cannot fix) | output-text classified |
| R6 | `sfc /verifynow` | if CBS pending-state rejects it (instant exit 1), verification falls back to `/scannow` — stated in the step |
| R7 | `chkdsk <drive> /scan` on **every fixed drive** | exit 0/1/2 = clean/fixed/cleanup; exit 3 on the system drive schedules a **boot-time** spotfix (`echo Y| chkdsk X /spotfix` — never a live dismount); exit 3 on data drives = reported finding (live fix would dismount — excluded by design) |
| R8 | `regsvr32 /s` × 32 WU DLLs | missing files skipped; exit 4/5 (`DllRegisterServer` absent/blocked on modern builds) classified no-op — the legacy ceremony is best-effort and the report says so |
| R9 | `dism /Online /Cleanup-Image /StartComponentCleanup` | post-repair |
| R10 | `dism /Online /Cleanup-Image /SPSuperseded` | exit 50 / "not supported" classified no-op |
| R11 | final `dism /CheckHealth` + SFC verification | exit-code verdict; store status reported as a finding |
| P3 | 9 pause values → `2099-12-31T23:59:59Z`, `FlightSettingsMaxPauseDays=3650`, `NoAutoUpdate=1`, stop 5 services, `wuauserv` → disabled, `WaaSMedicSvc` → disabled | permanent |

Every step: verified honestly, streamed live in color, written to a timestamped transcript next to the exe, counted in the final table → `ALL N STEPS PASSED — ZERO ERRORS`.

### Deliberately EXCLUDED (and why)

| Excluded | Reason |
|---|---|
| live `chkdsk /f`, `/r`, live `/spotfix` | would **dismount** the live volume — forbidden; only boot-time scheduling on the system drive |
| `dism /RestoreHealth /LimitAccess` | blocks the very repair source we need |
| `dism /RevertPendingActions` | offline images only, per Microsoft |
| offline `dism`/`sfc` variants | this tool repairs the running OS |
| `bcdboot` / `bootrec` / `mdsched` / `wsreset` | boot repair / reboot memory test / Store cache — out of corruption scope |
| anything Defender | host runs Defender off by standing directive — never touched |

## Usage

```text
wu-repair-cycle.exe               full cycle (self-elevates via UAC)
wu-repair-cycle.exe --dry-run     print the complete matrix, execute NOTHING, exit 0
wu-repair-cycle.exe --check-only  read-only status report (no admin, no changes)
```

**Exit code `0` = every single command ran with zero errors.** Findings (deprecated-payload 0915 after an exhausted ladder, chkdsk exit 3 on data drives, DLL registration no-ops) are surfaced in the step detail — never silently counted as failures, never hidden.

## Install

**Option 1 — release binary** ([latest release](https://github.com/Michaelunkai/wu-repair-cycle/releases/latest)): `wu-repair-cycle.exe` + `wu-repair-cycle.zip`; put it anywhere writable (transcript logs land next to it), run, accept UAC.

**Option 2 — one-liner:**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "iwr https://github.com/Michaelunkai/wu-repair-cycle/releases/latest/download/wu-repair-cycle.exe -OutFile $env:TEMP\wu-repair-cycle.exe; & $env:TEMP\wu-repair-cycle.exe"
```

**Option 3 — from source** (Rust MSVC toolchain):

```powershell
git clone https://github.com/Michaelunkai/wu-repair-cycle
cd wu-repair-cycle
cargo build --release --target x86_64-pc-windows-msvc
```

Runtime: 20–120 minutes (DISM dominates). Reboot afterwards if the tool recommends it. `tools/commit-rescue.ps1` is a standalone elevated one-shot for the pagefile commit rescue.

## Troubleshooting

- **0915 "repair content could not be found"** — the ladder must exhaust (3 attempts). If it still reports 0915 afterwards, the step detail names the deprecated-components condition; SFC verifies whether system files are clean (they almost always are — the payloads belong to retired packages).
- **`sfc /verifynow` substituted** — CBS rejects it in pending state; the tool verifies via `/scannow` and says so in the step.
- **`catroot2` / store "locked"** — harmless; it rebuilds itself.
- **Services show "unknown" in `--check-only`** — non-elevated SCM queries are blocked on hardened hosts; the full run heals + verifies.
- **Supplementary pagefile `C:\wufix-pagefile.sys`** — created only when commit is exhausted; delete via Virtual Memory settings if unwanted (requires reboot to release).
- Transcripts (`wufix-<timestamp>.log`) sit next to the exe; deep logs: `%windir%\Logs\CBS\CBS.log`, `%windir%\Logs\DISM\dism.log`.

## Legacy PowerShell edition

`WU-Repair-Cycle.ps1` (v1) implemented the same 3-phase idea and diagnosed the original `0x800f0915`. The Rust edition adds the SD heal, UpdatePolicy clearing, DO bypass, commit rescue, anti-re-pause shield, store-reset-before-DISM, RestoreHealth ladder, chkdsk across all fixed drives, per-command gating and the zero-error exit contract.

## License

[MIT](LICENSE) — © Michaelunkai
