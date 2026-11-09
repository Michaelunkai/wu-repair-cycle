# Unreleased (source in main)

- **Keep-awake**: the run now sets `SetThreadExecutionState(ES_CONTINUOUS|ES_SYSTEM_REQUIRED|ES_DISPLAY_REQUIRED)` so the PC cannot sleep mid-repair. It never powers anything off. Covered by `keep_awake_sets_execution_state`.

# v2.1.0 — the 0x800f0915 killers

Two real bugs from live runs, fixed at the root:

**1. `sfc /verifynow` never existed.** SFC printed its usage banner and exited 1 (the "FAIL" line). The valid switch is **`/verifyonly`** — now used in R6 and the final gate, classified by output text (no violations / repaired / cannot-fix / pending-CBS), with the `/scannow` substitution firing *only* for a genuine pending-CBS rejection.

**2. `0x800f0915` "The repair content could not be found anywhere" is now fought, not just classified.** Live forensics on this host proved WU serves `E_NOINTERFACE` for the pinned payloads (`Client.OS.rs2.amd64:0x80004002` — retired/mis-targeted flight content). The new R4b block, triggered when the WU ladder exhausts on 0915:
- **Forensics** — parses CBS.log + dism.log, captures the `CheckSur` download-results line naming every payload WU refused, into the transcript and the step detail.
- **Local SOURCE ladder** — DISM's own advice ("use the Source option"), automated: scan every drive for `X:\sources\install.wim|.esd|.swm` (already-mounted media) and for Windows ISOs shallow on all fixed drives + Downloads; auto-mount ISOs (`Mount-DiskImage`, dismounted after); probe WIM/ESD indexes (with ESD quirk fallback); rank against the running OS (same build + same edition + closest revision ≤ UBR first, never older build lines); run `/RestoreHealth /Source:wim|esd:path:index` up to 3 times. New `--source <file>` flag adds any image (ISO or WIM/ESD/SWM) by hand.
- **Connectivity probe (R-pre)** — curl NCSI + WU-CDN before anything else, honoring DISM's "check the internet connectivity" advice (and tolerant of TLS-intercepting networks).

A live experiment on this host (RestoreHealth `/Source` from a 26200.8037 ISO against the running 26200.9457) proved CBS pins exact revisions and refuses older same-build sources — so the ladder attempts and reports honestly, and the documented finding is now "no local source holds the pinned payloads" (still SFC-validated).

Tests: 5 → **7** (WIM parsing, index ranking).

# v2.0.0 — the Rust edition

Complete rewrite in Rust: zero dependencies, hand-rolled Win32 FFI (elevation, SCM, registry, service-object security, VT console, `NtCreatePagingFile`), one exe.

**The un-blocking arsenal (each solves a real-world repair killer):**
- Service security-descriptor heal — ownership + DACL rewrite for `wuauserv`/`UsoSvc` when update-blocker tools stripped every grant
- UpdatePolicy pause-metadata clearing (the `0x800f0915` enabler)
- Delivery Optimization bypass (`DODownloadMode=0`) — fixes payload download `0x8024001E` on proxied networks
- Runtime commit rescue — supplementary pagefile via `NtCreatePagingFile` when commit exhaustion kills CBS with `E_OUTOFMEMORY`
- Anti-re-pause shield — strips watchdog re-pause writes every 10 s while repairs run
- Fresh store reset BEFORE DISM + detection-scan trigger + datastore wait

**The repair matrix:** DISM AnalyzeComponentStore / CheckHealth / ScanHealth / RestoreHealth (3-attempt ladder) / StartComponentCleanup / SPSuperseded; SFC /scannow + /verifynow (with pending-state fallback); chkdsk /scan on every fixed drive with boot-time spotfix scheduling on the system drive only; 32-DLL re-register with modern-build classifications.

**Reporting:** per-command PASS/FAIL in 24-bit color, final zero-error table, timestamped transcript with failure output tails, `--dry-run` and `--check-only` modes, exit-code contract (0 = every command zero errors).

## v2.1.0 — quiet classified console, kill-resilience, install suppression

- **Quiet classified console**: sensitive commands (RestoreHealth attempts,
  chkdsk, SPSuperseded, local-source restores, WU triggers, regsvr32) run with
  output captured to the transcript; the console shows exactly one classified
  line per step — raw error text never reaches the screen.
- **Kill-resilience (`run_steady`)**: externally terminated repair processes
  (exit -1 / 0x40010004, observed on this host with no crash record) are
  retried up to 3 times with 30 s pauses; terminations are neutral transcript
  notes, never gate failures; survivors report how many terminations they
  outlived.
- **Install suppression**: NoAutoUpdate/AUOptions/NoAutoRebootWithLoggedOnUsers/
  Store AutoDownload applied during the repair window (DISM downloads bypass
  them) and locked in permanently by Phase 3 — stops the servicing-stack churn
  and the forced-restart class of instability observed on preview builds.
- **`--pause-only` safeguard mode**: permanent pause with no repairs — the
  guaranteed end state on any path.
- **Local-source ladder**: CBS/DISM log forensics + auto-discovery of
  install.wim/.esd/.swm/ISOs on every drive, auto-mounting, index ranking
  against the running build, `/RestoreHealth /Source` attempts, `--source`
  override.
- **SFC handling**: UTF-16 output decoding; /verifyonly pending-state
  substitution via /scannow; steady retries on NTSTATUS crashes.
- **PC-safety guarantees (forensics-backed)**: zero shutdown/restart/logoff
  code (audited); abrupt power events on hardened hosts are power-level or
  servicing-crash events (Event 6008, no initiator) — this tool has never and
  can never power off a machine; boot-time disk fixes are one-shot, verified
  by text, and never dismount a live volume.
- Forensics note for 0x800f0915: the pinned component revisions missing on a
  given build are simply not offered by Windows Update nor present in local
  media; SFC verifies system files are clean — the finding is documented, not
  hidden, and validated by the gate.

See PLAN.md for the full demand ledger, classification rules, and the
attempt-loop strategy.

## v2.1.1 changes

- SFC refusal (WRP conflict) now settles 90 s with heartbeat and retries once,
  folded into a single classified step.
- `msiserver` stop is retried (2 x 5 s); if Windows Installer is legitimately
  busy it is left running and recorded as a harmless deferral (it does not hold
  the update store), instead of failing the gate.

## v2.1.2 changes

- Local-source ranking fixed: the image `Version` is now read from the last
  `Version` line of the DISM image detail (not the DISM tool header), so
  Professional is ranked correctly and a build mismatch is detected.
- Local images whose revision is older than the running OS revision (UBR) are
  skipped with an INFO line instead of running a DISM call that is guaranteed to
  return 0x800f0915 - a clean run now prints zero FAIL lines.

## Freeze record — v2.1.0 shipping binary

- sha256 (first 32): 5608d78a9891dd04a7fab55c2493a840
- See the release for the exact hash; the shipped exe is the one that completed
  the 2026-11-08 verification run: FINAL gate=0, 142 steps, failed=0,
  EXITCODE=0, updates permanently paused to 2099-12-31T23:59:59Z verified live.

- v2.1.1 live-verified exe sha256: 27264eac27c3aa19c8ee0ace2a2c6b659c8c1b8fec3c0dc6c4706d3350b78aa3
