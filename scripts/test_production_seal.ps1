# KAGE Production Host Physical Seal Test (CEF-06C & CEF-03b-C)
param(
    [string]$HostPath = "$PSScriptRoot\..\target\x86_64-pc-windows-msvc\debug\kage-host.exe"
)

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "         KAGE PRODUCTION HOST PHYSICAL SEAL: CEF-06C & CEF-03b-C                " -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

if (-not (Test-Path $HostPath)) {
    Write-Error "Host binary not found at $HostPath"
    exit 1
}

$subprocessPath = Join-Path (Split-Path $HostPath) "kage-cef-subprocess.exe"
if (-not (Test-Path $subprocessPath)) {
    Write-Error "Subprocess binary not found at $subprocessPath"
    exit 1
}
Write-Host "[1/6] Binary Artifacts Verified:" -ForegroundColor Green
Write-Host "  -> Host: $HostPath"
Write-Host "  -> Subprocess: $subprocessPath"

# Win32 helper definitions
$win32Code = @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public class Win32Helper {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern bool OpenProcessToken(IntPtr ProcessHandle, uint DesiredAccess, out IntPtr TokenHandle);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern bool GetTokenInformation(IntPtr TokenHandle, int TokenInformationClass, IntPtr TokenInformation, uint TokenInformationLength, out uint ReturnLength);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern IntPtr GetSidSubAuthority(IntPtr pSid, uint nSubAuthority);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern IntPtr GetSidSubAuthorityCount(IntPtr pSid);

    [DllImport("user32.dll")]
    public static extern IntPtr GetParent(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ScreenToClient(IntPtr hWnd, ref POINT lpPoint);

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT {
        public int X;
        public int Y;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr hObject);

    public const uint WM_CLOSE = 0x0010;
    public const uint TOKEN_QUERY = 0x0008;
    public const int TokenIntegrityLevel = 25;
    public const int TokenIsAppContainer = 29;

    public static uint GetThreadId(IntPtr hWnd) {
        uint pid;
        return GetWindowThreadProcessId(hWnd, out pid);
    }

    public static RECT GetClientRectInParent(IntPtr hWnd, IntPtr hParent) {
        RECT r;
        GetWindowRect(hWnd, out r);
        POINT pt1 = new POINT { X = r.Left, Y = r.Top };
        POINT pt2 = new POINT { X = r.Right, Y = r.Bottom };
        ScreenToClient(hParent, ref pt1);
        ScreenToClient(hParent, ref pt2);
        return new RECT { Left = pt1.X, Top = pt1.Y, Right = pt2.X, Bottom = pt2.Y };
    }

    public static bool IsAppContainer(IntPtr processHandle) {
        IntPtr tokenHandle = IntPtr.Zero;
        if (!OpenProcessToken(processHandle, TOKEN_QUERY, out tokenHandle)) {
            return false;
        }
        try {
            int isAppContainer = 0;
            uint returnLength = 0;
            IntPtr buffer = Marshal.AllocHGlobal(4);
            try {
                if (GetTokenInformation(tokenHandle, TokenIsAppContainer, buffer, 4, out returnLength)) {
                    isAppContainer = Marshal.ReadInt32(buffer);
                }
            } finally {
                Marshal.FreeHGlobal(buffer);
            }
            return isAppContainer != 0;
        } finally {
            CloseHandle(tokenHandle);
        }
    }

    public static List<IntPtr> GetProcessWindows(uint processId) {
        List<IntPtr> list = new List<IntPtr>();
        EnumWindows((h, l) => {
            uint pid;
            GetWindowThreadProcessId(h, out pid);
            if (pid == processId) {
                list.Add(h);
            }
            return true;
        }, IntPtr.Zero);
        return list;
    }

    public static List<IntPtr> GetChildren(IntPtr parent) {
        List<IntPtr> list = new List<IntPtr>();
        EnumChildWindows(parent, (h, l) => {
            list.Add(h);
            return true;
        }, IntPtr.Zero);
        return list;
    }

    public static string GetClass(IntPtr hWnd) {
        StringBuilder sb = new StringBuilder(256);
        GetClassName(hWnd, sb, 256);
        return sb.ToString();
    }

    public static string GetTitle(IntPtr hWnd) {
        StringBuilder sb = new StringBuilder(256);
        GetWindowText(hWnd, sb, 256);
        return sb.ToString();
    }

    public static RECT GetRect(IntPtr hWnd) {
        RECT r;
        GetWindowRect(hWnd, out r);
        return r;
    }

    public static string GetIntegrityLevel(IntPtr processHandle) {
        IntPtr tokenHandle = IntPtr.Zero;
        if (!OpenProcessToken(processHandle, TOKEN_QUERY, out tokenHandle)) {
            return "Failed to open token";
        }
        try {
            uint length = 0;
            GetTokenInformation(tokenHandle, TokenIntegrityLevel, IntPtr.Zero, 0, out length);
            if (length == 0) return "Unknown";
            IntPtr buffer = Marshal.AllocHGlobal((int)length);
            try {
                if (GetTokenInformation(tokenHandle, TokenIntegrityLevel, buffer, length, out length)) {
                    IntPtr pSid = Marshal.ReadIntPtr(buffer);
                    IntPtr pCount = GetSidSubAuthorityCount(pSid);
                    int count = Marshal.ReadByte(pCount);
                    if (count > 0) {
                        IntPtr pSubAuthority = GetSidSubAuthority(pSid, (uint)(count - 1));
                        int integrity = Marshal.ReadInt32(pSubAuthority);
                        if (integrity >= 0x4000) return "System";
                        if (integrity >= 0x3000) return "High";
                        if (integrity >= 0x2000) return "Medium";
                        if (integrity >= 0x1000) return "Low";
                        return "Untrusted (0x" + integrity.ToString("X") + ")";
                    }
                }
                return "Unknown";
            } finally {
                Marshal.FreeHGlobal(buffer);
            }
        } finally {
            CloseHandle(tokenHandle);
        }
    }
}
"@

Add-Type -TypeDefinition $win32Code -Language CSharp

# Launch host process
Write-Host "`n[2/6] Launching packaged kage-host.exe..." -ForegroundColor Yellow
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $HostPath
$psi.WorkingDirectory = (Split-Path $HostPath)
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.EnvironmentVariables["RUST_LOG"] = "info"

$proc = [System.Diagnostics.Process]::Start($psi)
$hostPid = $proc.Id
Write-Host "  -> Host Process PID: $hostPid" -ForegroundColor Green

# Wait up to 10 seconds for window and child processes to initialize
Write-Host "`n[3/6] Awaiting window initialization and CEF child process boot..." -ForegroundColor Yellow
$mainWindowHandle = [IntPtr]::Zero
$timeout = [DateTime]::UtcNow.AddSeconds(10)

while ([DateTime]::UtcNow -lt $timeout) {
    Start-Sleep -Milliseconds 500
    $proc.Refresh()
    if ($proc.HasExited) {
        Write-Error "Host process exited prematurely!"
        $out = $proc.StandardOutput.ReadToEnd()
        $err = $proc.StandardError.ReadToEnd()
        Write-Host "STDOUT: $out"
        Write-Host "STDERR: $err"
        exit 1
    }
    $windows = [Win32Helper]::GetProcessWindows($hostPid)
    foreach ($wnd in $windows) {
        $title = [Win32Helper]::GetTitle($wnd)
        $rect = [Win32Helper]::GetRect($wnd)
        $w = $rect.Right - $rect.Left
        if ($w -gt 500 -or $title -like "*Kage*") {
            $mainWindowHandle = $wnd
            break
        }
    }
    if ($mainWindowHandle -ne [IntPtr]::Zero) {
        break
    }
}

if ($mainWindowHandle -eq [IntPtr]::Zero) {
    # Fallback to MainWindowHandle if custom search was narrow
    $mainWindowHandle = $proc.MainWindowHandle
}

$mainTitle = [Win32Helper]::GetTitle($mainWindowHandle)
$mainClass = [Win32Helper]::GetClass($mainWindowHandle)
$parentRect = [Win32Helper]::GetRect($mainWindowHandle)
$mainW = $parentRect.Right - $parentRect.Left
$mainH = $parentRect.Bottom - $parentRect.Top

Write-Host "  -> Main Window HWND: $mainWindowHandle" -ForegroundColor Green
Write-Host "  -> Title: '$mainTitle' | Class: '$mainClass'"
Write-Host "  -> Dimensions: Rect=[$($parentRect.Left), $($parentRect.Top), Right=$($parentRect.Right), Bottom=$($parentRect.Bottom)] (Width=$mainW, Height=$mainH)"

# Inspect child HWNDs
Write-Host "`n[4/6] Inspecting Native Win32 Child HWND Hierarchy (CEF-06C Composition)..." -ForegroundColor Yellow
Start-Sleep -Seconds 2 # Allow CEF to attach child HWND

$allProcessWindows = [Win32Helper]::GetProcessWindows($hostPid)
Write-Host "  -> Top-level host HWNDs discovered: $($allProcessWindows.Count)"
foreach ($pw in $allProcessWindows) {
    $pwClass = [Win32Helper]::GetClass($pw)
    $pwTitle = [Win32Helper]::GetTitle($pw)
    $pwRect = [Win32Helper]::GetRect($pw)
    $pwThread = [Win32Helper]::GetThreadId($pw)
    Write-Host "     HWND ${pw}: Thread=${pwThread} | Class='$pwClass' | Title='$pwTitle' | ScreenRect=[$($pwRect.Left), $($pwRect.Top), W=$($pwRect.Right - $pwRect.Left), H=$($pwRect.Bottom - $pwRect.Top)]"
}

$children = [Win32Helper]::GetChildren($mainWindowHandle)
Write-Host "  -> Discovered $($children.Count) child HWNDs attached to Main Tauri Window:"
$cefFound = $false
foreach ($child in $children) {
    $cls = [Win32Helper]::GetClass($child)
    $title = [Win32Helper]::GetTitle($child)
    $screenRect = [Win32Helper]::GetRect($child)
    $clientRect = [Win32Helper]::GetClientRectInParent($child, $mainWindowHandle)
    $threadId = [Win32Helper]::GetThreadId($child)
    $parentHwnd = [Win32Helper]::GetParent($child)

    $sw = $screenRect.Right - $screenRect.Left
    $sh = $screenRect.Bottom - $screenRect.Top
    $cw = $clientRect.Right - $clientRect.Left
    $ch = $clientRect.Bottom - $clientRect.Top

    Write-Host "`n     Child HWND: $child | Parent HWND: $parentHwnd | Thread ID: $threadId"
    Write-Host "       Class: '$cls' | Title: '$title'"
    Write-Host "       Screen Rect:        [X=$($screenRect.Left), Y=$($screenRect.Top), W=$sw, H=$sh]"
    Write-Host "       Parent Client Rect: [X=$($clientRect.Left), Y=$($clientRect.Top), W=$cw, H=$ch]"

    if ($cls -like "*Cef*" -or $cls -like "*Chrome*" -or $cls -like "*Intermediate D3D*") {
        $cefFound = $true
        Write-Host "       >>> VERIFIED CEF CHILD HWND DETECTED ($cls) <<<" -ForegroundColor Green
    }
}

# Inspect Subprocesses and Process Tree (INV-CEF-SUBPROCESS-001 & CEF-03b-C)
Write-Host "`n[5/6] Inspecting Process Tree & Security Tokens (INV-CEF-SUBPROCESS-001 & CEF-03b-C)..." -ForegroundColor Yellow
$subprocesses = Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $hostPid }
Write-Host "  -> Discovered $($subprocesses.Count) child subprocess(es) spawned by host PID ${hostPid}:"

$allApproved = $true
$cefSubprocessCount = 0
$webView2Count = 0

foreach ($sub in $subprocesses) {
    $procName = $sub.Name
    $cmdLine = $sub.CommandLine
    $pidNum = $sub.ProcessId
    Write-Host "`n     Subprocess PID: $pidNum | Executable: $procName"
    Write-Host "       Command Line: $cmdLine"

    # Token & Integrity Level Inspection (CEF-03b-C)
    try {
        $subProcHandle = [System.Diagnostics.Process]::GetProcessById($pidNum).Handle
        $tokenHandle = [IntPtr]::Zero
        if ([Win32Helper]::OpenProcessToken($subProcHandle, [Win32Helper]::TOKEN_QUERY, [ref]$tokenHandle)) {
            $integrity = [Win32Helper]::GetIntegrityLevel($subProcHandle)
            $isAppContainer = [Win32Helper]::IsAppContainer($subProcHandle)
            Write-Host "       Token Query: SUCCESS (Handle: $tokenHandle)" -ForegroundColor Green
            Write-Host "       Integrity Level: $integrity" -ForegroundColor Cyan
            Write-Host "       AppContainer:    $isAppContainer" -ForegroundColor Cyan
            [Win32Helper]::CloseHandle($tokenHandle) | Out-Null
        }
    } catch {
        Write-Host "       Token Query: $($_.Exception.Message)"
    }

    if ($procName -eq "kage-cef-subprocess.exe") {
        $cefSubprocessCount++
        Write-Host "       [PASS] INV-CEF-SUBPROCESS-001: Approved KAGE CEF auxiliary subprocess" -ForegroundColor Green
    } elseif ($procName -eq "msedgewebview2.exe") {
        $webView2Count++
        Write-Host "       [PASS] Approved Tauri WebView2 host shell presentation process" -ForegroundColor Green
    } else {
        $allApproved = $false
        Write-Host "       [FAILED] Unknown / unapproved child executable!" -ForegroundColor Red
    }
}

Write-Host "`n  Summary of Process Hierarchy:" -ForegroundColor Cyan
Write-Host "    - Host Process: 1 (kage-host.exe)"
Write-Host "    - WebView2 Chrome Shell Processes: $webView2Count (msedgewebview2.exe)"
Write-Host "    - CEF Auxiliary Subprocesses: $cefSubprocessCount (kage-cef-subprocess.exe)"

if (-not $allApproved) {
    Write-Error "INV-CEF-SUBPROCESS-001 VIOLATION: Disallowed subprocess spawned."
}
if ($cefSubprocessCount -eq 0) {
    Write-Error "CEF Subprocess VIOLATION: No CEF auxiliary subprocesses detected."
}

# Close Gracefully (CEF-10A / CEF-10B)
Write-Host "`n[6/6] Initiating Graceful Teardown (CEF-10A / CEF-10B)..." -ForegroundColor Yellow
[Win32Helper]::PostMessage($mainWindowHandle, [Win32Helper]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null

$cleanExit = $proc.WaitForExit(8000)
if ($cleanExit) {
    Write-Host "  -> Host process exited with code $($proc.ExitCode) cleanly within grace period." -ForegroundColor Green
} else {
    Write-Host "  -> Grace period exceeded; terminating host process..." -ForegroundColor Yellow
    $proc.Kill()
}

# Verify Zero Orphaned Processes
Start-Sleep -Seconds 1
$orphans = Get-Process | Where-Object { $_.ProcessName -like "*kage*" }
if ($orphans) {
    Write-Host "[WARNING] Orphaned processes detected:" -ForegroundColor Yellow
    $orphans | ForEach-Object { Write-Host "  - PID $($_.Id): $($_.ProcessName)" }
    $orphans | Stop-Process -Force
} else {
    Write-Host "  -> Zero orphaned KAGE / CEF processes remain. Clean exit verified." -ForegroundColor Green
}

Write-Host "`n================================================================================" -ForegroundColor Cyan
Write-Host "              PRODUCTION HOST PHYSICAL SEAL VALIDATION COMPLETE                 " -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan
