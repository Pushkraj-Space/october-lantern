# Bundle the native Windows Bus, built from the same pinned source as the Mac and Linux packages.
#   pwsh scripts/build-bus-windows.ps1      (Go 1.27+ required)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Commit = (Get-Content (Join-Path $Root 'engine\bus.json') -Raw | ConvertFrom-Json).commit
$Source = if ($env:OCTOBER_BUS_SRC) { $env:OCTOBER_BUS_SRC } else { Join-Path $Root 'build\october-bus-src' }
if (-not (Get-Command go -ErrorAction SilentlyContinue)) { throw 'Go 1.27+ is required to bundle October Bus.' }

function Invoke-Git([string[]] $Arguments) {
    & git @Arguments
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed" }
}
if (-not (Test-Path (Join-Path $Source '.git'))) { Invoke-Git @('clone', '--quiet', 'https://github.com/october-dev/october-bus.git', $Source) }
& git -C $Source cat-file -e "$Commit^{commit}" 2>$null
if ($LASTEXITCODE -ne 0) { Invoke-Git @('-C', $Source, 'fetch', '--quiet', 'origin') }

# Build from exactly the pinned commit, not whatever the checkout has.
$Work = Join-Path ([System.IO.Path]::GetTempPath()) "october-bus-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    Invoke-Git @('-C', $Source, 'worktree', 'add', '--detach', '--quiet', $Work, $Commit)
    Push-Location $Work
    try {
        $env:CGO_ENABLED = '0'
        $env:GOOS = 'windows'
        & go build -trimpath -ldflags '-s -w' -o october-bus.exe ./cmd/october-bus
        if ($LASTEXITCODE -ne 0) { throw 'building October Bus failed' }
    }
    finally {
        Pop-Location
    }
    $Dest = Join-Path $Root 'build\windows'
    New-Item -ItemType Directory -Force -Path $Dest | Out-Null
    Copy-Item (Join-Path $Work 'october-bus.exe') (Join-Path $Dest 'october-bus.exe') -Force
    Copy-Item (Join-Path $Work 'LICENSE') (Join-Path $Dest 'OCTOBER-BUS-LICENSE') -Force
    Set-Content -Path (Join-Path $Dest 'OCTOBER-BUS-COMMIT') -Value $Commit -Encoding ascii
    & (Join-Path $Dest 'october-bus.exe') --help | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'the October Bus build does not run' }
    Write-Host "Bundled native October Bus ($Commit)."
}
finally {
    & git -C $Source worktree remove --force $Work 2>$null
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
