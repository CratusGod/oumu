# Gives the running (installed) Oumu work for a few minutes and prints its memory as it goes:
# types text with mistakes into a scratch Notepad file and switches between windows.
# Uses the real keyboard focus: run it only when the user is away. Types only while the
# scratch file's window is in front. Keep this file ASCII (Windows PowerShell 5.1).
param([int]$Minutes = 5)
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class Fg {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public static string Title() { var s = new StringBuilder(512); GetWindowText(GetForegroundWindow(), s, 512); return s.ToString(); }
}
"@
$name = "oumu-soak-$PID.txt"
$file = Join-Path $env:TEMP $name
"Soak test file." | Out-File -Encoding ascii $file
Start-Process notepad.exe $file
Start-Sleep -Seconds 3
$sh = New-Object -ComObject WScript.Shell
function Mem($tag) {
    $p = Get-Process Oumu -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { Write-Host "$tag Oumu is not running"; return }
    $id = $p.Id
    $g = (Get-Counter "\GPU Process Memory(pid_$($id)*)\Shared Usage" -ErrorAction SilentlyContinue).CounterSamples | Select-Object -First 1
    Write-Host ("{0} {1:HH:mm:ss} ws {2:N0} MB private {3:N0} MB gpuShared {4:N0} MB" -f $tag, (Get-Date), ($p.WorkingSet64/1MB), ($p.PrivateMemorySize64/1MB), ($g.CookedValue/1MB))
}
$lines = @("Their is a sentense with mistakes in it. ", "I dont like apples and me and him goes to the store. ", "This are a other line that have erors. ", "She recieve the mesage yesterday and definately agree. ")
Mem "start"
$end = (Get-Date).AddMinutes($Minutes)
$typed = 0; $skipped = 0; $i = 0
while ((Get-Date) -lt $end) {
    $null = $sh.AppActivate($name)
    Start-Sleep -Milliseconds 400
    if ([Fg]::Title() -like "*$name*") {
        [System.Windows.Forms.SendKeys]::SendWait($lines[$i % $lines.Count])
        if ($i % 6 -eq 5) { [System.Windows.Forms.SendKeys]::SendWait("^a{DEL}") }
        $typed++
    } else { $skipped++ }
    Start-Sleep -Milliseconds 1500
    if ($i % 4 -eq 3) {
        # another window in front for a moment, as when the user switches apps
        $null = $sh.AppActivate("Program Manager")
        Start-Sleep -Milliseconds 800
    }
    if ($i % 20 -eq 19) { Mem "  ..." }
    $i++
}
Mem "end  "
Write-Host "typed $typed times, skipped $skipped (window not in front)"
$null = $sh.AppActivate($name)
Start-Sleep -Milliseconds 400
if ([Fg]::Title() -like "*$name*") { [System.Windows.Forms.SendKeys]::SendWait("^a{DEL}^s"); Start-Sleep -Milliseconds 500; [System.Windows.Forms.SendKeys]::SendWait("%{F4}") }
Start-Sleep -Seconds 1
Remove-Item $file -ErrorAction SilentlyContinue
