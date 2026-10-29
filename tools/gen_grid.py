#!/usr/bin/env python3
"""Generates a regular 3D moment-frame building for scaling measurements.

usage: gen_grid.py NX NY LEVELS OUT.json   (bays of 6 m, storeys of 3 m)
"""
import json
import sys

nx, ny, nl, out = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
joints, frames, loads = [], [], []
name = lambda i, j, k: f"J{i}_{j}_{k}"
for k in range(nl + 1):
    for j in range(ny + 1):
        for i in range(nx + 1):
            joints.append({"name": name(i, j, k), "x": 6.0 * i, "y": 6.0 * j, "z": 3.0 * k,
                           "restraint": [True] * 6 if k == 0 else [False] * 6})
            if k > 0:
                frames.append({"name": f"C{i}_{j}_{k}", "i": name(i, j, k - 1), "j": name(i, j, k), "section": "COL"})
                loads.append({"pattern": "WIND", "joint": name(i, j, k), "values": [1000.0, 0, 0, 0, 0, 0]})
                if i < nx:
                    frames.append({"name": f"BX{i}_{j}_{k}", "i": name(i, j, k), "j": name(i + 1, j, k), "section": "BEAM"})
                if j < ny:
                    frames.append({"name": f"BY{i}_{j}_{k}", "i": name(i, j, k), "j": name(i, j + 1, k), "section": "BEAM"})
model = {
    "schema_version": 1, "title": f"grid {nx}x{ny}x{nl}", "units": {"force": "N", "length": "m"},
    "materials": [{"name": "STEEL", "e": 200e9, "nu": 0.3, "unit_weight": 78500.0}],
    "sections": [{"name": "COL", "material": "STEEL", "area": 0.02, "i33": 3e-4, "i22": 1e-4, "j": 1e-5},
                 {"name": "BEAM", "material": "STEEL", "area": 0.01, "i33": 2e-4, "i22": 1e-5, "j": 5e-6}],
    "joints": joints, "frames": frames,
    "load_patterns": [{"name": "DEAD", "self_weight_multiplier": 1.0}, {"name": "WIND"}],
    "joint_loads": loads, "frame_loads": [],
    "load_cases": [{"type": "LinearStatic", "name": "DEAD", "loads": [{"pattern": "DEAD", "scale": 1.0}]},
                   {"type": "LinearStatic", "name": "WIND", "loads": [{"pattern": "WIND", "scale": 1.0}]}],
    "combinations": [],
}
json.dump(model, open(out, "w"))
print(f"{len(joints)} joints, {len(frames)} frames")
