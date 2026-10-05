param([string]$Destination = 'target/odin-toolchain')
$ErrorActionPreference = 'Stop'
if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') { throw 'Unsupported Windows Odin architecture.' }
$archive = 'odin-windows-amd64-dev-2026-09.zip'
$expected = (Get-Content (Join-Path $PSScriptRoot 'odin-releases.sha256') | Where-Object { $_.EndsWith("  $archive") }).Split(' ')[0]
if (-not $expected) { throw 'Missing Odin release checksum.' }
New-Item -ItemType Directory -Force $Destination | Out-Null
$Destination = (Resolve-Path $Destination).Path
$archivePath = Join-Path $Destination $archive
if (-not (Test-Path $archivePath)) {
    Invoke-WebRequest "https://github.com/odin-lang/Odin/releases/download/dev-2026-09/$archive" -OutFile "$archivePath.download"
    Move-Item "$archivePath.download" $archivePath
}
if ((Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Odin release checksum mismatch.' }
$extraction = Join-Path $Destination 'dev-2026-09'
Expand-Archive -Path $archivePath -DestinationPath $extraction -Force
$compilers = @(Get-ChildItem $extraction -Recurse -File -Filter odin.exe)
if ($compilers.Count -ne 1) { throw 'Expected exactly one Odin executable.' }
$compilers[0].FullName
