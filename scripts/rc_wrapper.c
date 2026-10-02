#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

int main(int argc, char* argv[]) {
    if (argc <= 1) {
        return 0;
    }

    char input_file[MAX_PATH] = {0};
    char output_file[MAX_PATH] = {0};
    char extra_args[2048] = {0};

    for (int i = 1; i < argc; i++) {
        char* arg = argv[i];
        if (arg[0] == '/' || arg[0] == '-') {
            char* opt = arg + 1;
            if (_strnicmp(opt, "fo", 2) == 0) {
                if (opt[2] != '\0') {
                    strncpy(output_file, opt + 2, MAX_PATH - 1);
                } else if (i + 1 < argc) {
                    strncpy(output_file, argv[++i], MAX_PATH - 1);
                }
            } else if (_strnicmp(opt, "d", 1) == 0) {
                strcat(extra_args, " -D");
                if (opt[1] != '\0') {
                    strcat(extra_args, opt + 1);
                } else if (i + 1 < argc) {
                    strcat(extra_args, argv[++i]);
                }
            } else if (_strnicmp(opt, "i", 1) == 0) {
                strcat(extra_args, " -I");
                if (opt[1] != '\0') {
                    strcat(extra_args, opt + 1);
                } else if (i + 1 < argc) {
                    strcat(extra_args, argv[++i]);
                }
            } else if (_stricmp(opt, "nologo") == 0) {
                // ignore
            } else if (_stricmp(opt, "?") == 0 || _stricmp(opt, "help") == 0) {
                return 0;
            } else if (_stricmp(opt, "v") == 0) {
                strcat(extra_args, " -v");
            }
        } else {
            strncpy(input_file, arg, MAX_PATH - 1);
        }
    }

    if (input_file[0] == '\0') {
        return 0;
    }

    if (output_file[0] == '\0') {
        strncpy(output_file, input_file, MAX_PATH - 5);
        char* dot = strrchr(output_file, '.');
        if (dot) *dot = '\0';
        strcat(output_file, ".res");
    }

    const char* windres = "C:\\Users\\sneha\\AppData\\Local\\Microsoft\\WinGet\\Packages\\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\\mingw64\\bin\\windres.exe";

    char cmdline[4096];
    snprintf(cmdline, sizeof(cmdline), "\"%s\" -O res -i \"%s\" -o \"%s\" %s", windres, input_file, output_file, extra_args);

    STARTUPINFOA si;
    PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));

    if (!CreateProcessA(NULL, cmdline, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi)) {
        return 1;
    }

    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD exit_code = 0;
    GetExitCodeProcess(pi.hProcess, &exit_code);
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);

    return (int)exit_code;
}
