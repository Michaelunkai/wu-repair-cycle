# The command matrix (authoritative)

Every command `wu-repair-cycle.exe` executes, with ok-codes, timeouts, retry ladders, and the classification rules that keep the zero-error gate honest. `ok-codes` are exit codes counted as success; anything else fails the gate unless an explicit classification rule applies (each one stated below — no silent passes).

## PHASE 1 — RESUME (all the un-blocking)

| Step | Action | ok criterion |
|---|---|---|
| resume-del-`<PauseValue>` × 9 | delete `UX\Settings\Pause{Updates,FeatureUpdates,QualityUpdates,PreviewBuilds}{StartTime,ExpiryTime,EndTime}` + `PauseUpdatesExpiryDate` | verified gone (absent = ok) |
| resume-upd-`<value>` × 4 | delete `UpdatePolicy\Settings\Paused{Quality,Feature}{Status,Date}` — **the 0x800f0915 metadata blocker** | verified gone |
| resume-pol × 10 | delete `AU\NoAutoUpdate`, `AU\UseWUServer`, `WU\WUServer`, `WU\WUStatusServer`, `WU\DoNotConnectToWindowsUpdateInternetLocations`, `WU\DisableWindowsUpdateAccess`, `WU\SetPolicyDrivenUpdateSourceFor{Quality,Feature,Driver,Content}Updates` | verified gone |
| resume-do-bypass | `DeliveryOptimization\Config\DODownloadMode = 0` — DO range-downloads fail `0x8024001E` on proxied networks | verified written |
| commit-rescue-pagefile | if commit free < 8 GB: `NtCreatePagingFile("C:\wufix-pagefile.sys", 32GB)` — raises commit limit at runtime (no reboot); CBS dies `E_OUTOFMEMORY` without it | verified headroom after |
| resume-sdheal-`<svc>` | if `wuauserv`/`UsoSvc` not even queryable: enable `SeTakeOwnershipPrivilege` → take ownership → rewrite DACL (Administrators+SYSTEM full) — undo update-blocker hardening | queryable after |
| resume-svc-wuauserv-demand / resume-medic-manual | `wuauserv` start→manual(3); `WaaSMedicSvc` registry Start→3 | verified |
| resume-svc-`<svc>` × 5 | start + verify `wuauserv, bits, cryptsvc, dosvc, UsoSvc` | wuauserv must be RUNNING (verified, 30 s patience); demand services ok when running or startable-idle (Start ∈ {2,3}); disabled(4) = FAIL |

**Anti-re-pause shield** (during all of PHASE 2): every 10 s strip all 9 UX pause values, 4 UpdatePolicy values, and `NoAutoUpdate`; count of suppressed watchdog re-writes reported in step `pause-shield`. The shield fully exits before PHASE 3 writes the permanent pause.

## PHASE 2 — REPAIR MATRIX

| # | id | command | ok-codes | timeout | ladder / classification |
|---|---|---|---|---|---|
| R-pre | net-connectivity | `curl -s -o NUL -w %{http_code}` × 3: NCSI over HTTP, NCSI over HTTPS (informational), Windows Update CDN | step ok = NCSI-http `200` **and** WU-CDN any non-000 code | 30 s each | honors DISM's "check the internet connectivity" advice; TLS-intercepting networks (schannel wrong-principal on NCSI-https) do not fail the step |
| R0 | reset-* | stop `wuauserv, bits, cryptsvc, dosvc, UsoSvc, msiserver` → rename `SoftwareDistribution` → `*.old[._ts]` → rename `catroot2` → restart services → `UsoClient StartScan` + WUA `DetectNow` → await `DataStore.edb` (≤180 s) | verified service states | — | renames lock-tolerant: persistent lock = PASS "keeping existing synced store / harmless, rebuilds" |
| R1 | dism-analyze | `dism /Online /Cleanup-Image /AnalyzeComponentStore` | 0 | 1800 s | long timeout — TiWorker contention can stall it |
| R2 | dism-checkhealth | `dism /Online /Cleanup-Image /CheckHealth` | 0 | 300 s | — |
| R3 | dism-scanhealth | `dism /Online /Cleanup-Image /ScanHealth` | 0 | 3600 s | — |
| R4 | dism-restore-1..3 | `dism /Online /Cleanup-Image /RestoreHealth` | 0, 3010, 1641 | 5400 s each | attempt 2 after ensure-services + 60 s; attempt 3 after a second full R0. `3010/1641` → reboot flag. **Classification**: `0x800f0915` after the fully-exhausted ladder → documented finding, accepted ONLY if R5 SFC proves zero integrity violations (else a validity-check step FAILS the gate) |
| R4b | dism-log-forensics + src-* + dism-restore-src-1..3 | forensic log parse + `/RestoreHealth /Source:wim\|esd:path:index` over discovered local images | 0, 3010, 1641 | 7200 s each | fires when the WU ladder exhausts on 0915. Forensics captures the `CheckSur` download-results line (payload identities). Source discovery: `X:\sources\install.*` on every drive (mounted media) + ISOs shallow on all fixed drives + Downloads + `--source <file>`; ISOs auto-mounted (`Mount-DiskImage`) and dismounted after (`src-unmount`). Indexes ranked: same build + same edition + closest revision ≤ UBR first; older build lines never tried. `0x800f0915/0x800f081f` from a source = revision-mismatch refusal (documented, folded into the finding). A source success = plain PASS, reboot flag on 3010 |
| R5 | sfc-scannow | `sfc /scannow` | 0 | 5400 s | text: "no integrity violations" = clean · "successfully repaired" = PASS + reboot · "could not/unable to fix / cannot perform" = one extra RestoreHealth + SFC pair, then honest verdict |
| R6 | sfc-verifyonly | `sfc /verifyonly` | 0 | 3600 s | text-classified: usage banner = invalid invocation (FAIL — the switch is v2.1.0-fixed; `/verifynow` never existed); pending-CBS rejection → `/scannow` substitution, stated in the step |
| R7 | chkdsk-`<D>`-scan | `chkdsk D: /scan` per fixed drive | 0, 1, 2, 3 | 3600 s per drive | 0 clean · 1 fixed · 2 cleanup · 3 on the SYSTEM drive → `cmd /c "echo Y| chkdsk D /spotfix"` (boot-time schedule, ok 0, sets reboot flag, never a live dismount) · 3 on data drives → PASS-with-finding (live fix would dismount — excluded by design) |
| R8 | dll-reg-* | `regsvr32 /s` × 32 WU DLLs | 0, 4, 5 | 60 s each | missing file → skipped; exit 4/5 = `DllRegisterServer` absent/blocked on modern builds → classified no-op (documented per step + summary counts) |
| R9 | dism-componentcleanup | `dism /Online /Cleanup-Image /StartComponentCleanup` | 0, 3010 | 3600 s | post-repair only |
| R10 | dism-spsuperseded | `dism /Online /Cleanup-Image /SPSuperseded` | 0, 3010 | 3600 s | exit 50 or "not supported" → classified no-op |
| R11 | dism-checkhealth-final + sfc verification | `dism /Online /Cleanup-Image /CheckHealth` + `sfc /verifyonly` (pending-CBS fallback `/scannow`) | 0 | 300 s / 3600 s | verdict by exit code; "repairable" store status reported as a FINDING (points at the R4/R4b classification) |

## PHASE 3 — PAUSE PERMANENTLY

| Step | Action |
|---|---|
| pause-set-`<name>` × 9 | Start-times = now UTC (`yyyy-MM-ddTHH:mm:ssZ`); all End/Expiry (incl. `PauseUpdatesExpiryDate`) = `2099-12-31T23:59:59Z` |
| pause-set-FlightSettingsMaxPauseDays | dword `3650` |
| pause-set-NoAutoUpdate | `AU\NoAutoUpdate = 1` |
| pause-stop-`<svc>` × 5 | stop + verify `wuauserv, bits, cryptsvc, dosvc, UsoSvc` |
| pause-disable-wuauserv | start type → disabled (4), verified |
| pause-disable-medic | registry `WaaSMedicSvc\Start = 4` |

## The zero-error gate

`exit code = 0` ⟺ every step's ok flag is true. Every classification rule is visible in the step's detail line — findings are reported, not buried.
