# Build a Windows release and (with -Publish) put it on GitHub, where the app's in-app updater
# finds it (crates/shared/src/update.rs): a release tagged v<version> carrying
# <App>-Setup.exe and <App>-Setup.exe.sha256.
#
#   powershell -File packaging\release.ps1            # build + check, files in target\dist
#   powershell -File packaging\release.ps1 -Publish   # ...and make the GitHub release (gh)
#
# The build maps this PC's folders out of the exe (dependency paths in panic messages would
# otherwise carry the user's home folder), then checks the exes for the home folder and user
# name before anything is published. Keep this file ASCII (Windows PowerShell 5.1).
param([switch]$Publish, [switch]$SkipBuild, [string]$Notes = "")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$app = (Get-Content (Join-Path $root "Cargo.toml") | Select-String -Pattern '"crates/(utsuri|oumu)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$Title = $app.Substring(0, 1).ToUpper() + $app.Substring(1)
$repo = "CratusGod/$app"
$version = ((Get-Content (Join-Path $root "crates\$app\Cargo.toml") | Select-String -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value)
Write-Host "== $Title $version"

# the same flags for every cargo run on the release folder, or cargo rebuilds everything
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
$env:RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~ --remap-path-prefix=$cargoHome\registry\src=/cargo/registry --remap-path-prefix=$cargoHome\git\checkouts=/cargo/git --remap-path-prefix=$root=."

if (-not $SkipBuild) {
    Push-Location $root
    try {
        cargo build --release -p $app
        if ($LASTEXITCODE -ne 0) { throw "the $app build failed" }
        cargo build --release -p setup
        if ($LASTEXITCODE -ne 0) { throw "the setup build failed" }
    } finally { Pop-Location }
}

$exe = Join-Path $root "target\release\$app.exe"
$setup = Join-Path $root "target\release\$app-setup.exe"

# nothing of this PC in what is published: the home folder, or the user name as a path
# segment (a bare name can be a dictionary word, or hide inside an ordinary identifier)
$bs = [string][char]92
$bad = @($env:USERPROFILE, $env:USERPROFILE.Replace($bs, "/"), ($bs + $env:USERNAME + $bs), ("/" + $env:USERNAME + "/"))
foreach ($f in @($exe, $setup)) {
    $bytes = [IO.File]::ReadAllBytes($f)
    $texts = @([Text.Encoding]::ASCII.GetString($bytes), [Text.Encoding]::Unicode.GetString($bytes), [Text.Encoding]::Unicode.GetString($bytes, 1, $bytes.Length - 1))
    foreach ($b in $bad) {
        foreach ($t in $texts) {
            if ($t.IndexOf($b, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "$f contains '$b': not publishing it" }
        }
    }
}
Write-Host "  ok   no home folder or user-name path in the exes"

$dist = Join-Path $root "target\dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$asset = Join-Path $dist "$Title-Setup.exe"
Copy-Item -Force $setup $asset
$sha = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLower()
"$sha  $Title-Setup.exe" | Out-File -Encoding ascii (Join-Path $dist "$Title-Setup.exe.sha256")
Write-Host "  ok   $asset"
Write-Host "       sha256 $sha"

if ($Publish) {
    $tag = "v$version"
    gh release view $tag -R $repo *> $null
    if ($LASTEXITCODE -eq 0) { throw "release $tag already exists: bump the version first" }
    if (-not $Notes) { $Notes = "$Title $version for Windows. Install with $Title-Setup.exe (no admin rights needed); an installed $Title $version or newer updates itself from here." }
    gh release create $tag -R $repo --title "$Title $version" --notes $Notes --latest $asset (Join-Path $dist "$Title-Setup.exe.sha256")
    if ($LASTEXITCODE -ne 0) { throw "gh release create failed" }
    Write-Host "  ok   published https://github.com/$repo/releases/tag/$tag"
}
