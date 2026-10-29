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
          combos=(), units=None, materials=None):
    return {
        "schema_version": 1,
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
# Station actions in the rust-fem convention (decision 0003): for this member
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
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "6 zero pivot(s)"}])

write("V-MECH-002", "Simply supported beam with torsion unrestrained at both ends", ["F08", "stability"], model(
    "V-MECH-002 torsional mechanism",
    [J("A", 0, 0, 0, [True, True, True, False, False, False]), J("B", 6, 0, 0, ROL)],
    [{"name": "F1", "i": "A", "j": "B", "section": "B"}],
    [SEC], [{"name": "LIVE"}],
    frame_loads=[{"pattern": "LIVE", "frame": "F1", "load": {"kind": "Uniform", "direction": "Gravity", "value": w}}],
), {"kind": "analytical", "source": "Rigid rotation about the member axis is unrestrained", "derivation": ["nullity(K) = 1 (RX)"]},
    expected_status=[{"case": "LC1", "completion": "error", "physical_status": "mechanism", "message_contains": "1 zero pivot(s)"}])

print("cases written to", ROOT)
