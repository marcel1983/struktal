#!/usr/bin/env python3
"""Runs rust-fem solve on generated grids with each backend and prints a table.

usage: python3 tools/bench.py OUTDIR [backend ...]   (needs `cargo build --release`)
BENCH_MAX_SIZES=n limits the run to the first n sizes.
Single run per size; stage timings come from the run manifest.
"""
import json
import os
import resource
import subprocess
import sys
import time
from pathlib import Path

out = Path(sys.argv[1])
backends = sys.argv[2:] or ["skyline", "faer"]
sizes = [(4, 4, 5), (8, 8, 10), (12, 12, 20), (16, 16, 30), (20, 20, 40), (24, 24, 60)]
sizes = sizes[: int(os.environ.get("BENCH_MAX_SIZES", len(sizes)))]
out.mkdir(parents=True, exist_ok=True)
exe = Path(__file__).resolve().parent.parent / "target" / "release" / "rust-fem"
print("| backend | modelo | ecuaciones | entradas del factor | factor MB | ensamblaje s | factorización s | 2 soluciones s | total s | RSS pico MiB | residuo | estado |")
print("|---|---|---|---|---|---|---|---|---|---|---|---|")
for nx, ny, nl in sizes:
    model = out / f"grid_{nx}_{ny}_{nl}.json"
    if not model.exists():
        subprocess.run([sys.executable, str(Path(__file__).parent / "gen_grid.py"), str(nx), str(ny), str(nl), str(model)],
                       check=True, capture_output=True)
    for b in backends:
        res = out / f"res_{b}_{nx}_{ny}_{nl}.json"
        t = time.perf_counter()
        p = subprocess.run([str(exe), "solve", str(model), "--backend", b, "--memory-budget-mib", "4096", "-o", str(res)],
                           capture_output=True, text=True)
        dt = time.perf_counter() - t
        # ru_maxrss of children is a running maximum; run each in its own process group
        rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss / 1024
        d = json.loads(res.read_text())
        m, s = d["manifest"], d["manifest"]["solver"] or {}
        tm = m.get("timings", {})
        ok = all(c["status"]["completion"] == "complete" for c in d["cases"])
        resid = max((c["checks"] or {}).get("residual_relative", 0) for c in d["cases"]) if ok else float("nan")
        print(f"| {b} | {nx}x{ny}x{nl} | {m['equations']} | {s.get('profile_entries', '-')} | "
              f"{s.get('factor_bytes', s.get('required_bytes', 0)) / 1e6:.1f} | {tm.get('assembly_s', 0):.2f} | "
              f"{tm.get('factorization_s', 0):.2f} | {tm.get('solve_s', 0):.2f} | {dt:.2f} | {rss:.0f} | "
              f"{resid:.1e} | {'ok' if ok else d['cases'][0]['status']['physical_status'] + ': ' + '; '.join(d['cases'][0]['messages'])[:60]} |",
              flush=True)
