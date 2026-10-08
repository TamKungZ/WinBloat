param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Repository,
    [Parameter(Mandatory = $true)][hashtable]$InstallerPaths,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = "Stop"
$arches = @(
    @{ Name = "x64"; Winget = "x64" },
    @{ Name = "x86"; Winget = "x86" },
    @{ Name = "arm64"; Winget = "arm64" }
)
$versionPath = Join-Path $OutputPath "t\TamKungZ\WinBloat\$Version"
New-Item -ItemType Directory -Force -Path $versionPath | Out-Null

@"
PackageIdentifier: TamKungZ.WinBloat
PackageVersion: $Version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.6.0
"@ | Set-Content -Encoding utf8 (Join-Path $versionPath "TamKungZ.WinBloat.yaml")

$installerEntries = foreach ($arch in $arches) {
    $installerPath = $InstallerPaths[$arch.Name]
    if (-not $installerPath -or -not (Test-Path -LiteralPath $installerPath)) {
        throw "Missing $($arch.Name) installer: $installerPath"
    }
    $sha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash
    $installerName = "winbloat-setup-$Version-$($arch.Name).exe"
    $downloadUrl = "https://github.com/$Repository/releases/download/v$Version/$installerName"
@"
  - Architecture: $($arch.Winget)
    InstallerType: inno
    Scope: machine
    InstallerUrl: $downloadUrl
    InstallerSha256: $sha256
    InstallerSwitches:
      Silent: /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-
      SilentWithProgress: /SILENT /SUPPRESSMSGBOXES /NORESTART /SP-
    UpgradeBehavior: install
    ProductCode: "{2F760392-199C-4F6D-8FE1-0B4744A029B4}"
"@
}
$installerEntries = $installerEntries -join "`n"
@"
PackageIdentifier: TamKungZ.WinBloat
PackageVersion: $Version
InstallerLocale: en-US
MinimumOSVersion: 10.0.19041.0
Installers:
$installerEntries
ManifestType: installer
ManifestVersion: 1.6.0
"@ | Set-Content -Encoding utf8 (Join-Path $versionPath "TamKungZ.WinBloat.installer.yaml")

@"
PackageIdentifier: TamKungZ.WinBloat
PackageVersion: $Version
PackageLocale: en-US
Publisher: TamKungZ_
PublisherUrl: https://github.com/TamKungZ
PublisherSupportUrl: https://github.com/$Repository/issues
Author: TamKungZ_
PackageName: WinBloat
Moniker: winbloat
License: MIT
LicenseUrl: https://github.com/$Repository/blob/v$Version/LICENSE
Copyright: Copyright (c) 2026 TamKungZ_
ShortDescription: Read-only disk usage scanner for Windows
Description: Browse directory sizes, find large files and folders, and view size totals by file type.
Tags:
  - disk-usage
  - analyzer
  - treemap
  - windows
ManifestType: defaultLocale
ManifestVersion: 1.6.0
"@ | Set-Content -Encoding utf8 (Join-Path $versionPath "TamKungZ.WinBloat.locale.en-US.yaml")

Write-Host "Winget manifests created at $versionPath"
