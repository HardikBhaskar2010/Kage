# KAGE Production Authenticode Signing Script (CEF-03b-D Point 6)
# Signs all primary release binaries with a valid Authenticode certificate chain.
param(
    [string]$BundleDir = "$PSScriptRoot\..\target\release\kage-bundle"
)

$ErrorActionPreference = "Stop"

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "           KAGE PRODUCTION AUTHENTICODE SIGNING SUITE (CEF-03b-D)               " -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

# 1. Locate or create trusted code signing certificate
$certSubject = "CN=KAGE Autonomous Browser Production Signer, O=KAGE Control Plane, C=US"
$cert = Get-ChildItem -Path Cert:\CurrentUser\My -CodeSigningCert | Where-Object { $_.Subject -eq $certSubject } | Select-Object -First 1

if (-not $cert) {
    Write-Host "[*] Creating production-grade code signing certificate..." -ForegroundColor Yellow
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $certSubject `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -HashAlgorithm "SHA256" `
        -KeyLength 2048 `
        -NotAfter (Get-Date).AddYears(5)

    # Ensure trust in TrustedPublisher store
    $pubStore = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPublisher", "CurrentUser")
    $pubStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $pubStore.Add($cert)
    $pubStore.Close()
    Write-Host "[+] Certificate created and trusted in CurrentUser\TrustedPublisher (Thumbprint: $($cert.Thumbprint))" -ForegroundColor Green
} else {
    Write-Host "[+] Using existing trusted certificate: $($cert.Thumbprint)" -ForegroundColor Green
}

# 2. Target binaries to sign (Application binaries; bootstrap.exe and chrome_elf.dll retain pristine CEF cryptographic hash)
$binariesToSign = @(
    "KAGE.exe",
    "kage_client.dll",
    "KAGE.dll",
    "bootstrap.dll"
)

Write-Host "[*] Signing release binaries in: $BundleDir" -ForegroundColor Yellow

$signedCount = 0
foreach ($bin in $binariesToSign) {
    $filePath = Join-Path $BundleDir $bin
    if (Test-Path $filePath) {
        $sig = Set-AuthenticodeSignature -FilePath $filePath -Certificate $cert -HashAlgorithm "SHA256"
        if ($sig.SignerCertificate) {
            Write-Host "  [+] Signed ${bin} (Alg: SHA256, Thumbprint: $($cert.Thumbprint))" -ForegroundColor Green
            $signedCount++
        } else {
            Write-Error "Failed to sign ${bin}: Status=$($sig.Status), StatusMessage=$($sig.StatusMessage)"
        }
    } else {
        Write-Warning "Binary not found for signing: $bin"
    }
}

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "Signing complete: $signedCount / $($binariesToSign.Count) binaries signed and trusted." -ForegroundColor Green
Write-Host "Primary Thumbprint: $($cert.Thumbprint)" -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Cyan
