# Headless test of Oumu's UI (Rune tooling): every panel view and overlay renders, and
# scripted taps and typing change what they should (checked in the element tree).
# No window opens and nothing touches the screen, mouse or keyboard; Oumu's data goes to a
# temp folder. Keep this file ASCII (Windows PowerShell 5.1).
param([switch]$Release)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root ("target\" + $(if ($env:CRATUS_PROFILE) { $env:CRATUS_PROFILE } elseif ($Release) { "release" } else { "debug" }) + "\oumu.exe")
$out = Join-Path $root "target\oumu-headless-test"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
$env:OUMU_DATA_DIR = Join-Path $out "data"
New-Item -ItemType Directory -Force $env:OUMU_DATA_DIR | Out-Null
$script:pass = 0; $script:fail = 0
. (Join-Path $PSScriptRoot "overflow.ps1")

function Check($name, $ok, $detail = "") {
    if ($ok) { $script:pass++; Write-Host "  ok   $name $detail" }
    else { $script:fail++; Write-Host "  FAIL $name $detail" -ForegroundColor Red }
}

function Run([string[]]$a) {
    $p = Start-Process -FilePath $exe -ArgumentList $a -NoNewWindow -PassThru -RedirectStandardError (Join-Path $out "stderr.txt") -RedirectStandardOutput (Join-Path $out "stdout.txt")
    $null = $p.Handle  # keeps ExitCode readable after a timed wait
    # a run that hangs (or sits behind a crash box) is killed and fails, never stalls the suite
    if (-not $p.WaitForExit(90000)) {
        Write-Host "  (killed after 90 s: $($a -join ' '))" -ForegroundColor Red
        try { $p.Kill() } catch {}
        return 124
    }
    $p.WaitForExit()
    return $p.ExitCode
}

function TreeText($path) {
    if (-not (Test-Path $path)) { return "" }
    $raw = Get-Content -Raw -Encoding UTF8 $path
    return ([regex]::Matches($raw, '"(?:name|text)":\s*"((?:[^"\\]|\\.)*)"') | ForEach-Object { $_.Groups[1].Value }) -join "`n"
}

function Script($window, $name, $events, $at, [string[]]$expect, [string[]]$absent = @()) {
    $sp = Join-Path $out "$name.json"
    ('{"events": [' + ($events -join ",") + ']}') | Out-File -Encoding ascii $sp
    $tree = Join-Path $out "$name.tree.json"
    $code = Run @("--window", $window, "--script", $sp, "--tree", $tree, "--at", "$at")
    $t = TreeText $tree
    $missing = @($expect | Where-Object { $t -notlike "*$_*" })
    $present = @($absent | Where-Object { $t -like "*$_*" })
    $over = @(Get-Overflows $tree)
    Check "$window / $name" ($code -eq 0 -and $missing.Count -eq 0 -and $present.Count -eq 0 -and $over.Count -eq 0) $(if ($missing.Count) { "missing: " + ($missing -join " | ") } elseif ($present.Count) { "unexpected: " + ($present -join " | ") } elseif ($over.Count) { "text cut off: " + ($over -join " | ") } else { "" })
}

function Tap($key, $t) { return '{"t":' + $t + ',"type":"tap","key":"' + $key + '"}' }

Write-Host "== windows render"
Run @("--list-windows") | Out-Null
$list = Get-Content (Join-Path $out "stdout.txt") | Where-Object { $_ -and $_ -notmatch "^rune:" }
$panels = @("review", "review-empty", "chat", "chat-thread", "plagiarism", "tone", "performance", "paraphraser", "paraphraser-result", "humanizer", "reactions", "translate", "explain", "rewriter", "detector", "detector-loading", "history", "onboarding", "settings", "settings-update", "review-update")
$surfaces = @("tab", "tab-open", "button", "button-linting", "card", "card-long", "card-mute")
foreach ($w in $panels + $surfaces) {
    if ($list -notcontains $w) { Check "listed: $w" $false; continue }
    $png = Join-Path $out "$w.png"
    $code = Run @("--window", $w, "--snap", $png, "--at", "1.0")
    $min = if ($panels -contains $w) { 50000 } else { 1500 }
    Check "renders: $w" ($code -eq 0 -and (Test-Path $png) -and (Get-Item $png).Length -gt $min) ("{0:N0} bytes" -f $(if (Test-Path $png) { (Get-Item $png).Length } else { 0 }))
}

Write-Host "== nothing cut off"
foreach ($w in $panels + $surfaces) { Script $w "layout-$w" @() 1.0 @() }

Write-Host "== review"
Script "review" "first-open" @() 0.5 @("Why?", "Review suggestions", "Correctness", "Spelling", "library", "liberty", "Accept", "Dismiss", "Skip this app")
Script "review" "clarity-tab" @((Tap "cat/clarity" 0.2)) 0.6 @("Wordy phrase", "I think") @("tomorow")
Script "review" "open-second" @((Tap "alert/a2" 0.2)) 0.6 @("tomorrow", "tomorow")
Script "review-empty" "no-field" @() 0.5 @("Click into a text field", "Explorer")
Script "review" "rail-chat" @((Tap "rail/chat" 0.2)) 0.6 @("Oumu AI Chat", "Summarize this page", "Review 7 suggestions")
Script "review" "rail-settings" @((Tap "rail/settings" 0.2)) 0.6 @("Your AI key", "Appearance", "Skipped apps")

Write-Host "== tools"
Script "chat" "type" @((Tap "chat-input" 0.2), '{"t":0.3,"type":"type","text":"Hello Oumu"}') 1.2 @("Hello Oumu")
Script "chat-thread" "thread" @() 0.5 @("Summarize this page.", "library tomorrow")
Script "humanizer" "voices" @() 0.5 @("Create a voice", "The Naturalist", "The Scholar")
Script "humanizer" "create" @((Tap "create-voice" 0.2)) 0.6 @("Writing sample", "Save voice")
Script "reactions" "audiences" @((Tap "audience" 0.2)) 0.6 @("My manager", "Investors", "Create audience")
Script "paraphraser" "styles" @() 0.5 @("Change writing style", "Academic", "Shorten")
Script "paraphraser-result" "result" @() 0.5 @("heading to the library", "Replace text", "Copy", "Show changes")
Script "paraphraser-result" "diff" @((Tap "diff" 0.2)) 0.6 @("Hide changes")
Script "rewriter" "ready" @() 0.5 @("AI Rewriter is ready to go", "Scan my text")
Script "review" "rewriter-auto-scan" @((Tap "rail/rewriter" 0.3)) 2.0 @("AI phrases reviewed", "it is important to note that", "delve into") @("Scan my text")
Script "review" "rewriter-dismiss" @((Tap "rail/rewriter" 0.3), (Tap "dismiss" 1.5)) 2.2 @("1/2")
Script "humanizer" "voice-form" @((Tap "create-voice" 0.2)) 0.8 @("e.g. The Storyteller", "Paste something you wrote")
Script "humanizer" "voice-needs-name" @((Tap "create-voice" 0.2), (Tap "save-voice" 0.5)) 1.0 @("Give the voice a name first.")
Script "detector" "hook" @() 0.5 @("Detect AI generated text", "Check this text")
Script "detector-loading" "steps" @() 0.5 @("Scanning your text")
Script "plagiarism" "hook" @() 0.5 @("Check for plagiarism", "APA")
Script "translate" "ready" @() 0.5 @("Ready to translate.", "Translate all", "Auto-detect", "English (American)")
Script "explain" "ready" @() 0.5 @("Spot something confusing?", "Explain the whole page")
Script "tone" "no-ai" @((Tap "detect" 0.2)) 2.5 @("Set up AI in Settings")
Script "performance" "score" @() 0.5 @("72", "Overall score", "Readability (Flesch)", "3 issues")
Script "onboarding" "choices" @() 0.5 @("Choose how Oumu thinks", "Your AI key", "Local model (optional)", "Skip for now")

Write-Host "== settings"
Script "settings" "placeholders" @() 0.5 @("AQ.", "Optional, e.g. http://localhost:8888")
Script "settings" "advanced" @((Tap "advanced" 0.2)) 0.6 @("Base URL", "Test connection")
Script "settings" "key-kind" @((Tap "key" 0.2), '{"t":0.3,"type":"type","text":"sk-ant-x"}') 1.2 @("Anthropic key")
Script "settings" "name" @() 0.5 @("Name (used for the avatar and greetings)", "Dock side", "Icon rail")

Write-Host "== updates"
Script "settings" "update-controls" @() 0.5 @("Check for updates automatically", "Check for updates", "is the newest version")
Script "settings-update" "update-found" @() 0.5 @("Oumu 0.2.1 is available (you have", "Update now")
Script "review-update" "banner" @() 0.5 @("Oumu 0.2.1 is available", "Oumu closes and opens again by itself.", "Update", "Later")
Script "review-update" "banner-later" @((Tap "update-later" 0.2)) 0.6 @("Oumu") @("Oumu 0.2.1 is available")
Script "settings" "updates-off" @('{"t":0.2,"type":"wheel","x":200,"y":400,"dy":-3000}', (Tap "check-updates" 0.8)) 1.4 @()
$sf = Get-ChildItem -Recurse -Filter settings.json $env:OUMU_DATA_DIR | Select-Object -First 1
$saved = if ($sf) { Get-Content -Raw $sf.FullName | ConvertFrom-Json } else { $null }
Check "turning automatic update checks off is saved" ($saved -and $saved.check_updates -eq $false)

Write-Host "== overlays"
Script "card" "spelling" @() 0.5 @("Correctness", "library", "Accept", "Dismiss", "Add to dictionary")
Script "card-mute" "offer" @() 0.5 @("Turn it off", "dismissed")
Script "card-long" "diff" @() 0.5 @("I think", "Accept") @("Add to dictionary")
Script "tab-open" "tab" @() 0.5 @("Open Oumu")
Script "button" "button" @() 0.5 @("Open Oumu")

Write-Host ""
Write-Host ("{0} passed, {1} failed" -f $script:pass, $script:fail)
if ($script:fail -gt 0) { exit 1 }
