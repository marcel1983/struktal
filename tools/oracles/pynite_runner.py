"""Solves a Struktal model with Pynite. External reference only (decision 0013).

Pynite has Y up and Struktal Z up, so the model is rotated about X by a proper
rotation: (x, y, z) -> (x, z, -y). Translations and rotations transform alike, and
Pynite's local y, z then coincide with Struktal's 2, 3 up to a half turn on vertical
members, which does not change any inertia.
"""
import importlib.metadata
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from Pynite import FEModel3D  # noqa: E402

from common import Unsupported, main  # noqa: E402


def to_pynite(v):
    return [v[0], v[2], -v[1]]


def from_pynite(v):
    return [v[0], -v[2], v[1]]


def check_supported(m, loads):
    if m.get("constraints"):
        raise Unsupported("no general constraints")
    for j in m["joints"]:
        if any(j["springs"][d] > 0.0 and loads["imposed"].get(j["name"], [0.0] * 6)[d] != 0.0 for d in range(6)):
            raise Unsupported(f"joint {j['name']}: no moving grounded spring end")
    for fr in m["frames"]:
        if any(fr["off_i"]) or any(fr["off_j"]):
            raise Unsupported(f"frame {fr['name']}: no end offsets")
        if fr["sec"]["as2"] > 0 or fr["sec"]["as3"] > 0:
            raise Unsupported(f"frame {fr['name']}: no shear deformation")
        if fr["rel_has_springs"]:
            raise Unsupported(f"frame {fr['name']}: no partial fixity")
        if loads["frames"][fr["name"]]["moment_segments"]:
            raise Unsupported(f"frame {fr['name']}: no distributed moments")


def solve_case(m, loads):
    check_supported(m, loads)
    f = FEModel3D()
    for j in m["joints"]:
        f.add_node(j["name"], *to_pynite(j["xyz"]))
    for mat in m["materials"]:
        f.add_material(mat["name"], mat["e"], mat["g"], mat["nu"], 0.0)
    for s in m["sections"]:
        f.add_section(s["name"], s["area"], s["i22"], s["i33"], s["j"])
    for fr in m["frames"]:
        f.add_member(fr["name"], fr["i"], fr["j"], fr["mat"]["name"], fr["section"], rotation=fr.get("angle_deg", 0.0))
        if any(fr["rel_i"]) or any(fr["rel_j"]):
            f.def_releases(fr["name"], *fr["rel_i"], *fr["rel_j"])
    for a in m.get("areas", []):
        if a["sec"].get("bending_thickness") is not None:
            raise Unsupported(f"area {a['name']}: a separate bending thickness")
        if len(a["joints"]) != 4:
            raise Unsupported(f"area {a['name']}: this engine has no three-node shell")
        f.add_quad(a["name"], *a["joints"], a["sec"]["thickness"], a["sec"]["material"])
    dirs = ["DX", "DY", "DZ", "RX", "RY", "RZ"]
    for j in m["joints"]:
        r = j["restraint"]
        # The rotation is a permutation with a sign, so booleans just permute.
        f.def_support(j["name"], r[0], r[2], r[1], r[3], r[5], r[4])
        k = j["springs"]
        for d, v in enumerate([k[0], k[2], k[1], k[3], k[5], k[4]]):
            if v > 0.0:
                f.def_support_spring(j["name"], dirs[d], v)
    for name, vals in loads["imposed"].items():
        t, w = to_pynite(vals[:3]), to_pynite(vals[3:])
        for d, v in enumerate(t + w):
            if v != 0.0:
                f.def_node_disp(name, dirs[d], v)
    for name, vals in loads["joints"].items():
        t, w = to_pynite(vals[:3]), to_pynite(vals[3:])
        for label, v in zip(["FX", "FY", "FZ", "MX", "MY", "MZ"], t + w):
            if v != 0.0:
                f.add_node_load(name, label, v)
    for fr in m["frames"]:
        span = loads["frames"][fr["name"]]
        for label, v in zip(["FX", "FY", "FZ"], to_pynite(span["uniform"])):
            if v != 0.0:
                f.add_member_dist_load(fr["name"], label, v, v)
        for rel, g in span["points"]:
            for label, v in zip(["FX", "FY", "FZ"], to_pynite(g)):
                if v != 0.0:
                    f.add_member_pt_load(fr["name"], label, v, rel * fr["length"])
        for start, end, w1, w2 in span["segments"]:
            for label, a, b in zip(["FX", "FY", "FZ"], to_pynite(w1), to_pynite(w2)):
                if a != 0.0 or b != 0.0:
                    f.add_member_dist_load(fr["name"], label, a, b, start * fr["length"], end * fr["length"])
        for rel, g in span["moments"]:
            for label, v in zip(["MX", "MY", "MZ"], to_pynite(g)):
                if v != 0.0:
                    f.add_member_pt_load(fr["name"], label, v, rel * fr["length"])
    f.analyze_linear(check_stability=True)

    out = {}
    combo = "Combo 1"
    for j in m["joints"]:
        n = f.nodes[j["name"]]
        disp = from_pynite([n.DX[combo], n.DY[combo], n.DZ[combo]]) + from_pynite([n.RX[combo], n.RY[combo], n.RZ[combo]])
        reaction = from_pynite([n.RxnFX[combo], n.RxnFY[combo], n.RxnFZ[combo]]) + from_pynite([n.RxnMX[combo], n.RxnMY[combo], n.RxnMZ[combo]])
        for d in range(6):
            if j["springs"][d] > 0.0:
                reaction[d] = -j["springs"][d] * disp[d]
        out[j["name"]] = {"disp": disp, "reaction": reaction}
    return out


if __name__ == "__main__":
    main("Pynite", importlib.metadata.version("PyNiteFEA"), solve_case)
