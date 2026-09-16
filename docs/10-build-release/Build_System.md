# KAGE Build System Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-BLD-001 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | DevOps, Build Pipeline & Distribution Engineering |

---

## 1. Executive Summary & Build Topology

Building KAGE requires coordinating three disparate toolchains into a single, unified build pipeline:
1. **Frontend Toolchain:** Node.js, Vite, TypeScript, React, Tailwind CSS (authors the Browser Shell UI).
2. **Native Host Toolchain:** Rust, Cargo, Tauri CLI (`tauri-build`), C++17 compiler (MSVC on Windows, Clang on macOS/Linux).
3. **Chromium Embedding Subsystem:** Pre-compiled CEF binary distribution archives, `libcef_dll_wrapper` static library, and platform-specific helper subprocesses (`kage-cef-subprocess`).

```
╔═════════════════════════════════════════════════════════════════════════════════════╗
║                            PRIME ARCHITECTURAL INVARIANT                            ║
║                                                                                     ║
║        AI NEVER GETS BROWSER AUTHORITY DIRECTLY. EVERY AI ACTION BECOMES            ║
║        A TYPED TOOL BUS REQUEST, AND THE TOOL BUS—NOT THE MODEL, PROMPT,            ║
║        PLUGIN, OR UI—OWNS VALIDATION, AUTHORIZATION, EXECUTION,                     ║
║        CANCELLATION, AND AUDITING.                                                  ║
╚═════════════════════════════════════════════════════════════════════════════════════╝
```

---

## 2. Directory & Dependency Structure

```
kage/
├── src-ui/                      # React / TypeScript Browser Shell
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
├── src-tauri/                   # Tauri Rust Native Host
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs                 # Links CEF and manages asset copying
│   └── src/
├── src-subprocess/              # Lightweight Chromium Subprocess Executable
│   ├── CMakeLists.txt
│   └── kage_subprocess.cpp
├── third_party/
│   └── cef/                     # Downloaded CEF Binary Distribution
│       ├── include/             # CEF C/C++ Headers
│       ├── libcef_dll_wrapper/  # C++ Wrapper Source
│       └── Release/             # libcef.dll / icudtl.dat / pak files
└── scripts/
    ├── fetch-cef.py             # Downloads & validates pinned CEF release
    └── build.ps1 / build.sh     # Orchestration scripts
```

---

## 3. CEF Binary Acquisition & Build Integration

Because the Chromium Embedded Framework binaries are ~140 MB uncompressed, they are not committed to Git. Instead, they are retrieved via a deterministic bootstrap script:

### 3.1 Pinned Fetch Script (`scripts/fetch-cef.py`)
- Reads the pinned CEF milestone from `cef_version.json` (e.g. `128.0.6613.120`).
- Downloads the platform archive from Spotify's official CEF distribution CDN.
- Verifies the SHA-256 cryptographic checksum against the known release hash.
- Unpacks headers into `third_party/cef/include/` and binaries into `third_party/cef/Release/`.

### 3.2 Cargo Build Script (`src-tauri/build.rs`)
```rust
fn main() {
    // 1. Compile libcef_dll_wrapper via CMake
    let dst = cmake::Config::new("../third_party/cef")
        .define("CEF_RUNTIME_LIBRARY_FLAG", "/MD")
        .build();

    // 2. Link native libraries
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native=../third_party/cef/Release");
    println!("cargo:rustc-link-lib=static=libcef_dll_wrapper");
    println!("cargo:rustc-link-lib=dylib=libcef");

    // 3. Instruct Tauri to bundle CEF runtime assets into target directory
    tauri_build::build();
}
```

---

## 4. Subprocess Compilation Pipeline

On Windows and Linux, running renderer and GPU processes from a dedicated lightweight executable prevents redundant Tauri runtime initialization:

```cpp
// src-subprocess/kage_subprocess.cpp
#include "include/cef_app.h"

int main(int argc, char* argv[]) {
    CefMainArgs main_args(argc, argv);
    // Execute Chromium subprocess (Renderer, GPU, Utility)
    return CefExecuteProcess(main_args, nullptr, nullptr);
}
```

- **Compilation:** Compiled with `/O2` and statically linked C++ runtime to produce a minimal binary (`< 1.5 MB`).
- **macOS Exception:** On macOS, Apple sandboxing requires helper bundles (`KAGE Helper.app`, `KAGE Helper (Renderer).app`) signed with explicit sandbox entitlements.

---

## 5. Development vs. Production Configurations

| Configuration | Frontend (Vite) | Host (Rust) | CEF Debugging | Asset Loading |
|---|---|---|---|---|
| **Development** | HMR Dev Server (`localhost:5173`) | `cargo run` (Debug profile) | Remote debugging port active; DevTools enabled | In-memory web server via Vite HMR |
| **Production** | Static Production Bundle (`dist/`) | `cargo build --release` (LTO enabled, stripped symbols) | Remote debugging port loopback only with auth token | Embedded in Tauri bundle via custom protocol (`kage://`) |

---

## 6. Packaging & Platform Artifacts

KAGE builds native platform installers using Tauri's native packaging engine:

### 6.1 Windows (x86_64)
- **Installer Types:** NSIS standalone executable installer (`.exe`) and WiX MSI package (`.msi`).
- **Bundle Contents:** `kage.exe`, `kage-cef-subprocess.exe`, `libcef.dll`, `icudtl.dat`, `*.pak`, `v8_context_snapshot.bin`.
- **Code Signing:** Authenticode signed with EV Code Signing Certificate via `signtool.exe`.

### 6.2 macOS (Apple Silicon arm64 & Intel x86_64)
- **Installer Type:** Notarized Apple Disk Image (`.dmg`) and `.app` bundle.
- **Bundle Hierarchy:** `KAGE.app` containing `Contents/Frameworks/Chromium Embedded Framework.framework` and helper apps.
- **Notarization:** Signed with Apple Developer ID and notarized via `xcrun notarytool`.

### 6.3 Linux (x86_64)
- **Installer Types:** Debian package (`.deb`) and standalone `.AppImage`.
- **Dependencies:** Bundled CEF shared objects (`libcef.so`) linked against system GTK3/NSS libraries.

---

## 7. Continuous Integration (CI) Architecture

Because Chromium binaries exceed 100 MB, CI runners (GitHub Actions) utilize aggressive multi-layer caching:

```yaml
# CI Pipeline Workflow Summary
jobs:
  build:
    strategy:
      matrix:
        os: [windows-latest, macos-14, ubuntu-22.04]
    steps:
      - uses: actions/checkout@v4
      - name: Cache CEF Binary Archive
        uses: actions/cache@v4
        with:
          path: third_party/cef
          key: cef-${{ runner.os }}-${{ hashFiles('cef_version.json') }}
      - name: Fetch CEF (if cache miss)
        run: python scripts/fetch-cef.py
      - name: Build Frontend
        run: npm ci && npm run build
      - name: Build & Test Native Host
        run: cargo test --release
      - name: Package Tauri Release
        run: npm run tauri build
```
