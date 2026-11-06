#!/usr/bin/env python3
"""Check that regenerated verification files match what is committed.

Run after tools/gen_cases.py. The math library rounds sin and cos differently
across platforms, so numbers may differ in the last bits: they are compared
with a relative tolerance. Anything else must be identical.
"""
import json
import subprocess
import sys
from pathlib import Path

REL_TOL = 1e-13


def differences(old, new, path="$"):
    if isinstance(old, dict) and isinstance(new, dict):
        if old.keys() != new.keys():
            yield f"{path}: keys {sorted(old.keys() ^ new.keys())}"
            return
        for k in old:
            # The hash follows the bytes of model.json, whose numbers are compared.
            if k != "model_sha256":
                yield from differences(old[k], new[k], f"{path}.{k}")
    elif isinstance(old, list) and isinstance(new, list):
        if len(old) != len(new):
            yield f"{path}: length {len(old)} -> {len(new)}"
            return
        for k, (a, b) in enumerate(zip(old, new)):
            yield from differences(a, b, f"{path}[{k}]")
    elif type(old) in (int, float) and type(new) in (int, float):
        if abs(old - new) > REL_TOL * max(abs(old), abs(new)):
            yield f"{path}: {old!r} -> {new!r}"
    elif old != new:
        yield f"{path}: {old!r} -> {new!r}"


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, encoding="utf-8").stdout


def main():
    bad = 0
    untracked = git("ls-files", "--others", "--exclude-standard", "--", "verification").split()
    for name in untracked:
        print(f"{name}: generated but not committed")
        bad += 1
    changed = git("diff", "--name-only", "--", "verification").split()
    for name in changed:
        if not name.endswith(".json") or not Path(name).exists():
            print(f"{name}: changed")
            bad += 1
            continue
        old = json.loads(git("show", f"HEAD:{name}"))
        new = json.loads(Path(name).read_text(encoding="utf-8"))
        found = list(differences(old, new))
        for d in found[:5]:
            print(f"{name}: {d}")
        bad += len(found)
    print(f"{len(changed)} files differ in bytes, {bad} real differences")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
