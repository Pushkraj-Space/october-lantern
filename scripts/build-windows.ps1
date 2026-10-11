# Build the Rust desktop alongside its Rust engine sidecar for Windows, into build\windows.
#   pwsh scripts/build-windows.ps1 [-Debug]
param([switch] $Debug)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') { throw 'This script builds the Windows app.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Install stable Rust from https://rustup.rs' }

$BuildProfile = if ($Debug) { 'debug' } else { 'release' }
$Flags = if ($Debug) { @('--locked') } else { @('--locked', '--release') }
# Shared target folder avoids rebuilding dependencies used by both Rust executables.
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $Root 'build\target' }
foreach ($crate in 'engine', 'desktop') {
    Push-Location (Join-Path $Root $crate)
    try {
        & cargo build @Flags
        if ($LASTEXITCODE -ne 0) { throw "building $crate failed" }
    }
    finally {
        Pop-Location
    }
}

$Dest = Join-Path $Root 'build\windows'
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$Built = Join-Path $env:CARGO_TARGET_DIR $BuildProfile
Copy-Item (Join-Path $Built 'lantern-engine.exe') (Join-Path $Dest 'lantern-engine.exe') -Force
Copy-Item (Join-Path $Built 'lantern-desktop.exe') (Join-Path $Dest 'october-lantern.exe') -Force
Copy-Item (Join-Path $Root 'logo.png') (Join-Path $Dest 'october-lantern.png') -Force
Copy-Item (Join-Path $Root 'LICENSE') (Join-Path $Dest 'LICENSE') -Force

# The release app has no console, so wait for it explicitly and read its answer from a file.
$App = Join-Path $Dest 'october-lantern.exe'
$Out = Join-Path $Dest 'check-sidecar.txt'
$Check = Start-Process -FilePath $App -ArgumentList '--check-sidecar' -Wait -PassThru -NoNewWindow -RedirectStandardOutput $Out
Get-Content $Out
Remove-Item $Out
if ($Check.ExitCode -ne 0) { throw 'The desktop app could not talk to the engine' }
Write-Host "Run: `"$App`""
