# One-time: register two on-demand tasks that run elevated WITHOUT UAC prompts.
# WUFixRunElevated  -> the full cycle exe
# WUFixPauseOnly    -> the exe with --pause-only (safeguard end state)
$ErrorActionPreference = 'Stop'
$dir = 'F:\study\Platforms\Windows\fixers\system-integrity\WindowsUpdate\Repair\wu-repair-cycle'
$exe = Join-Path $dir 'wu-repair-cycle.exe'
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel HighestAvailable
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Hours 4)
foreach ($spec in @(
    @{ n = 'WUFixRunElevated'; a = $exe; arg = '' },
    @{ n = 'WUFixPauseOnly'; a = $exe; arg = '--pause-only' }
)) {
    $action = if ($spec.arg) { New-ScheduledTaskAction -Execute $spec.a -Argument $spec.arg -WorkingDirectory $dir }
              else { New-ScheduledTaskAction -Execute $spec.a -WorkingDirectory $dir }
    Unregister-ScheduledTask -TaskName $spec.n -Confirm:$false -ErrorAction SilentlyContinue
    Register-ScheduledTask -TaskName $spec.n -Action $action -Principal $principal -Settings $settings | Out-Null
}
Write-Output 'REGISTERED:'
Get-ScheduledTask -TaskName WUFixRunElevated, WUFixPauseOnly | Select-Object TaskName, State | Format-Table -AutoSize | Out-String
