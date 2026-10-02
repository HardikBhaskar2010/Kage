$mingw_bin = "C:\Users\sneha\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin"
$llvm_bin = "C:\Users\sneha\.llvm\SourceDir\LLVM\bin"

$env:PATH = "$llvm_bin;$mingw_bin;" + $env:PATH
$env:INCLUDE = "C:\Users\sneha\Videos\Kage\.xwin\crt\include;C:\Users\sneha\Videos\Kage\.xwin\sdk\include\ucrt;C:\Users\sneha\Videos\Kage\.xwin\sdk\include\um;C:\Users\sneha\Videos\Kage\.xwin\sdk\include\shared"
$env:LIB = "C:\Users\sneha\Videos\Kage\.xwin\crt\lib\x64;C:\Users\sneha\Videos\Kage\.xwin\sdk\lib\um\x64;C:\Users\sneha\Videos\Kage\.xwin\sdk\lib\ucrt\x64"

# Ensure host build scripts compile with GCC for windows-gnu host
$env:HOST_CC = "$mingw_bin\gcc.exe"
$env:HOST_CXX = "$mingw_bin\g++.exe"
$env:CC_x86_64_pc_windows_gnu = "$mingw_bin\gcc.exe"
$env:CXX_x86_64_pc_windows_gnu = "$mingw_bin\g++.exe"
$env:HOST_CFLAGS = ""
$env:HOST_CXXFLAGS = ""
$env:CFLAGS = ""
$env:CXXFLAGS = ""

# Target (MSVC) build uses clang-cl wrapper cl.exe
$env:CC_x86_64_pc_windows_msvc = "$llvm_bin\cl.exe"
$env:CXX_x86_64_pc_windows_msvc = "$llvm_bin\cl.exe"
$env:CFLAGS_x86_64_pc_windows_msvc = "--target=x86_64-pc-windows-msvc"
$env:CXXFLAGS_x86_64_pc_windows_msvc = "--target=x86_64-pc-windows-msvc"

# Compiler and Linker for CMake
$env:CC = "$llvm_bin\cl.exe"
$env:CXX = "$llvm_bin\cl.exe"
$env:CMAKE_LINKER = "$llvm_bin\lld-link.exe"
$env:RC = "$llvm_bin\rc.exe"
$env:MT = "$llvm_bin\mt.exe"

& cargo @args
$cargoExitCode = $LASTEXITCODE

# Automatic cache governance: prevent target folder from ballooning beyond 20 GB
if (Test-Path "$PSScriptRoot\clean_cache.ps1") {
    & "$PSScriptRoot\clean_cache.ps1" -ThresholdGB 20
}

exit $cargoExitCode
