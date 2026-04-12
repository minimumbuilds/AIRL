#!/usr/bin/env python3
"""Check that no stdlib/*.airl defn names shadow Rust builtins.

Reads builtin names from api-manifest.json, greps stdlib/*.airl for
(defn name ...) patterns, and reports any intersection as errors.
Exit 1 if any shadows found, 0 if clean.
"""
import json, re, sys
from pathlib import Path

root = Path(__file__).parent.parent
manifest_path = root / "api-manifest.json"
stdlib_path = root / "stdlib"

if not manifest_path.exists():
    print("ERROR: api-manifest.json not found", file=sys.stderr)
    sys.exit(1)

manifest = json.loads(manifest_path.read_text())
builtins = {b["name"] for b in manifest.get("builtins", [])}

shadows = []
for src in sorted(stdlib_path.glob("*.airl")):
    text = src.read_text()
    for m in re.finditer(r'^\(defn\s+([\w?!*/+\-<>=]+)', text, re.MULTILINE):
        name = m.group(1)
        if name in builtins:
            shadows.append(f"  {src.relative_to(root)}: (defn {name} ...) shadows Rust builtin")

if shadows:
    print("ERROR: stdlib defns shadow Rust builtins (dead code):")
    print("\n".join(shadows))
    print(f"\n{len(shadows)} shadow(s) found. Remove the defn bodies.")
    sys.exit(1)

stdlib_files = list(stdlib_path.glob("*.airl"))
print(f"lint-shadows: OK ({len(stdlib_files)} stdlib files, {len(builtins)} builtins, 0 shadows)")
