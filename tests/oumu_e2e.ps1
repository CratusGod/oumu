# Oumu E2E (Windows): the real oumu.exe against a WPF test editor (its own exe, so no
# other app is touched). A real mouse hover opens the suggestion card and a real click
# on Accept fixes the text; the debug probe (OUMU_DEBUG_PORT) checks state and runs the
# AI tools. Settings, history and logs live in target\e2e\oumu-run-*; the user's own AI
# key is copied into that test settings file (never printed) unless -NoAI.
#   powershell -ExecutionPolicy Bypass -File tests\oumu_e2e.ps1 [-NoAI] [-Release]
param([switch]$NoAI, [switch]$Release)
$ErrorActionPreference = "Stop"
$ws = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $ws ("target\" + $(if ($env:CRATUS_PROFILE) { $env:CRATUS_PROFILE } elseif ($Release) { "release" } else { "debug" }) + "\oumu.exe")
$run = Join-Path $ws ("target\e2e\oumu-run-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
$data = Join-Path $run "data"
New-Item -ItemType Directory -Force $data | Out-Null
$port = 17999
$results = New-Object System.Collections.ArrayList
function Check($name, $ok, $detail) {
  [void]$results.Add([pscustomobject]@{ Case = $name; Ok = [bool]$ok; Detail = "$detail" })
  Write-Host ("{0} {1} {2}" -f $(if ($ok) { "ok  " } else { "FAIL" }), $name, $detail)
}

# --- the test editor: a WPF TextBox in its own process
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$editor = Join-Path $run "OumuTestEditor.exe"
$src = @'
using System;
using System.Windows;
using System.Windows.Controls;
public static class OumuTestEditor {
    [STAThread]
    public static void Main(string[] args) {
        var app = new Application();
        var w = new Window { Title = "Oumu test editor", Width = 760, Height = 360, Left = 600, Top = 260, WindowStartupLocation = WindowStartupLocation.Manual, Topmost = true };
        var tb = new TextBox { TextWrapping = TextWrapping.Wrap, FontSize = 20, AcceptsReturn = true };
        tb.Text = "Their is a mistake in this sentense. She dont like apples, and me and him goes to the store yesterday. In order to make a decision, we need to think about it.";
        w.Content = tb;
        w.Loaded += (s, e) => { tb.Focus(); tb.CaretIndex = 10; };
        app.Run(w);
    }
}
'@
$refs = "PresentationFramework", "PresentationCore", "WindowsBase", "System.Xaml" | ForEach-Object { [System.Reflection.Assembly]::LoadWithPartialName($_).Location }
Add-Type -TypeDefinition $src -ReferencedAssemblies $refs -OutputAssembly $editor -OutputType WindowsApplication

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class In {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(int f, int x, int y, int d, int e);
  public static void Move(int x, int y) { SetCursorPos(x, y); mouse_event(1, 0, 0, 0, 0); }
  public static void Glide(int x0, int y0, int x1, int y1) { for (int i = 1; i <= 12; i++) { Move(x0 + (x1 - x0) * i / 12, y0 + (y1 - y0) * i / 12); System.Threading.Thread.Sleep(15); } }
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  public static string Under(int x, int y) { var p = new POINT { X = x, Y = y }; var h = GetAncestor(WindowFromPoint(p), 2); var sb = new System.Text.StringBuilder(256); GetWindowText(h, sb, 256); return sb.ToString(); }
  public static void Click(int x, int y) { Move(x, y); System.Threading.Thread.Sleep(80); mouse_event(2, 0, 0, 0, 0); System.Threading.Thread.Sleep(60); mouse_event(4, 0, 0, 0, 0); }
}
'@
[In]::SetProcessDPIAware() | Out-Null

# --- settings for this run (AI config copied from the user's own Oumu)
$settings = @{ onboarded = $true; show_button = $true; show_underlines = $true; target_language = "Vietnamese" }
$real = Join-Path $env:APPDATA "dev.cratusgod.oumu\settings.json"
$hasAI = $false
if (-not $NoAI -and (Test-Path $real)) {
  $llm = (Get-Content $real -Raw | ConvertFrom-Json).llm
  if ($llm -and $llm.provider -and $llm.provider -ne "none") { $settings.llm = $llm; $hasAI = $true }
}
# a Gemini key handed to this run (OUMU_E2E_KEY) replaces the copied one; never printed
if (-not $NoAI -and $env:OUMU_E2E_KEY) {
  $model = ""
  if ($settings.llm -and $settings.llm.provider -eq "gemini") { $model = $settings.llm.model }
  $settings.llm = @{ provider = "gemini"; base_url = "https://generativelanguage.googleapis.com/v1beta"; api_key = $env:OUMU_E2E_KEY; model = $model }
  $hasAI = $true
}
$settings | ConvertTo-Json -Depth 6 | Out-File -Encoding utf8 (Join-Path $data "settings.json")

function Start-Oumu($extraArgs) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo $exe
  $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
  if ($extraArgs) { $psi.Arguments = $extraArgs }
  $psi.EnvironmentVariables["OUMU_DATA_DIR"] = $data
  $psi.EnvironmentVariables["OUMU_PIPE"] = '\\.\pipe\oumu-e2e'
  $psi.EnvironmentVariables["OUMU_DEBUG_PORT"] = "$port"
  $psi.EnvironmentVariables["OUMU_E2E"] = "1"
  # OUMU_E2E_STDERR=file: keep what the app prints (a few lines, e.g. RUNE_GPU_LOG=1)
  if ($env:OUMU_E2E_STDERR -and -not $extraArgs) { $psi.RedirectStandardError = $true }
  [System.Diagnostics.Process]::Start($psi)
}
function Api($path, $body = @{}) {
  Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$port$path" -Body ($body | ConvertTo-Json -Depth 8 -Compress) -ContentType "application/json; charset=utf-8" -TimeoutSec 120
}
function WaitFor($secs, [scriptblock]$cond) {
  $end = (Get-Date).AddSeconds($secs)
  while ((Get-Date) -lt $end) { try { $v = & $cond; if ($v) { return $v } } catch {} ; Start-Sleep -Milliseconds 250 }
  return $null
}
$log = Join-Path $data "oumu.log"

$oumu = Start-Oumu
$ed = $null
try {
  $h = WaitFor 20 { (Api "/health").ok }
  Check "starts, probe answers" $h ""
  $ed = Start-Process $editor -PassThru
  $st = WaitFor 20 { $s = Api "/state"; if ($s.alerts.Count -ge 3 -and $s.geometry.rects.Count -ge 3) { $s } }
  $orig = @($st.alerts | ForEach-Object { $_.original }) -join ", "
  Check "reads the field and finds mistakes" ($st -and $st.alerts.Count -ge 3) "$orig"
  Check "underlines every mistake" ($st.geometry.rects.Count -ge 3) "$($st.geometry.rects.Count) underlines"
  $score = $st.score
  Check "scores the text" ($score.words -gt 20) "words=$($score.words) overall=$($score.overall)"

  # hover "sentense" with the real mouse: the card opens
  $a = $st.alerts | Where-Object { $_.original -eq "sentense" } | Select-Object -First 1
  $r = $st.geometry.rects | Where-Object { $_.id -eq $a.id } | Select-Object -First 1
  $gx = [int]($st.geometry.origin_x + $r.x + $r.w / 2); $gy = [int]($st.geometry.origin_y + $r.y + $r.h - 4)
  [In]::Glide($gx - 40, $gy + 60, $gx, $gy)
  $acc = WaitFor 8 { (Get-Content $log -ErrorAction SilentlyContinue | Select-String "card-accept (\d+):(\d+)" | Select-Object -Last 1) }
  Check "hover opens the suggestion card" $acc "$gx,$gy"
  if ($acc) {
    $m = $acc.Matches[0]
    $ax = [int]$m.Groups[1].Value; $ay = [int]$m.Groups[2].Value
    [In]::Glide($gx, $gy, $ax, $ay)
    Start-Sleep -Milliseconds 300
    Write-Host ("  under Accept: " + [In]::Under($ax, $ay))
    [In]::Click($ax, $ay)
    $fixed = WaitFor 10 { $s = Api "/state"; if ($s.doc.text -like "*sentence.*") { $s } }
    Check "Accept on the card fixes the word in the app" $fixed $(if ($fixed) { "text now has 'sentence'" } else { (Api "/state").doc.text })
  }
  [In]::SetCursorPos(200, 900) | Out-Null

  # hover every underline five times: the card must open each time without the app stalling
  # (an overlay that changed size made Rune rebuild its caches, slower each time)
  $st = Api "/state"
  $marks = @($st.geometry.rects)
  $logBefore = @(Get-Content $log -ErrorAction SilentlyContinue).Count
  for ($round = 0; $round -lt 5; $round++) {
    foreach ($r in $marks) {
      $hx = [int]($st.geometry.origin_x + $r.x + $r.w / 2); $hy = [int]($st.geometry.origin_y + $r.y + $r.h - 4)
      [In]::Glide($hx - 30, $hy + 50, $hx, $hy)
      Start-Sleep -Milliseconds 700
      [In]::Glide($hx, $hy, $hx - 30, $hy + 160)
      Start-Sleep -Milliseconds 600
    }
  }
  [In]::SetCursorPos(200, 900) | Out-Null
  $stalls = @(Get-Content $log -ErrorAction SilentlyContinue | Select-Object -Skip $logBefore | Select-String "ui thread was busy for ([0-9]+) ms" | ForEach-Object { [int]$_.Matches[0].Groups[1].Value })
  $worst = if ($stalls.Count) { ($stalls | Measure-Object -Maximum).Maximum } else { 0 }
  Check "hovering many times stays smooth" ($worst -lt 400) "$($marks.Count * 5) hovers, worst stall $worst ms"

  # the probe's Accept and Dismiss
  $st = WaitFor 10 { $s = Api "/state"; if ($s.alerts | Where-Object { $_.original -eq "Their" }) { $s } }
  $their = $st.alerts | Where-Object { $_.original -eq "Their" } | Select-Object -First 1
  if ($their) {
    Api "/accept" @{ id = $their.id; suggestion = 0 } | Out-Null
    $ok = WaitFor 10 { $s = Api "/state"; if ($s.doc.text -like "There is*") { $s } }
    Check "Accept (panel path) fixes 'Their'" $ok $(if ($ok) { "There is a mistake..." } else { "" })
  } else { Check "Accept (panel path) fixes 'Their'" $false "no 'Their' alert" }
  $st = Api "/state"
  $dont = $st.alerts | Where-Object { $_.original -eq "dont" } | Select-Object -First 1
  if ($dont) {
    Api "/dismiss" @{ id = $dont.id } | Out-Null
    $gone = WaitFor 5 { $s = Api "/state"; if (-not ($s.alerts | Where-Object { $_.original -eq "dont" })) { $true } }
    Check "Dismiss hides the suggestion" $gone ""
  }

  # the panel: a second launch opens it; Esc-style tuck hides it
  $second = Start-Oumu
  $second.WaitForExit(10000) | Out-Null
  $p = WaitFor 8 { $s = Api "/state"; if ($s.panel.visible -and -not $s.panel.tucked) { $s.panel } }
  Check "a second launch opens the running Oumu's panel" $p "x=$($p.x) y=$($p.y)"
  # it slides all the way out (a tuck mid-slide once left it opening half off screen)
  Start-Sleep -Milliseconds 600
  $scr = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $sc = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
  $px = (Api "/state").panel.x
  Check "the panel opens fully on screen" ($px -ge -1 -and $px -lt $scr.Width) "x=$px"
  # tuck it at once and open it again: it must still open fully
  Api "/panel" @{ tucked = $true } | Out-Null
  Start-Sleep -Milliseconds 50
  Api "/panel" @{ tucked = $false } | Out-Null
  Start-Sleep -Milliseconds 120
  Api "/panel" @{ tucked = $true } | Out-Null
  Start-Sleep -Milliseconds 600
  Api "/panel" @{ tucked = $false } | Out-Null
  Start-Sleep -Milliseconds 900
  $px = (Api "/state").panel.x
  Check "a tuck mid-slide doesn't leave it half open" ($px -ge -1 -and $px -lt $scr.Width) "x=$px"
  Api "/panel" @{ tucked = $true } | Out-Null
  $t = WaitFor 5 { $s = Api "/state"; if ($s.panel.tucked -and $s.panel.tab_visible) { $true } }
  Check "tucking hides the panel behind the edge tab" $t ""

  # tools that need no AI
  $rw = Api "/feature" @{ name = "rewriter_scan"; args = @{} }
  Check "AI Rewriter scan (offline)" ($rw.words -gt 0) "words=$($rw.words) phrases=$($rw.phrases.Count)"
  $sc = Api "/feature" @{ name = "score"; args = @{} }
  Check "Performance score" ($sc.words -gt 0) "words=$($sc.words)"
  try { $sp = Api "/feature" @{ name = "spot_check"; args = @{} } } catch { $sp = $null; $spErr = if ($_.ErrorDetails.Message) { $_.ErrorDetails.Message } else { $_.Exception.Message } }
  Check "Spot check" ($sp -ne $null) $(if ($sp) { "sentences=$($sp.sentences)" } else { ($spErr -replace 'AQ.[0-9A-Za-z_.-]{10,}', '***' -replace 'AIza[0-9A-Za-z_-]{10,}', '***') })
  try { $pl = Api "/feature" @{ name = "plagiarism"; args = @{} }; Check "Plagiarism check (web search)" ($pl -ne $null) "$(($pl | ConvertTo-Json -Compress -Depth 3).Substring(0, [Math]::Min(80, ($pl | ConvertTo-Json -Compress -Depth 3).Length)))" } catch { Check "Plagiarism check (web search)" $false $_.Exception.Message }

  # AI tools (the user's own key)
  if ($hasAI) {
    foreach ($f in @(
      @{ n = "ping_llm"; a = @{} },
      @{ n = "paraphrase"; a = @{ style = "Professional" } },
      @{ n = "translate"; a = @{ target = "Vietnamese" } },
      @{ n = "tone"; a = @{} },
      @{ n = "explain"; a = @{ whole = $true } },
      @{ n = "detector"; a = @{} },
      @{ n = "humanize"; a = @{} },
      @{ n = "reactions"; a = @{} },
      @{ n = "summarize"; a = @{} }
    )) {
      Start-Sleep -Seconds 7  # stay under a free key's per-minute limit
      try {
        $v = Api "/feature" @{ name = $f.n; args = $f.a }
        $j = $v | ConvertTo-Json -Compress -Depth 4
        Check ("AI " + $f.n) ($j.Length -gt 10) $j.Substring(0, [Math]::Min(90, $j.Length))
      } catch {
        $msg = if ($_.ErrorDetails.Message) { $_.ErrorDetails.Message } else { $_.Exception.Message }
        # never show a key: URLs and messages can carry it
        $msg = $msg -replace '(key=)[^&"s]+', '$1***' -replace 'AIza[0-9A-Za-z_-]{10,}', '***' -replace 'AQ.[0-9A-Za-z_.-]{10,}', '***' -replace '(sk-|gsk_|sk-ant-)[0-9A-Za-z_-]{8,}', '***'
        Check ("AI " + $f.n) $false $msg.Substring(0, [Math]::Min(160, $msg.Length))
      }
    }

    # --- every tool from the panel itself: open its view, press its button through UI
    # Automation (no mouse), wait for the panel's "done" line in the log
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    function Find-Named($procId, $name) {
      $root = [System.Windows.Automation.AutomationElement]::RootElement
      $pc = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ProcessIdProperty), $procId
      $nc = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty), $name
      foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $pc)) {
        $e = $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $nc)
        if ($e) { return $e }
      }
      return $null
    }
    function Press($procId, $name) {
      $e = WaitFor 8 { Find-Named $procId $name }
      if (-not $e) { return $false }
      $ip = $null
      if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$ip)) { $ip.Invoke(); return $true }
      return $false
    }
    function LogMark() { @(Get-Content $log -ErrorAction SilentlyContinue).Count }
    function WaitLog($mark, $pattern, $secs) {
      WaitFor $secs { @(Get-Content $log -ErrorAction SilentlyContinue) | Select-Object -Skip $mark | Where-Object { $_ -match $pattern } | Select-Object -Last 1 }
    }
    foreach ($t in @(
      @{ view = "paraphraser"; press = "Professional"; feature = "paraphrase" },
      @{ view = "humanizer"; press = "The Naturalist"; feature = "humanize" },
      @{ view = "translate"; press = "Translate all"; feature = "translate" },
      @{ view = "explain"; press = "Explain the whole page"; feature = "explain" },
      @{ view = "detector"; press = "Check this text"; feature = "detector" },
      @{ view = "reactions"; press = "Predict reactions"; feature = "reactions" },
      @{ view = "tone"; press = "Reload"; feature = "tone" },
      @{ view = "chat"; press = "Summarize this page"; feature = "chat" }
    )) {
      Start-Sleep -Seconds 5  # a free key's per-minute limit
      $mark = LogMark
      Api "/panel" @{ view = $t.view; focus = $false } | Out-Null
      Start-Sleep -Milliseconds 800
      # every button on the view has a name a screen reader can say
      $bc = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ControlTypeProperty), ([System.Windows.Automation.ControlType]::Button)
      $pc = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ProcessIdProperty), $oumu.Id
      $unnamed = 0
      foreach ($w in [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $pc)) {
        foreach ($b in $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $bc)) { if (-not $b.Current.Name.Trim()) { $unnamed++ } }
      }
      Check ("buttons named: " + $t.view) ($unnamed -eq 0) "$unnamed without a name"
      $pressed = Press $oumu.Id $t.press
      $done = if ($pressed) { WaitLog $mark ("panel: " + $t.feature + " done") 150 } else { $null }
      $line = if ($done) { ($done -replace '^.*panel: ', '') } elseif ($pressed) { "no answer in 150 s" } else { "button '$($t.press)' not found" }
      $line = $line -replace 'AIza[0-9A-Za-z_-]{10,}', '***' -replace 'AQ.[0-9A-Za-z_.-]{10,}', '***'
      Check ("panel " + $t.view) ($done -and $done -match "done ok") $line.Substring(0, [Math]::Min(140, $line.Length))
    }
    Api "/panel" @{ tucked = $true } | Out-Null

    # --- a highlight outside any text box (a web page in a Chromium app, like Claude's chat or
    # a browser): `oumu --translate` must translate exactly the highlighted sentence
    $sentence = "The weather is lovely today, so we are going for a long walk by the river."
    $page = Join-Path $run "highlight.html"
    "<!doctype html><meta charset=utf-8><title>Oumu highlight test</title><body style='font:20px sans-serif;padding:40px'><p>Some text before.</p><p id=t>$sentence</p><p>Some text after.</p><script>onload=()=>{const r=document.createRange();r.selectNodeContents(document.getElementById('t'));const s=getSelection();s.removeAllRanges();s.addRange(r);}</script>" | Out-File -Encoding utf8 $page
    $browser = @("$env:LOCALAPPDATA\imput\Helium\Application\chrome.exe", "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Google\Chrome\Application\chrome.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($browser) {
      # a throwaway profile: the user's own browser profile is never touched
      $prof = Join-Path $run "browser-profile"
      $br = Start-Process $browser -ArgumentList "--user-data-dir=`"$prof`"", "--no-first-run", "--no-default-browser-check", "--force-renderer-accessibility", "--app=file:///$($page -replace '\\','/')" -PassThru
      try {
        $ready = WaitFor 20 { $w = Get-Process -Id $br.Id -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process -Filter "Name='$(Split-Path $browser -Leaf)'" | Where-Object { $_.CommandLine -like "*$prof*" } | Select-Object -First 1 }
        Start-Sleep -Seconds 3
        $mark = LogMark
        $p2 = Start-Oumu "--translate"
        $got = WaitLog $mark "translate: [0-9]+ characters highlighted" 15
        $n = if ($got -match "translate: ([0-9]+) characters") { [int]$Matches[1] } else { -1 }
        Check "highlight on a web page is read" ($n -eq $sentence.Length) "$n characters (the sentence has $($sentence.Length))"
        $done = WaitLog $mark "panel: translate done" 150
        Check "highlight on a web page is translated" ($done -and $done -match "done ok") $(if ($done) { ($done -replace '^.*panel: ', '') } else { "no answer" })
        # the panel's own button reads the same highlight after Oumu took the focus
        Start-Sleep -Seconds 5
        $mark = LogMark
        Api "/panel" @{ view = "translate"; focus = $true } | Out-Null
        Start-Sleep -Milliseconds 800
        # the view still shows the first translation: back to its start screen
        if (Find-Named $oumu.Id "Back") { Press $oumu.Id "Back" | Out-Null; Start-Sleep -Milliseconds 500 }
        $pressed = Press $oumu.Id "Translate highlighted text"
        $got = if ($pressed) { WaitLog $mark "translate: [0-9]+ characters highlighted" 15 } else { $null }
        $n = if ($got -match "translate: ([0-9]+) characters") { [int]$Matches[1] } else { -1 }
        Check "panel button reads the highlight" ($n -eq $sentence.Length) $(if ($pressed) { "$n characters" } else { "button not found" })
      } finally {
        Get-CimInstance Win32_Process -Filter "Name='$(Split-Path $browser -Leaf)'" | Where-Object { $_.CommandLine -like "*$prof*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
      }
    } else { Check "highlight on a web page" $false "no Chromium browser found to test with" }
  } else { Check "AI tools" $true "skipped (no AI key set up)" }
} finally {
  if ($ed -and -not $ed.HasExited) { Stop-Process -Id $ed.Id -Force }
  if (-not $oumu.HasExited) { Stop-Process -Id $oumu.Id -Force }
  if ($env:OUMU_E2E_STDERR) { try { $oumu.StandardError.ReadToEnd() | Out-File -Encoding ascii $env:OUMU_E2E_STDERR } catch {} }
}
$pass = @($results | Where-Object Ok).Count
$md = "# Oumu E2E - $pass/$($results.Count) passed`n`nRun ``$run```n`n| Case | Result | Detail |`n|---|---|---|`n"
foreach ($r in $results) { $md += "| $($r.Case) | $(if ($r.Ok) { "PASS" } else { "FAIL" }) | $($r.Detail -replace '\|', '/') |`n" }
$md | Out-File -Encoding utf8 (Join-Path $run "report.md")
$md | Out-File -Encoding utf8 (Join-Path $ws "target\e2e\oumu-latest.md")
Write-Host "`n$pass/$($results.Count) passed"
if ($pass -ne $results.Count) { exit 1 }
