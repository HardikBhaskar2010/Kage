# KAGE Production Release Packaging Script (CEF-03b-D)
# Assembles the canonical 18-asset CEF 152 release redistribution bundle
param(
    [string]$OutputDir = "$PSScriptRoot\..\target\release\kage-bundle",
    [string]$CefDir = $env:KAGE_CEF_DIR,
    [string]$Validate = "true"
)

$ErrorActionPreference = "Stop"

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "            KAGE PRODUCTION RELEASE PACKAGING & CEF BUNDLE (CEF-03b-D)          " -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

# 1. Discover CEF distribution directory if not provided
$cefDirValid = $false
if (-not [string]::IsNullOrWhiteSpace($CefDir)) {
    if (Test-Path $CefDir) {
        $cefDirValid = $true
    }
}

if (-not $cefDirValid) {
    Write-Host "[*] Searching for CEF Windows distribution directory..." -ForegroundColor Yellow
    $candidateRoots = @(
        "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\debug\build",
        "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\release\build",
        "$PSScriptRoot\..\target\debug\build",
        "$PSScriptRoot\..\target\release\build"
    )

    foreach ($root in $candidateRoots) {
        if (Test-Path $root) {
            $matching = Get-ChildItem -Path $root -Directory -Filter "cef-dll-sys-*" | ForEach-Object {
                Join-Path $_.FullName "out\cef_windows_x86_64"
            } | Where-Object { Test-Path $_ }
            if ($matching) {
                $CefDir = ($matching | Select-Object -First 1).ToString()
                $cefDirValid = $true
                break
            }
        }
    }
}

if (-not $cefDirValid) {
    Write-Error "Could not locate CEF Windows binary distribution (cef_windows_x86_64). Please set KAGE_CEF_DIR."
    exit 1
}

Write-Host "[+] Discovered CEF Distribution: $CefDir" -ForegroundColor Green

# 2. Canonical redistribution manifest (15 assets + locales + bootstrap + client)
$canonicalAssets = @(
    "chrome_elf.dll",
    "d3dcompiler_47.dll",
    "dxcompiler.dll",
    "dxil.dll",
    "libcef.dll",
    "libEGL.dll",
    "libGLESv2.dll",
    "v8_context_snapshot.bin",
    "vk_swiftshader.dll",
    "vk_swiftshader_icd.json",
    "vulkan-1.dll",
    "chrome_100_percent.pak",
    "chrome_200_percent.pak",
    "resources.pak",
    "icudtl.dat"
)

# 3. Prepare output directory
Write-Host "[*] Assembling release bundle in: $OutputDir" -ForegroundColor Yellow
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

# 4. Copy canonical runtime assets
$copiedCount = 0
foreach ($asset in $canonicalAssets) {
    $srcPath = Join-Path $CefDir $asset
    $dstPath = Join-Path $OutputDir $asset
    if (Test-Path $srcPath) {
        Copy-Item -Path $srcPath -Destination $dstPath -Force
        $copiedCount++
    } else {
        Write-Warning "Missing canonical asset in CEF distribution: $asset"
    }
}
Write-Host "[+] Copied $copiedCount / $($canonicalAssets.Count) canonical CEF runtime assets" -ForegroundColor Green

# 5. Copy locales directory
$srcLocales = Join-Path $CefDir "locales"
$dstLocales = Join-Path $OutputDir "locales"
if (Test-Path $srcLocales) {
    if (-not (Test-Path $dstLocales)) {
        New-Item -ItemType Directory -Path $dstLocales -Force | Out-Null
    }
    Copy-Item -Path "$srcLocales\*.pak" -Destination $dstLocales -Force
    $localeCount = (Get-ChildItem -Path $dstLocales -Filter "*.pak").Count
    Write-Host "[+] Copied $localeCount locale PAK files to $dstLocales" -ForegroundColor Green
} else {
    Write-Warning "locales/ directory not found in CEF distribution: $srcLocales"
}

# 6. Copy and package bootstrap architecture binaries
# KAGE.exe is the packaged bootstrap.exe linking cef_sandbox.lib
$srcBootstrap = Join-Path $CefDir "bootstrap.exe"
if (Test-Path $srcBootstrap) {
    Copy-Item -Path $srcBootstrap -Destination (Join-Path $OutputDir "bootstrap.exe") -Force
    Copy-Item -Path $srcBootstrap -Destination (Join-Path $OutputDir "KAGE.exe") -Force
    Write-Host "[+] Packaged bootstrap.exe and KAGE.exe (links cef_sandbox.lib)" -ForegroundColor Green
} else {
    Write-Warning "bootstrap.exe missing in CEF distribution: $srcBootstrap"
}

# 7. Package KAGE client library (kage_client.dll, KAGE.dll, bootstrap.dll)
$targetRoots = @(
    "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\release",
    "$PSScriptRoot\..\target\release",
    "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\debug",
    "$PSScriptRoot\..\target\debug"
)

$clientDllFound = $false
foreach ($root in $targetRoots) {
    $clientDll = Join-Path $root "kage_client.dll"
    if (Test-Path $clientDll) {
        Copy-Item -Path $clientDll -Destination (Join-Path $OutputDir "kage_client.dll") -Force
        Copy-Item -Path $clientDll -Destination (Join-Path $OutputDir "KAGE.dll") -Force
        Copy-Item -Path $clientDll -Destination (Join-Path $OutputDir "bootstrap.dll") -Force
        Write-Host "[+] Packaged kage_client.dll (and KAGE.dll, bootstrap.dll) from $root" -ForegroundColor Green
        $clientDllFound = $true
        break
    }
}
if (-not $clientDllFound) {
    Write-Warning "kage_client.dll not found in target directories. Build crates/kage-client first."
}

# Purge legacy binaries that violate canonical packaging architecture
$legacyBinaries = @("kage-cef-subprocess.exe", "kage-host.exe")
foreach ($legacy in $legacyBinaries) {
    $legacyPath = Join-Path $OutputDir $legacy
    if (Test-Path $legacyPath) {
        Remove-Item -Path $legacyPath -Force
        Write-Host "[*] Removed legacy non-bootstrap binary: $legacy" -ForegroundColor Yellow
    }
}

# 8. Sign release bundle with production Authenticode signature
$signerScript = Join-Path $PSScriptRoot "sign_release_bundle.ps1"
if (Test-Path $signerScript) {
    Write-Host "[*] Executing Authenticode signing suite..." -ForegroundColor Yellow
    & $signerScript -BundleDir $OutputDir
}

# 9. Bundle Manifest & Verification Summary
$bundleFiles = Get-ChildItem -Path $OutputDir -File
$totalSizeMB = [math]::Round(($bundleFiles | Measure-Object -Property Length -Sum).Sum / 1MB, 2)
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "Bundle Assembly Complete: $($bundleFiles.Count) files ($totalSizeMB MB)" -ForegroundColor Green
Write-Host "Location: $OutputDir" -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Cyan

if ($Validate -eq "true" -or $Validate -eq "1" -or $Validate -eq $true) {
    Write-Host "[*] Executing SandboxPackagingValidator integration test..." -ForegroundColor Yellow
    & "$PSScriptRoot\run_cargo.ps1" test --test phase2d_sandbox_packaging '--' --nocapture
}
