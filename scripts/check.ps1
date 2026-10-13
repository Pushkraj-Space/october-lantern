# The Windows gate: engine and desktop format, lints and tests. CI runs it on every push; on
# macOS and Linux, scripts/check.sh is the gate.
#   pwsh scripts/check.ps1
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
# One target folder for both crates, as on Linux.
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $Root 'build\target' }

function Invoke-Cargo([string[]] $Arguments) {
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') failed" }
}

# One version for every platform: the engine's, which the Mac app and the releases use too.
$versionOf = { param($crate) (Select-String -Path (Join-Path $Root "$crate\Cargo.toml") -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value }
$engineVersion = & $versionOf 'engine'
$desktopVersion = & $versionOf 'desktop'
if ($engineVersion -ne $desktopVersion) { throw "desktop/Cargo.toml is $desktopVersion but engine/Cargo.toml is $engineVersion; give both the same version" }

foreach ($crate in 'engine', 'desktop') {
    Write-Host "==> $crate"
    Push-Location (Join-Path $Root $crate)
    try {
        Invoke-Cargo @('fmt', '--all', '--', '--check')
        Invoke-Cargo @('clippy', '--all-targets', '--locked', '--', '-D', 'warnings')
        Invoke-Cargo @('test', '--locked')
    }
    finally {
        Pop-Location
    }
}
Write-Host '==> checks passed'
