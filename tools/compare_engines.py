#!/usr/bin/env python3
"""Compares Struktal with external engines over the verification cases.

Optional and outside the engine: it needs each engine in its own Python, and CI
does not run it. An external engine is a second opinion, not the reference: when
they disagree, the closed form in the case decides (decision 0013).

    python tools/compare_engines.py --struktal target/debug/struktal \
        --engine opensees=PATH/python --engine pynite=PATH/python [V-DIA-001 ...]

Exit code 1 if any comparable result differs.
"""
import argparse
import json
import math
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUNNERS = {"opensees": "opensees_runner.py", "pynite": "pynite_runner.py"}
COMPONENTS = ["UX", "UY", "UZ", "RX", "RY", "RZ"]
REACTIONS = ["FX", "FY", "FZ", "MX", "MY", "MZ"]
RTOL = 1e-6        # relative to the larger of the two values
FLOOR = 1e-9       # relative to the largest value of that kind in the case


def struktal_results(binary, model):
    out = subprocess.run([str(Path(binary).resolve()), "solve", str(model)], capture_output=True, text=True)
    if not out.stdout.strip():
        return {"error": out.stderr.strip()}
    run = json.loads(out.stdout)
    return {c["name"]: c for c in run["cases"]}


def kinds(values, extent):
    """Noise floors [translations or forces, rotations or moments]. A zero moment
    is noise relative to force x size, and a zero rotation to translation / size."""
    low = max((abs(v[k]) for v in values for k in range(3)), default=0.0)
    high = max((abs(v[k]) for v in values for k in range(3, 6)), default=0.0)
    return low, high, extent


def case_rtol(case_dir):
    """Discretized results differ between formulations by more than rounding."""
    tol = json.loads((case_dir / "case.json").read_text(encoding="utf-8"))["tolerances_id"]
    return {"mesh-convergence-v1": 0.03, "drilling-penalty-v1": 0.12, "linear-closed-form-area-v1": 1e-5}.get(tol, RTOL)


def compare_case(st, ref, joints_model, rtol=RTOL):
    """Returns (checks, worst relative error, what, list of differences)."""
    points = [(j["x"], j["y"], j["z"]) for j in joints_model.values()]
    extent = max((max(c) - min(c) for c in zip(*points)), default=1.0) or 1.0
    ld, hd, _ = kinds([j["displacement"] for j in st["joints"]], extent)
    lr, hr, _ = kinds([j["reaction"] for j in st["joints"] if j["reaction"]], extent)
    # Each kind sets the other's scale: under moments alone, forces are only noise.
    sd = [max(ld, hd * extent), max(hd, ld / extent)]
    sr = [max(lr, hr / extent), max(hr, lr * extent)]
    checks, worst, what, diffs = 0, 0.0, "", []

    def check(a, b, floor, label):
        nonlocal checks, worst, what
        checks += 1
        if not (math.isfinite(a) and math.isfinite(b)):
            diffs.append(f"{label}: non-finite, struktal {a}, engine {b}")
            return
        allowed = rtol * max(abs(a), abs(b)) + max(FLOOR, rtol / 1000) * floor
        err = abs(a - b)
        ratio = err / allowed if allowed > 0 else (0.0 if err == 0 else float("inf"))
        if ratio > worst:
            worst, what = ratio, label
        if ratio > 1.0:
            diffs.append(f"{label}: struktal {a:.8e}, engine {b:.8e}")

    for j in st["joints"]:
        name = j["joint"]
        if j.get("generated"):
            continue  # only Struktal's analysis mesh has these points
        r = ref["joints"].get(name)
        if r is None:
            diffs.append(f"joint {name} missing in the engine")
            continue
        for d in range(6):
            check(j["displacement"][d], r["disp"][d], sd[0 if d < 3 else 1], f"{name} {COMPONENTS[d]}")
        info = joints_model[name]
        for d in range(6):
            if j["reaction"] and (info.get("restraint", [False] * 6)[d] or (info.get("springs") or [0.0] * 6)[d] > 0):
                check(j["reaction"][d], r["reaction"][d], sr[0 if d < 3 else 1], f"{name} {REACTIONS[d]}")
    return checks, worst, what, diffs


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--struktal", required=True)
    ap.add_argument("--engine", action="append", required=True, help="name=python, with name in " + ", ".join(RUNNERS))
    ap.add_argument("cases", nargs="*", help="case ids, or paths to model.json files of any model")
    args = ap.parse_args()
    engines = dict(e.split("=", 1) for e in args.engine)

    targets = []  # (label, model, rtol, discretized)
    for c in args.cases or sorted(p.name for p in (ROOT / "verification" / "cases").iterdir()):
        if c.endswith(".json"):
            # A model with no closed form: shells always differ by formulation.
            spec = json.loads(Path(c).read_text(encoding="utf-8"))
            targets.append((Path(c).stem, Path(c), RTOL, bool(spec.get("areas"))))
        else:
            d = ROOT / "verification" / "cases" / c
            targets.append((c, d / "model.json", case_rtol(d), case_rtol(d) > 1e-3))
    any_diff = False
    totals = {}
    with tempfile.TemporaryDirectory() as tmp:
        for label, model, rtol, discretized in targets:
            spec = json.loads(model.read_text(encoding="utf-8"))
            if any(s.get("shape") for s in spec["sections"]):
                for engine in engines:
                    print(f"{label:<15} {engine:<9} {'':<8} n/a       parametric sections are not translated")
                    totals[(engine, "n/a")] = totals.get((engine, "n/a"), 0) + 1
                continue
            st = struktal_results(args.struktal, model)
            joints_model = {j["name"]: j for j in spec["joints"]}
            for engine, python in engines.items():
                out = Path(tmp) / f"{label}-{engine}.json"
                run = subprocess.run([python, str(ROOT / "tools" / "oracles" / RUNNERS[engine]), str(model), str(out)],
                                     capture_output=True, text=True, cwd=tmp)
                if not out.exists():
                    tail = (run.stderr or run.stdout).strip().splitlines()[-1:] or ["no output"]
                    print(f"{label:<15} {engine:<9} {'':<8} CRASH     {tail[0][:150]}")
                    totals[(engine, "crash")] = totals.get((engine, "crash"), 0) + 1
                    continue
                ref = json.loads(out.read_text(encoding="utf-8"))
                for case, r in ref["cases"].items():
                    s = st.get(case)
                    state, detail = "", ""
                    if r["status"] == "unsupported" or r["status"] == "skipped":
                        state, detail = "n/a", r["reason"]
                    elif "periods" in r and s is not None and s.get("modal"):
                        ours = [q["period"] for q in s["modal"]["modes"]]
                        pairs = list(zip(ours, r["periods"]))
                        bad = [f"mode {k + 1}: struktal {a:.8e}, engine {b:.8e}" for k, (a, b) in enumerate(pairs)
                               if abs(a - b) > 1e-6 * max(abs(a), abs(b))]
                        state = ("FORM-DIFF" if discretized else "DIFF") if bad else "match"
                        any_diff = any_diff or (bool(bad) and not discretized)
                        worst = max((abs(a - b) / max(abs(a), abs(b)) for a, b in pairs), default=0.0)
                        detail = f"{len(pairs)} periods, worst relative difference {worst:.2g}" + ("; " + "; ".join(bad[:3]) if bad else "")
                    elif s is None or s["status"]["completion"] != "complete":
                        # Struktal stopped, e.g. a mechanism: did the engine stop too?
                        state = "both-stop" if r["status"] == "failed" else "ONLY-ST"
                        detail = "Struktal: " + (s["status"]["physical_status"] if s else "no result")
                        if r["status"] != "failed":
                            detail += "; the engine returned numbers"
                    elif r["status"] == "failed":
                        # Struktal solved what the engine could not: as worth a look as a difference.
                        state, detail = "ENG-FAIL", r["reason"]
                        any_diff = True
                    else:
                        checks, worst, what, diffs = compare_case(s, r, joints_model, rtol)
                        # At a mesh, formulations legitimately differ: the case's
                        # own reference judges it, so this is shown but not a failure.
                        state = ("FORM-DIFF" if discretized else "DIFF") if diffs else "match"
                        any_diff = any_diff or (bool(diffs) and not discretized)
                        detail = f"{checks} values, worst {worst:.2g} of tolerance ({what})"
                        if diffs:
                            detail = f"{len(diffs)}/{checks} differ; " + "; ".join(diffs[:3])
                    totals[(engine, state)] = totals.get((engine, state), 0) + 1
                    print(f"{label:<15} {engine:<9} {case:<8} {state:<9} {detail[:190]}")
    print("\nsummary:")
    for (engine, state), n in sorted(totals.items()):
        print(f"  {engine:<9} {state:<9} {n}")
    return 1 if any_diff else 0


if __name__ == "__main__":
    raise SystemExit(main())
