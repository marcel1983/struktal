"""Reads a Struktal model for the external engines, independently of the Rust code.

Everything is converted to SI here. A runner only maps entities to its engine:
it implements `solve_case(model, loads)` and returns, per joint, the displacement
and reaction vectors in global axes. Nothing here is copied from another engine.
"""
import json
import math
import sys
from pathlib import Path

FORCE = {"N": 1.0, "kN": 1e3, "MN": 1e6, "kgf": 9.80665, "tonf": 9806.65, "lbf": 4.4482216152605, "kip": 4448.2216152605}
LENGTH = {"m": 1.0, "cm": 0.01, "mm": 0.001, "ft": 0.3048, "in": 0.0254}
VERTICAL_SIN_TOL = 1e-3


class Unsupported(Exception):
    """The engine cannot represent this model faithfully: never compare it."""


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def dot(a, b):
    return sum(p * q for p, q in zip(a, b))


def unit(a):
    n = math.sqrt(dot(a, a))
    return [v / n for v in a]


def local_axes(xi, xj, angle_deg):
    """spec/units-and-conventions.md: axis 1 from I to J; plane 1-2 contains Z with
    axis 2 upward, or axis 2 = +X for vertical members; 3 = 1 x 2; then the angle."""
    e1 = unit([b - a for a, b in zip(xi, xj)])
    if math.hypot(e1[0], e1[1]) < VERTICAL_SIN_TOL:
        e2 = [1.0, 0.0, 0.0]
        e2 = unit([v - dot(e2, e1) * c for v, c in zip(e2, e1)])
    else:
        z = [0.0, 0.0, 1.0]
        e2 = unit([v - dot(z, e1) * c for v, c in zip(z, e1)])
    e3 = cross(e1, e2)
    c, s = math.cos(math.radians(angle_deg)), math.sin(math.radians(angle_deg))
    return e1, [c * p + s * q for p, q in zip(e2, e3)], [-s * p + c * q for p, q in zip(e2, e3)]


def nodal_shares(corners, axes):
    """Share of the area of each corner, by integrating the bilinear shape
    functions; they carry a uniform load as consistent nodal forces."""
    xy = [(dot(c, axes[0]), dot(c, axes[1])) for c in corners]
    g = 1 / math.sqrt(3)
    signs = [(-1, -1), (1, -1), (1, 1), (-1, 1)]
    shares = [0.0] * 4
    for r in (-g, g):
        for s in (-g, g):
            dr = [0.25 * a * (1 + s * b) for a, b in signs]
            ds = [0.25 * b * (1 + r * a) for a, b in signs]
            j = (sum(d * p[0] for d, p in zip(dr, xy)) * sum(d * p[1] for d, p in zip(ds, xy))
                 - sum(d * p[1] for d, p in zip(dr, xy)) * sum(d * p[0] for d, p in zip(ds, xy)))
            for k, (a, b) in enumerate(signs):
                shares[k] += 0.25 * (1 + r * a) * (1 + s * b) * j
    return shares


def load_model(path):
    m = json.loads(Path(path).read_text(encoding="utf-8"))
    u = m.get("units") or {"force": "N", "length": "m"}
    f, l = FORCE[u["force"]], LENGTH[u["length"]]
    stress = f / l**2
    for mat in m["materials"]:
        mat["e"] *= stress
        mat["g"] = mat["g"] * stress if mat.get("g") is not None else mat["e"] / (2 * (1 + mat["nu"]))
        mat["unit_weight"] = mat.get("unit_weight", 0.0) * f / l**3
    for s in m["sections"]:
        if s.get("shape"):
            raise SystemExit("parametric sections are not translated: their properties would only repeat the same formulas")
        s["area"] *= l**2
        for k in ("i33", "i22", "j"):
            s[k] *= l**4
        s["as2"] = s.get("as2", 0.0) * l**2
        s["as3"] = s.get("as3", 0.0) * l**2
    for j in m["joints"]:
        for k in "xyz":
            j[k] *= l
        j["restraint"] = j.get("restraint") or [False] * 6
        sp = j.get("springs") or [0.0] * 6
        j["springs"] = [v * f / l for v in sp[:3]] + [v * f * l for v in sp[3:]]
        j["xyz"] = [j["x"], j["y"], j["z"]]
    by_joint = {j["name"]: j for j in m["joints"]}
    by_section = {s["name"]: s for s in m["sections"]}
    by_material = {x["name"]: x for x in m["materials"]}
    for fr in m["frames"]:
        off = fr.get("offsets") or {}
        fr["off_i"] = [v * l for v in off.get("i", [0.0] * 3)]
        fr["off_j"] = [v * l for v in off.get("j", [0.0] * 3)]
        fr["xi"] = [a + b for a, b in zip(by_joint[fr["i"]]["xyz"], fr["off_i"])]
        fr["xj"] = [a + b for a, b in zip(by_joint[fr["j"]]["xyz"], fr["off_j"])]
        fr["length"] = math.dist(fr["xi"], fr["xj"])
        fr["axes"] = local_axes(fr["xi"], fr["xj"], fr.get("angle_deg", 0.0))
        fr["sec"] = by_section[fr["section"]]
        fr["mat"] = by_material[fr["sec"]["material"]]
        rel = fr.get("releases") or {}
        fr["rel_i"] = rel.get("i", [False] * 6)
        fr["rel_j"] = rel.get("j", [False] * 6)
        springs = [*rel.get("i_springs", []), *rel.get("j_springs", [])]
        fr["rel_has_springs"] = any(v != 0 for v in springs)
    by_area_section = {s["name"]: s for s in m.get("area_sections", [])}
    for s in m.get("area_sections", []):
        s["thickness"] *= l
        if s.get("bending_thickness") is not None:
            s["bending_thickness"] *= l
        s["mat"] = by_material[s["material"]]
    for a in m.get("areas", []):
        a["corners"] = [by_joint[n]["xyz"] for n in a["joints"]]
        a["sec"] = by_area_section[a["section"]]
        e3 = unit(cross([q - p for p, q in zip(a["corners"][0], a["corners"][2])],
                        [q - p for p, q in zip(a["corners"][1], a["corners"][3])]))
        h = cross([0.0, 0.0, 1.0], e3)
        e1 = unit(cross([0.0, 1.0, 0.0], e3)) if math.sqrt(dot(h, h)) < 1e-3 else unit(h)
        a["axes"] = [e1, cross(e3, e1), e3]
        a["shares"] = nodal_shares(a["corners"], a["axes"])
    for al in m.get("area_loads", []):
        al["value"] *= f / l**2
    for jl in m.get("joint_loads", []):
        jl["values"] = [v * f for v in jl["values"][:3]] + [v * f * l for v in jl["values"][3:]]
    for jd in m.get("joint_displacements", []):
        jd["values"] = [v * l for v in jd["values"][:3]] + jd["values"][3:]
    for fl in m.get("frame_loads", []):
        load = fl["load"]
        factor = {"Uniform": f / l, "Point": f, "Trapezoidal": f / l, "PointMoment": f * l, "TrapezoidalMoment": f}[load["kind"]]
        for key in ("value", "value_start", "value_end"):
            if key in load:
                load[key] *= factor
    m["by_joint"] = by_joint
    return m


def direction(name, axes):
    return {
        "Local1": axes[0], "Local2": axes[1], "Local3": axes[2],
        "GlobalX": [1.0, 0.0, 0.0], "GlobalY": [0.0, 1.0, 0.0], "GlobalZ": [0.0, 0.0, 1.0],
        "Gravity": [0.0, 0.0, -1.0],
    }[name]


def case_loads(m, case):
    """Loads of one case, scaled, in SI and global axes."""
    scale = {}
    for sp in case["loads"]:
        scale[sp["pattern"]] = scale.get(sp["pattern"], 0.0) + sp["scale"]
    joints, imposed = {}, {}
    for jl in m.get("joint_loads", []):
        sc = scale.get(jl["pattern"], 0.0)
        if sc != 0.0:
            t = joints.setdefault(jl["joint"], [0.0] * 6)
            joints[jl["joint"]] = [a + sc * b for a, b in zip(t, jl["values"])]
    for jd in m.get("joint_displacements", []):
        sc = scale.get(jd["pattern"], 0.0)
        if sc != 0.0:
            t = imposed.setdefault(jd["joint"], [0.0] * 6)
            imposed[jd["joint"]] = [a + sc * b for a, b in zip(t, jd["values"])]
    frames = {
        fr["name"]: {"uniform": [0.0] * 3, "points": [], "segments": [], "moments": [], "moment_segments": []}
        for fr in m["frames"]
    }
    by_frame = {fr["name"]: fr for fr in m["frames"]}
    for p in m.get("load_patterns", []):
        sc = scale.get(p["name"], 0.0) * p.get("self_weight_multiplier", 0.0)
        if sc != 0.0:
            for fr in m["frames"]:
                frames[fr["name"]]["uniform"][2] -= fr["mat"]["unit_weight"] * fr["sec"]["area"] * sc
    for fl in m.get("frame_loads", []):
        sc = scale.get(fl["pattern"], 0.0)
        if sc == 0.0:
            continue
        load = fl["load"]
        d = direction(load["direction"], by_frame[fl["frame"]]["axes"])
        slot = frames[fl["frame"]]
        kind = load["kind"]
        if kind in ("Uniform", "Point", "PointMoment"):
            g = [c * load["value"] * sc for c in d]
            if kind == "Uniform":
                slot["uniform"] = [a + b for a, b in zip(slot["uniform"], g)]
            else:
                slot["points" if kind == "Point" else "moments"].append((load["rel"], g))
        else:
            segment = (load["rel_start"], load["rel_end"], [c * load["value_start"] * sc for c in d],
                       [c * load["value_end"] * sc for c in d])
            slot["segments" if kind == "Trapezoidal" else "moment_segments"].append(segment)
    # Area loads and self-weight reach every engine as the same nodal forces.
    intensity = {a["name"]: [0.0] * 3 for a in m.get("areas", [])}
    for p in m.get("load_patterns", []):
        sc = scale.get(p["name"], 0.0) * p.get("self_weight_multiplier", 0.0)
        if sc != 0.0:
            for a in m.get("areas", []):
                intensity[a["name"]][2] -= a["sec"]["mat"]["unit_weight"] * a["sec"]["thickness"] * sc
    by_area = {a["name"]: a for a in m.get("areas", [])}
    for al in m.get("area_loads", []):
        sc = scale.get(al["pattern"], 0.0)
        if sc != 0.0:
            d = direction(al["direction"], by_area[al["area"]]["axes"])
            intensity[al["area"]] = [p + c * al["value"] * sc for p, c in zip(intensity[al["area"]], d)]
    for a in m.get("areas", []):
        w = intensity[a["name"]]
        if any(v != 0.0 for v in w):
            for name, share in zip(a["joints"], a["shares"]):
                t = joints.setdefault(name, [0.0] * 6)
                joints[name] = [t[k] + (w[k] * share if k < 3 else 0.0) for k in range(6)]
    return {"joints": joints, "imposed": imposed, "frames": frames}


def combine(results, m):
    """Linear combinations by superposition of the solved cases."""
    pending = list(m.get("combinations", []))
    while pending:
        ready = [c for c in pending if all(t["case"] in results for t in c["cases"])]
        if not ready:
            for c in pending:
                results[c["name"]] = {"status": "skipped", "reason": "depends on a case not solved"}
            return
        for c in ready:
            pending.remove(c)
            if c.get("type") == "Envelope":
                results[c["name"]] = {"status": "skipped", "reason": "envelopes are not compared"}
                continue
            if any(results[t["case"]].get("status") != "ok" for t in c["cases"]):
                results[c["name"]] = {"status": "skipped", "reason": "depends on a case not solved"}
                continue
            joints = {}
            for t in c["cases"]:
                for name, r in results[t["case"]]["joints"].items():
                    a = joints.setdefault(name, {"disp": [0.0] * 6, "reaction": [0.0] * 6})
                    a["disp"] = [x + t["scale"] * y for x, y in zip(a["disp"], r["disp"])]
                    a["reaction"] = [x + t["scale"] * y for x, y in zip(a["reaction"], r["reaction"])]
            results[c["name"]] = {"status": "ok", "joints": joints}


def main(engine, version, solve_case):
    """Usage: runner.py model.json output.json. Engines also print to stdout."""
    m = load_model(sys.argv[1])
    results = {}
    for case in m.get("load_cases", []):
        try:
            joints = solve_case(m, case_loads(m, case))
            results[case["name"]] = {"status": "ok", "joints": joints}
        except Unsupported as e:
            results[case["name"]] = {"status": "unsupported", "reason": str(e)}
        except Exception as e:  # noqa: BLE001  an engine failure is a result to report
            results[case["name"]] = {"status": "failed", "reason": f"{type(e).__name__}: {e}"[:300]}
    combine(results, m)
    Path(sys.argv[2]).write_text(json.dumps({"engine": engine, "version": version, "cases": results}), encoding="utf-8")
