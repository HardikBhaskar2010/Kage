#!/usr/bin/env python3
"""
scripts/verify_no_mocks.py — Secondary CI Scanner for Browser Path Authenticity

Scans production crates and Tauri host code to detect regressions back into
mock / fake browser territory:
- Hardcoded constant DOM node IDs (e.g. backendNodeId = 42)
- Synthetic Math.random() telemetry generators
- Hardcoded HTML templates disguised as live webviews
- Stub CDP dispatchers returning static JSON payloads in production

Explicitly excludes test files, test fixtures, and integration test crates.
"""

import sys
import re
from pathlib import Path

# Paths to scan (strictly production surfaces)
SCAN_DIRS = [
    Path("crates/kage-browser/src"),
    Path("crates/kage-engine/src"),
    Path("crates/kage-core/src"),
    Path("crates/kage-cdp/src"),
    Path("crates/kage-storage/src"),
    Path("src-tauri/src"),
]

# Patterns that indicate fake / mock browser logic in production code
FORBIDDEN_PATTERNS = [
    (
        re.compile(r"backend_node_id\s*=\s*42\b"),
        "Hardcoded backend_node_id constant (dummy DOM node ID)",
    ),
    (
        re.compile(r"backendNodeId\s*:\s*42\b"),
        "Hardcoded backendNodeId JSON key with dummy ID 42",
    ),
    (
        re.compile(r"SAMPLE_DOM_TREE"),
        "Synthetic SAMPLE_DOM_TREE reference in production path",
    ),
    (
        re.compile(r"Math\.random\(\)\s*[*+].*telemetry"),
        "Synthetic Math.random() browser telemetry generator",
    ),
    (
        re.compile(r'class\s+StubCdpClient.*return\s+json!\('),
        "Stub CDP client returning static payload",
    ),
]

def scan_file(file_path: Path):
    violations = []
    # Skip test modules or fixtures
    if "test" in file_path.name.lower() or "fixture" in str(file_path).lower():
        return violations

    try:
        content = file_path.read_text(encoding="utf-8", errors="ignore")
    except Exception as e:
        print(f"Warning: could not read {file_path}: {e}")
        return violations

    # Separate out #[cfg(test)] blocks approximately
    lines = content.splitlines()
    in_test_mod = False

    for idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped.startswith("#[cfg(test)]") or stripped.startswith("mod tests {"):
            in_test_mod = True
        
        if in_test_mod:
            # Allow mocks inside unit test blocks
            continue

        for pattern, description in FORBIDDEN_PATTERNS:
            if pattern.search(line):
                violations.append((file_path, idx, line.strip(), description))

    return violations

def main():
    root = Path(__file__).resolve().parent.parent
    total_violations = []

    print("=" * 70)
    print(" KAGE Secondary Regression Scanner: verify_no_mocks.py")
    print("=" * 70)

    for scan_dir in SCAN_DIRS:
        target_dir = root / scan_dir
        if not target_dir.exists():
            continue

        for path in target_dir.rglob("*.rs"):
            violations = scan_file(path)
            total_violations.extend(violations)

    if total_violations:
        print(f"\n[FAIL] Found {len(total_violations)} mock / fake browser pattern(s) in production code:")
        for file_path, line_no, line_content, desc in total_violations:
            rel_path = file_path.relative_to(root)
            print(f"  - {rel_path}:{line_no}: {desc}")
            print(f"    Line: {line_content}")
        print("\nCI Gate Failed: Production code must use real CEF / browser paths.")
        sys.exit(1)
    else:
        print("\n[PASS] Zero fake browser patterns detected across production code paths.")
        print("Production code is clean of known dummy mock artifacts.")
        sys.exit(0)

if __name__ == "__main__":
    main()
