# Authenticode-signs Windows files when a code-signing certificate is provided, and does nothing
# otherwise. The certificate comes from the environment (in CI, repository secrets):
#   WINDOWS_CERTIFICATE           the .pfx file, base64-encoded
#   WINDOWS_CERTIFICATE_PASSWORD  its password
#   pwsh scripts/sign-windows.ps1 build\windows\october-lantern.exe ...
param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $Files)
$ErrorActionPreference = 'Stop'
if (-not $env:WINDOWS_CERTIFICATE) {
    Write-Host 'No code-signing certificate configured; files are left unsigned.'
    return
}
$SignTool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Directory.Parent.Name } -ErrorAction SilentlyContinue | Select-Object -Last 1
if (-not $SignTool) { throw 'signtool.exe not found; install the Windows SDK.' }
$Pfx = Join-Path ([System.IO.Path]::GetTempPath()) "lantern-$([guid]::NewGuid().ToString('N')).pfx"
try {
    [System.IO.File]::WriteAllBytes($Pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    foreach ($file in $Files) {
        & $SignTool.FullName sign /f $Pfx /p $env:WINDOWS_CERTIFICATE_PASSWORD /fd sha256 /tr http://timestamp.digicert.com /td sha256 /d 'October Lantern' $file
        if ($LASTEXITCODE -ne 0) { throw "signing $file failed" }
    }
}
finally {
    Remove-Item $Pfx -ErrorAction SilentlyContinue
}
