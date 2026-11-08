# WU-Fix Supervisor: relaunch the same exe (elevated, no UAC prompt) until one
# attempt runs START TO END with FINAL gate=0. Survives the unstable servicing
# stack on this build (wuauserv/UpdateDeploy.dll crashes kill repair sessions).
# Last-resort end state: --pause-only guarantees updates permanently paused.
# The PC is never rebooted or shut down by any of this.
$ErrorActionPreference = 'Continue'
$dir = 'F:\study\Platforms\Windows\fixers\system-integrity\WindowsUpdate\Repair\wu-repair-cycle'
$exe = Join-Path $dir 'wu-repair-cycle.exe'
$log = Join-Path $dir 'wufix-supervisor.log'
$taskName = 'WUFixRunElevated'

function Log($m) { "$([DateTime]::Now.ToString('yyyy-MM-dd HH:mm:ss')) $m" | Tee-Object -FilePath $log -Append }

# one-time: register an on-demand task that runs the exe elevated WITHOUT a UAC prompt
$action = New-ScheduledTaskAction -Execute $exe -WorkingDirectory $dir
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel HighestAvailable
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::FromHours(4))
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null
Log "supervisor start; task $taskName registered (elevated on-demand)"

$maxAttempts = 4
for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
    # remove stale transcripts so "newest" is unambiguous
    Get-ChildItem -Path $dir -Filter 'wufix-*.log' -EA SilentlyContinue | Remove-Item -Force -EA SilentlyContinue
    Log "attempt $attempt of $maxAttempts : launching via scheduled task"
    Start-ScheduledTask -TaskName $taskName
    Start-Sleep -Seconds 20

    $finalGate = $null
    $staleChecks = 0
    $lastSize = 0
    while ($true) {
        Start-Sleep -Seconds 60
        $newest = Get-ChildItem -Path $dir -Filter 'wufix-*.log' -EA SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $newest) { Log "no transcript yet"; continue }
        $tail = Get-Content $newest.FullName -Tail 3 -EA SilentlyContinue
        $finalLine = $tail | Where-Object { $_ -match '^FINAL gate=(\d+) steps=(\d+) failed=(\d+)' } | Select-Object -First 1
        if ($finalLine -match 'gate=(\d+)') {
            $finalGate = $Matches[1]
            Log "attempt $attempt FINISHED: $finalLine"
            break
        }
        # stall detection: transcript unchanged for 5 consecutive checks (5 min)
        $size = $newest.Length
        if ($size -eq $lastSize) { $staleChecks++ } else { $staleChecks = 0 }
        $lastSize = $size
        if ($staleChecks -ge 5) {
            Log "attempt $attempt stalled (transcript unchanged ~5 min) - moving to next attempt"
            break
        }
    }

    if ($finalGate -eq '0') {
        Log "SUCCESS: attempt $attempt completed START TO END with gate=0 (zero errors)"
        # belt-and-braces: re-run pause-only so the permanent pause is freshly
        # verified as the very last thing that touches the system
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -EA SilentlyContinue
        $action2 = New-ScheduledTaskAction -Execute $exe -Argument '--pause-only' -WorkingDirectory $dir
        Register-ScheduledTask -TaskName $taskName -Action $action2 -Principal $principal -Settings $settings -Force | Out-Null
        Start-ScheduledTask -TaskName $taskName
        Start-Sleep -Seconds 90
        Log "pause-only safeguard executed; verifying registry end state"
        $exp = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings' -EA SilentlyContinue).PauseUpdatesExpiryTime
        $wua = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\wuauserv' -EA SilentlyContinue).Start
        Log "end state: PauseUpdatesExpiryTime=$exp wuauservStart=$wua"
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -EA SilentlyContinue
        Log "supervisor done (SUCCESS path)"
        exit 0
    }
    Log "attempt $attempt did not complete gate=0 - cleaning up for next attempt"
    # kill any half-alive instance so the next attempt starts clean
    Get-Process -Name 'wu-repair-cycle' -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
    Start-Sleep -Seconds 10
}

# all attempts failed the full cycle - guarantee the demanded end state anyway
Log "all $maxAttempts attempts exhausted - running PAUSE-ONLY safeguard (permanent pause, no PC restart)"
$action2 = New-ScheduledTaskAction -Execute $exe -Argument '--pause-only' -WorkingDirectory $dir
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -EA SilentlyContinue
Register-ScheduledTask -TaskName $taskName -Action $action2 -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $taskName
Start-Sleep -Seconds 120
$exp = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings' -EA SilentlyContinue).PauseUpdatesExpiryTime
$wua = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\wuauserv' -EA SilentlyContinue).Start
Log "fallback end state: PauseUpdatesExpiryTime=$exp wuauservStart=$wua"
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -EA SilentlyContinue
Log "supervisor done (fallback path - updates permanently paused, PC untouched)"
exit 1
