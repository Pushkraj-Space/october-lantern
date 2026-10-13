# Authenticode-signs Windows files, and does nothing when no signing is configured. Two ways, from
# the environment (in CI, repository secrets and variables):
#
# Azure Artifact Signing (formerly Trusted Signing), used when AZURE_SIGNING_ACCOUNT is set:
#   AZURE_SIGNING_ENDPOINT   the account's region endpoint, e.g. https://eus.codesigning.azure.net/
#   AZURE_SIGNING_ACCOUNT    the Artifact Signing account name
#   AZURE_SIGNING_PROFILE    the certificate profile name
#   AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET
#                            an app registration with the "Artifact Signing Certificate Profile
#                            Signer" role on the account (read by Azure's EnvironmentCredential)
#
# A .pfx certificate:
#   WINDOWS_CERTIFICATE           the .pfx file, base64-encoded
#   WINDOWS_CERTIFICATE_PASSWORD  its password
#
#   pwsh scripts/sign-windows.ps1 build\windows\october-lantern.exe ...
param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $Files)
$ErrorActionPreference = 'Stop'

# Microsoft's signtool plug-in for Artifact Signing, pinned by version and SHA-256.
$ClientVersion = '1.0.128'
$ClientSha256 = '74bd7d27e6ce1051409c38d9b46bc8df0400ecd643d51ffbf2ac00869061e40b'

if (-not $env:AZURE_SIGNING_ACCOUNT -and -not $env:WINDOWS_CERTIFICATE) {
    Write-Host 'No code signing configured; files are left unsigned.'
    return
}
$SignTool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Directory.Parent.Name } -ErrorAction SilentlyContinue | Select-Object -Last 1
if (-not $SignTool) { throw 'signtool.exe not found; install the Windows SDK.' }

function Assert-Signed([string] $File) {
    $signature = Get-AuthenticodeSignature $File
    if ($signature.Status -ne 'Valid') { throw "$File isn't validly signed: $($signature.Status) $($signature.StatusMessage)" }
}

if ($env:AZURE_SIGNING_ACCOUNT) {
    foreach ($name in 'AZURE_SIGNING_ENDPOINT', 'AZURE_SIGNING_PROFILE', 'AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET') {
        if (-not (Get-Item "env:$name" -ErrorAction SilentlyContinue).Value) { throw "Azure Artifact Signing needs $name." }
    }
    $Work = Join-Path ([System.IO.Path]::GetTempPath()) "lantern-signing-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $Work | Out-Null
    try {
        $Package = Join-Path $Work 'client.zip'
        $url = "https://api.nuget.org/v3-flatcontainer/microsoft.artifactsigning.client/$ClientVersion/microsoft.artifactsigning.client.$ClientVersion.nupkg"
        Invoke-WebRequest -Uri $url -OutFile $Package -UseBasicParsing
        $hash = (Get-FileHash $Package -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $ClientSha256) { throw "The Artifact Signing client download doesn't match its pinned SHA-256 ($hash)." }
        Expand-Archive -Path $Package -DestinationPath (Join-Path $Work 'client')
        $Dlib = Join-Path $Work 'client\bin\x64\Azure.CodeSigning.Dlib.dll'
        if (-not (Test-Path $Dlib)) { throw 'Azure.CodeSigning.Dlib.dll is missing from the Artifact Signing client.' }
        $Metadata = Join-Path $Work 'metadata.json'
        @{
            Endpoint               = $env:AZURE_SIGNING_ENDPOINT
            CodeSigningAccountName = $env:AZURE_SIGNING_ACCOUNT
            CertificateProfileName = $env:AZURE_SIGNING_PROFILE
        } | ConvertTo-Json | Set-Content -Path $Metadata -Encoding utf8
        foreach ($file in $Files) {
            & $SignTool.FullName sign /v /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256 /d 'October Lantern' /dlib $Dlib /dmdf $Metadata $file
            if ($LASTEXITCODE -ne 0) { throw "signing $file failed" }
            Assert-Signed $file
        }
    }
    finally {
        Remove-Item $Work -Recurse -Force -ErrorAction SilentlyContinue
    }
    return
}

$Pfx = Join-Path ([System.IO.Path]::GetTempPath()) "lantern-$([guid]::NewGuid().ToString('N')).pfx"
try {
    [System.IO.File]::WriteAllBytes($Pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    foreach ($file in $Files) {
        & $SignTool.FullName sign /f $Pfx /p $env:WINDOWS_CERTIFICATE_PASSWORD /fd sha256 /tr http://timestamp.digicert.com /td sha256 /d 'October Lantern' $file
        if ($LASTEXITCODE -ne 0) { throw "signing $file failed" }
        Assert-Signed $file
    }
}
finally {
    Remove-Item $Pfx -ErrorAction SilentlyContinue
}
