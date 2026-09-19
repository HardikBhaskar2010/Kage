$llvm_bin = "C:\Users\sneha\.llvm\SourceDir\LLVM\bin"
$env:PATH = "$llvm_bin;" + $env:PATH
Write-Host "link.exe is at: $((Get-Command link.exe -ErrorAction SilentlyContinue).Path)"
Write-Host "cl.exe is at: $((Get-Command cl.exe -ErrorAction SilentlyContinue).Path)"
Write-Host "ninja.exe is at: $((Get-Command ninja.exe -ErrorAction SilentlyContinue).Path)"
