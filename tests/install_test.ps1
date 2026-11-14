# Sandbox test of Utsuri-Setup.exe and Oumu-Setup.exe: install -> check -> run -> uninstall -> check.
# Everything it writes carries the suffix " (test)" and lives in a temp folder; the real
# installs (and the old Python Utsuri / Tauri Oumu) are only read, before and after, to prove
# they were not touched. Keep this file ASCII (Windows PowerShell 5.1).
param([string]$Only = "")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$rel = Join-Path $root "target\release"
$suffix = " (test)"
$sandbox = Join-Path $env:TEMP "cratus-install-test"
$uninst = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
$run = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$sendTo = Join-Path $env:APPDATA "Microsoft\Windows\SendTo"
$script:pass = 0; $script:fail = 0

function Check($name, $ok) {
    if ($ok) { $script:pass++; Write-Host "  ok   $name" }
    else { $script:fail++; Write-Host "  FAIL $name" -ForegroundColor Red }
}

function Snapshot {
    # the real entries, to compare before/after
    $s = @()
    foreach ($k in @("Utsuri", "Oumu", "{9910EB1A-E5FE-43F9-B79D-1CAF25C3AF14}_is1")) {
        $p = Join-Path $uninst $k
        if (Test-Path -LiteralPath $p) { $s += "$k=" + (Get-ItemProperty -LiteralPath $p).UninstallString }
    }
    $r = Get-ItemProperty -LiteralPath $run
    foreach ($v in @("Utsuri", "Oumu")) { $s += "run:$v=" + $r.$v }
    $s += "shell=" + (Test-Path -LiteralPath "HKCU:\Software\Classes\*\shell\Utsuri")
    $s += "sendto=" + (Test-Path -LiteralPath (Join-Path $sendTo "Utsuri.lnk"))
    $s += "start:U=" + (Test-Path -LiteralPath (Join-Path $startMenu "Utsuri.lnk"))
    $s += "start:O=" + (Test-Path -LiteralPath (Join-Path $startMenu "Oumu.lnk"))
    return ($s -join "`n")
}

function RunWait($exe, [string]$argLine) {
    $p = Start-Process -FilePath $exe -ArgumentList $argLine -PassThru -Wait -WindowStyle Hidden
    return $p.ExitCode
}

function Test-App($name, $setup, $exeName, [bool]$hasShell) {
    Write-Host "== $name"
    $key = $name + "test"
    $display = $name + $suffix
    $dir = Join-Path $sandbox $name
    $exe = Join-Path $dir $exeName
    if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }

    # an older build still installed: a silent setup must refuse rather than ask
    $oldKey = if ($name -eq "Utsuri") { "{9910EB1A-E5FE-43F9-B79D-1CAF25C3AF14}_is1" } else { "Oumu" }
    $oldPath = Join-Path $uninst $oldKey
    $live = $false
    if (Test-Path -LiteralPath $oldPath) { $us = (Get-ItemProperty -LiteralPath $oldPath).UninstallString; $live = ($us -notlike "*--uninstall*") -and ($us -match '^"([^"]+)"') -and (Test-Path -LiteralPath $Matches[1]) }
    if ($live) {
        $code = RunWait $setup "--silent --suffix `"$suffix`" --dir `"$dir`""
        Check "silent setup refuses while the old $name is installed (exit $code)" ($code -ne 0 -and -not (Test-Path -LiteralPath $exe))
    }

    $code = RunWait $setup "--silent --ignore-old --suffix `"$suffix`" --dir `"$dir`""
    Check "setup exit code 0 (got $code)" ($code -eq 0)
    Check "exe installed" (Test-Path -LiteralPath $exe)
    Check "exe matches the release build" ((Get-FileHash -LiteralPath $exe).Hash -eq (Get-FileHash -LiteralPath (Join-Path $rel $exeName.ToLower())).Hash)
    Check "no leftover .new file" (-not (Test-Path -LiteralPath "$exe.new"))
    $lnk = Join-Path $startMenu "$display.lnk"
    Check "Start menu shortcut" (Test-Path -LiteralPath $lnk)
    if (Test-Path -LiteralPath $lnk) {
        $t = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk).TargetPath
        Check "shortcut points at the exe" ($t -eq $exe)
    }
    $k = Join-Path $uninst $key
    Check "Apps entry" (Test-Path -LiteralPath $k)
    if (Test-Path -LiteralPath $k) {
        $p = Get-ItemProperty -LiteralPath $k
        Check "DisplayName = $display" ($p.DisplayName -eq $display)
        Check "Publisher = CratusGod" ($p.Publisher -eq "CratusGod")
        Check "DisplayVersion set" ([string]$p.DisplayVersion -ne "")
        Check "UninstallString runs the exe with --uninstall" ($p.UninstallString -eq "`"$exe`" --uninstall --suffix `"$suffix`"")
        Check "QuietUninstallString" ($p.QuietUninstallString -like "*--uninstall --silent*")
        Check "InstallLocation" ($p.InstallLocation -eq $dir)
        Check "NoModify/NoRepair" ($p.NoModify -eq 1 -and $p.NoRepair -eq 1)
    }
    $rv = (Get-ItemProperty -LiteralPath $run).$display
    Check "starts at sign-in (Run value)" ($rv -like "`"$exe`"*")
    if ($hasShell) {
        foreach ($b in @("*", "Directory")) {
            $sk = "HKCU:\Software\Classes\$b\shell\$key"
            Check "right-click entry ($b)" (Test-Path -LiteralPath $sk)
            if (Test-Path -LiteralPath $sk) {
                Check "  label" ((Get-ItemProperty -LiteralPath $sk).MUIVerb -like "$display*")
                Check "  command" ((Get-ItemProperty -LiteralPath "$sk\command").'(default)' -eq "`"$exe`" `"%1`"")
            }
        }
        Check "Send To shortcut" (Test-Path -LiteralPath (Join-Path $sendTo "$display.lnk"))
    }

    # run the installed copy (on test pipes and data so it can't meet the real apps)
    $env:UTSURI_PIPE = "utsuri-install-test"; $env:UTSURI_NO_WELCOME = "1"
    $env:OUMU_PIPE = "\\.\pipe\oumu-install-test"; $env:OUMU_DATA_DIR = Join-Path $sandbox "oumu-data"
    $env:OUMU_DEBUG_PORT = "17998"
    $proc = Start-Process -FilePath $exe -ArgumentList "--quiet" -PassThru
    Start-Sleep -Seconds 4
    Check "installed app runs" (-not $proc.HasExited)

    # uninstall the way Settings -> Apps does (silently); it must stop the running copy
    $code = RunWait $exe "--uninstall --silent --suffix `"$suffix`""
    Check "uninstall exit code 0 (got $code)" ($code -eq 0)
    Start-Sleep -Milliseconds 500
    Check "running copy stopped" ($proc.HasExited)
    for ($i = 0; $i -lt 20 -and (Test-Path -LiteralPath $dir); $i++) { Start-Sleep -Milliseconds 500 }
    Check "install folder removed" (-not (Test-Path -LiteralPath $dir))
    Check "Start menu shortcut removed" (-not (Test-Path -LiteralPath $lnk))
    Check "Apps entry removed" (-not (Test-Path -LiteralPath $k))
    Check "Run value removed" ($null -eq (Get-ItemProperty -LiteralPath $run).$display)
    if ($hasShell) {
        Check "right-click entries removed" (-not (Test-Path -LiteralPath "HKCU:\Software\Classes\*\shell\$key") -and -not (Test-Path -LiteralPath "HKCU:\Software\Classes\Directory\shell\$key"))
        Check "Send To removed" (-not (Test-Path -LiteralPath (Join-Path $sendTo "$display.lnk")))
    }
    foreach ($v in @("UTSURI_PIPE", "UTSURI_NO_WELCOME", "OUMU_PIPE", "OUMU_DATA_DIR", "OUMU_DEBUG_PORT")) { Remove-Item "Env:$v" -ErrorAction SilentlyContinue }
}

function Test-OldVersions {
    # an older build's Apps entry under a test key name (never a real one)
    Write-Host "== old versions (test key CratusOldTest)"
    $setup = Join-Path $rel "utsuri-setup.exe"
    $dir = Join-Path $sandbox "old-case"
    $exe = Join-Path $dir "Utsuri.exe"
    $oldKey = Join-Path $uninst "CratusOldTest"
    $oldDir = Join-Path $sandbox "old"
    New-Item -ItemType Directory -Force $oldDir | Out-Null
    $fake = Join-Path $oldDir "olduninst.exe"
    $common = "--silent --suffix `"$suffix`" --dir `"$dir`" --old-key CratusOldTest"
    function Remove-Test { if (Test-Path -LiteralPath $exe) { RunWait $exe "--uninstall --silent --suffix `"$suffix`"" | Out-Null; for ($i = 0; $i -lt 20 -and (Test-Path -LiteralPath $dir); $i++) { Start-Sleep -Milliseconds 500 } } }
    function New-Old($cmd) { New-Item -Path $oldKey -Force | Out-Null; Set-ItemProperty -LiteralPath $oldKey -Name DisplayName -Value "Old test app"; Set-ItemProperty -LiteralPath $oldKey -Name UninstallString -Value $cmd }

    # 1. a dead entry (its uninstaller is gone): cleaned up, install goes on
    New-Old "`"$oldDir\missing.exe`""
    $code = RunWait $setup $common
    Check "dead old entry: setup succeeds (exit $code)" ($code -eq 0 -and (Test-Path -LiteralPath $exe))
    Check "dead old entry removed" (-not (Test-Path -LiteralPath $oldKey))
    Remove-Test

    # 2. the old uninstaller is there but doesn't remove anything (user cancelled): stop
    Copy-Item "$env:WINDIR\System32\cmd.exe" $fake -Force
    New-Old "`"$fake`" /C exit 0"
    $code = RunWait $setup "$common --remove-old"
    Check "cancelled old uninstall: setup stops (exit $code)" ($code -ne 0 -and -not (Test-Path -LiteralPath $exe))
    Check "old entry kept" (Test-Path -LiteralPath $oldKey)

    # 3. an NSIS/Inno-style uninstaller: relaunches from %TEMP% and finishes 15 s later
    $tmpCopy = Join-Path $env:TEMP "cratus-olduninst-tmp.exe"
    $regPath = "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\CratusOldTest"
    New-Old "`"$fake`" /C copy /y `"$fake`" `"$tmpCopy`" >nul & start `"`" `"$tmpCopy`" /C `"ping -n 16 127.0.0.1 >nul & reg delete $regPath /f >nul & del `"$fake`"`""
    $t = [Diagnostics.Stopwatch]::StartNew()
    $code = RunWait $setup "$common --remove-old"
    $secs = [int]$t.Elapsed.TotalSeconds
    Check "slow old uninstaller: setup waited for it (${secs}s, exit $code)" ($code -eq 0 -and $secs -ge 14)
    Check "old entry gone" (-not (Test-Path -LiteralPath $oldKey))
    Check "old uninstaller gone" (-not (Test-Path -LiteralPath $fake))
    $p = Get-ItemProperty -LiteralPath (Join-Path $uninst "Utsuritest") -ErrorAction SilentlyContinue
    Check "new Apps entry intact after the old uninstaller finished" ($p -and $p.UninstallString -like "`"$exe`" --uninstall*" -and $p.DisplayVersion -eq "2.0.0")
    Check "new Run value intact" ((Get-ItemProperty -LiteralPath $run).$("Utsuri" + $suffix) -like "`"$exe`"*")
    Remove-Test
    Check "cleaned up" (-not (Test-Path -LiteralPath $dir) -and -not (Test-Path -LiteralPath (Join-Path $uninst "Utsuritest")))
    if (Test-Path -LiteralPath $oldKey) { Remove-Item -LiteralPath $oldKey -Recurse -Force }
    Remove-Item -LiteralPath $tmpCopy -Force -ErrorAction SilentlyContinue
}

$before = Snapshot
if ($Only -eq "" -or $Only -eq "old") { Test-OldVersions }
if ($Only -eq "" -or $Only -eq "utsuri") { Test-App "Utsuri" (Join-Path $rel "utsuri-setup.exe") "Utsuri.exe" $true }
if ($Only -eq "" -or $Only -eq "oumu") { Test-App "Oumu" (Join-Path $rel "oumu-setup.exe") "Oumu.exe" $false }
$after = Snapshot
Write-Host "== real installs"
Check "real entries untouched" ($before -eq $after)
if ($before -ne $after) { Write-Host "before:`n$before`nafter:`n$after" }
if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue }
Write-Host ""
Write-Host "$($script:pass) passed, $($script:fail) failed"
if ($script:fail -gt 0) { exit 1 }
