param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Identity,
    [Parameter(Mandatory = $true)][string]$Publisher,
    [Parameter(Mandatory = $true)][ValidateSet("x86", "x64", "arm64")][string]$Architecture,
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = "Stop"
if ($Version -notmatch '^\d+\.\d+\.\d+(\.\d+)?$') {
    throw "MSIX version must contain three or four numeric components."
}
if ($Version.Split('.').Count -eq 3) {
    $Version = "$Version.0"
}
if ($Identity -notmatch '^[A-Za-z0-9.-]{3,50}$') {
    throw "MSIX_PACKAGE_IDENTITY must be a valid Partner Center package identity name."
}
if ($Publisher -notmatch '^CN=.+$') {
    throw "MSIX_PUBLISHER must match the Partner Center publisher, including its CN= prefix."
}

$packageDir = Join-Path $env:RUNNER_TEMP "winbloat-msix"
$assetsDir = Join-Path $packageDir "Assets"
New-Item -ItemType Directory -Force -Path $assetsDir | Out-Null
Copy-Item -LiteralPath $ExecutablePath -Destination (Join-Path $packageDir "winbloat-gui.exe")

Add-Type -AssemblyName System.Drawing
$icon = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path "assets\icon.ico"))
try {
    $source = $icon.ToBitmap()
    try {
        foreach ($size in @(44, 50, 150, 310)) {
            $bitmap = New-Object System.Drawing.Bitmap($size, $size)
            try {
                $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
                try {
                    $graphics.Clear([System.Drawing.Color]::Transparent)
                    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                    $graphics.DrawImage($source, 0, 0, $size, $size)
                    $bitmap.Save((Join-Path $assetsDir "Square${size}x${size}Logo.png"), [System.Drawing.Imaging.ImageFormat]::Png)
                }
                finally {
                    $graphics.Dispose()
                }
            }
            finally {
                $bitmap.Dispose()
            }
        }
        $wide = New-Object System.Drawing.Bitmap(310, 150)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($wide)
            try {
                $graphics.Clear([System.Drawing.Color]::Transparent)
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.DrawImage($source, 80, 0, 150, 150)
                $wide.Save((Join-Path $assetsDir "Wide310x150Logo.png"), [System.Drawing.Imaging.ImageFormat]::Png)
            }
            finally {
                $graphics.Dispose()
            }
        }
        finally {
            $wide.Dispose()
        }
    }
    finally {
        $source.Dispose()
    }
}
finally {
    $icon.Dispose()
}

$manifest = @'
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
         IgnorableNamespaces="uap rescap">
  <Identity Name="__IDENTITY__" Publisher="__PUBLISHER__" Version="__VERSION__" ProcessorArchitecture="__ARCHITECTURE__" />
  <Properties>
    <DisplayName>WinBloat</DisplayName>
    <PublisherDisplayName>TamKungZ_</PublisherDisplayName>
    <Description>Read-only disk usage scanner for Windows</Description>
    <Logo>Assets\Square50x50Logo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Applications>
    <Application Id="WinBloat" Executable="winbloat-gui.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="WinBloat" Description="Read-only disk usage scanner"
                          BackgroundColor="transparent" Square44x44Logo="Assets\Square44x44Logo.png"
                          Square150x150Logo="Assets\Square150x150Logo.png">
        <uap:DefaultTile Wide310x150Logo="Assets\Wide310x150Logo.png" Square310x310Logo="Assets\Square310x310Logo.png" />
      </uap:VisualElements>
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
'@
$manifest = $manifest.Replace("__IDENTITY__", [System.Security.SecurityElement]::Escape($Identity))
$manifest = $manifest.Replace("__PUBLISHER__", [System.Security.SecurityElement]::Escape($Publisher))
$manifest = $manifest.Replace("__VERSION__", $Version)
$manifest = $manifest.Replace("__ARCHITECTURE__", $Architecture)
Set-Content -LiteralPath (Join-Path $packageDir "AppxManifest.xml") -Value $manifest -Encoding utf8

$sdkRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
$makeAppx = Get-ChildItem -Path $sdkRoot -Filter makeappx.exe -Recurse |
    Where-Object { $_.Directory.Name -eq "x64" } |
    Sort-Object { $_.Directory.Parent.Name } -Descending |
    Select-Object -First 1
if (-not $makeAppx) {
    throw "Windows SDK makeappx.exe was not found on the runner."
}
$outputFullPath = [System.IO.Path]::GetFullPath($OutputPath)
& $makeAppx.FullName pack /d $packageDir /p $outputFullPath /o
if ($LASTEXITCODE -ne 0) {
    throw "makeappx failed with exit code $LASTEXITCODE."
}
