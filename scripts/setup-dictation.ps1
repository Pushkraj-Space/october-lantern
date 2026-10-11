# Install the local CPU speech runtime and multilingual base model for Lantern on Windows, with no
# administrator rights. Needs git and the Visual Studio C++ build tools (which Rust's MSVC
# toolchain already uses); CMake comes from those tools, PATH, or uv.
#   pwsh scripts/setup-dictation.ps1
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Commit = 'd1be6fde11ac6e0407606b4e42fe72d34add8037'
$Sum = '60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe'
$Source = Join-Path $Root 'build\whisper-src'
$Build = Join-Path $Root 'build\whisper-build'
$Speech = Join-Path $env:LOCALAPPDATA 'October Lantern\speech'
$Config = Join-Path $env:APPDATA 'October Lantern\settings.json'

function Invoke-Checked([string] $Program, [string[]] $Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program $($Arguments -join ' ') failed" }
}

$CMake = (Get-Command cmake -ErrorAction SilentlyContinue).Source
if (-not $CMake) {
    # Visual Studio's own copy.
    $VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $VsWhere) {
        $Vs = & $VsWhere -latest -products * -property installationPath
        $Candidate = Join-Path "$Vs" 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
        if ($Vs -and (Test-Path $Candidate)) { $CMake = $Candidate }
    }
}
if (-not $CMake) {
    if (-not (Get-Command uv -ErrorAction SilentlyContinue)) { throw 'Install CMake (or the Visual Studio C++ build tools, or uv for a local CMake).' }
    $Tools = Join-Path $Root 'build\speech-tools'
    Invoke-Checked uv @('pip', 'install', '--target', $Tools, 'cmake==4.4.4')
    $CMake = Join-Path $Tools 'cmake\data\bin\cmake.exe'
}

if (-not (Test-Path (Join-Path $Source '.git'))) {
    Invoke-Checked git @('clone', '--quiet', '--depth', '1', '--branch', 'v1.9.5', 'https://github.com/ggml-org/whisper.cpp.git', $Source)
}
if ((& git -C $Source rev-parse HEAD) -ne $Commit) { throw 'Unexpected whisper.cpp source revision.' }
Invoke-Checked $CMake @('-S', $Source, '-B', $Build, '-DBUILD_SHARED_LIBS=OFF', '-DWHISPER_BUILD_TESTS=OFF', '-DWHISPER_CURL=OFF', '-DGGML_NATIVE=OFF')
Invoke-Checked $CMake @('--build', $Build, '--config', 'Release', '--target', 'whisper-cli', '--parallel')
$Built = @((Join-Path $Build 'bin\Release\whisper-cli.exe'), (Join-Path $Build 'bin\whisper-cli.exe')) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $Built) { throw 'whisper-cli.exe was not built.' }

New-Item -ItemType Directory -Force -Path $Speech | Out-Null
$Cli = Join-Path $Speech 'whisper-cli.exe'
Copy-Item $Built "$Cli.new" -Force
Move-Item "$Cli.new" $Cli -Force
$Model = Join-Path $Speech 'ggml-base.bin'
if (-not (Test-Path $Model) -or (Get-FileHash -Algorithm SHA256 $Model).Hash.ToLower() -ne $Sum) {
    $Part = Join-Path $Speech ".model-$([guid]::NewGuid().ToString('N'))"
    try {
        Invoke-WebRequest -Uri 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin' -OutFile $Part -UseBasicParsing
        if ((Get-FileHash -Algorithm SHA256 $Part).Hash.ToLower() -ne $Sum) { throw 'The speech model download did not match its checksum.' }
        Move-Item $Part $Model -Force
    }
    finally {
        Remove-Item $Part -ErrorAction SilentlyContinue
    }
}
Copy-Item (Join-Path $Source 'LICENSE') (Join-Path $Speech 'WHISPER-LICENSE') -Force

# Point Lantern's preferences at the runtime, keeping everything else in them.
New-Item -ItemType Directory -Force -Path (Split-Path $Config) | Out-Null
$Settings = if (Test-Path $Config) { Get-Content $Config -Raw | ConvertFrom-Json } else { [pscustomobject]@{} }
foreach ($pair in @(@('whisper', $Cli), @('whisper_model', $Model), @('speech_language', 'auto'))) {
    $Settings | Add-Member -NotePropertyName $pair[0] -NotePropertyValue $pair[1] -Force
}
$Temporary = "$Config.dictation-$([guid]::NewGuid().ToString('N'))"
# No byte-order mark: Lantern reads plain UTF-8 JSON.
[System.IO.File]::WriteAllText($Temporary, ($Settings | ConvertTo-Json -Depth 10))
Move-Item $Temporary $Config -Force

Invoke-Checked $Cli @('-m', $Model, '-f', (Join-Path $Source 'samples\jfk.wav'), '-l', 'en', '-nt', '-np')
Write-Host 'Local dictation installed. Reopen Lantern to load the speech settings.'
