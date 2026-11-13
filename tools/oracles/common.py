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
GRAVITY = 9.80665          # m/s2: load of a mass source to mass, and weight of a mass
AXIS_TOL_REL = 1e-6        # a joint this close to a frame axis connects to it (decision 0015)
UPWARD_TOL_REL = 1e-9      # an upward mass-source load below this share is rounding
HEIGHT_TOL_REL = 1e-9      # a joint this close to the lowest one is at the base (decision 0024)


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
        mat["mass_density"] = mat.get("mass_density", 0.0) * f / l**4
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
        am = j.get("added_mass") or [0.0] * 6
        j["added_mass"] = [v * f / l for v in am[:3]] + [v * f * l for v in am[3:]]
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
                        [q - p for p, q in zip(a["corners"][1], a["corners"][-1])]))
        h = cross([0.0, 0.0, 1.0], e3)
        e1 = unit(cross([0.0, 1.0, 0.0], e3)) if math.sqrt(dot(h, h)) < 1e-3 else unit(h)
        a["axes"] = [e1, cross(e3, e1), e3]
        if len(a["joints"]) == 3:
            e12, e13 = [[q - p for p, q in zip(a["corners"][0], a["corners"][k])] for k in (1, 2)]
            normal = cross(e12, e13)
            a["shares"] = [math.sqrt(dot(normal, normal)) / 6.0] * 3
        else:
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
    for c in m.get("load_cases", []):
        if c["type"] == "ResponseSpectrum":
            c["scale"] *= l  # the function is in g units, the scale turns it into length/s2
    m["by_joint"] = by_joint
    expand_mesh(m)
    return m


def expand_mesh(m):
    """Frames as the pieces of the analysis mesh: equal divisions and, when asked,
    the joints on their axis (decision 0015). A cut lands on a joint if one is
    there; otherwise a new joint, which only the engines have."""
    m["frames_orig"] = m["frames"]
    user_joints = list(m["joints"])
    pieces = []
    for fr in m["frames_orig"]:
        mesh = fr.get("auto_mesh") or {}
        n = mesh.get("segments", 1)
        length, xi, e1 = fr["length"], fr["xi"], fr["axes"][0]
        tol = AXIS_TOL_REL * length
        cuts = []
        if (mesh.get("at_joints") or n > 1) and (any(fr["off_i"]) or any(fr["off_j"])):
            m["unsupported"] = f"frame {fr['name']}: this translator does not split frames with offsets"
        if mesh.get("at_joints"):
            for j in user_joints:
                if j["name"] in (fr["i"], fr["j"]):
                    continue
                d = [p - q for p, q in zip(j["xyz"], xi)]
                along = dot(d, e1)
                if along <= tol or along >= length - tol:
                    continue
                if math.sqrt(sum((d[k] - along * e1[k]) ** 2 for k in range(3))) <= tol:
                    cuts.append((along / length, j["name"]))
        for k in range(1, n):
            rel = k / n
            if all(abs(c - rel) * length > tol for c, _ in cuts):
                cuts.append((rel, None))
        cuts.sort(key=lambda c: c[0])
        ends = [(0.0, fr["i"])]
        for k, (rel, name) in enumerate(cuts):
            if name is None:
                name = f"{fr['name']}~{k + 1}"
                xyz = [a + rel * length * b for a, b in zip(xi, e1)]
                joint = {"name": name, "xyz": xyz, "x": xyz[0], "y": xyz[1], "z": xyz[2], "restraint": [False] * 6,
                         "springs": [0.0] * 6, "added_mass": [0.0] * 6, "generated": True}
                m["joints"].append(joint)
                m["by_joint"][name] = joint
            ends.append((rel, name))
        ends.append((1.0, fr["j"]))
        for k in range(len(ends) - 1):
            (r0, a), (r1, b) = ends[k], ends[k + 1]
            p = dict(fr, name=fr["name"] if len(ends) == 2 else f"{fr['name']}#{k + 1}", i=a, j=b, orig=fr["name"], r0=r0, r1=r1)
            if len(ends) > 2:
                p["xi"] = m["by_joint"][a]["xyz"] if k > 0 else fr["xi"]
                p["xj"] = m["by_joint"][b]["xyz"] if k < len(ends) - 2 else fr["xj"]
                p["length"] = math.dist(p["xi"], p["xj"])
                p["axes"] = local_axes(p["xi"], p["xj"], fr.get("angle_deg", 0.0))
                p["rel_i"] = fr["rel_i"] if k == 0 else [False] * 6
                p["rel_j"] = fr["rel_j"] if k == len(ends) - 2 else [False] * 6
                p["off_i"] = fr["off_i"] if k == 0 else [0.0] * 3
                p["off_j"] = fr["off_j"] if k == len(ends) - 2 else [0.0] * 3
            pieces.append(p)
    m["frames"] = pieces


def lumped_mass(m):
    """Mass per joint [6], in SI, as the mass source gives it: half of each piece at
    each end and each area by the share of its corners, on translations; added
    masses as given; the downward load of the source patterns over gravity."""
    if "_lumped" in m:
        return m["_lumped"]
    src = m.get("mass_source") or {}
    elements, added = src.get("elements", True), src.get("added_masses", True)
    lumped = {j["name"]: list(j["added_mass"]) if added else [0.0] * 6 for j in m["joints"]}
    if elements:
        for fr in m["frames"]:
            half = 0.5 * fr["mat"]["mass_density"] * fr["sec"]["area"] * fr["length"]
            for end in (fr["i"], fr["j"]):
                for d in range(3):
                    lumped[end][d] += half
        for a in m.get("areas", []):
            density = a["sec"]["mat"]["mass_density"] * a["sec"]["thickness"]
            for name, share in zip(a["joints"], a["shares"]):
                for d in range(3):
                    lumped[name][d] += density * share
    if src.get("patterns"):
        loads = case_loads(m, {"loads": src["patterns"]}, automatic=False)
        down = {name: v[2] for name, v in loads["joints"].items()}
        for fr in m["frames"]:
            span = loads["frames"][fr["name"]]
            if span["points"] or span["segments"] or span["moments"] or span["moment_segments"]:
                raise Unsupported("mass source: only joint, uniform and area loads are translated")
            w = span["uniform"]
            if any(w):
                if any(fr["rel_i"]) or any(fr["rel_j"]):
                    raise Unsupported(f"mass source: a uniform load on frame {fr['orig']}, which has releases")
                for end in (fr["i"], fr["j"]):
                    down[end] = down.get(end, 0.0) + w[2] * fr["length"] / 2
        largest = max((abs(v) for v in down.values()), default=0.0)
        for name, fz in down.items():
            if fz > UPWARD_TOL_REL * largest and not all(m["by_joint"][name]["restraint"][:3]):
                raise RuntimeError(f"mass source: joint {name} has a net upward load")
            for d in range(3):
                lumped[name][d] += max(-fz, 0.0) / GRAVITY
    m["_lumped"] = lumped
    return lumped


def auto_seismic_forces(m, a):
    """V = C g sum(m), shared as V m h^k / sum(m h^k), h from the lowest joint (decision 0024)."""
    lumped = lumped_mass(m)
    d = "XYZ".index(a["direction"])
    z = {j["name"]: j["xyz"][2] for j in m["joints"]}
    low, high = min(z.values()), max(z.values())
    weight = GRAVITY * sum(v[d] for v in lumped.values())
    share = {n: lumped[n][d] * (h - low) ** a["exponent"] for n, h in z.items() if h - low > HEIGHT_TOL_REL * (high - low)}
    total = sum(share.values())
    if weight == 0.0 or total == 0.0:
        raise RuntimeError("auto seismic: no mass above the lowest joint")
    return {n: [a["coefficient"] * weight * v / total if k == d else 0.0 for k in range(6)] for n, v in share.items() if v}


def spectrum_acceleration(points, period):
    """Linear in the period, flat beyond the ends."""
    if period <= points[0][0]:
        return points[0][1]
    if period >= points[-1][0]:
        return points[-1][1]
    for (t0, a0), (t1, a1) in zip(points, points[1:]):
        if t0 <= period <= t1:
            return a0 + (a1 - a0) * (period - t0) / (t1 - t0)
    raise ValueError(period)


def cqc_rho(wi, wj, z):
    r = wi / wj
    den = (1 - r * r) ** 2 + 4 * z * z * r * (1 + r) ** 2
    return 1.0 if den == 0 else 8 * z * z * (1 + r) * r**1.5 / den


def combine_modes(per_mode, omegas, case):
    """Each value combined across modes, by SRSS or CQC, as a magnitude."""
    n = len(per_mode)
    cqc = case.get("combination", "Cqc") == "Cqc"
    rho = [[cqc_rho(omegas[i], omegas[j], case.get("damping", 0.05)) if cqc else float(i == j) for j in range(n)] for i in range(n)]

    def comb(values):
        return math.sqrt(max(0.0, sum(rho[i][j] * values[i] * values[j] for i in range(n) for j in range(n))))

    joints = {}
    for name in per_mode[0]:
        joints[name] = {key: [comb([r[name][key][k] for r in per_mode]) for k in range(6)] for key in ("disp", "reaction")}
    return joints


def empty_loads(m):
    return {"joints": {}, "imposed": {},
            "frames": {fr["name"]: {"uniform": [0.0] * 3, "points": [], "segments": [], "moments": [], "moment_segments": []}
                       for fr in m["frames"]}}


def direction(name, axes):
    return {
        "Local1": axes[0], "Local2": axes[1], "Local3": axes[2],
        "GlobalX": [1.0, 0.0, 0.0], "GlobalY": [0.0, 1.0, 0.0], "GlobalZ": [0.0, 0.0, 1.0],
        "Gravity": [0.0, 0.0, -1.0],
    }[name]


def case_loads(m, case, automatic=True):
    """Loads of one case, scaled, in SI and global axes, on the pieces of the mesh."""
    scale = {}
    for sp in case["loads"]:
        scale[sp["pattern"]] = scale.get(sp["pattern"], 0.0) + sp["scale"]
    joints, imposed = {}, {}
    for jl in m.get("joint_loads", []):
        sc = scale.get(jl["pattern"], 0.0)
        if sc != 0.0:
            t = joints.setdefault(jl["joint"], [0.0] * 6)
            joints[jl["joint"]] = [a + sc * b for a, b in zip(t, jl["values"])]
    for p in m.get("load_patterns", []):
        sc = scale.get(p["name"], 0.0)
        if p.get("auto_seismic") and sc != 0.0:
            if not automatic:
                raise Unsupported(f"pattern {p['name']} is computed from the mass")
            for name, f in auto_seismic_forces(m, p["auto_seismic"]).items():
                t = joints.setdefault(name, [0.0] * 6)
                joints[name] = [a + sc * b for a, b in zip(t, f)]
    for jd in m.get("joint_displacements", []):
        sc = scale.get(jd["pattern"], 0.0)
        if sc != 0.0:
            t = imposed.setdefault(jd["joint"], [0.0] * 6)
            imposed[jd["joint"]] = [a + sc * b for a, b in zip(t, jd["values"])]
    frames = {
        fr["name"]: {"uniform": [0.0] * 3, "points": [], "segments": [], "moments": [], "moment_segments": []}
        for fr in m["frames_orig"]
    }
    by_frame = {fr["name"]: fr for fr in m["frames_orig"]}
    for p in m.get("load_patterns", []):
        sc = scale.get(p["name"], 0.0) * p.get("self_weight_multiplier", 0.0)
        if sc != 0.0:
            for fr in m["frames_orig"]:
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
    return {"joints": joints, "imposed": imposed, "frames": {p["name"]: piece_loads(frames[p["orig"]], p) for p in m["frames"]}}


def piece_loads(span, p):
    """The span loads of a frame that fall on one of its pieces, relative to it.
    A concentrated load exactly at a cut goes to the piece before it."""
    r0, r1 = p["r0"], p["r1"]
    width = r1 - r0
    out = {"uniform": list(span["uniform"]), "points": [], "segments": [], "moments": [], "moment_segments": []}
    for key in ("points", "moments"):
        for rel, g in span[key]:
            if (rel > r0 or r0 == 0.0) and (rel <= r1 or r1 == 1.0):
                out[key].append(((rel - r0) / width, g))
    for key in ("segments", "moment_segments"):
        for a, b, va, vb in span[key]:
            lo, hi = max(a, r0), min(b, r1)
            if lo < hi:
                at = [[x + (y - x) * (r - a) / (b - a) for x, y in zip(va, vb)] for r in (lo, hi)]
                out[key].append(((lo - r0) / width, (hi - r0) / width, at[0], at[1]))
    return out


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


def main(engine, version, solve_case, solve_modes=None, solve_spectrum=None):
    """Usage: runner.py model.json output.json. Engines also print to stdout."""
    m = load_model(sys.argv[1])
    results = {}
    for case in m.get("load_cases", []):
        if m.get("unsupported"):
            results[case["name"]] = {"status": "unsupported", "reason": m["unsupported"]}
            continue
        if case["type"] in ("Modal", "ResponseSpectrum"):
            solve = solve_modes if case["type"] == "Modal" else solve_spectrum
            if solve is None:
                results[case["name"]] = {"status": "skipped", "reason": f"{case['type']} is not translated for this engine"}
                continue
            try:
                if case["type"] == "Modal":
                    results[case["name"]] = {"status": "ok", "periods": solve(m, case["modes"]), "joints": {}}
                else:
                    results[case["name"]] = {"status": "ok", "joints": solve(m, case)}
            except Unsupported as e:
                results[case["name"]] = {"status": "unsupported", "reason": str(e)}
            except Exception as e:  # noqa: BLE001
                results[case["name"]] = {"status": "failed", "reason": f"{type(e).__name__}: {e}"[:300]}
            continue
        try:
            joints = solve_case(m, case_loads(m, case))
            results[case["name"]] = {"status": "ok", "joints": joints}
        except Unsupported as e:
            results[case["name"]] = {"status": "unsupported", "reason": str(e)}
        except Exception as e:  # noqa: BLE001  an engine failure is a result to report
            results[case["name"]] = {"status": "failed", "reason": f"{type(e).__name__}: {e}"[:300]}
    combine(results, m)
    Path(sys.argv[2]).write_text(json.dumps({"engine": engine, "version": version, "cases": results}), encoding="utf-8")
