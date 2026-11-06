#!/usr/bin/env python3
"""Generates verification models and analytical references.

Expected values are computed here from closed-form beam theory and statics,
independently of the Rust engine. Never paste engine output into a case.

Run from the repository root:  python3 tools/gen_cases.py
Re-running rewrites model.json and case.json and refreshes model_sha256; do
that only when a case is intentionally changed, and review the diff.
"""
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "verification" / "cases"
TOL_FILE = "../../tolerances.json"
ORACLE_PENDING = {"sap2000": "oracle_pending", "independent": "analytical"}

E, NU = 200e9, 0.3


def model(title, joints, frames, sections, patterns, joint_loads=(), frame_loads=(), cases=None,
          combos=(), units=None, materials=None, schema=1, constraints=()):
    # Cases created before schema v2 stay at v1 on purpose: they exercise the
    # v1 -> v2 migration end to end (decision 0007).
    extra = {"constraints": list(constraints)} if constraints else {}
    return {
        **extra,
        "schema_version": schema,
        "title": title,
        "units": units or {"force": "N", "length": "m"},
        "materials": materials or [{"name": "STEEL", "e": E, "nu": NU, "unit_weight": 0.0}],
        "sections": sections,
        "joints": joints,
        "frames": frames,
        "load_patterns": patterns,
        "joint_loads": list(joint_loads),
        "frame_loads": list(frame_loads),
        "load_cases": cases or [{"type": "LinearStatic", "name": "LC1", "loads": [{"pattern": patterns[0]["name"], "scale": 1.0}]}],
        "combinations": list(combos),
    }


def J(name, x, y, z, r=None):
    return {"name": name, "x": x, "y": y, "z": z, "restraint": r or [False] * 6}


FIX = [True] * 6
SEC = {"name": "B", "material": "STEEL", "area": 0.01, "i33": 1e-4, "i22": 1e-4, "j": 2e-4}


def exp(result, case, entity, component, value, unit, rel=None):
    d = {"result": result, "case": case, "entity": entity, "component": component, "value": value, "unit": unit}
    if rel is not None:
        d["rel"] = rel
    return d


def write(case_id, title, features, mdl, reference, expected=(), expected_status=(), tol="linear-closed-form-v1"):
    d = ROOT / case_id
    d.mkdir(parents=True, exist_ok=True)
    text = json.dumps(mdl, indent=2) + "\n"
    (d / "model.json").write_text(text, encoding="utf-8", newline="\n")
    case = {
        "case_id": case_id,
        "title": title,
        "feature_ids": features,
        "model": "model.json",
        "model_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        "tolerances_file": TOL_FILE,
        "tolerances_id": tol,
        "reference": reference,
        "expected_status": list(expected_status),
        "expected": list(expected),
        "oracle": ORACLE_PENDING,
    }
    (d / "case.json").write_text(json.dumps(case, indent=2) + "\n", encoding="utf-8", newline="\n")


# ---------------------------------------------------------------- V-LIN-001
L, P, I = 3.0, 10_000.0, 1e-4
uz = -P * L**3 / (3 * E * I)          # -0.0045
ry = P * L**2 / (2 * E * I)           # 0.00225
work = P * abs(uz) / 2                 # 22.5
ref_cant = {
    "kind": "analytical",
    "source": "Euler-Bernoulli cantilever with tip load (guide §21.1)",
    "derivation": ["Uz = -P L^3/(3 E I)", "Ry = P L^2/(2 E I)", "Rz = P, My = -P L", "W = P |Uz| / 2"],
}
zero_disp = [exp("joint_displacement", "LC1", "2", c, 0.0, u) for c, u in
             [("UX", "m"), ("UY", "m"), ("RX", "rad"), ("RZ", "rad")]]
base = model(
    "V-LIN-001 cantilever, tip load",
    [J("1", 0, 0, 0, FIX), J("2", 3, 0, 0)],
    [{"name": "F1", "i": "1", "j": "2", "section": "B"}],
    [SEC], [{"name": "TIP"}],
    joint_loads=[{"pattern": "TIP", "joint": "2", "values": [0, 0, -P, 0, 0, 0]}],
)
# Station actions in the struktal convention (decision 0003): for this member
# local 1 = +X, 2 = +Z, 3 = -Y. Free body [x, L]: M3(x) = -P (L - x).
write("V-LIN-001", "Cantilever along X, tip load -Z", ["F01", "F02", "F06", "F08", "F18"], base, ref_cant, [
    exp("joint_displacement", "LC1", "2", "UZ", uz, "m"),
    exp("joint_displacement", "LC1", "2", "RY", ry, "rad"),
    *zero_disp,
    exp("joint_reaction", "LC1", "1", "FZ", P, "N"),
    exp("joint_reaction", "LC1", "1", "MY", -P * L, "N*m"),
    exp("joint_reaction", "LC1", "1", "FX", 0.0, "N"),
    exp("joint_reaction", "LC1", "1", "MX", 0.0, "N*m"),
    exp("external_work", "LC1", "", "", work, "J"),
    exp("frame_station", "LC1", "F1", "M3", -P * L, "N*m", 0.0),
    exp("frame_station", "LC1", "F1", "M3", -P * L / 2, "N*m", 0.5),
    exp("frame_station", "LC1", "F1", "M3", 0.0, "N*m", 1.0),
    exp("frame_station", "LC1", "F1", "V2", -P, "N", 0.5),
    exp("frame_station", "LC1", "F1", "M2", 0.0, "N*m", 0.5),
    exp("frame_station", "LC1", "F1", "P", 0.0, "N", 0.5),
])

# Rotated 45° about Z: same physics, global components rotate.
c = L / math.sqrt(2)
rot = json.loads(json.dumps(base))
rot["title"] = "V-LIN-001-ROT cantilever rotated 45 deg about Z"
rot["joints"][1].update({"x": c, "y": c})
write("V-LIN-001-ROT", "Cantilever rotated 45 deg in plan", ["F01", "F02", "F08"], rot, ref_cant, [
    exp("joint_displacement", "LC1", "2", "UZ", uz, "m"),
    exp("joint_displacement", "LC1", "2", "RX", -ry / math.sqrt(2), "rad"),
    exp("joint_displacement", "LC1", "2", "RY", ry / math.sqrt(2), "rad"),
    exp("joint_displacement", "LC1", "2", "UX", 0.0, "m"),
    exp("joint_reaction", "LC1", "1", "FZ", P, "N"),
    exp("joint_reaction", "LC1", "1", "MX", P * c, "N*m"),
    exp("joint_reaction", "LC1", "1", "MY", -P * c, "N*m"),
    exp("external_work", "LC1", "", "", work, "J"),
])

# Reversed connectivity: member from free end to support.
rev = json.loads(json.dumps(base))
rev["title"] = "V-LIN-001-REV cantilever with reversed connectivity"
rev["frames"][0].update({"i": "2", "j": "1"})
write("V-LIN-001-REV", "Cantilever with I/J reversed, same supports", ["F01", "F08"], rev, ref_cant, [
    exp("joint_displacement", "LC1", "2", "UZ", uz, "m"),
    exp("joint_displacement", "LC1", "2", "RY", ry, "rad"),
    exp("joint_reaction", "LC1", "1", "FZ", P, "N"),
    exp("joint_reaction", "LC1", "1", "MY", -P * L, "N*m"),
    exp("external_work", "LC1", "", "", work, "J"),
])

# Same structure in kN and mm; results are reported in SI and must match.
knmm = json.loads(json.dumps(base))
knmm["title"] = "V-LIN-001-KNMM cantilever in kN, mm"
knmm["units"] = {"force": "kN", "length": "mm"}
knmm["materials"][0]["e"] = 200.0                     # kN/mm²
knmm["sections"][0].update({"area": 1e4, "i33": 1e8, "i22": 1e8, "j": 2e8})
knmm["joints"][1]["x"] = 3000.0
knmm["joint_loads"][0]["values"] = [0, 0, -10.0, 0, 0, 0]
write("V-LIN-001-KNMM", "Cantilever with kN/mm input units", ["F01", "F08", "units"], knmm, ref_cant, [
    exp("joint_displacement", "LC1", "2", "UZ", uz, "m"),
    exp("joint_displacement", "LC1", "2", "RY", ry, "rad"),
    exp("joint_reaction", "LC1", "1", "FZ", P, "N"),
    exp("joint_reaction", "LC1", "1", "MY", -P * L, "N*m"),
    exp("external_work", "LC1", "", "", work, "J"),
])

# ---------------------------------------------------------------- V-LIN-002
La, Pa, A = 2.0, 100_000.0, 0.01
ux = Pa * La / (E * A)
write("V-LIN-002", "Axial bar, tension", ["F02", "F08"], model(
    "V-LIN-002 axial bar",
    [J("1", 0, 0, 0, FIX), J("2", La, 0, 0)],
    [{"name": "F1", "i": "1", "j": "2", "section": "B"}],
    [SEC], [{"name": "AX"}],
    joint_loads=[{"pattern": "AX", "joint": "2", "values": [Pa, 0, 0, 0, 0, 0]}],
), {"kind": "analytical", "source": "Hooke's law for a prismatic bar", "derivation": ["u = P L/(E A)", "N = P (tension)"]}, [
    exp("joint_displacement", "LC1", "2", "UX", ux, "m"),
    exp("joint_displacement", "LC1", "2", "UZ", 0.0, "m"),
    exp("joint_reaction", "LC1", "1", "FX", -Pa, "N"),
    exp("frame_station", "LC1", "F1", "P", Pa, "N", 0.5),
    exp("external_work", "LC1", "", "", Pa * ux / 2, "J"),
])

# ---------------------------------------------------------------- V-LIN-003
# Simply supported, two elements, live UDL + self-weight + linear combination.
Ls, w = 6.0, 5000.0
gamma = 78_500.0
wd = gamma * A
PIN = [True, True, True, True, False, False]
ROL = [False, True, True, False, False, False]


def ss_mid_defl(q):
    return -5 * q * Ls**4 / (384 * E * I)


def ss_end_rot(q):
    return q * Ls**3 / (24 * E * I)


mats = [{"name": "STEEL", "e": E, "nu": NU, "unit_weight": gamma}]
write("V-LIN-003", "Simply supported beam: UDL, self-weight, 1.2D+1.6L", ["F02", "F06", "F07", "F08", "F16", "F18"], model(
    "V-LIN-003 simply supported beam",
    [J("A", 0, 0, 0, PIN), J("M", Ls / 2, 0, 0), J("B", Ls, 0, 0, ROL)],
    [{"name": "F1", "i": "A", "j": "M", "section": "B"}, {"name": "F2", "i": "M", "j": "B", "section": "B"}],
    [SEC], [{"name": "DEAD", "self_weight_multiplier": 1.0}, {"name": "LIVE"}],
    frame_loads=[
        {"pattern": "LIVE", "frame": "F1", "load": {"kind": "Uniform", "direction": "Gravity", "value": w}},
        {"pattern": "LIVE", "frame": "F2", "load": {"kind": "Uniform", "direction": "Gravity", "value": w}},
    ],
    cases=[
        {"type": "LinearStatic", "name": "DEAD", "loads": [{"pattern": "DEAD", "scale": 1.0}]},
        {"type": "LinearStatic", "name": "LIVE", "loads": [{"pattern": "LIVE", "scale": 1.0}]},
    ],
    combos=[{"type": "LinearAdd", "name": "ULS", "cases": [{"case": "DEAD", "scale": 1.2}, {"case": "LIVE", "scale": 1.6}]}],
    materials=mats,
), {"kind": "analytical", "source": "Simply supported Euler-Bernoulli beam, uniform load",
    "derivation": ["delta_mid = 5 q L^4/(384 E I)", "theta_end = q L^3/(24 E I)", "M_mid = q L^2/8", "M(x) = q x (L - x)/2", "R = q L/2"]}, [
    exp("joint_displacement", "LIVE", "M", "UZ", ss_mid_defl(w), "m"),
    exp("joint_displacement", "LIVE", "A", "RY", ss_end_rot(w), "rad"),
    exp("joint_displacement", "LIVE", "B", "RY", -ss_end_rot(w), "rad"),
    exp("joint_reaction", "LIVE", "A", "FZ", w * Ls / 2, "N"),
    exp("joint_reaction", "LIVE", "B", "FZ", w * Ls / 2, "N"),
    exp("frame_station", "LIVE", "F1", "M3", w * Ls**2 / 8, "N*m", 1.0),
    exp("frame_station", "LIVE", "F2", "M3", w * Ls**2 / 8, "N*m", 0.0),
    exp("frame_station", "LIVE", "F1", "M3", w * 0.75 * (Ls - 0.75) / 2, "N*m", 0.25),
    exp("frame_station", "LIVE", "F1", "M3", w * 1.5 * (Ls - 1.5) / 2, "N*m", 0.5),
    exp("joint_displacement", "DEAD", "M", "UZ", ss_mid_defl(wd), "m"),
    exp("joint_reaction", "DEAD", "A", "FZ", wd * Ls / 2, "N"),
    exp("joint_displacement", "ULS", "M", "UZ", 1.2 * ss_mid_defl(wd) + 1.6 * ss_mid_defl(w), "m"),
    exp("joint_reaction", "ULS", "B", "FZ", (1.2 * wd + 1.6 * w) * Ls / 2, "N"),
    exp("frame_station", "ULS", "F2", "M3", (1.2 * wd + 1.6 * w) * Ls**2 / 8, "N*m", 0.0),
])

# ---------------------------------------------------------------- V-LIN-004
# Vertical cantilevers: default local axes and axis angle with I33 != I22.
Lc, Pc, I33, I22 = 3.0, 1000.0, 2e-4, 1e-4
secc = {"name": "C", "material": "STEEL", "area": 0.01, "i33": I33, "i22": I22, "j": 2e-4}
write("V-LIN-004", "Vertical cantilevers: default local axes and 90 deg axis angle", ["F01", "F02", "local-axes"], model(
    "V-LIN-004 vertical columns",
    [J("A0", 0, 0, 0, FIX), J("A1", 0, 0, Lc), J("B0", 5, 0, 0, FIX), J("B1", 5, 0, Lc)],
    [{"name": "CA", "i": "A0", "j": "A1", "section": "C"},
     {"name": "CB", "i": "B0", "j": "B1", "section": "C", "angle_deg": 90.0}],
    [secc], [{"name": "H"}],
    joint_loads=[{"pattern": "H", "joint": "A1", "values": [Pc, 0, 0, 0, 0, 0]},
                 {"pattern": "H", "joint": "B1", "values": [Pc, 0, 0, 0, 0, 0]}],
), {"kind": "analytical", "source": "Cantilever tip load; documented SAP2000 default axes for vertical members (local 2 = +X), oracle pending",
    "derivation": ["CA bends about local 3 (I33): u = P L^3/(3 E I33)", "CB (angle 90) bends about local 2 (I22): u = P L^3/(3 E I22)"]}, [
    exp("joint_displacement", "LC1", "A1", "UX", Pc * Lc**3 / (3 * E * I33), "m"),
    exp("joint_displacement", "LC1", "A1", "RY", Pc * Lc**2 / (2 * E * I33), "rad"),
    exp("joint_displacement", "LC1", "B1", "UX", Pc * Lc**3 / (3 * E * I22), "m"),
    exp("joint_displacement", "LC1", "B1", "RY", Pc * Lc**2 / (2 * E * I22), "rad"),
    exp("joint_reaction", "LC1", "A0", "MY", -Pc * Lc, "N*m"),
])

# ---------------------------------------------------------------- V-LIN-005
# Interior point load on a single simply supported element.
Lp, Pp, a = 4.0, 20_000.0, 1.0
b = Lp - a
RA = Pp * b / Lp
write("V-LIN-005", "Simply supported beam, interior point load", ["F06", "F08", "F18"], model(
    "V-LIN-005 point load at L/4",
    [J("A", 0, 0, 0, PIN), J("B", Lp, 0, 0, ROL)],
    [{"name": "F1", "i": "A", "j": "B", "section": "B"}],
    [SEC], [{"name": "PL"}],
    frame_loads=[{"pattern": "PL", "frame": "F1", "load": {"kind": "Point", "direction": "Gravity", "rel": a / Lp, "value": Pp}}],
), {"kind": "analytical", "source": "Simply supported beam with a concentrated load",
    "derivation": ["theta_A = P a b (L + b)/(6 E I L)", "theta_B = P a b (L + a)/(6 E I L)", "R_A = P b / L", "M(x<=a) = R_A x", "M(x>=a) = R_A x - P (x - a)"]}, [
    exp("joint_displacement", "LC1", "A", "RY", Pp * a * b * (Lp + b) / (6 * E * I * Lp), "rad"),
    exp("joint_displacement", "LC1", "B", "RY", -Pp * a * b * (Lp + a) / (6 * E * I * Lp), "rad"),
    exp("joint_reaction", "LC1", "A", "FZ", RA, "N"),
    exp("joint_reaction", "LC1", "B", "FZ", Pp * a / Lp, "N"),
    exp("frame_station", "LC1", "F1", "M3", RA * a, "N*m", 0.25),
    exp("frame_station", "LC1", "F1", "M3", RA * 2.0 - Pp * (2.0 - a), "N*m", 0.5),
    exp("frame_station", "LC1", "F1", "V2", -RA, "N", 0.0),
    exp("frame_station", "LC1", "F1", "V2", -(RA - Pp), "N", 0.5),
])

# ---------------------------------------------------------------- mechanisms
write("V-MECH-001", "Free body: no supports", ["F08", "stability"], model(
    "V-MECH-001 free body",
    [J("1", 0, 0, 0), J("2", 3, 0, 0)],
    [{"name": "F1", "i": "1", "j": "2", "section": "B"}],
    [SEC], [{"name": "TIP"}],
    joint_loads=[{"pattern": "TIP", "joint": "2", "values": [0, 0, -P, 0, 0, 0]}],
), {"kind": "analytical", "source": "Unsupported 3D body has 6 rigid-body modes", "derivation": ["nullity(K) = 6"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 6 (exact)"}])

write("V-MECH-002", "Simply supported beam with torsion unrestrained at both ends", ["F08", "stability"], model(
    "V-MECH-002 torsional mechanism",
    [J("A", 0, 0, 0, [True, True, True, False, False, False]), J("B", 6, 0, 0, ROL)],
    [{"name": "F1", "i": "A", "j": "B", "section": "B"}],
    [SEC], [{"name": "LIVE"}],
    frame_loads=[{"pattern": "LIVE", "frame": "F1", "load": {"kind": "Uniform", "direction": "Gravity", "value": w}}],
), {"kind": "analytical", "source": "Rigid rotation about the member axis is unrestrained", "derivation": ["nullity(K) = 1 (RX)"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 1 (exact)"},
                     {"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": " RX"}])

# Multi-element free body: plain pivot counts are unreliable here (decision
# 0005); the verified null-space analysis must give the exact nullity (0006).
gj, gf = [], []
for k in range(3):
    for j in range(3):
        for i in range(3):
            gj.append(J(f"N{i}{j}{k}", 4.0 * i, 4.0 * j, 3.0 * k))
            if k > 0:
                gf.append({"name": f"C{i}{j}{k}", "i": f"N{i}{j}{k-1}", "j": f"N{i}{j}{k}", "section": "B"})
            if i > 0:
                gf.append({"name": f"X{i}{j}{k}", "i": f"N{i-1}{j}{k}", "j": f"N{i}{j}{k}", "section": "B"})
            if j > 0:
                gf.append({"name": f"Y{i}{j}{k}", "i": f"N{i}{j-1}{k}", "j": f"N{i}{j}{k}", "section": "B"})
write("V-MECH-003", "Unsupported 3D frame (27 joints, 54 bars)", ["F08", "stability"], model(
    "V-MECH-003 free 3D frame",
    gj, gf, [SEC], [{"name": "H"}],
    joint_loads=[{"pattern": "H", "joint": "N222", "values": [1000.0, 0, 0, 0, 0, 0]}],
), {"kind": "analytical", "source": "Unsupported 3D body has 6 rigid-body modes", "derivation": ["nullity(K) = 6"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 6 (exact)"}])

# Same frame held only in translation at one joint: free rotation about it.
gj4 = json.loads(json.dumps(gj))
gj4[0]["restraint"] = [True, True, True, False, False, False]
write("V-MECH-004", "3D frame held in translation at a single joint", ["F08", "stability"], model(
    "V-MECH-004 frame pinned at one joint",
    gj4, gf, [SEC], [{"name": "H"}],
    joint_loads=[{"pattern": "H", "joint": "N222", "values": [1000.0, 0, 0, 0, 0, 0]}],
), {"kind": "analytical", "source": "A rigid body fixed at one point keeps its three rotations", "derivation": ["nullity(K) = 3"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 3 (exact)"}])

# Supported cantilever plus an unconnected floating bar: local mechanism.
write("V-MECH-005", "Supported cantilever plus an unconnected floating bar", ["F08", "stability"], model(
    "V-MECH-005 floating bar",
    [J("1", 0, 0, 0, FIX), J("2", 3, 0, 0), J("3", 0, 2, 0), J("4", 3, 2, 0)],
    [{"name": "F1", "i": "1", "j": "2", "section": "B"}, {"name": "F2", "i": "3", "j": "4", "section": "B"}],
    [SEC], [{"name": "TIP"}],
    joint_loads=[{"pattern": "TIP", "joint": "2", "values": [0, 0, -P, 0, 0, 0]}],
), {"kind": "analytical", "source": "The floating bar has 6 rigid-body modes; the cantilever is stable", "derivation": ["nullity(K) = 6"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 6 (exact)"}])

# ---------------------------------------------------------------- Timoshenko
G = E / (2 * (1 + NU))
STIM = {"name": "T", "material": "STEEL", "area": 0.01, "i33": 1e-4, "i22": 2e-4, "j": 2e-4, "as2": 0.004, "as3": 0.006}
SEB = dict(STIM, name="TEB", as2=0.0, as3=0.0)
Pt = 20_000.0


def tip(L, I, As):
    """Timoshenko cantilever tip deflection and rotation, tip load Pt."""
    shear = Pt * L / (G * As) if As > 0 else 0.0
    return Pt * L**3 / (3 * E * I) + shear, Pt * L**2 / (2 * E * I)


exp_t = []
for jn, L, sec in [("S", 1.0, STIM), ("L", 10.0, STIM), ("E", 1.0, SEB)]:
    dz, rz = tip(L, sec["i33"], sec["as2"])   # -Z load: local 2, I33, As2
    dy, ry = tip(L, sec["i22"], sec["as3"])   # +Y load: local -3, I22, As3
    exp_t += [
        exp("joint_displacement", "VERT", jn + "1", "UZ", -dz, "m"),
        exp("joint_displacement", "VERT", jn + "1", "RY", rz, "rad"),
        exp("joint_displacement", "HORZ", jn + "1", "UY", dy, "m"),
        exp("joint_displacement", "HORZ", jn + "1", "RZ", ry, "rad"),
    ]
tim_joints, tim_frames, tim_jl = [], [], []
for k, (jn, L, sec) in enumerate([("S", 1.0, STIM), ("L", 10.0, STIM), ("E", 1.0, SEB)]):
    y0 = 3.0 * k
    tim_joints += [J(jn + "0", 0, y0, 0, FIX), J(jn + "1", L, y0, 0)]
    tim_frames.append({"name": "F" + jn, "i": jn + "0", "j": jn + "1", "section": sec["name"]})
    tim_jl += [{"pattern": "VERT", "joint": jn + "1", "values": [0, 0, -Pt, 0, 0, 0]},
               {"pattern": "HORZ", "joint": jn + "1", "values": [0, Pt, 0, 0, 0, 0]}]
write("V-TIM-001", "Timoshenko cantilevers: short, long, and As = 0", ["F02-003", "F08"], model(
    "V-TIM-001 Timoshenko cantilevers",
    tim_joints, tim_frames, [STIM, SEB], [{"name": "VERT"}, {"name": "HORZ"}],
    joint_loads=tim_jl,
    cases=[{"type": "LinearStatic", "name": "VERT", "loads": [{"pattern": "VERT", "scale": 1.0}]},
           {"type": "LinearStatic", "name": "HORZ", "loads": [{"pattern": "HORZ", "scale": 1.0}]}],
    schema=2,
), {"kind": "analytical", "source": "Timoshenko cantilever with tip load; G = E/(2(1+nu))",
    "derivation": ["delta = P L^3/(3 E I) + P L/(G As)", "theta = P L^2/(2 E I) (shear does not rotate the section)",
                   "As = 0: shear deformation neglected (Euler-Bernoulli)"]}, exp_t)


def fixed_fixed_timoshenko(L, EI, GAs, P, a, q, n=4000, kb=None):
    """Beam clamped at A, downward point load P at a and downward uniform q.
    End B has its transverse displacement fixed and its rotation: fixed
    (kb=None), free (kb=0, moment release) or joined to the support by a
    rotational spring kb > 0 (adds 1/kb to the moment flexibility).
    Virtual work with composite Simpson integration (independent of the Rust
    flexibility formulas). Returns reactions, M(x) and the deflection at a.
    Sign rule: sagging moment positive; an upward force F at s > x gives
    M(x) = F (s - x), V(x) = -F; a ccw couple C at the right end gives M = C."""
    def M0(x):
        m = -q * (L - x) ** 2 / 2
        if x < a:
            m += -P * (a - x)
        return m

    def V0(x):
        v = q * (L - x)
        if x < a:
            v += P
        return v
    m1, v1 = (lambda x: L - x), (lambda x: -1.0)      # unit upward force at B
    m2, v2 = (lambda x: 1.0), (lambda x: 0.0)          # unit ccw couple at B
    ma, va = (lambda x: (a - x) if x < a else 0.0), (lambda x: -1.0 if x < a else 0.0)  # unit upward at a

    def integ(f):
        tot = 0.0
        for lo, hi in [(0.0, a), (a, L)]:
            h = (hi - lo) / n
            s_ = f(lo + 1e-15) + f(hi - 1e-15)
            for i in range(1, n):
                s_ += (4 if i % 2 else 2) * f(lo + i * h)
            tot += s_ * h / 3
        return tot

    def w(Ma, Va, Mb, Vb):
        return integ(lambda x: Ma(x) * Mb(x) / EI + Va(x) * Vb(x) / GAs)
    d11, d12, d22 = w(m1, v1, m1, v1), w(m1, v1, m2, v2), w(m2, v2, m2, v2)
    d10, d20 = w(M0, V0, m1, v1), w(M0, V0, m2, v2)
    if kb == 0:
        X1, X2 = -d10 / d11, 0.0
    else:
        if kb is not None:
            d22 += 1.0 / kb
        det = d11 * d22 - d12 * d12
        X1 = -(d22 * d10 - d12 * d20) / det
        X2 = -(-d12 * d10 + d11 * d20) / det
    M = lambda x: M0(x) + X1 * m1(x) + X2 * m2(x)
    V = lambda x: V0(x) + X1 * v1(x) + X2 * v2(x)
    up_at_a = w(M, V, ma, va)                          # upward deflection at a
    RA = P + q * L - X1
    MA = P * a + q * L * L / 2 - X1 * L - X2           # ccw support moment at A
    return {"RA": RA, "MA": MA, "RB": X1, "MB": X2, "down_a": -up_at_a, "M": M}


Lf, af, Pf, qf = 2.0, 0.6, 50_000.0, 20_000.0
SFF = {"name": "D", "material": "STEEL", "area": 0.02, "i33": 2e-4, "i22": 1e-4, "j": 1e-4, "as2": 0.01, "as3": 0.01}
ref = fixed_fixed_timoshenko(Lf, E * SFF["i33"], G * SFF["as2"], Pf, af, qf)
ref_eb = fixed_fixed_timoshenko(Lf, E * SFF["i33"], 1e30, Pf, af, qf)
# Beam 1: one element with the point load in the span (tests fixed-end forces).
# Beam 2: two elements with a joint under the load (tests deflection).
ff_joints = [J("A1", 0, 0, 0, FIX), J("B1", Lf, 0, 0, FIX),
             J("A2", 0, 4, 0, FIX), J("C2", af, 4, 0), J("B2", Lf, 4, 0, FIX)]
ff_frames = [{"name": "G1", "i": "A1", "j": "B1", "section": "D"},
             {"name": "G2a", "i": "A2", "j": "C2", "section": "D"},
             {"name": "G2b", "i": "C2", "j": "B2", "section": "D"}]
ff_fl = [{"pattern": "L", "frame": "G1", "load": {"kind": "Point", "direction": "Gravity", "rel": af / Lf, "value": Pf}},
         {"pattern": "L", "frame": "G1", "load": {"kind": "Uniform", "direction": "Gravity", "value": qf}},
         {"pattern": "L", "frame": "G2a", "load": {"kind": "Uniform", "direction": "Gravity", "value": qf}},
         {"pattern": "L", "frame": "G2b", "load": {"kind": "Uniform", "direction": "Gravity", "value": qf}}]
exp_ff = []
for a_, b_ in [("A1", "B1"), ("A2", "B2")]:
    exp_ff += [exp("joint_reaction", "LC1", a_, "FZ", ref["RA"], "N"),
               exp("joint_reaction", "LC1", a_, "MY", -ref["MA"], "N*m"),
               exp("joint_reaction", "LC1", b_, "FZ", ref["RB"], "N"),
               exp("joint_reaction", "LC1", b_, "MY", -ref["MB"], "N*m")]
exp_ff.append(exp("joint_displacement", "LC1", "C2", "UZ", -ref["down_a"], "m"))
write("V-TIM-002", "Clamped-clamped deep beam (Phi ~ 0.16): point + uniform load", ["F02-003", "F06-003", "F06-004", "F08"], model(
    "V-TIM-002 clamped deep beam",
    ff_joints, ff_frames, [SFF], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "C2", "values": [0, 0, -Pf, 0, 0, 0]}],
    frame_loads=ff_fl, schema=2,
), {"kind": "analytical", "source": "Force method with virtual work (bending + shear), Simpson integration in tools/gen_cases.py",
    "derivation": ["redundants V_B, M_B from compatibility", "deflection at the load by the unit-load method",
                   f"Euler-Bernoulli would give R_A = {ref_eb['RA']:.3f} N, M_A = {ref_eb['MA']:.3f} N m, deflection {ref_eb['down_a']:.6e} m",
                   f"Timoshenko: R_A = {ref['RA']:.3f} N, M_A = {ref['MA']:.3f} N m, deflection {ref['down_a']:.6e} m"]}, exp_ff)

# ---------------------------------------------------------------- releases
Lr, ar, Pr, qr = 4.0, 1.5, 40_000.0, 10_000.0
SR = {"name": "R", "material": "STEEL", "area": 0.01, "i33": 1e-4, "i22": 1e-4, "j": 2e-4, "as2": 0.004, "as3": 0.004}
EIr, GAr = E * SR["i33"], G * SR["as2"]
kspr = 5.0e6   # N m / rad
rel_pin_j = {"j": [False, False, False, False, False, True]}
rel_spr_j = {"j": [False, False, False, False, False, True], "j_springs": [0, 0, 0, 0, 0, kspr]}
rel_ss = {"i": [False, False, False, False, False, True], "j": [False, False, False, False, False, True]}
refs = {"H1": fixed_fixed_timoshenko(Lr, EIr, GAr, Pr, ar, qr, kb=0),
        "H2": fixed_fixed_timoshenko(Lr, EIr, GAr, Pr, ar, qr, kb=kspr)}
# Simply supported by releasing M3 at both ends: statically determinate.
RB3 = (Pr * ar + qr * Lr * Lr / 2) / Lr
RA3 = Pr + qr * Lr - RB3
M3ss = lambda x: RA3 * x - qr * x * x / 2 - (Pr * (x - ar) if x > ar else 0.0)
refs["H3"] = {"RA": RA3, "MA": 0.0, "RB": RB3, "MB": 0.0, "M": M3ss}
rj, rf, rfl, exp_r = [], [], [], []
for k, (nm, rel) in enumerate([("H1", rel_pin_j), ("H2", rel_spr_j), ("H3", rel_ss)]):
    y0 = 3.0 * k
    rj += [J(nm + "A", 0, y0, 0, FIX), J(nm + "B", Lr, y0, 0, FIX)]
    rf.append({"name": nm, "i": nm + "A", "j": nm + "B", "section": "R", "releases": rel})
    rfl += [{"pattern": "L", "frame": nm, "load": {"kind": "Point", "direction": "Gravity", "rel": ar / Lr, "value": Pr}},
            {"pattern": "L", "frame": nm, "load": {"kind": "Uniform", "direction": "Gravity", "value": qr}}]
    r = refs[nm]
    exp_r += [exp("joint_reaction", "LC1", nm + "A", "FZ", r["RA"], "N"),
              exp("joint_reaction", "LC1", nm + "A", "MY", -r["MA"], "N*m"),
              exp("joint_reaction", "LC1", nm + "B", "FZ", r["RB"], "N"),
              exp("joint_reaction", "LC1", nm + "B", "MY", -r["MB"], "N*m")]
    for rel_ in (0.0, 0.5, 1.0):
        exp_r.append(exp("frame_station", "LC1", nm, "M3", r["M"](rel_ * Lr), "N*m", rel_))
write("V-REL-001", "End releases and partial fixity on beams between clamped joints", ["F04-003", "F02-003", "F06-003", "F06-004"], model(
    "V-REL-001 released beams",
    rj, rf, [SR], [{"name": "L"}], frame_loads=rfl, schema=2,
), {"kind": "analytical", "source": "Force method with virtual work (bending + shear); spring adds 1/k to the moment flexibility; simply supported case by statics",
    "derivation": ["H1: M3 released at J (fixed-pinned)", f"H2: M3 spring k = {kspr} N m/rad at J", "H3: M3 released at both ends (simply supported)"]}, exp_r)

# Two-bar truss made of frame members with releases.
Ct = (2.0, 0.0, 3.0)
Ft = (10_000.0, 0.0, -50_000.0)
L13 = math.sqrt(13.0)
e1, e2 = (2 / L13, 0.0, 3 / L13), (-2 / L13, 0.0, 3 / L13)
# N1 e1 + N2 e2 = F (x, z):  [[e1x, e2x], [e1z, e2z]] [N1, N2] = [Fx, Fz]
det_t = e1[0] * e2[2] - e2[0] * e1[2]
N1 = (Ft[0] * e2[2] - e2[0] * Ft[2]) / det_t
N2 = (e1[0] * Ft[2] - Ft[0] * e1[2]) / det_t
At = 0.002
def unit_forces(fx, fz):
    return ((fx * e2[2] - e2[0] * fz) / det_t, (e1[0] * fz - fx * e1[2]) / det_t)
n1x, n2x = unit_forces(1.0, 0.0)
n1z, n2z = unit_forces(0.0, 1.0)
ux = (N1 * n1x + N2 * n2x) * L13 / (E * At)
uz = (N1 * n1z + N2 * n2z) * L13 / (E * At)
ST = {"name": "TR", "material": "STEEL", "area": At, "i33": 1e-6, "i22": 1e-6, "j": 1e-6}
truss_rel = {"i": [False, False, False, True, True, True], "j": [False, False, False, False, True, True]}
write("V-REL-002", "Two-bar truss built from frame members with releases", ["F04-003", "F08"], model(
    "V-REL-002 released truss",
    [J("A", 0, 0, 0, FIX), J("B", 4, 0, 0, FIX), J("C", *Ct, [False, False, False, True, True, True])],
    [{"name": "T1", "i": "A", "j": "C", "section": "TR", "releases": truss_rel},
     {"name": "T2", "i": "B", "j": "C", "section": "TR", "releases": truss_rel}],
    [ST], [{"name": "F"}],
    joint_loads=[{"pattern": "F", "joint": "C", "values": [Ft[0], 0, Ft[2], 0, 0, 0]}], schema=2,
), {"kind": "analytical", "source": "Joint equilibrium and virtual work for a pin-jointed truss",
    "derivation": ["N1 e1 + N2 e2 = F at the apex", "u = sum N n L / (E A)", "moments released: zero end moments and zero support moments"]}, [
    exp("frame_station", "LC1", "T1", "P", N1, "N", 0.5),
    exp("frame_station", "LC1", "T2", "P", N2, "N", 0.5),
    exp("frame_station", "LC1", "T1", "M3", 0.0, "N*m", 0.0),
    exp("frame_station", "LC1", "T1", "M2", 0.0, "N*m", 0.5),
    exp("joint_displacement", "LC1", "C", "UX", ux, "m"),
    exp("joint_displacement", "LC1", "C", "UZ", uz, "m"),
    exp("joint_reaction", "LC1", "A", "FX", -N1 * e1[0], "N"),
    exp("joint_reaction", "LC1", "A", "FZ", -N1 * e1[2], "N"),
    exp("joint_reaction", "LC1", "B", "FZ", -N2 * e2[2], "N"),
    exp("joint_reaction", "LC1", "A", "MY", 0.0, "N*m"),
    exp("joint_reaction", "LC1", "B", "MY", 0.0, "N*m"),
    exp("joint_reaction", "LC1", "C", "MY", 0.0, "N*m"),
])

# Cantilever pinned at its support by a release: mechanism of nullity 1.
write("V-MECH-006", "Cantilever with M3 released at the clamped end", ["F04-003", "F08", "stability"], model(
    "V-MECH-006 released cantilever",
    [J("1", 0, 0, 0, FIX), J("2", 3, 0, 0)],
    [{"name": "F1", "i": "1", "j": "2", "section": "B", "releases": {"i": [False, False, False, False, False, True]}}],
    [SEC], [{"name": "TIP"}],
    joint_loads=[{"pattern": "TIP", "joint": "2", "values": [0, 0, -P, 0, 0, 0]}], schema=2,
), {"kind": "analytical", "source": "A bar pinned at one end rotates freely in the 1-2 plane", "derivation": ["nullity(K) = 1"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 1 (exact)"},
                     {"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "moves 1/2 joints"}])

# ---------------------------------------------------------------- offsets
Lo, ao, eo, Po = 3.0, 0.5, 0.4, 10_000.0
EIo = E * I
SOS = dict(SR, name="RS")


def lin(a, b):
    return tuple(p + q for p, q in zip(a, b))


def offset_tip(EI, GAs=None):
    """Cantilever of flexible length Lo, then a rigid arm ao carrying Po down.
    The element end sees force Po and moment Po ao. Returns the joint deflection
    (magnitude) and rotation."""
    shear = Po * Lo / GAs if GAs else 0.0
    d_end = Po * Lo**3 / (3 * EI) + Po * ao * Lo**2 / (2 * EI) + shear
    th = Po * Lo**2 / (2 * EI) + Po * ao * Lo / EI
    return d_end + th * ao, th


dA, thA = offset_tip(EIo)
dS, thS = offset_tip(EIo, G * SOS["as2"])
co = 1 / math.sqrt(2)
# Eccentric axial load. Both joints lie on the X axis and the element runs
# parallel to it, eo above. The end J sees the axial force and My = -eo Po.
uxE = Po * Lo / (E * A) + eo**2 * Po * Lo / EIo
uzE = eo * Po * Lo**2 / (2 * EIo)
ryE = -eo * Po * Lo / EIo
off_cases = [{"type": "LinearStatic", "name": n, "loads": [{"pattern": n, "scale": 1.0}]} for n in ["TIP", "ROT", "TIM", "ECC"]]
write("V-OFF-001", "Rigid end offsets: tip arm, plan rotation, shear, eccentric axial load", ["F04-004", "F02-003", "F08", "F18"], model(
    "V-OFF-001 rigid end offsets",
    [J("T0", 0, 0, 0, FIX), J("T1", Lo + ao, 0, 0),
     J("R0", 0, 8, 0, FIX), J("R1", (Lo + ao) * co, 8 + (Lo + ao) * co, 0),
     J("S0", 0, 16, 0, FIX), J("S1", Lo + ao, 16, 0),
     J("E0", 0, 24, 0, FIX), J("E1", Lo, 24, 0)],
    [{"name": "FT", "i": "T0", "j": "T1", "section": "B", "offsets": {"j": [-ao, 0, 0]}},
     {"name": "FR", "i": "R0", "j": "R1", "section": "B", "offsets": {"j": [-ao * co, -ao * co, 0]}},
     {"name": "FS", "i": "S0", "j": "S1", "section": "RS", "offsets": {"j": [-ao, 0, 0]}},
     {"name": "FE", "i": "E0", "j": "E1", "section": "B", "offsets": {"i": [0, 0, eo], "j": [0, 0, eo]}}],
    [SEC, SOS], [{"name": n} for n in ["TIP", "ROT", "TIM", "ECC"]],
    joint_loads=[{"pattern": "TIP", "joint": "T1", "values": [0, 0, -Po, 0, 0, 0]},
                 {"pattern": "ROT", "joint": "R1", "values": [0, 0, -Po, 0, 0, 0]},
                 {"pattern": "TIM", "joint": "S1", "values": [0, 0, -Po, 0, 0, 0]},
                 {"pattern": "ECC", "joint": "E1", "values": [Po, 0, 0, 0, 0, 0]}],
    cases=off_cases, schema=3,
), {"kind": "analytical", "source": "Cantilever under end force and moment, plus rigid-arm kinematics u_joint = u_end + theta x r",
    "derivation": ["d_end = P L^3/(3 E I) + P a L^2/(2 E I) [+ P L/(G As)]", "theta = P L^2/(2 E I) + P a L/(E I)", "d_joint = d_end + theta a",
                   "eccentric: N = P, My = -e P, u_joint = P L/(E A) + e^2 P L/(E I), and the support moment about the joint line is 0"]}, [
    exp("joint_displacement", "TIP", "T1", "UZ", -dA, "m"),
    exp("joint_displacement", "TIP", "T1", "RY", thA, "rad"),
    exp("joint_reaction", "TIP", "T0", "FZ", Po, "N"),
    exp("joint_reaction", "TIP", "T0", "MY", -Po * (Lo + ao), "N*m"),
    exp("frame_station", "TIP", "FT", "M3", -Po * (Lo + ao), "N*m", 0.0),
    exp("frame_station", "TIP", "FT", "M3", -Po * (Lo / 2 + ao), "N*m", 0.5),
    exp("frame_station", "TIP", "FT", "M3", -Po * ao, "N*m", 1.0),
    exp("frame_station", "TIP", "FT", "V2", -Po, "N", 0.5),
    exp("external_work", "TIP", "", "", Po * dA / 2, "J"),
    exp("joint_displacement", "ROT", "R1", "UZ", -dA, "m"),
    exp("joint_displacement", "ROT", "R1", "RX", -thA * co, "rad"),
    exp("joint_displacement", "ROT", "R1", "RY", thA * co, "rad"),
    exp("joint_reaction", "ROT", "R0", "MX", Po * (Lo + ao) * co, "N*m"),
    exp("joint_reaction", "ROT", "R0", "MY", -Po * (Lo + ao) * co, "N*m"),
    exp("joint_displacement", "TIM", "S1", "UZ", -dS, "m"),
    exp("joint_displacement", "TIM", "S1", "RY", thS, "rad"),
    exp("joint_displacement", "ECC", "E1", "UX", uxE, "m"),
    exp("joint_displacement", "ECC", "E1", "UZ", uzE, "m"),
    exp("joint_displacement", "ECC", "E1", "RY", ryE, "rad"),
    exp("joint_reaction", "ECC", "E0", "FX", -Po, "N"),
    exp("joint_reaction", "ECC", "E0", "MY", 0.0, "N*m"),
    exp("joint_reaction", "ECC", "E0", "FZ", 0.0, "N"),
    exp("frame_station", "ECC", "FE", "P", Po, "N", 0.5),
    exp("frame_station", "ECC", "FE", "M3", eo * Po, "N*m", 0.0),
    exp("frame_station", "ECC", "FE", "M3", eo * Po, "N*m", 1.0),
    exp("frame_station", "ECC", "FE", "V2", 0.0, "N", 0.5),
    exp("external_work", "ECC", "", "", Po * uxE / 2, "J"),
])

# Span loads act on the flexible length only, and the joints take the arms.
Lc2, ac2, qc2 = 4.0, 0.6, 12_000.0
Rc2 = qc2 * Lc2 / 2
Mf2 = qc2 * Lc2**2 / 12
off2 = {"i": [ac2, 0, 0], "j": [-ac2, 0, 0]}
write("V-OFF-002", "Offsets with span load: clamped and simply supported flexible length", ["F04-004", "F04-003", "F06-003"], model(
    "V-OFF-002 offsets and releases under uniform load",
    [J("A", 0, 0, 0, FIX), J("B", Lc2 + 2 * ac2, 0, 0, FIX), J("C", 0, 5, 0, FIX), J("D", Lc2 + 2 * ac2, 5, 0, FIX)],
    [{"name": "G1", "i": "A", "j": "B", "section": "B", "offsets": off2},
     {"name": "G2", "i": "C", "j": "D", "section": "B", "offsets": off2,
      "releases": {"i": [False, False, False, False, False, True], "j": [False, False, False, False, False, True]}}],
    [SEC], [{"name": "Q"}],
    frame_loads=[{"pattern": "Q", "frame": f, "load": {"kind": "Uniform", "direction": "Gravity", "value": qc2}} for f in ["G1", "G2"]],
    schema=3,
), {"kind": "analytical", "source": "Fixed-end and simply supported beam of length L, with rigid arms a to the supports",
    "derivation": ["R = q L/2", "clamped: M_end = q L^2/12, M_mid = q L^2/24, M_support = M_end + R a",
                   "released: M_end = 0, M_mid = q L^2/8, M_support = R a"]}, [
    exp("joint_reaction", "LC1", "A", "FZ", Rc2, "N"),
    exp("joint_reaction", "LC1", "A", "MY", -(Mf2 + Rc2 * ac2), "N*m"),
    exp("joint_reaction", "LC1", "B", "MY", Mf2 + Rc2 * ac2, "N*m"),
    exp("frame_station", "LC1", "G1", "M3", -Mf2, "N*m", 0.0),
    exp("frame_station", "LC1", "G1", "M3", qc2 * Lc2**2 / 24, "N*m", 0.5),
    exp("joint_reaction", "LC1", "C", "FZ", Rc2, "N"),
    exp("joint_reaction", "LC1", "C", "MY", -Rc2 * ac2, "N*m"),
    exp("joint_reaction", "LC1", "D", "MY", Rc2 * ac2, "N*m"),
    exp("frame_station", "LC1", "G2", "M3", 0.0, "N*m", 0.0),
    exp("frame_station", "LC1", "G2", "M3", qc2 * Lc2**2 / 8, "N*m", 0.5),
])

# ---------------------------------------------------------------- diaphragms


def solve3(a, b):
    """Gauss elimination with partial pivoting for a 3x3 system."""
    m = [list(r) + [v] for r, v in zip(a, b)]
    for i in range(3):
        p = max(range(i, 3), key=lambda r: abs(m[r][i]))
        m[i], m[p] = m[p], m[i]
        for r in range(i + 1, 3):
            f = m[r][i] / m[i][i]
            m[r] = [x - f * y for x, y in zip(m[r], m[i])]
    x = [0.0] * 3
    for i in (2, 1, 0):
        x[i] = (m[i][3] - sum(m[i][k] * x[k] for k in range(i + 1, 3))) / m[i][i]
    return x


def rigid_floor(cols, loads):
    """Floor on independent columns. `cols`: (a, b, ka, kb, kt) about the
    reference, loads: (a, b, Fa, Fb, Mn). Unknowns (ua, ub, th) with
    u_a(col) = ua - th b, u_b(col) = ub + th a; the matrix comes from the strain
    energy of the columns, the load vector from virtual work."""
    K = [[0.0] * 3 for _ in range(3)]
    for a_, b_, ka, kb, kt in cols:
        K[0][0] += ka
        K[0][2] += -ka * b_
        K[1][1] += kb
        K[1][2] += kb * a_
        K[2][2] += ka * b_**2 + kb * a_**2 + kt
    K[2][0], K[2][1] = K[0][2], K[1][2]
    F = [0.0] * 3
    for a_, b_, fa, fb, mn in loads:
        F[0] += fa
        F[1] += fb
        F[2] += mn - b_ * fa + a_ * fb
    return solve3(K, F)


Hd = 3.0
C1 = {"name": "C1", "material": "STEEL", "area": 0.02, "i33": 2e-4, "i22": 1e-4, "j": 1e-4}
C2 = {"name": "C2", "material": "STEEL", "area": 0.02, "i33": 1e-4, "i22": 3e-4, "j": 2e-4}
# Reference joint M is not over the stiffness center and has no elements. Its
# UZ, RX and RY are not tied by the diaphragm and have no stiffness: restrained.
xm, ym = 2.5, 1.5
cols1 = [("1", 0.0, 0.0, C1), ("2", 6.0, 0.0, C2), ("3", 6.0, 4.0, C1), ("4", 0.0, 4.0, C2)]
Fx1, Fy3, Mz1 = 80_000.0, -30_000.0, 50_000.0
# Vertical member: local 2 = +X, 3 = +Y, so UX bends with I33 and UY with I22.
props1 = [(x - xm, y - ym, 3 * E * s["i33"] / Hd**3, 3 * E * s["i22"] / Hd**3, G * s["j"] / Hd) for _, x, y, s in cols1]
ux1, uy1, th1 = rigid_floor(props1, [(0.0, 0.0, Fx1, 0.0, Mz1), (6.0 - xm, 4.0 - ym, 0.0, Fy3, 0.0)])
exp_d1 = [exp("joint_displacement", "LC1", "M", "UX", ux1, "m"),
          exp("joint_displacement", "LC1", "M", "UY", uy1, "m"),
          exp("joint_displacement", "LC1", "M", "RZ", th1, "rad")]
work1 = 0.0
for (nm, x, y, s), (a_, b_, ka, kb, kt) in zip(cols1, props1):
    ua, ub = ux1 - th1 * b_, uy1 + th1 * a_
    Vx, Vy = ka * ua, kb * ub
    exp_d1 += [exp("joint_displacement", "LC1", "T" + nm, "UX", ua, "m"),
               exp("joint_displacement", "LC1", "T" + nm, "UY", ub, "m"),
               exp("joint_displacement", "LC1", "T" + nm, "RZ", th1, "rad"),
               exp("joint_displacement", "LC1", "T" + nm, "UZ", 0.0, "m"),
               exp("joint_displacement", "LC1", "T" + nm, "RY", Vx * Hd**2 / (2 * E * s["i33"]), "rad"),
               exp("joint_displacement", "LC1", "T" + nm, "RX", -Vy * Hd**2 / (2 * E * s["i22"]), "rad"),
               exp("joint_reaction", "LC1", "B" + nm, "FX", -Vx, "N"),
               exp("joint_reaction", "LC1", "B" + nm, "FY", -Vy, "N"),
               exp("joint_reaction", "LC1", "B" + nm, "MY", -Vx * Hd, "N*m"),
               exp("joint_reaction", "LC1", "B" + nm, "MX", Vy * Hd, "N*m"),
               exp("joint_reaction", "LC1", "B" + nm, "MZ", -kt * th1, "N*m")]
work1 = 0.5 * (Fx1 * ux1 + Mz1 * th1 + Fy3 * (uy1 + th1 * (6.0 - xm)))
exp_d1.append(exp("external_work", "LC1", "", "", work1, "J"))
write("V-DIA-001", "Rigid floor on unequal cantilever columns: eccentric plan loads", ["F04-005", "F08"], model(
    "V-DIA-001 rigid floor on four columns",
    [J("M", xm, ym, Hd, [False, False, True, True, True, False])]
    + [J("B" + nm, x, y, 0, FIX) for nm, x, y, _ in cols1] + [J("T" + nm, x, y, Hd) for nm, x, y, _ in cols1],
    [{"name": "C" + nm, "i": "B" + nm, "j": "T" + nm, "section": s["name"]} for nm, _, _, s in cols1],
    [C1, C2], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "M", "values": [Fx1, 0, 0, 0, 0, Mz1]},
                 {"pattern": "L", "joint": "T3", "values": [0, Fy3, 0, 0, 0, 0]}],
    schema=3,
    constraints=[{"kind": "Diaphragm", "name": "D1", "axis": "Z", "joints": ["M", "T1", "T2", "T3", "T4"]}],
), {"kind": "analytical", "source": "Three-DOF rigid floor from the strain energy of cantilever columns (3 E I / H^3, G J / H), solved in tools/gen_cases.py",
    "derivation": ["u_col = u - theta (y - ym), v_col = v + theta (x - xm)", "K q = F, with F from virtual work",
                   "base reactions from cantilever statics", "top RX and RY are free: not tied by the diaphragm"]}, exp_d1)

# Shear building: floor RX and RY restrained, so columns bend in double curvature.
xp, yp = 3.0, 2.0
plan = [("1", -xp, -yp), ("2", xp, -yp), ("3", xp, yp), ("4", -xp, yp)]
kx2, ky2, kt2 = 12 * E * C1["i33"] / Hd**3, 12 * E * C1["i22"] / Hd**3, G * C1["j"] / Hd
Kx2, Ky2 = 4 * kx2, 4 * ky2
Kth2 = sum(kx2 * b_**2 + ky2 * a_**2 + kt2 for _, a_, b_ in plan)
Fa, Fb = 60_000.0, 45_000.0
Ttop = -(-yp) * Fa            # F = Fa along X at floor 2, corner 1 (r = (-xp, -yp))
Tlow = xp * Fb                # F = Fb along Y at floor 1, corner 3 (r = (xp, yp))
ux_f1, uy_f1, th_f1 = Fa / Kx2, Fb / Ky2, (Tlow + Ttop) / Kth2
ux_f2, uy_f2, th_f2 = ux_f1 + Fa / Kx2, uy_f1, th_f1 + Ttop / Kth2
FLOOR = [False, False, False, True, True, False]
jj = [J(f"G{n}", xp + a_, yp + b_, 0, FIX) for n, a_, b_ in plan]
ff2, exp_d2 = [], []
for lev, (ux0, uy0, th0, z) in {"A": (ux_f1, uy_f1, th_f1, Hd), "B": (ux_f2, uy_f2, th_f2, 2 * Hd)}.items():
    below = "G" if lev == "A" else "A"
    for n, a_, b_ in plan:
        jj.append(J(f"{lev}{n}", xp + a_, yp + b_, z, FLOOR))
        ff2.append({"name": f"C{lev}{n}", "i": f"{below}{n}", "j": f"{lev}{n}", "section": "C1"})
        exp_d2 += [exp("joint_displacement", "LC1", f"{lev}{n}", "UX", ux0 - th0 * b_, "m"),
                   exp("joint_displacement", "LC1", f"{lev}{n}", "UY", uy0 + th0 * a_, "m"),
                   exp("joint_displacement", "LC1", f"{lev}{n}", "RZ", th0, "rad"),
                   exp("joint_displacement", "LC1", f"{lev}{n}", "UZ", 0.0, "m")]
for n, a_, b_ in plan:
    Vx, Vy = kx2 * (ux_f1 - th_f1 * b_), ky2 * (uy_f1 + th_f1 * a_)
    exp_d2 += [exp("joint_reaction", "LC1", f"G{n}", "FX", -Vx, "N"),
               exp("joint_reaction", "LC1", f"G{n}", "FY", -Vy, "N"),
               exp("joint_reaction", "LC1", f"G{n}", "MY", -Vx * Hd / 2, "N*m"),
               exp("joint_reaction", "LC1", f"G{n}", "MX", Vy * Hd / 2, "N*m"),
               exp("joint_reaction", "LC1", f"G{n}", "MZ", -kt2 * th_f1, "N*m")]
write("V-DIA-002", "Two-storey shear building with eccentric loads on corner joints", ["F04-005", "F08"], model(
    "V-DIA-002 two-storey shear building",
    jj, ff2, [C1], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "B1", "values": [Fa, 0, 0, 0, 0, 0]},
                 {"pattern": "L", "joint": "A3", "values": [0, Fb, 0, 0, 0, 0]}],
    schema=3,
    constraints=[{"kind": "Diaphragm", "name": "FA", "joints": [f"A{n}" for n, _, _ in plan]},
                 {"kind": "Diaphragm", "name": "FB", "joints": [f"B{n}" for n, _, _ in plan]}],
), {"kind": "analytical", "source": "Shear building: storey shear and torque over storey stiffness (12 E I / H^3, G J / H), about the plan center",
    "derivation": ["drift = storey shear / sum(k)", "storey twist = storey torque / sum(k r^2 + G J / H)",
                   "the plan is doubly symmetric, so translation and twist are uncoupled about its center", "base moment = V H / 2"]}, exp_d2)

# Same formula for planes normal to X and Y: (a, b, n) is right-handed.
SQ = {"name": "SQ", "material": "STEEL", "area": 0.02, "i33": 2e-4, "i22": 2e-4, "j": 1e-4}
kq, ktq = 3 * E * SQ["i33"] / Hd**3, G * SQ["j"] / Hd
pts3 = [(0.0, 0.0), (5.0, 0.0), (5.0, 2.0), (0.0, 3.0)]
Fa3, Mn3 = 40_000.0, 25_000.0
ua3, ub3, th3 = rigid_floor([(a_, b_, kq, kq, ktq) for a_, b_ in pts3], [(0.0, 0.0, Fa3, 0.0, 0.0), (5.0, 2.0, 0.0, 0.0, Mn3)])
jj3, ff3, jl3, con3, exp_d3 = [], [], [], [], []
AX = "XYZ"
for n_, shift in [(0, 0.0), (1, 40.0)]:
    a_i, b_i = (n_ + 1) % 3, (n_ + 2) % 3
    names = []
    for k_, (a_, b_) in enumerate(pts3):
        base, top = [shift, shift, shift], [shift, shift, shift]
        base[a_i] += a_
        base[b_i] += b_
        top[a_i] += a_
        top[b_i] += b_
        top[n_] += Hd
        tag = AX[n_]
        jj3 += [J(f"{tag}B{k_}", *base, FIX), J(f"{tag}T{k_}", *top)]
        ff3.append({"name": f"{tag}C{k_}", "i": f"{tag}B{k_}", "j": f"{tag}T{k_}", "section": "SQ"})
        names.append(f"{tag}T{k_}")
        ua_, ub_ = ua3 - th3 * b_, ub3 + th3 * a_
        exp_d3 += [exp("joint_displacement", "LC1", f"{tag}T{k_}", "U" + AX[a_i], ua_, "m"),
                   exp("joint_displacement", "LC1", f"{tag}T{k_}", "U" + AX[b_i], ub_, "m"),
                   exp("joint_displacement", "LC1", f"{tag}T{k_}", "R" + AX[n_], th3, "rad"),
                   exp("joint_displacement", "LC1", f"{tag}T{k_}", "U" + AX[n_], 0.0, "m"),
                   exp("joint_reaction", "LC1", f"{tag}B{k_}", "F" + AX[a_i], -kq * ua_, "N"),
                   exp("joint_reaction", "LC1", f"{tag}B{k_}", "F" + AX[b_i], -kq * ub_, "N"),
                   exp("joint_reaction", "LC1", f"{tag}B{k_}", "M" + AX[n_], -ktq * th3, "N*m")]
    con3.append({"kind": "Diaphragm", "name": f"D{AX[n_]}", "axis": AX[n_], "joints": names})
    fv = [0.0] * 6
    fv[a_i] = Fa3
    mv = [0.0] * 6
    mv[3 + n_] = Mn3
    jl3 += [{"pattern": "L", "joint": names[0], "values": fv}, {"pattern": "L", "joint": names[2], "values": mv}]
write("V-DIA-003", "Rigid floors normal to X and to Y: same closed form as the Z case", ["F04-005"], model(
    "V-DIA-003 diaphragms normal to X and Y",
    jj3, ff3, [SQ], [{"name": "L"}], joint_loads=jl3, schema=3, constraints=con3,
), {"kind": "analytical", "source": "Three-DOF rigid floor with axes (a, b, n) right-handed; square section, so local axes do not matter",
    "derivation": ["u_a = ua - theta b, u_b = ub + theta a", "k = 3 E I / H^3, kt = G J / H", "reference is the first joint, with a = b = 0"]}, exp_d3)

# Columns pinned at the base: the floor can sway in both plan directions, but
# torsion of the columns holds its rotation. Each column keeps a rigid rotation.
write("V-MECH-007", "Rigid floor on columns pinned at the base: sway mechanism", ["F04-005", "F04-003", "F08", "stability"], model(
    "V-MECH-007 sway mechanism under a diaphragm",
    [J("B" + nm, x, y, 0, FIX) for nm, x, y, _ in cols1] + [J("T" + nm, x, y, Hd) for nm, x, y, _ in cols1],
    [{"name": "C" + nm, "i": "B" + nm, "j": "T" + nm, "section": "C1",
      "releases": {"i": [False, False, False, False, True, True]}} for nm, _, _, _ in cols1],
    [C1], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "T1", "values": [Fx1, 0, 0, 0, 0, 0]}],
    schema=3,
    constraints=[{"kind": "Diaphragm", "name": "D1", "joints": ["T1", "T2", "T3", "T4"]}],
), {"kind": "analytical", "source": "Pinned-base bars rotate rigidly; the floor translates in X and Y, its rotation is held by column torsion",
    "derivation": ["nullity(K) = 2"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 2 (exact)"}])

print("cases written to", ROOT)
