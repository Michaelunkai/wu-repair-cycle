<#
 WU-Repair-Cycle.ps1
 ===================
 Resumes Windows Update, runs the FULL repair battery, then re-pauses
 Windows Update PERMANENTLY (until 2099 + service disabled).

 Safe to run any number of times - every run first removes every pause
 mechanism again before repairing.

 PHASE 1  RESUME  - removes ALL pause keys (incl. PauseUpdatesExpiryDate /
                    FlightSettingsMaxPauseDays scheme), clears NoAutoUpdate /
                    WSUS / online-blocking policies, re-enables + starts all
                    update services. DISM RestoreHealth needs this online.
 PHASE 2  REPAIR  - DISM CheckHealth -> ScanHealth -> RestoreHealth,
                    SFC /scannow, full Windows Update component reset
                    (SoftwareDistribution + catroot2), WU DLL re-register.
 PHASE 3  PAUSE   - writes pause-until-2099 registry keys, NoAutoUpdate=1,
                    wuauserv stopped + disabled, WaaSMedicSvc disabled so
                    nothing silently re-enables updates.

 Requires : admin (self-elevates). -CheckOnly mode needs no admin and
            changes nothing (read-only status report).
 Runtime  : 10-60 minutes (DISM RestoreHealth dominates). Log saved next
            to the script. Reboot afterwards if the script says so.
#>

param([switch]$CheckOnly)

$ErrorActionPreference = 'Continue'
$scriptDir = Split-Path -Parent $PSCommandPath

$uxKey = 'HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings'
$wuPol = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate'
$auPol = "$wuPol\AU"
$wuSvcs = @('wuauserv','bits','cryptsvc','dosvc','UsoSvc')

# ------------- read-only status mode (no admin needed, changes nothing) -------------
if ($CheckOnly) {
    Write-Host 'WU-Repair-Cycle  -  -CheckOnly (read-only status report, nothing is changed)' -ForegroundColor Cyan
    Write-Host ''
    Write-Host "Pause keys under: $uxKey"
    foreach ($v in @('PauseUpdatesStartTime','PauseUpdatesExpiryTime','PauseUpdatesExpiryDate',
                     'PauseFeatureUpdatesStartTime','PauseFeatureUpdatesEndTime',
                     'PauseQualityUpdatesStartTime','PauseQualityUpdatesEndTime',
                     'PausePreviewBuildsStartTime','PausePreviewBuildsEndTime')) {
        $val = (Get-ItemProperty -Path $uxKey -Name $v -ErrorAction SilentlyContinue).$v
        if (-not $val) { $val = '<absent - updates NOT paused>' }
        Write-Host ('   {0,-32} {1}' -f $v, $val)
    }
    $noAuto = (Get-ItemProperty -Path $auPol -Name NoAutoUpdate -ErrorAction SilentlyContinue).NoAutoUpdate
    if ($null -eq $noAuto) { $noAuto = '<not set>' }
    Write-Host ('   {0,-32} {1}' -f 'NoAutoUpdate policy', $noAuto)
    Write-Host ''
    Write-Host 'Services:'
    foreach ($s in $wuSvcs) {
        $sv = Get-Service -Name $s -ErrorAction SilentlyContinue
        if ($sv) {
            Write-Host ('   {0,-12} {1}' -f $s, $sv.Status)
        } else {
            $st = (Get-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Services\$s" -Name Start -ErrorAction SilentlyContinue).Start
            if ($null -ne $st) { Write-Host ('   {0,-12} Start={1}  (2=auto, 3=manual, 4=disabled)' -f $s, $st) }
            else { Write-Host ('   {0,-12} <missing>' -f $s) }
        }
    }
    $medic = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Services\WaaSMedicSvc' -Name Start -ErrorAction SilentlyContinue).Start
    if ($null -eq $medic) { $medic = '<not set>' }
    Write-Host ('   {0,-12} Start={1}  (3=manual, 4=disabled)' -f 'WaaSMedicSvc', $medic)
    Write-Host ''
    Write-Host 'Run without -CheckOnly for the full cycle: RESUME -> REPAIR -> PERMANENT RE-PAUSE.' -ForegroundColor Cyan
    exit 0
}

# ---------------- self-elevate ----------------
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "Requesting administrator rights..." -ForegroundColor Yellow
    Start-Process powershell.exe -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"{0}"' -f $PSCommandPath))
    exit
}

$log = Join-Path $scriptDir ('WU-Repair-Cycle_{0}.log' -f (Get-Date -Format 'yyyyMMdd_HHmmss'))
Start-Transcript -Path $log | Out-Null
$sw = [Diagnostics.Stopwatch]::StartNew()
$reboot = $false

$farIso = '2099-12-31T23:59:59Z'

function Banner([string]$t) {
    Write-Host ''
    Write-Host ('=' * 70) -ForegroundColor Cyan
    Write-Host "  $t" -ForegroundColor Cyan
    Write-Host ('=' * 70) -ForegroundColor Cyan
}
function Step([string]$t) { Write-Host "`n[*] $t" -ForegroundColor White }

function Stop-SvcSafe([string]$n) {
    if (-not (Get-Service $n -ErrorAction SilentlyContinue)) { return }
    if ((Get-Service $n).Status -ne 'Stopped') { sc.exe stop $n | Out-Null }
    for ($i = 0; $i -lt 20 -and ((Get-Service $n -ErrorAction SilentlyContinue).Status -ne 'Stopped'); $i++) { Start-Sleep -Milliseconds 500 }
    Write-Host ('    {0}: {1}' -f $n, (Get-Service $n -ErrorAction SilentlyContinue).Status) -ForegroundColor Gray
}
function Start-SvcSafe([string]$n) {
    sc.exe start $n 2>$null | Out-Null
    for ($i = 0; $i -lt 20 -and ((Get-Service $n -ErrorAction SilentlyContinue).Status -ne 'Running'); $i++) { Start-Sleep -Milliseconds 500 }
    Write-Host ('    {0}: {1}' -f $n, (Get-Service $n -ErrorAction SilentlyContinue).Status) -ForegroundColor Gray
}

# ============================================================
Banner 'PHASE 1 / 3 : RESUME Windows Update (remove every pause mechanism)'

Step 'Removing Settings pause keys (all variants)'
$pauseVals = @('PauseUpdatesStartTime','PauseUpdatesExpiryTime','PauseUpdatesExpiryDate',
               'PauseFeatureUpdatesStartTime','PauseFeatureUpdatesEndTime',
               'PauseQualityUpdatesStartTime','PauseQualityUpdatesEndTime',
               'PausePreviewBuildsStartTime','PausePreviewBuildsEndTime')
if (Test-Path $uxKey) {
    foreach ($v in $pauseVals) {
        if (Get-ItemProperty -Path $uxKey -Name $v -ErrorAction SilentlyContinue) {
            Remove-ItemProperty -Path $uxKey -Name $v -Force
            Write-Host "    removed $v" -ForegroundColor Gray
        }
    }
}

Step 'Clearing group-policy blocks (NoAutoUpdate / WSUS / online-blocking)'
if (Test-Path $auPol) {
    Remove-ItemProperty -Path $auPol -Name 'NoAutoUpdate','UseWUServer' -ErrorAction SilentlyContinue
}
foreach ($n in @('WUServer','WUStatusServer','DoNotConnectToWindowsUpdateInternetLocations',
                 'DisableWindowsUpdateAccess','SetPolicyDrivenUpdateSourceForQualityUpdates',
                 'SetPolicyDrivenUpdateSourceForFeatureUpdates','SetPolicyDrivenUpdateSourceForDriverUpdates',
                 'SetPolicyDrivenUpdateSourceForContentUpdates')) {
    Remove-ItemProperty -Path $wuPol -Name $n -ErrorAction SilentlyContinue
}
Write-Host '    policy blocks cleared (if any existed)' -ForegroundColor Gray

Step 'Restoring + starting update services'
sc.exe config wuauserv start= demand | Out-Null
try { Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Services\WaaSMedicSvc' -Name Start -Value 3 -ErrorAction SilentlyContinue } catch {}
foreach ($s in $wuSvcs) { Start-SvcSafe $s }
Start-Sleep -Seconds 3

# ============================================================
Banner 'PHASE 2 / 3 : REPAIR (DISM + SFC + full Windows Update reset)'
Write-Host 'This phase can take 10-60 minutes. Do not close this window.' -ForegroundColor Yellow

Step 'DISM CheckHealth (quick component-store corruption check)'
dism.exe /Online /Cleanup-Image /CheckHealth
$c = $LASTEXITCODE; Write-Host "    exit code: $c" -ForegroundColor Gray
if ($c -eq 3010 -or $c -eq 1641) { $reboot = $true }

Step 'DISM ScanHealth (deep component-store scan)'
dism.exe /Online /Cleanup-Image /ScanHealth
$c = $LASTEXITCODE; Write-Host "    exit code: $c" -ForegroundColor Gray
if ($c -eq 3010 -or $c -eq 1641) { $reboot = $true }

Step 'DISM RestoreHealth (repairs component store - downloads from Windows Update)'
dism.exe /Online /Cleanup-Image /RestoreHealth
$c = $LASTEXITCODE; Write-Host "    exit code: $c" -ForegroundColor Gray
if ($c -eq 3010 -or $c -eq 1641) { $reboot = $true }

Step 'SFC /scannow (system file repair)'
sfc.exe /scannow
$c = $LASTEXITCODE
Write-Host "    exit code: $c  (0 = clean or repaired; non-zero = check CBS.log)" -ForegroundColor Gray
if ($c -ne 0) { $reboot = $true }

Step 'Resetting Windows Update components (SoftwareDistribution + catroot2)'
foreach ($s in $wuSvcs) { Stop-SvcSafe $s }
Stop-SvcSafe msiserver
Start-Sleep -Seconds 2
$stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$sd = Join-Path $env:windir 'SoftwareDistribution'
if (Test-Path $sd) {
    $dst = "$sd.old"; if (Test-Path $dst) { $dst = '{0}_{1}' -f $dst, $stamp }
    try {
        Rename-Item $sd $dst -Force -ErrorAction Stop
        Write-Host "    SoftwareDistribution -> $(Split-Path -Leaf $dst)" -ForegroundColor Gray
    } catch { Write-Host "    could not rename SoftwareDistribution ($($_.Exception.Message)) - continuing" -ForegroundColor Yellow }
}
$cr = Join-Path $env:windir 'System32\catroot2'
if (Test-Path $cr) {
    $dst = "$cr.old"; if (Test-Path $dst) { $dst = '{0}_{1}' -f $dst, $stamp }
    try {
        Rename-Item $cr $dst -Force -ErrorAction Stop
        Write-Host "    catroot2 -> $(Split-Path -Leaf $dst)" -ForegroundColor Gray
    } catch { Write-Host '    could not rename catroot2 (locked - safe to ignore, it rebuilds)' -ForegroundColor Yellow }
}

Step 'Restarting update services (fresh store rebuilds automatically)'
foreach ($s in @('cryptsvc','bits','dosvc','wuauserv','UsoSvc')) { Start-SvcSafe $s }
Start-Sleep -Seconds 3

Step 'Re-registering Windows Update system DLLs'
$dlls = @('atl.dll','urlmon.dll','mshtml.dll','shdocvw.dll','browseui.dll','jscript.dll',
          'vbscript.dll','scrrun.dll','msxml.dll','msxml3.dll','msxml6.dll','actxprxy.dll',
          'softpub.dll','wintrust.dll','dssenh.dll','rsaenh.dll','gpkcsp.dll','sccbase.dll',
          'slbcsp.dll','cryptdlg.dll','oleaut32.dll','ole32.dll','shell32.dll','initpki.dll',
          'qmgr.dll','qmgrprxy.dll','wuaueng.dll','wucltux.dll','wuapi.dll','wups.dll',
          'wups2.dll','wuwebv.dll')
$ok = 0; $miss = 0
foreach ($d in $dlls) {
    $p = Join-Path $env:windir "System32\$d"
    if (Test-Path $p) { regsvr32.exe /s $p; $ok++ } else { $miss++ }
}
Write-Host ('    registered: {0}, not present (skipped): {1}' -f $ok, $miss) -ForegroundColor Gray

# ============================================================
Banner 'PHASE 3 / 3 : PAUSE Windows Update PERMANENTLY'

Step "Writing pause-until-2099 registry keys"
$nowIso = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
$pauseMap = @{
    'PauseUpdatesStartTime'        = $nowIso
    'PauseUpdatesExpiryTime'       = $farIso
    'PauseUpdatesExpiryDate'       = $farIso
    'PauseFeatureUpdatesStartTime' = $nowIso
    'PauseFeatureUpdatesEndTime'   = $farIso
    'PauseQualityUpdatesStartTime' = $nowIso
    'PauseQualityUpdatesEndTime'   = $farIso
    'PausePreviewBuildsStartTime'  = $nowIso
    'PausePreviewBuildsEndTime'    = $farIso
}
foreach ($k in $pauseMap.Keys) {
    New-ItemProperty -Path $uxKey -Name $k -Value $pauseMap[$k] -PropertyType String -Force | Out-Null
}
New-ItemProperty -Path $uxKey -Name 'FlightSettingsMaxPauseDays' -Value 3650 -PropertyType DWord -Force | Out-Null
Write-Host "    all update types paused until $farIso" -ForegroundColor Gray

Step 'NoAutoUpdate=1 policy'
if (-not (Test-Path $auPol)) { New-Item -Path $auPol -Force | Out-Null }
New-ItemProperty -Path $auPol -Name 'NoAutoUpdate' -Value 1 -PropertyType DWord -Force | Out-Null
Write-Host '    automatic updates disabled by policy' -ForegroundColor Gray

Step 'Stopping + disabling update services'
foreach ($s in $wuSvcs) { Stop-SvcSafe $s }
sc.exe config wuauserv start= disabled | Out-Null
Write-Host '    wuauserv: disabled' -ForegroundColor Gray

Step 'Disabling WaaSMedicSvc (stops Windows from re-enabling updates)'
try {
    Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Services\WaaSMedicSvc' -Name Start -Value 4 -ErrorAction Stop
    Write-Host '    WaaSMedicSvc: disabled' -ForegroundColor Gray
} catch {
    Write-Host '    WaaSMedicSvc is protected and could not be disabled - pause keys + service block still hold' -ForegroundColor Yellow
}

# ============================================================
Banner 'DONE - summary'
Write-Host ('  Runtime : {0:mm} min {0:ss} sec' -f $sw.Elapsed)
Write-Host ("  Log     : {0}" -f $log)
if ($reboot) { Write-Host '  REBOOT RECOMMENDED - repairs completed that need it.' -ForegroundColor Yellow }
Write-Host ''
Write-Host '  End state: updates paused until 2099, NoAutoUpdate=1, wuauserv disabled.' -ForegroundColor Green
Write-Host '  The next run of this script automatically resumes updates again first.' -ForegroundColor Green
Stop-Transcript | Out-Null
Write-Host ''
Write-Host 'Press any key to close...'
$null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown')
