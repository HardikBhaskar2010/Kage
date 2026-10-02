# KAGE Target & Cargo Cache Cleanup Utility
# Safely purges obsolete build targets, stale incremental caches, duplicate
# compiled crate artifacts in deps/, old CEF wrapper copies, and stale debug symbols.

param(
    [switch]$Aggressive = $false,
    [double]$ThresholdGB = 0
)

$ErrorActionPreference = "SilentlyContinue"

$targetDir = Join-Path $PSScriptRoot "..\target"
if (-not (Test-Path $targetDir)) {
    Write-Host "Target directory not found: $targetDir"
    exit 0
}

$beforeBytes = (Get-ChildItem -Path $targetDir -Recurse -File | Measure-Object -Property Length -Sum).Sum
$beforeGB = [math]::Round($beforeBytes / 1GB, 2)

if ($ThresholdGB -gt 0 -and $beforeGB -lt $ThresholdGB) {
    Write-Host "[*] Target directory ($beforeGB GB) is under threshold ($ThresholdGB GB). Cleanup skipped." -ForegroundColor Gray
    exit 0
}

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "               KAGE BUILD CACHE & ARTIFACT CLEANUP UTILITY                      " -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "[*] Current target folder size: $beforeGB GB" -ForegroundColor Yellow

# 1. Remove obsolete target architectures (e.g. windows-gnu legacy artifacts)
# NOTE: Never delete target\release if it contains the packaged kage-bundle!
$obsoleteTargets = @(
    (Join-Path $targetDir "x86_64-pc-windows-gnu"),
    (Join-Path $targetDir "debug")
)
foreach ($dir in $obsoleteTargets) {
    if (Test-Path $dir) {
        Write-Host "[-] Removing obsolete root target: $(Split-Path $dir -Leaf)..." -ForegroundColor Yellow
        Remove-Item -Recurse -Force $dir
    }
}

# 2. Prune stale duplicate hashed artifacts in deps/ (keeps only the newest for each crate)
$depsDirs = @(
    (Join-Path $targetDir "x86_64-pc-windows-msvc\debug\deps"),
    (Join-Path $targetDir "x86_64-pc-windows-msvc\release\deps")
)
$regex = '^(.+)-[0-9a-f]{16}\.(.+)$'
foreach ($dDir in $depsDirs) {
    if (Test-Path $dDir) {
        Write-Host "[*] Deduplicating stale hashed compiler artifacts in $(Split-Path (Split-Path $dDir -Parent) -Leaf)\deps..." -ForegroundColor Yellow
        $files = Get-ChildItem -Path $dDir -File
        $groups = $files | Where-Object { $_.Name -match $regex } | Group-Object {
            if ($_.Name -match $regex) { "$($Matches[1]).$($Matches[2])" }
        }
        $deletedCount = 0
        $freedBytes = 0
        foreach ($g in $groups) {
            if ($g.Count -gt 1) {
                # Keep newest 1, delete older versions
                $sorted = $g.Group | Sort-Object LastWriteTime -Descending
                $toRemove = $sorted | Select-Object -Skip 1
                foreach ($f in $toRemove) {
                    $freedBytes += $f.Length
                    Remove-Item -Path $f.FullName -Force
                    $deletedCount++
                }
            }
        }
        $freedGB = [math]::Round($freedBytes / 1GB, 2)
        Write-Host "[+] Removed $deletedCount stale artifacts, reclaimed $freedGB GB from $(Split-Path (Split-Path $dDir -Parent) -Leaf)\deps" -ForegroundColor Green
    }
}

# 3. Purge incremental compilation caches (reconstructed automatically on next compile)
$incrementalDirs = Get-ChildItem -Path $targetDir -Recurse -Directory -Filter "incremental"
foreach ($inc in $incrementalDirs) {
    Write-Host "[-] Purging incremental compilation cache: $($inc.FullName)..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $inc.FullName
}

# 4. Clean up stale cef-dll-sys builds (keep only the newest 1 debug and 1 release)
$buildRoots = @(
    (Join-Path $targetDir "x86_64-pc-windows-msvc\debug\build"),
    (Join-Path $targetDir "x86_64-pc-windows-msvc\release\build")
)
foreach ($bRoot in $buildRoots) {
    if (Test-Path $bRoot) {
        $cefDirs = Get-ChildItem -Path $bRoot -Directory -Filter "cef-dll-sys-*" | Sort-Object LastWriteTime -Descending
        if ($cefDirs.Count -gt 1) {
            # Skip the newest, remove the rest
            $toDelete = $cefDirs | Select-Object -Skip 1
            foreach ($del in $toDelete) {
                Write-Host "[-] Removing stale CEF distribution copy: $($del.Name)..." -ForegroundColor Yellow
                Remove-Item -Recurse -Force $del.FullName
            }
        }
    }
}

# 5. In Aggressive mode, remove all .pdb debug symbols and old test runner executables
if ($Aggressive) {
    Write-Host "[*] Aggressive mode: Purging intermediate .pdb and test .exe files..." -ForegroundColor Yellow
    Get-ChildItem -Path $targetDir -Recurse -Filter "*.pdb" -File | Where-Object {
        $_.FullName -notmatch "kage-bundle"
    } | Remove-Item -Force
    Get-ChildItem -Path $targetDir -Recurse -Filter "*test*.exe" -File | Where-Object {
        $_.FullName -notmatch "kage-bundle"
    } | Remove-Item -Force
}

$afterBytes = (Get-ChildItem -Path $targetDir -Recurse -File | Measure-Object -Property Length -Sum).Sum
$afterGB = [math]::Round($afterBytes / 1GB, 2)
$reclaimedGB = [math]::Round(($beforeBytes - $afterBytes) / 1GB, 2)

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "[+] Target size after cleanup: $afterGB GB" -ForegroundColor Green
Write-Host "[+] Reclaimed disk space: $reclaimedGB GB" -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Cyan

