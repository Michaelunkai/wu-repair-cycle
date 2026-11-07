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
