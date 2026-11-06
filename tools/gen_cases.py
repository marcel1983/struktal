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
          combos=(), units=None, materials=None, schema=1, constraints=(), imposed=()):
    # Cases created before schema v2 stay at v1 on purpose: they exercise the
    # v1 -> v2 migration end to end (decision 0007).
    extra = {"constraints": list(constraints)} if constraints else {}
    if imposed:
        extra["joint_displacements"] = list(imposed)
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
off_cases = [{"type": "LinearStatic", "name": n, "loads": [{"pattern": n, "scale": 1.0}]} for n in ["TIP", "ROT", "ECC"]]
write("V-OFF-001", "Rigid end offsets: tip arm, plan rotation, eccentric axial load", ["F04-004", "F08", "F18"], model(
    "V-OFF-001 rigid end offsets",
    [J("T0", 0, 0, 0, FIX), J("T1", Lo + ao, 0, 0),
     J("R0", 0, 8, 0, FIX), J("R1", (Lo + ao) * co, 8 + (Lo + ao) * co, 0),
     J("E0", 0, 24, 0, FIX), J("E1", Lo, 24, 0)],
    [{"name": "FT", "i": "T0", "j": "T1", "section": "B", "offsets": {"j": [-ao, 0, 0]}},
     {"name": "FR", "i": "R0", "j": "R1", "section": "B", "offsets": {"j": [-ao * co, -ao * co, 0]}},
     {"name": "FE", "i": "E0", "j": "E1", "section": "B", "offsets": {"i": [0, 0, eo], "j": [0, 0, eo]}}],
    [SEC], [{"name": n} for n in ["TIP", "ROT", "ECC"]],
    joint_loads=[{"pattern": "TIP", "joint": "T1", "values": [0, 0, -Po, 0, 0, 0]},
                 {"pattern": "ROT", "joint": "R1", "values": [0, 0, -Po, 0, 0, 0]},
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

# Apart from V-OFF-001 because OpenSees' Timoshenko element ignores the arm of
# its joint offsets, and one such bar would leave the whole model not comparable.
write("V-OFF-003", "Rigid end offset with shear deformation", ["F04-004", "F02-003"], model(
    "V-OFF-003 offset on a shear-deformable bar",
    [J("S0", 0, 0, 0, FIX), J("S1", Lo + ao, 0, 0)],
    [{"name": "FS", "i": "S0", "j": "S1", "section": "RS", "offsets": {"j": [-ao, 0, 0]}}],
    [SOS], [{"name": "TIM"}],
    joint_loads=[{"pattern": "TIM", "joint": "S1", "values": [0, 0, -Po, 0, 0, 0]}],
    schema=3,
), {"kind": "analytical", "source": "Same as V-OFF-001 with the shear term of the cantilever",
    "derivation": ["d_end = P L^3/(3 E I) + P a L^2/(2 E I) + P L/(G As)", "theta = P L^2/(2 E I) + P a L/(E I)", "d_joint = d_end + theta a"]}, [
    exp("joint_displacement", "LC1", "S1", "UZ", -dS, "m"),
    exp("joint_displacement", "LC1", "S1", "RY", thS, "rad"),
    exp("joint_reaction", "LC1", "S0", "FZ", Po, "N"),
    exp("joint_reaction", "LC1", "S0", "MY", -Po * (Lo + ao), "N*m"),
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

# ---------------------------------------------------------------- body
Lb = 4.0
SBD = {"name": "BD", "material": "STEEL", "area": 0.012, "i33": 3e-4, "i22": 1e-4, "j": 2e-4}
EI3, EI2, GJb, EAb = E * SBD["i33"], E * SBD["i22"], G * SBD["j"], E * SBD["area"]


def vcross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def tip_of_x_cantilever(F, M):
    """Tip of a cantilever along +X under an end force and moment. Local 2 = +Z,
    so Z bending uses I33 and Y bending I22 (Euler-Bernoulli)."""
    u = (F[0] * Lb / EAb,
         F[1] * Lb**3 / (3 * EI2) + M[2] * Lb**2 / (2 * EI2),
         F[2] * Lb**3 / (3 * EI3) - M[1] * Lb**2 / (2 * EI3))
    th = (M[0] * Lb / GJb,
          -F[2] * Lb**2 / (2 * EI3) + M[1] * Lb / EI3,
          F[1] * Lb**2 / (2 * EI2) + M[2] * Lb / EI2)
    return u, th


Fb3 = (5_000.0, -8_000.0, -12_000.0)
rP = (0.7, 1.1, -0.5)   # loaded joint P, from the tip, not on any element
rQ = (-0.4, 0.3, 0.9)   # unloaded joint Q, kinematics only
ub, thb = tip_of_x_cantilever(Fb3, vcross(rP, Fb3))
uP = lin(ub, vcross(thb, rP))
uQ = lin(ub, vcross(thb, rQ))
at_base = lin((Lb, 0.0, 0.0), rP)
Mbase = vcross(at_base, Fb3)
exp_b = [exp("external_work", "LC1", "", "", 0.5 * sum(f * v for f, v in zip(Fb3, uP)), "J")]
for nm, uu in [("T", ub), ("P", uP), ("Q", uQ)]:
    for c_, v_ in zip(["UX", "UY", "UZ"], uu):
        exp_b.append(exp("joint_displacement", "LC1", nm, c_, v_, "m"))
    for c_, v_ in zip(["RX", "RY", "RZ"], thb):
        exp_b.append(exp("joint_displacement", "LC1", nm, c_, v_, "rad"))
for c_, v_ in zip(["FX", "FY", "FZ"], Fb3):
    exp_b.append(exp("joint_reaction", "LC1", "A", c_, -v_, "N"))
for c_, v_ in zip(["MX", "MY", "MZ"], Mbase):
    exp_b.append(exp("joint_reaction", "LC1", "A", c_, -v_, "N*m"))
write("V-BODY-001", "Body constraint: oblique load on a joint rigidly tied to a cantilever tip", ["F04-005", "F08"], model(
    "V-BODY-001 rigid body at a cantilever tip",
    [J("A", 0, 0, 0, FIX), J("T", Lb, 0, 0), J("P", *lin((Lb, 0, 0), rP)), J("Q", *lin((Lb, 0, 0), rQ))],
    [{"name": "F1", "i": "A", "j": "T", "section": "BD"}],
    [SBD], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "P", "values": [*Fb3, 0, 0, 0]}],
    schema=3,
    constraints=[{"kind": "Body", "name": "B1", "joints": ["T", "P", "Q"]}],
), {"kind": "analytical", "source": "Cantilever flexibility under the load moved to the tip, plus rigid-body kinematics",
    "derivation": ["M_tip = r x F", "u_tip, theta from the cantilever formulas in each plane and torsion", "u_joint = u_tip + theta x r",
                   "base reaction = -(F, x_P x F)"]}, exp_b)

# ---------------------------------------------------------------- equal
Le1, Le2, Pe3 = 2.0, 3.0, 30_000.0
ke1, ke2 = 3 * E * I / Le1**3, 3 * E * I / Le2**3
de = Pe3 / (ke1 + ke2)
# Two cantilevers meeting at coincident joints R and S: only UZ is tied, so
# each tip keeps its own rotation and shares the load by stiffness.
exp_e = [exp("joint_displacement", "LC1", "R", "UZ", -de, "m"),
         exp("joint_displacement", "LC1", "S", "UZ", -de, "m"),
         exp("joint_displacement", "LC1", "R", "RY", ke1 * de * Le1**2 / (2 * E * I), "rad"),
         exp("joint_displacement", "LC1", "S", "RY", -ke2 * de * Le2**2 / (2 * E * I), "rad"),
         exp("joint_reaction", "LC1", "A", "FZ", ke1 * de, "N"),
         exp("joint_reaction", "LC1", "A", "MY", -ke1 * de * Le1, "N*m"),
         exp("joint_reaction", "LC1", "B", "FZ", ke2 * de, "N"),
         exp("joint_reaction", "LC1", "B", "MY", ke2 * de * Le2, "N*m"),
         exp("frame_station", "LC1", "F1", "M3", 0.0, "N*m", 1.0),
         exp("frame_station", "LC1", "F2", "M3", 0.0, "N*m", 0.0),
         exp("external_work", "LC1", "", "", Pe3 * de / 2, "J")]
write("V-EQ-001", "Equal constraint: load shared by two cantilevers with tied UZ at coincident joints", ["F04-005"], model(
    "V-EQ-001 equal UZ between two cantilevers",
    [J("A", 0, 0, 0, FIX), J("R", Le1, 0, 0), J("S", Le1, 0, 0), J("B", Le1 + Le2, 0, 0, FIX)],
    [{"name": "F1", "i": "A", "j": "R", "section": "B"}, {"name": "F2", "i": "S", "j": "B", "section": "B"}],
    [SEC], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "R", "values": [0, 0, -Pe3, 0, 0, 0]}],
    schema=3,
    constraints=[{"kind": "Equal", "name": "E1", "components": [False, False, True, False, False, False], "joints": ["R", "S"]}],
), {"kind": "analytical", "source": "Two springs in parallel, k = 3 E I / L^3 for each cantilever",
    "derivation": ["delta = P / (k1 + k2)", "V_i = k_i delta, M_base = V_i L_i", "only the shear crosses the joint: zero moment at the tips"]}, exp_e)

# A bar inside a body deforms in no way, so it holds nothing: six free modes.
# Oblique on purpose: its reduced stiffness is pure rounding noise, which must
# not pass as a pivot (decision 0011).
write("V-MECH-008", "Bar with both ends in one body and no supports", ["F04-005", "F08", "stability"], model(
    "V-MECH-008 rigid body without supports",
    [J("1", 0, 0, 0), J("2", 3, 1.7, 0.9)],
    [{"name": "F1", "i": "1", "j": "2", "section": "B"}],
    [SEC], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "2", "values": [0, 0, -P, 0, 0, 0]}],
    schema=3,
    constraints=[{"kind": "Body", "name": "B1", "joints": ["1", "2"]}],
), {"kind": "analytical", "source": "A free rigid body has six rigid-body modes", "derivation": ["nullity(K) = 6"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "nullity = 6 (exact)"}])


# ---------------------------------------------------------------- springs
def JS(name, x, y, z, r=None, springs=None):
    d = J(name, x, y, z, r)
    d["springs"] = springs
    return d


Ls1, Pk, kt1, kr1 = 3.0, 10_000.0, 2.0e6, 5.0e6
kc = 3 * E * I / Ls1**3
d1 = Pk / (kc + kt1)
Rt1 = kt1 * d1
# Rotational spring at the tip: theta from compatibility with M_y = -kr theta.
th2 = (Pk * Ls1**2 / (2 * E * I)) / (1 + kr1 * Ls1 / (E * I))
uz2 = -Pk * Ls1**3 / (3 * E * I) + kr1 * th2 * Ls1**2 / (2 * E * I)
exp_s1 = [exp("joint_displacement", "LC1", "T1", "UZ", -d1, "m"),
          exp("joint_reaction", "LC1", "T1", "FZ", Rt1, "N"),
          exp("joint_reaction", "LC1", "A1", "FZ", Pk - Rt1, "N"),
          exp("joint_reaction", "LC1", "A1", "MY", -(Pk - Rt1) * Ls1, "N*m"),
          exp("joint_displacement", "LC1", "T2", "UZ", uz2, "m"),
          exp("joint_displacement", "LC1", "T2", "RY", th2, "rad"),
          exp("joint_reaction", "LC1", "T2", "MY", -kr1 * th2, "N*m"),
          exp("joint_reaction", "LC1", "A2", "FZ", Pk, "N"),
          exp("joint_reaction", "LC1", "A2", "MY", -Pk * Ls1 + kr1 * th2, "N*m"),
          exp("external_work", "LC1", "", "", 0.5 * Pk * (d1 - uz2), "J")]
write("V-SPR-001", "Joint springs: translational and rotational at a cantilever tip", ["F04-002"], model(
    "V-SPR-001 cantilevers on tip springs",
    [J("A1", 0, 0, 0, FIX), JS("T1", Ls1, 0, 0, None, [0, 0, kt1, 0, 0, 0]),
     J("A2", 0, 4, 0, FIX), JS("T2", Ls1, 4, 0, None, [0, 0, 0, 0, kr1, 0])],
    [{"name": "F1", "i": "A1", "j": "T1", "section": "B"}, {"name": "F2", "i": "A2", "j": "T2", "section": "B"}],
    [SEC], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "T1", "values": [0, 0, -Pk, 0, 0, 0]},
                 {"pattern": "L", "joint": "T2", "values": [0, 0, -Pk, 0, 0, 0]}],
    schema=3,
), {"kind": "analytical", "source": "Springs in parallel; cantilever under tip force and restoring moment",
    "derivation": ["delta = P / (3 E I / L^3 + k)", "theta = (P L^2 / (2 E I)) / (1 + kr L / (E I))", "u = -P L^3/(3 E I) + kr theta L^2/(2 E I)"]}, exp_s1)

# Floor with no frames at all: only springs and the diaphragm hold it.
pts_s = [(0.0, 0.0, 3.0e6, 1.0e6), (6.0, 0.0, 1.0e6, 2.0e6), (6.0, 4.0, 2.0e6, 4.0e6)]
Fa_s, Mn_s = 25_000.0, 40_000.0
uas, ubs, ths = rigid_floor([(a_, b_, ka, kb, 0.0) for a_, b_, ka, kb in pts_s],
                            [(0.0, 0.0, Fa_s, 0.0, 0.0), (6.0, 4.0, 0.0, 0.0, Mn_s)])
HELD = [False, False, True, True, True, False]
exp_s2 = []
for k_, (a_, b_, ka, kb) in enumerate(pts_s):
    ua_, ub_ = uas - ths * b_, ubs + ths * a_
    exp_s2 += [exp("joint_displacement", "LC1", f"N{k_}", "UX", ua_, "m"),
               exp("joint_displacement", "LC1", f"N{k_}", "UY", ub_, "m"),
               exp("joint_displacement", "LC1", f"N{k_}", "RZ", ths, "rad"),
               exp("joint_reaction", "LC1", f"N{k_}", "FX", -ka * ua_, "N"),
               exp("joint_reaction", "LC1", f"N{k_}", "FY", -kb * ub_, "N")]
write("V-SPR-002", "Rigid floor held only by springs on tied joints", ["F04-002", "F04-005"], model(
    "V-SPR-002 floor on springs",
    [JS(f"N{k_}", a_, b_, 0, HELD, [ka, kb, 0, 0, 0, 0]) for k_, (a_, b_, ka, kb) in enumerate(pts_s)],
    [], [SEC], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "N0", "values": [Fa_s, 0, 0, 0, 0, 0]},
                 {"pattern": "L", "joint": "N2", "values": [0, 0, 0, 0, 0, Mn_s]}],
    schema=3,
    constraints=[{"kind": "Diaphragm", "name": "D1", "joints": ["N0", "N1", "N2"]}],
), {"kind": "analytical", "source": "Three-DOF rigid floor on springs, same closed form as V-DIA-001 with no columns",
    "derivation": ["K q = F from the energy of the springs", "spring reaction = -k u"]}, exp_s2)

# ---------------------------------------------------------------- imposed
Lm, dm, qm = 5.0, 0.012, 8_000.0
Vm = 12 * E * I * dm / Lm**3
Mm = 6 * E * I * dm / Lm**2
Mq = qm * Lm**2 / 12
Rq = qm * Lm / 2
# Case SET: end B of a clamped beam moves down by dm. Case Q: uniform load.
# Case BOTH must be their sum, solved as one case. Combination 2 SET + Q.
exp_m = []
for case_, s_, q_ in [("SET", 1.0, 0.0), ("Q", 0.0, 1.0), ("BOTH", 1.0, 1.0), ("CMB", 2.0, 1.0)]:
    exp_m += [exp("joint_displacement", case_, "B", "UZ", -s_ * dm, "m"),
              exp("joint_reaction", case_, "A", "FZ", s_ * Vm + q_ * Rq, "N"),
              exp("joint_reaction", case_, "B", "FZ", -s_ * Vm + q_ * Rq, "N"),
              exp("joint_reaction", case_, "A", "MY", -s_ * Mm - q_ * Mq, "N*m"),
              exp("joint_reaction", case_, "B", "MY", -s_ * Mm + q_ * Mq, "N*m"),
              exp("frame_station", case_, "F1", "M3", -s_ * Mm - q_ * Mq, "N*m", 0.0),
              exp("frame_station", case_, "F1", "M3", q_ * qm * Lm**2 / 24, "N*m", 0.5),
              exp("frame_station", case_, "F1", "M3", s_ * Mm - q_ * Mq, "N*m", 1.0)]
exp_m.append(exp("external_work", "SET", "", "", 0.5 * Vm * dm, "J"))
write("V-SET-001", "Support settlement on a clamped beam, alone, with span load and in a combination", ["F04-006", "F06-003", "F16"], model(
    "V-SET-001 clamped beam, settlement",
    [J("A", 0, 0, 0, FIX), J("B", Lm, 0, 0, FIX)],
    [{"name": "F1", "i": "A", "j": "B", "section": "B"}],
    [SEC], [{"name": "S"}, {"name": "UDL"}],
    frame_loads=[{"pattern": "UDL", "frame": "F1", "load": {"kind": "Uniform", "direction": "Gravity", "value": qm}}],
    cases=[{"type": "LinearStatic", "name": "SET", "loads": [{"pattern": "S", "scale": 1.0}]},
           {"type": "LinearStatic", "name": "Q", "loads": [{"pattern": "UDL", "scale": 1.0}]},
           {"type": "LinearStatic", "name": "BOTH", "loads": [{"pattern": "S", "scale": 1.0}, {"pattern": "UDL", "scale": 1.0}]}],
    combos=[{"type": "LinearAdd", "name": "CMB", "cases": [{"case": "SET", "scale": 2.0}, {"case": "Q", "scale": 1.0}]}],
    schema=3,
    imposed=[{"pattern": "S", "joint": "B", "values": [0, 0, -dm, 0, 0, 0]}],
), {"kind": "analytical", "source": "Clamped beam under relative end displacement; superposition with the fixed-end uniform load",
    "derivation": ["V = 12 E I d / L^3, M = 6 E I d / L^2, antisymmetric", "q: R = q L/2, M_end = q L^2/12, M_mid = q L^2/24", "W = V d / 2"]}, exp_m)

# Imposed rotation; shear-deformable end movement; middle support of a
# two-span beam; moving grounded end of a spring. Independent load cases.
tm = 0.003
SHD = dict(SR, name="SH")
PHI = 12 * E * SHD["i33"] / (G * SHD["as2"] * Lm**2)
Vt = 12 * E * SHD["i33"] * dm / (Lm**3 * (1 + PHI))
Lt = 4.0
Rmid = 6 * E * I * dm / Lt**3
ks, ug = 1.5e6, -0.02
kcs = 3 * E * I / Ls1**3
us = ks * ug / (ks + kcs)
Rs = ks * (ug - us)
names_i = ["ROT", "TIM", "SPAN", "GND"]
write("V-SET-002", "Imposed rotation, shear deformation, middle support, grounded spring end", ["F04-006", "F04-002", "F02-003"], model(
    "V-SET-002 imposed displacements",
    [J("A", 0, 0, 0, FIX), J("B", Lm, 0, 0, FIX),
     J("C", 0, 4, 0, FIX), J("D", Lm, 4, 0, FIX),
     J("E", 0, 8, 0, PIN), J("M", Lt, 8, 0, [False, False, True, False, False, False]), J("F", 2 * Lt, 8, 0, ROL),
     J("G", 0, 12, 0, FIX), JS("H", Ls1, 12, 0, None, [0, 0, ks, 0, 0, 0])],
    [{"name": "F1", "i": "A", "j": "B", "section": "B"}, {"name": "F2", "i": "C", "j": "D", "section": "SH"},
     {"name": "F3", "i": "E", "j": "M", "section": "B"}, {"name": "F4", "i": "M", "j": "F", "section": "B"},
     {"name": "F5", "i": "G", "j": "H", "section": "B"}],
    [SEC, SHD], [{"name": n} for n in names_i],
    cases=[{"type": "LinearStatic", "name": n, "loads": [{"pattern": n, "scale": 1.0}]} for n in names_i],
    schema=3,
    imposed=[{"pattern": "ROT", "joint": "A", "values": [0, 0, 0, 0, tm, 0]},
             {"pattern": "TIM", "joint": "D", "values": [0, 0, -dm, 0, 0, 0]},
             {"pattern": "SPAN", "joint": "M", "values": [0, 0, -dm, 0, 0, 0]},
             {"pattern": "GND", "joint": "H", "values": [0, 0, ug, 0, 0, 0]}],
), {"kind": "analytical", "source": "Slope-deflection of a clamped beam, Timoshenko shear factor, simple beam of span 2L under a central force, springs in series",
    "derivation": ["M_A = 4 E I t/L, M_B = 2 E I t/L, V = 6 E I t/L^2", "V = 12 E I d/(L^3 (1 + Phi)), Phi = 12 E I/(G As L^2)",
                   "R_mid = 6 E I d/L^3 for spans L, end rotation = R_mid L^2/(4 E I)",
                   "u = k u_g/(k + 3 E I/L^3), spring force = k (u_g - u)"]}, [
    exp("joint_reaction", "ROT", "A", "MY", 4 * E * I * tm / Lm, "N*m"),
    exp("joint_reaction", "ROT", "B", "MY", 2 * E * I * tm / Lm, "N*m"),
    exp("joint_reaction", "ROT", "A", "FZ", -6 * E * I * tm / Lm**2, "N"),
    exp("joint_reaction", "ROT", "B", "FZ", 6 * E * I * tm / Lm**2, "N"),
    exp("joint_displacement", "ROT", "A", "RY", tm, "rad"),
    exp("external_work", "ROT", "", "", 2 * E * I * tm**2 / Lm, "J"),
    exp("joint_reaction", "TIM", "C", "FZ", Vt, "N"),
    exp("joint_reaction", "TIM", "D", "FZ", -Vt, "N"),
    exp("joint_reaction", "TIM", "C", "MY", -Vt * Lm / 2, "N*m"),
    exp("external_work", "TIM", "", "", 0.5 * Vt * dm, "J"),
    exp("joint_reaction", "SPAN", "M", "FZ", -Rmid, "N"),
    exp("joint_reaction", "SPAN", "E", "FZ", Rmid / 2, "N"),
    exp("joint_reaction", "SPAN", "F", "FZ", Rmid / 2, "N"),
    exp("joint_displacement", "SPAN", "E", "RY", Rmid * Lt**2 / (4 * E * I), "rad"),
    exp("joint_displacement", "SPAN", "F", "RY", -Rmid * Lt**2 / (4 * E * I), "rad"),
    exp("joint_displacement", "SPAN", "M", "RY", 0.0, "rad"),
    exp("frame_station", "SPAN", "F3", "M3", Rmid * Lt / 2, "N*m", 1.0),
    exp("external_work", "SPAN", "", "", 0.5 * Rmid * dm, "J"),
    exp("joint_displacement", "GND", "H", "UZ", us, "m"),
    exp("joint_reaction", "GND", "H", "FZ", Rs, "N"),
    exp("joint_reaction", "GND", "G", "FZ", -Rs, "N"),
    exp("joint_reaction", "GND", "G", "MY", Rs * Ls1, "N*m"),
    exp("external_work", "GND", "", "", 0.5 * Rs * ug, "J"),
])

# ---------------------------------------------------------------- span loads
Lt3, qt3 = 6.0, 9_000.0


def trap(frame, q_start, q_end, a=0.0, b=1.0, direction="Gravity"):
    return {"pattern": "L", "frame": frame, "load": {"kind": "Trapezoidal", "direction": direction, "rel_start": a,
                                                     "rel_end": b, "value_start": q_start, "value_end": q_end}}


# Simply supported, 0 at A to q at B: in two elements, so the mid deflection is a
# joint; in one element; and clamped at both ends.
write("V-TRAP-001", "Triangular load: simply supported and clamped beams", ["F06-006", "F08", "F18"], model(
    "V-TRAP-001 triangular load",
    [J("A", 0, 0, 0, PIN), J("M", Lt3 / 2, 0, 0), J("B", Lt3, 0, 0, ROL),
     J("C", 0, 3, 0, PIN), J("D", Lt3, 3, 0, ROL),
     J("E", 0, 6, 0, FIX), J("F", Lt3, 6, 0, FIX)],
    [{"name": "F1", "i": "A", "j": "M", "section": "B"}, {"name": "F2", "i": "M", "j": "B", "section": "B"},
     {"name": "F3", "i": "C", "j": "D", "section": "B"}, {"name": "F4", "i": "E", "j": "F", "section": "B"}],
    [SEC], [{"name": "L"}],
    frame_loads=[trap("F1", 0.0, qt3 / 2), trap("F2", qt3 / 2, qt3), trap("F3", 0.0, qt3), trap("F4", 0.0, qt3)],
    schema=4,
), {"kind": "analytical", "source": "Classic results for a triangular load on a beam",
    "derivation": ["simply supported: R_A = q L/6, R_B = q L/3, M(x) = q L x/6 - q x^3/(6 L)",
                   "delta_mid = 5 q L^4/(768 E I), theta_A = 7 q L^3/(360 E I), theta_B = q L^3/(45 E I)",
                   "clamped: R_E = 3 q L/20, R_F = 7 q L/20, M_E = q L^2/30, M_F = q L^2/20"]}, [
    exp("joint_displacement", "LC1", "M", "UZ", -5 * qt3 * Lt3**4 / (768 * E * I), "m"),
    exp("joint_displacement", "LC1", "A", "RY", 7 * qt3 * Lt3**3 / (360 * E * I), "rad"),
    exp("joint_displacement", "LC1", "B", "RY", -qt3 * Lt3**3 / (45 * E * I), "rad"),
    exp("joint_reaction", "LC1", "A", "FZ", qt3 * Lt3 / 6, "N"),
    exp("joint_reaction", "LC1", "B", "FZ", qt3 * Lt3 / 3, "N"),
    exp("joint_reaction", "LC1", "C", "FZ", qt3 * Lt3 / 6, "N"),
    exp("joint_reaction", "LC1", "D", "FZ", qt3 * Lt3 / 3, "N"),
    exp("frame_station", "LC1", "F3", "M3", qt3 * Lt3 * (Lt3 / 2) / 6 - qt3 * (Lt3 / 2)**3 / (6 * Lt3), "N*m", 0.5),
    exp("frame_station", "LC1", "F3", "M3", qt3 * Lt3 * (Lt3 / 4) / 6 - qt3 * (Lt3 / 4)**3 / (6 * Lt3), "N*m", 0.25),
    exp("joint_reaction", "LC1", "E", "FZ", 3 * qt3 * Lt3 / 20, "N"),
    exp("joint_reaction", "LC1", "F", "FZ", 7 * qt3 * Lt3 / 20, "N"),
    exp("joint_reaction", "LC1", "E", "MY", -qt3 * Lt3**2 / 30, "N*m"),
    exp("joint_reaction", "LC1", "F", "MY", qt3 * Lt3**2 / 20, "N*m"),
    exp("frame_station", "LC1", "F4", "M3", -qt3 * Lt3**2 / 30, "N*m", 0.0),
    exp("frame_station", "LC1", "F4", "M3", -qt3 * Lt3**2 / 20, "N*m", 1.0),
])


def simpson(fn, lo, hi, n=2000):
    h = (hi - lo) / n
    return h / 3 * (fn(lo) + fn(hi) + sum((4 if k % 2 else 2) * fn(lo + k * h) for k in range(1, n)))


def partial_cantilever(q_of, lo, hi, L_, EI_, GAs_):
    """Tip of a cantilever by summing tip load results along the loaded zone:
    a load P at s gives P s^2 (3 L - s)/(6 E I) + P s/(G As) and P s^2/(2 E I)."""
    shear = (lambda s: q_of(s) * s / GAs_) if GAs_ else (lambda s: 0.0)
    return (simpson(lambda s: q_of(s) * s**2 * (3 * L_ - s) / (6 * EI_) + shear(s), lo, hi),
            simpson(lambda s: q_of(s) * s**2 / (2 * EI_), lo, hi),
            simpson(q_of, lo, hi),
            simpson(lambda s: q_of(s) * s, lo, hi))


# Partial loads on cantilevers: constant, linear, in a shear-deformable bar, and
# a gravity load on an inclined bar. a, b are absolute distances.
Lp2, ap2, bp2 = 5.0, 1.0, 4.0
q1p, q2p = 6_000.0, 14_000.0
linear_q = lambda s: q1p + (q2p - q1p) * (s - ap2) / (bp2 - ap2)
exp_p, exp_ps = [], []
for fr_, jt, base, q_fn, s_, GA_, v1, v2 in [
    ("PC", "C1", "C0", lambda s: q1p, SEC, None, q1p, q1p),
    ("PL", "L1", "L0", linear_q, SEC, None, q1p, q2p),
    ("PS", "S1", "S0", linear_q, SR, G * SR["as2"], q1p, q2p),
]:
    dtip, ttip, Ftot, Mtot = partial_cantilever(q_fn, ap2, bp2, Lp2, E * s_["i33"], GA_)
    # Section at x, inside the loaded zone: only the load beyond x counts.
    x_mid = Lp2 / 2
    M_mid = simpson(lambda s: q_fn(s) * (s - x_mid), x_mid, bp2)
    (exp_ps if fr_ == "PS" else exp_p).extend([
        exp("joint_displacement", "LC1", jt, "UZ", -dtip, "m"),
        exp("joint_displacement", "LC1", jt, "RY", ttip, "rad"),
        exp("joint_reaction", "LC1", base, "FZ", Ftot, "N"),
        exp("joint_reaction", "LC1", base, "MY", -Mtot, "N*m"),
        exp("frame_station", "LC1", fr_, "M3", -Mtot, "N*m", 0.0),
        exp("frame_station", "LC1", fr_, "M3", -M_mid, "N*m", 0.5),
        exp("frame_station", "LC1", fr_, "V2", -simpson(q_fn, x_mid, bp2), "N", 0.5),
        exp("frame_station", "LC1", fr_, "M3", 0.0, "N*m", 1.0)])
# Inclined 45 deg in XZ: triangular gravity load along the bar, 0 at the base.
ci = math.sqrt(0.5)
Li, qi = 4.0, 5_000.0
Fi = qi * Li / 2
exp_p += [exp("joint_reaction", "LC1", "I0", "FZ", Fi, "N"),
          exp("joint_reaction", "LC1", "I0", "FX", 0.0, "N"),
          exp("joint_reaction", "LC1", "I0", "MY", -Fi * (2 * Li / 3) * ci, "N*m")]
ref_partial = {"kind": "analytical", "source": "Integral of the cantilever tip-load results over the loaded zone, by Simpson in tools/gen_cases.py",
               "derivation": ["load P at s: tip deflection P s^2 (3 L - s)/(6 E I) + P s/(G As), rotation P s^2/(2 E I)",
                              "section actions from the load beyond the section", "inclined bar: F = q L/2 at 2 L/3 along the bar"]}
write("V-TRAP-002", "Partial span loads: constant, linear, inclined bar under gravity", ["F06-006", "F18"], model(
    "V-TRAP-002 partial span loads",
    [J("C0", 0, 0, 0, FIX), J("C1", Lp2, 0, 0), J("L0", 0, 3, 0, FIX), J("L1", Lp2, 3, 0),
     J("I0", 0, 9, 0, FIX), J("I1", Li * ci, 9, Li * ci)],
    [{"name": "PC", "i": "C0", "j": "C1", "section": "B"}, {"name": "PL", "i": "L0", "j": "L1", "section": "B"},
     {"name": "PI", "i": "I0", "j": "I1", "section": "B"}],
    [SEC], [{"name": "L"}],
    frame_loads=[trap("PC", q1p, q1p, ap2 / Lp2, bp2 / Lp2), trap("PL", q1p, q2p, ap2 / Lp2, bp2 / Lp2), trap("PI", 0.0, qi)],
    schema=4,
), ref_partial, exp_p)
# Apart, as in V-OFF-003: Pynite has no shear deformation, and this bar alone
# would leave the model above with no external comparison.
write("V-TRAP-003", "Partial linear span load with shear deformation", ["F06-006", "F02-003", "F18"], model(
    "V-TRAP-003 partial load on a shear-deformable bar",
    [J("S0", 0, 0, 0, FIX), J("S1", Lp2, 0, 0)],
    [{"name": "PS", "i": "S0", "j": "S1", "section": "R"}],
    [SR], [{"name": "L"}],
    frame_loads=[trap("PS", q1p, q2p, ap2 / Lp2, bp2 / Lp2)],
    schema=4,
), ref_partial, exp_ps)

# Span moments on cantilevers along +X (local 1 = +X, 2 = +Z, 3 = -Y).
Lm3, am3, M0 = 5.0, 2.0, 30_000.0
SM = {"name": "M", "material": "STEEL", "area": 0.01, "i33": 2e-4, "i22": 1e-4, "j": 3e-4}
EIm3, EIm2, GJm = E * SM["i33"], E * SM["i22"], G * SM["j"]
m0, t0 = 8_000.0, 6_000.0


def moment_load(frame, direction, value, rel):
    return {"pattern": "L", "frame": frame, "load": {"kind": "PointMoment", "direction": direction, "rel": rel, "value": value}}


def moment_trap(frame, direction, v1, v2):
    return {"pattern": "L", "frame": frame, "load": {"kind": "TrapezoidalMoment", "direction": direction, "rel_start": 0.0,
                                                     "rel_end": 1.0, "value_start": v1, "value_end": v2}}


rel3 = am3 / Lm3
thM3 = M0 * am3 / EIm3
thM2 = M0 * am3 / EIm2


def moment_model(title, names, loads):
    return model(
        title,
        [j_ for k_, n_ in enumerate(names) for j_ in (J(n_ + "0", 0, 3.0 * k_, 0, FIX), J(n_ + "1", Lm3, 3.0 * k_, 0))],
        [{"name": "F" + n_, "i": n_ + "0", "j": n_ + "1", "section": "M"} for n_ in names],
        [SM], [{"name": "L"}], frame_loads=loads, schema=4)


write("V-MOM-001", "Concentrated span moments about each local axis", ["F06-007", "F18"], moment_model(
    "V-MOM-001 concentrated span moments", ["A", "B", "C"],
    [moment_load("FA", "GlobalY", M0, rel3), moment_load("FB", "GlobalZ", M0, rel3), moment_load("FC", "Local1", M0, rel3)],
), {"kind": "analytical", "source": "Cantilever under a moment: only the part from the support to the moment bends",
    "derivation": ["theta = M a/(E I), tip deflection = M a^2/(2 E I) + M a (L - a)/(E I)", "torque: rotation T a/(G J)"]}, [
    exp("joint_displacement", "LC1", "A1", "RY", thM3, "rad"),
    exp("joint_displacement", "LC1", "A1", "UZ", -(M0 * am3**2 / (2 * EIm3) + thM3 * (Lm3 - am3)), "m"),
    exp("joint_reaction", "LC1", "A0", "MY", -M0, "N*m"),
    exp("frame_station", "LC1", "FA", "M3", -M0, "N*m", 0.25),
    exp("frame_station", "LC1", "FA", "M3", 0.0, "N*m", 0.5),
    exp("joint_displacement", "LC1", "B1", "RZ", thM2, "rad"),
    exp("joint_displacement", "LC1", "B1", "UY", M0 * am3**2 / (2 * EIm2) + thM2 * (Lm3 - am3), "m"),
    exp("joint_reaction", "LC1", "B0", "MZ", -M0, "N*m"),
    exp("frame_station", "LC1", "FB", "M2", M0, "N*m", 0.25),
    exp("frame_station", "LC1", "FB", "M2", 0.0, "N*m", 0.5),
    exp("joint_displacement", "LC1", "C1", "RX", M0 * am3 / GJm, "rad"),
    exp("joint_reaction", "LC1", "C0", "MX", -M0, "N*m"),
    exp("frame_station", "LC1", "FC", "T", M0, "N*m", 0.25),
    exp("frame_station", "LC1", "FC", "T", 0.0, "N*m", 0.5),
])
write("V-MOM-002", "Distributed span moments: uniform bending and linear torque", ["F06-007", "F18"], moment_model(
    "V-MOM-002 distributed span moments", ["D", "E"],
    [moment_trap("FD", "GlobalY", m0, m0), moment_trap("FE", "Local1", 0.0, t0)],
), {"kind": "analytical", "source": "Cantilever under distributed moments, by integrating the section actions",
    "derivation": ["uniform moment m: theta = m L^2/(2 E I), deflection = m L^3/(3 E I)",
                   "torque 0 to t: T(x) = t (L^2 - x^2)/(2 L), rotation t L^2/(3 G J)"]}, [
    exp("joint_displacement", "LC1", "D1", "RY", m0 * Lm3**2 / (2 * EIm3), "rad"),
    exp("joint_displacement", "LC1", "D1", "UZ", -m0 * Lm3**3 / (3 * EIm3), "m"),
    exp("joint_reaction", "LC1", "D0", "MY", -m0 * Lm3, "N*m"),
    exp("frame_station", "LC1", "FD", "M3", -m0 * Lm3 / 2, "N*m", 0.5),
    exp("joint_displacement", "LC1", "E1", "RX", t0 * Lm3**2 / (3 * GJm), "rad"),
    exp("joint_reaction", "LC1", "E0", "MX", -t0 * Lm3 / 2, "N*m"),
    exp("frame_station", "LC1", "FE", "T", 3 * t0 * Lm3 / 8, "N*m", 0.5),
])

# ---------------------------------------------------------------- subdivision
Lz, qz = 5.0, 8_000.0


def udl(frame, q=qz):
    return {"pattern": "L", "frame": frame, "load": {"kind": "Uniform", "direction": "Gravity", "value": q}}


def ss_defl(q, L_, x):
    return q * x * (L_**3 - 2 * L_ * x**2 + x**3) / (24 * E * I)


# FA runs through joint M, a support: two-span continuous beam from one frame.
# FB in 4 pieces: generated joints report the deflection inside the frame.
# FC: 3 pieces, with offsets and end releases only at the outer ends.
# FD: 5 pieces, a concentrated force exactly on a cut and a moment inside a piece.
ac3, Lc3, qc3 = 0.4, 6.0, 7_000.0
Ld3, Mz3, Pz3 = 5.0, 12_000.0, 9_000.0
sP = 0.6 * Ld3
aM = 0.5 * Ld3
thD = Pz3 * sP**2 / (2 * E * I) + Mz3 * aM / (E * I)
uD = Pz3 * sP**2 * (3 * Ld3 - sP) / (6 * E * I) + Mz3 * aM**2 / (2 * E * I) + Mz3 * aM * (Ld3 - aM) / (E * I)
RMb = Pz3 * sP + Mz3
write("V-MESH-001", "Frames subdivided: through a support, evenly, with offsets and releases, loads on cuts", ["F01-006", "F06-003", "F06-006", "F06-007"], model(
    "V-MESH-001 automatic subdivision",
    [J("A", 0, 0, 0, PIN), J("M", Lz, 0, 0, [False, False, True, False, False, False]), J("B", 2 * Lz, 0, 0, ROL),
     J("C", 0, 3, 0, PIN), J("D", Lz, 3, 0, ROL),
     J("E", 0, 6, 0, FIX), J("F", Lc3 + 2 * ac3, 6, 0, FIX),
     J("G", 0, 9, 0, FIX), J("H", Ld3, 9, 0)],
    [{"name": "FA", "i": "A", "j": "B", "section": "B", "auto_mesh": {"at_joints": True}},
     {"name": "FB", "i": "C", "j": "D", "section": "B", "auto_mesh": {"segments": 4}},
     {"name": "FC", "i": "E", "j": "F", "section": "B", "auto_mesh": {"segments": 3},
      "offsets": {"i": [ac3, 0, 0], "j": [-ac3, 0, 0]},
      "releases": {"i": [False, False, False, False, False, True], "j": [False, False, False, False, False, True]}},
     {"name": "FD", "i": "G", "j": "H", "section": "B", "auto_mesh": {"segments": 5}}],
    [SEC], [{"name": "L"}],
    frame_loads=[udl("FA"), udl("FB"), trap("FC", 0.0, qc3),
                 {"pattern": "L", "frame": "FD", "load": {"kind": "Point", "direction": "Gravity", "rel": 0.6, "value": Pz3}},
                 moment_load("FD", "GlobalY", Mz3, 0.5)],
    schema=5,
), {"kind": "analytical", "source": "Classic results: two-span beam, simply supported beam, triangular load, cantilever",
    "derivation": ["two spans L: R_A = 3 q L/8, R_M = 5 q L/4, M_M = -q L^2/8", "delta(x) = q x (L^3 - 2 L x^2 + x^3)/(24 E I)",
                   "released ends with arms a: M_support = R a", "cantilever: superposition of a force at s and a moment at a"]}, [
    exp("joint_reaction", "LC1", "A", "FZ", 3 * qz * Lz / 8, "N"),
    exp("joint_reaction", "LC1", "M", "FZ", 5 * qz * Lz / 4, "N"),
    exp("joint_reaction", "LC1", "B", "FZ", 3 * qz * Lz / 8, "N"),
    exp("frame_station", "LC1", "FA", "M3", -qz * Lz**2 / 8, "N*m", 0.5),
    exp("frame_station", "LC1", "FA", "M3", qz * Lz**2 / 16, "N*m", 0.25),
    exp("joint_displacement", "LC1", "FB~1", "UZ", -ss_defl(qz, Lz, Lz / 4), "m"),
    exp("joint_displacement", "LC1", "FB~2", "UZ", -ss_defl(qz, Lz, Lz / 2), "m"),
    exp("joint_displacement", "LC1", "FB~3", "UZ", -ss_defl(qz, Lz, 3 * Lz / 4), "m"),
    exp("frame_station", "LC1", "FB", "M3", qz * Lz**2 / 8, "N*m", 0.5),
    exp("frame_station", "LC1", "FB", "V2", qz * Lz / 2 * -1, "N", 0.0),
    exp("joint_reaction", "LC1", "E", "FZ", qc3 * Lc3 / 6, "N"),
    exp("joint_reaction", "LC1", "E", "MY", -qc3 * Lc3 / 6 * ac3, "N*m"),
    exp("joint_reaction", "LC1", "F", "MY", qc3 * Lc3 / 3 * ac3, "N*m"),
    exp("frame_station", "LC1", "FC", "M3", qc3 * Lc3 * (Lc3 / 2) / 6 - qc3 * (Lc3 / 2)**3 / (6 * Lc3), "N*m", 0.5),
    exp("frame_station", "LC1", "FC", "M3", 0.0, "N*m", 0.0),
    exp("frame_station", "LC1", "FC", "M3", 0.0, "N*m", 1.0),
    exp("joint_displacement", "LC1", "H", "UZ", -uD, "m"),
    exp("joint_displacement", "LC1", "H", "RY", thD, "rad"),
    exp("joint_reaction", "LC1", "G", "FZ", Pz3, "N"),
    exp("joint_reaction", "LC1", "G", "MY", -RMb, "N*m"),
    exp("frame_station", "LC1", "FD", "M3", 0.0, "N*m", 1.0),
])

# Two bars crossing at C: one runs through C and is cut there; the other arrives
# at C, pinned so it only gives a transverse spring. Two springs in parallel.
Lg, Pg = 6.0, 30_000.0
k1g, k2g = 48 * E * I / Lg**3, 3 * E * I / (Lg / 2)**3
dg = Pg / (k1g + k2g)
write("V-MESH-002", "Crossing bars connected by subdivision at a joint", ["F01-006", "F04-003"], model(
    "V-MESH-002 crossing bars",
    [J("P", 0, 0, 0, PIN), J("Q", Lg, 0, 0, ROL), J("C", Lg / 2, 0, 0), J("R", Lg / 2, Lg / 2, 0, FIX)],
    [{"name": "F1", "i": "P", "j": "Q", "section": "B", "auto_mesh": {"at_joints": True}},
     {"name": "F2", "i": "R", "j": "C", "section": "B", "releases": {"j": [False, False, False, True, True, True]}}],
    [SEC], [{"name": "L"}],
    joint_loads=[{"pattern": "L", "joint": "C", "values": [0, 0, -Pg, 0, 0, 0]}],
    schema=5,
), {"kind": "analytical", "source": "Simply supported beam with a central load and a cantilever, as springs in parallel",
    "derivation": ["k1 = 48 E I/L^3, k2 = 3 E I/(L/2)^3", "delta = P/(k1 + k2)", "V_R = k2 delta, M_R = V_R L/2"]}, [
    exp("joint_displacement", "LC1", "C", "UZ", -dg, "m"),
    exp("joint_reaction", "LC1", "P", "FZ", k1g * dg / 2, "N"),
    exp("joint_reaction", "LC1", "R", "FZ", k2g * dg, "N"),
    # The bar is pushed down at C, at r = (0, -L/2, 0) from R: r x F = +V L/2 about
    # X, which the support balances.
    exp("joint_reaction", "LC1", "R", "MX", -k2g * dg * Lg / 2, "N*m"),
    exp("frame_station", "LC1", "F1", "M3", k1g * dg * Lg / 4, "N*m", 0.5),
])

# ---------------------------------------------------------------- envelopes


def env(result, case, entity, component, bound, value, governing, unit, rel=None):
    d = exp(result, case, entity, component, value, unit, rel)
    d["bound"], d["governing"] = bound, governing
    return d


# Simply supported beam under down, heavier and upward uniform loads, and a
# combination of them. Linear in the load, so each extreme has a known source.
Le3, qD, qL3, qW = 6.0, 4_000.0, 6_000.0, -5_000.0
qU = 1.2 * qD + 1.6 * qL3
scale_w = 1.1


def per_load(q):
    return {"R": q * Le3 / 2, "M": q * Le3**2 / 8, "d": -5 * q * Le3**4 / (384 * E * I)}


vals = {"DEAD": per_load(qD), "LIVE": per_load(qL3), "ULS": per_load(qU), "WIND": per_load(scale_w * qW)}
write("V-ENV-001", "Envelope of cases and a combination, with the governing term", ["F16-002", "F16-001"], model(
    "V-ENV-001 envelope",
    [J("A", 0, 0, 0, PIN), J("M", Le3 / 2, 0, 0), J("B", Le3, 0, 0, ROL)],
    [{"name": "F1", "i": "A", "j": "M", "section": "B"}, {"name": "F2", "i": "M", "j": "B", "section": "B"}],
    [SEC], [{"name": "D"}, {"name": "L3"}, {"name": "W"}],
    frame_loads=[{"pattern": p_, "frame": f_, "load": {"kind": "Uniform", "direction": "Gravity", "value": v_}}
                 for p_, v_ in [("D", qD), ("L3", qL3), ("W", qW)] for f_ in ["F1", "F2"]],
    cases=[{"type": "LinearStatic", "name": n_, "loads": [{"pattern": p_, "scale": 1.0}]}
           for n_, p_ in [("DEAD", "D"), ("LIVE", "L3"), ("WIND", "W")]],
    combos=[{"type": "LinearAdd", "name": "ULS", "cases": [{"case": "DEAD", "scale": 1.2}, {"case": "LIVE", "scale": 1.6}]},
            {"type": "Envelope", "name": "ENV", "cases": [{"case": "DEAD", "scale": 1.0}, {"case": "ULS", "scale": 1.0},
                                                          {"case": "WIND", "scale": scale_w}]}],
    schema=5,
), {"kind": "analytical", "source": "Simply supported beam under a uniform load, per term of the envelope",
    "derivation": ["R = q L/2, M_mid = q L^2/8, delta_mid = 5 q L^4/(384 E I)", "the largest down load governs the max reaction; the scaled upward load governs the min"]}, [
    env("envelope_joint_reaction", "ENV", "A", "FZ", "max", vals["ULS"]["R"], "ULS", "N"),
    env("envelope_joint_reaction", "ENV", "A", "FZ", "min", vals["WIND"]["R"], "WIND", "N"),
    env("envelope_joint_displacement", "ENV", "M", "UZ", "min", vals["ULS"]["d"], "ULS", "m"),
    env("envelope_joint_displacement", "ENV", "M", "UZ", "max", vals["WIND"]["d"], "WIND", "m"),
    env("envelope_frame_station", "ENV", "F1", "M3", "max", vals["ULS"]["M"], "ULS", "N*m", 1.0),
    env("envelope_frame_station", "ENV", "F1", "M3", "min", vals["WIND"]["M"], "WIND", "N*m", 1.0),
    exp("joint_reaction", "ULS", "A", "FZ", vals["ULS"]["R"], "N"),
])

# ---------------------------------------------------------------- shapes


def rect_parts(parts):
    """(h, b, y, z) rectangles: area and inertias by the parallel-axis theorem.
    A different route from the outer-minus-inner formulas of the engine."""
    area = sum(h * b for h, b, _, _ in parts)
    i33 = sum(b * h**3 / 12 + h * b * y**2 for h, b, y, _ in parts)
    i22 = sum(h * b**3 / 12 + h * b * z**2 for h, b, _, z in parts)
    return area, i33, i22


def disc(d):
    """Area and inertia of a disc by quadrature, y = r sin t."""
    r = d / 2
    w = lambda t: 2 * r * math.cos(t)
    area = simpson(lambda t: w(t) * r * math.cos(t), -math.pi / 2, math.pi / 2)
    inertia = simpson(lambda t: (r * math.sin(t))**2 * w(t) * r * math.cos(t), -math.pi / 2, math.pi / 2)
    return area, inertia


def rect_torsion(a, b):
    """Saint-Venant series, a >= b."""
    s_ = sum(math.tanh((2 * k + 1) * math.pi * a / (2 * b)) / (2 * k + 1)**5 for k in range(100))
    return a * b**3 / 3 * (1 - 192 / math.pi**5 * (b / a) * s_)


h_, b_, tf_, tw_ = 0.5, 0.2, 0.02, 0.012
hw_ = h_ - 2 * tf_
d_ = (h_ - tf_) / 2
w_ = (b_ - tw_) / 2
ai, i33i, i22i = rect_parts([(tf_, b_, d_, 0.0), (tf_, b_, -d_, 0.0), (hw_, tw_, 0.0, 0.0)])
ab, i33b, i22b = rect_parts([(tf_, b_, d_, 0.0), (tf_, b_, -d_, 0.0), (hw_, tw_, 0.0, w_), (hw_, tw_, 0.0, -w_)])
ar, i33r, i22r = rect_parts([(h_, b_, 0.0, 0.0)])
Dc, Dp, tp = 0.4, 0.5, 0.01
ac, ic = disc(Dc)
ao_, io_ = disc(Dp)
ah_, ih_ = disc(Dp - 2 * tp)
ap, ip = ao_ - ah_, io_ - ih_
hm_, bm_ = h_ - tf_, b_ - tw_
props_s = {
    "RECT": ({"kind": "Rectangle", "depth": h_, "width": b_}, ar, i33r, i22r, rect_torsion(h_, b_), 5 / 6 * ar, 5 / 6 * ar),
    "CIRC": ({"kind": "Circle", "diameter": Dc}, ac, ic, ic, 2 * ic, 0.9 * ac, 0.9 * ac),
    "PIPE": ({"kind": "Pipe", "diameter": Dp, "wall": tp}, ap, ip, ip, 2 * ip, 0.5 * ap, 0.5 * ap),
    "BOX": ({"kind": "Box", "depth": h_, "width": b_, "flange_t": tf_, "web_t": tw_}, ab, i33b, i22b,
            2 * tf_ * tw_ * bm_**2 * hm_**2 / (bm_ * tw_ + hm_ * tf_), 2 * h_ * tw_, 2 * b_ * tf_),
    "ISEC": ({"kind": "ISection", "depth": h_, "width": b_, "flange_t": tf_, "web_t": tw_}, ai, i33i, i22i,
             (2 * b_ * tf_**3 + (h_ - tf_) * tw_**3) / 3, h_ * tw_, 5 / 3 * b_ * tf_),
}
Ls3, Pl3 = 4.0, 25_000.0
jj_s, ff_s, jl_s, secs_s, exp_s = [], [], [], [], []
for k_, (nm, (shape, A_, I3_, I2_, J_, As2_, As3_)) in enumerate(props_s.items()):
    y0 = 3.0 * k_
    secs_s.append({"name": nm, "material": "STEEL", "shape": shape})
    jj_s += [J(nm + "0", 0, y0, 0, FIX), J(nm + "1", Ls3, y0, 0)]
    ff_s.append({"name": "F" + nm, "i": nm + "0", "j": nm + "1", "section": nm})
    for p_, vals_ in [("AX", [Pl3, 0, 0, 0, 0, 0]), ("Z", [0, 0, -Pl3, 0, 0, 0]), ("Y", [0, Pl3, 0, 0, 0, 0]), ("T", [0, 0, 0, Pl3, 0, 0])]:
        jl_s.append({"pattern": p_, "joint": nm + "1", "values": vals_})
    exp_s += [exp("joint_displacement", "AX", nm + "1", "UX", Pl3 * Ls3 / (E * A_), "m"),
              exp("joint_displacement", "Z", nm + "1", "UZ", -(Pl3 * Ls3**3 / (3 * E * I3_) + Pl3 * Ls3 / (G * As2_)), "m"),
              exp("joint_displacement", "Z", nm + "1", "RY", Pl3 * Ls3**2 / (2 * E * I3_), "rad"),
              exp("joint_displacement", "Y", nm + "1", "UY", Pl3 * Ls3**3 / (3 * E * I2_) + Pl3 * Ls3 / (G * As3_), "m"),
              exp("joint_displacement", "T", nm + "1", "RX", Pl3 * Ls3 / (G * J_), "rad")]
# Section without its shear: the deflection is Euler-Bernoulli.
secs_s.append({"name": "RECTEB", "material": "STEEL", "shape": props_s["RECT"][0], "neglect_shear": True})
jj_s += [J("EB0", 0, 30, 0, FIX), J("EB1", Ls3, 30, 0)]
ff_s.append({"name": "FEB", "i": "EB0", "j": "EB1", "section": "RECTEB"})
jl_s.append({"pattern": "Z", "joint": "EB1", "values": [0, 0, -Pl3, 0, 0, 0]})
exp_s.append(exp("joint_displacement", "Z", "EB1", "UZ", -Pl3 * Ls3**3 / (3 * E * i33r), "m"))
write("V-SEC-001", "Parametric sections: axial, both bending planes with shear, and torsion", ["F02-002", "F02-003"], model(
    "V-SEC-001 parametric sections",
    jj_s, ff_s, secs_s, [{"name": n_} for n_ in ["AX", "Z", "Y", "T"]],
    joint_loads=jl_s,
    cases=[{"type": "LinearStatic", "name": n_, "loads": [{"pattern": n_, "scale": 1.0}]} for n_ in ["AX", "Z", "Y", "T"]],
    schema=5,
), {"kind": "analytical", "source": "Cantilever formulas, with properties computed in tools/gen_cases.py: rectangles by parallel axes, discs by quadrature",
    "derivation": ["u = P L/(E A)", "delta = P L^3/(3 E I) + P L/(G As)", "theta_x = T L/(G J)",
                   "As and thin-wall J follow the rules of decision 0017, which this case fixes but does not prove against SAP2000"]}, exp_s)

print("cases written to", ROOT)
