# WU-Repair-Cycle

![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%20Windows%2011-blue)
![Shell](https://img.shields.io/badge/Windows%20PowerShell-5.1-5391FE?logo=powershell)
![License](https://img.shields.io/badge/license-MIT-green)
![Release](https://img.shields.io/badge/release-v1.0.0-orange)

**One script, one run: resume Windows Update → run the complete repair battery → pause Windows Update permanently again.**

If you keep Windows Update hard-paused (registry pause-until-2099, `NoAutoUpdate`, disabled `wuauserv`), the built-in repair tools can't do their job — `DISM /RestoreHealth` needs Windows Update online to download replacement payloads. This script opens the gates, fixes everything, and locks them again behind you. Every run is self-contained: it always un-pauses first, so you can run it as often as you like.

---

## What one run does

| Phase | Action |
|---|---|
| **1 — RESUME** | Removes every pause mechanism: all `PauseUpdates*` registry keys (including the `PauseUpdatesExpiryDate` / `FlightSettingsMaxPauseDays` scheme), `NoAutoUpdate` / WSUS / online-blocking policies, re-enables **WaaSMedicSvc** and starts `wuauserv`, `bits`, `cryptsvc`, `dosvc`, `UsoSvc`. |
| **2 — REPAIR** | `DISM /CheckHealth` → `DISM /ScanHealth` → `DISM /RestoreHealth` → `SFC /scannow`, then a full Windows Update component reset (stops services, renames `SoftwareDistribution` and `catroot2` so both rebuild clean), then re-registers all 32 Windows Update system DLLs. |
| **3 — PAUSE** | Immediately re-pauses **permanently**: pause keys rewritten until **2099-12-31**, `NoAutoUpdate=1`, `wuauserv` stopped + **disabled**, `WaaSMedicSvc` disabled so Windows cannot silently re-enable updates. |

Every run writes a timestamped transcript log (`WU-Repair-Cycle_<timestamp>.log`) next to the script and tells you at the end whether a **reboot is recommended**.

## Requirements

- Windows 10 or 11 (works on 24H2/25H2)
- Administrator rights — the script **self-elevates** (UAC prompt) if not already elevated
- 10–60 minutes of patience (the DISM phase dominates)

## Install

**Option 1 — one-liner (download latest release + run):**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "iwr https://github.com/Michaelunkai/wu-repair-cycle/releases/latest/download/WU-Repair-Cycle.ps1 -OutFile $env:TEMP\WU-Repair-Cycle.ps1; & $env:TEMP\WU-Repair-Cycle.ps1"
```

**Option 2 — download the zip** from the [latest release](https://github.com/Michaelunkai/wu-repair-cycle/releases/latest), unzip anywhere, double-click `Run-WU-Repair.cmd`.

**Option 3 — from source:** clone the repo and run the script directly (it is plain Windows PowerShell 5.1, no dependencies).

## Usage

```powershell
# full cycle: RESUME -> REPAIR -> PERMANENT RE-PAUSE
powershell -NoProfile -ExecutionPolicy Bypass -File WU-Repair-Cycle.ps1

# read-only status check: shows current pause keys + service states, changes NOTHING
powershell -NoProfile -ExecutionPolicy Bypass -File WU-Repair-Cycle.ps1 -CheckOnly
```

Or just double-click `Run-WU-Repair.cmd`.

`-CheckOnly` needs no admin rights and is completely non-invasive — use it any time you want to see whether updates are currently paused and how.

## How to un-pause manually later

If you ever want updates back on *without* running the repair cycle:

```powershell
sc.exe config wuauserv start= demand
reg delete "HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings" /v PauseUpdatesExpiryTime /f
reg delete "HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings" /v PauseUpdatesExpiryDate /f
```
…and delete the remaining `Pause*Updates*` keys, set `WaaSMedicSvc` `Start=3`, and remove the `NoAutoUpdate` policy. (Or simply run this script and stop it after Phase 1.)

## Troubleshooting

- **"Access is denied" when querying services** — you are not elevated; the script self-elevates when run normally, only `-CheckOnly` is safe unelevated.
- **`catroot2` rename fails** — the folder was locked by `cryptsvc`; safe to ignore, it rebuilds itself.
- **DISM exit code `3010`** — success, reboot required; the script flags this at the end.
- **WaaSMedicSvc could not be disabled** — the service is protected on some builds; the pause keys + disabled `wuauserv` still hold.
- **Store app updates stop while paused** — expected side effect of disabling `wuauserv`; the next run resumes everything automatically.
- Deep logs: `%windir%\Logs\CBS\CBS.log`, `%windir%\Logs\DISM\dism.log`, plus the script's own transcript.

## Notes

- Old component stores are kept as `SoftwareDistribution.old*` / `catroot2.old*` — delete them once a run has verified successful to reclaim space.
- Nothing phones home, no telemetry, no dependencies — one `.ps1` and a `.cmd` launcher.

## License

[MIT](LICENSE) — © Michaelunkai
