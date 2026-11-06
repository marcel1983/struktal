"""Solves a Struktal model with OpenSeesPy. External reference only: no OpenSees code
is used by the engine. Needs Python 3.12 with openseespy 3.8 (decision 0013)."""
import importlib.metadata
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import openseespy.opensees as ops  # noqa: E402

from common import Unsupported, dot, main  # noqa: E402


def check_supported(m):
    for fr in m["frames"]:
        moment_only = not any(fr["rel_i"][:4]) and not any(fr["rel_j"][:4])
        if fr["rel_has_springs"] or not moment_only:
            raise Unsupported(f"frame {fr['name']}: only full M2/M3 releases map to elasticBeamColumn")
        s = fr["sec"]
        if (s["as2"] > 0) != (s["as3"] > 0):
            raise Unsupported(f"frame {fr['name']}: ElasticTimoshenkoBeam needs both shear areas")
        if s["as2"] > 0 and (any(fr["rel_i"]) or any(fr["rel_j"])):
            raise Unsupported(f"frame {fr['name']}: ElasticTimoshenkoBeam has no releases")
        if s["as2"] > 0 and (any(fr["off_i"]) or any(fr["off_j"])):
            # Its transformation has no offset terms: the offset shortens the
            # element, with no rigid arm. Probed: MY = -P L instead of -P (L + a).
            raise Unsupported(f"frame {fr['name']}: ElasticTimoshenkoBeam ignores the arm of joint offsets")


def solve_case(m, loads):
    check_supported(m)
    for fr in m["frames"]:
        span = loads["frames"][fr["name"]]
        if fr["sec"]["as2"] > 0 and span["points"]:
            # Uniform loads work. A point load only prints "load type unknown" and
            # the analysis goes on without it, returning success.
            raise Unsupported(f"frame {fr['name']}: ElasticTimoshenkoBeam drops point loads")

    ops.wipe()
    ops.model("basic", "-ndm", 3, "-ndf", 6)
    tags = {j["name"]: k + 1 for k, j in enumerate(m["joints"])}
    for j in m["joints"]:
        ops.node(tags[j["name"]], *j["xyz"])

    # Imposed values become sp constraints: a dof cannot also carry a plain fix.
    moved = {name: [v != 0.0 for v in vals] for name, vals in loads["imposed"].items()}
    for j in m["joints"]:
        mv = moved.get(j["name"], [False] * 6)
        fix = [int(r and not x) for r, x in zip(j["restraint"], mv)]
        if any(fix):
            ops.fix(tags[j["name"]], *fix)

    next_tag = len(m["joints"]) + 1
    next_mat = 1
    next_ele = 1
    ground_of = {}
    for j in m["joints"]:
        dirs = [d for d in range(6) if j["springs"][d] > 0.0]
        if not dirs:
            continue
        ground = next_tag
        next_tag += 1
        ground_of[j["name"]] = ground
        ops.node(ground, *j["xyz"])
        mv = moved.get(j["name"], [False] * 6)
        ops.fix(ground, *[int(not (mv[d] and d in dirs)) for d in range(6)])
        mats = []
        for d in dirs:
            ops.uniaxialMaterial("Elastic", next_mat, j["springs"][d])
            mats.append(next_mat)
            next_mat += 1
        ops.element("zeroLength", next_ele, ground, tags[j["name"]], "-mat", *mats, "-dir", *[d + 1 for d in dirs])
        next_ele += 1

    ele_of = {}
    for k, fr in enumerate(m["frames"]):
        s, mat = fr["sec"], fr["mat"]
        # OpenSees local z is the vecxz direction, so its y and z are Struktal's 2 and 3.
        ops.geomTransf("Linear", k + 1, *fr["axes"][2], "-jntOffset", *fr["off_i"], *fr["off_j"])
        i, j = tags[fr["i"]], tags[fr["j"]]
        if s["as2"] > 0:
            ops.element("ElasticTimoshenkoBeam", next_ele, i, j, mat["e"], mat["g"], s["area"], s["j"], s["i22"], s["i33"],
                        s["as2"], s["as3"], k + 1)
        else:
            args = []
            rel_z = (1 if fr["rel_i"][5] else 0) + (2 if fr["rel_j"][5] else 0)
            rel_y = (1 if fr["rel_i"][4] else 0) + (2 if fr["rel_j"][4] else 0)
            if rel_z:
                args += ["-releasez", rel_z]
            if rel_y:
                args += ["-releasey", rel_y]
            ops.element("elasticBeamColumn", next_ele, i, j, s["area"], mat["e"], mat["g"], s["j"], s["i22"], s["i33"], k + 1, *args)
        ele_of[fr["name"]] = next_ele
        next_ele += 1

    for c in m.get("constraints", []):
        names = c["joints"]
        master, slaves = tags[names[0]], [tags[n] for n in names[1:]]
        if c["kind"] == "Diaphragm":
            ops.rigidDiaphragm("XYZ".index(c.get("axis", "Z")) + 1, master, *slaves)
        elif c["kind"] == "Body":
            for s_ in slaves:
                ops.rigidLink("beam", master, s_)
        else:
            dofs = [d + 1 for d in range(6) if c["components"][d]]
            for s_ in slaves:
                ops.equalDOF(master, s_, *dofs)

    ops.timeSeries("Constant", 1)
    ops.pattern("Plain", 1, 1)
    for name, vals in loads["joints"].items():
        ops.load(tags[name], *vals)
    for name, vals in loads["imposed"].items():
        j = m["by_joint"][name]
        for d, v in enumerate(vals):
            if v != 0.0:
                target = tags[name] if j["restraint"][d] else ground_of[name]
                ops.sp(target, d + 1, v)
    for fr in m["frames"]:
        span = loads["frames"][fr["name"]]
        e1, e2, e3 = fr["axes"]
        if any(v != 0.0 for v in span["uniform"]):
            w = span["uniform"]
            ops.eleLoad("-ele", ele_of[fr["name"]], "-type", "-beamUniform", dot(w, e2), dot(w, e3), dot(w, e1))
        for rel, g in span["points"]:
            ops.eleLoad("-ele", ele_of[fr["name"]], "-type", "-beamPoint", dot(g, e2), dot(g, e3), rel, dot(g, e1))

    ops.system("UmfPack")
    ops.numberer("RCM")
    ops.constraints("Transformation")
    ops.integrator("LoadControl", 1.0)
    ops.algorithm("Linear")
    ops.analysis("Static")
    if ops.analyze(1) != 0:
        raise RuntimeError("analysis failed: singular system")
    ops.reactions()

    out = {}
    for j in m["joints"]:
        tag = tags[j["name"]]
        disp = list(ops.nodeDisp(tag))
        reaction = list(ops.nodeReaction(tag))
        imposed = loads["imposed"].get(j["name"], [0.0] * 6)
        for d in range(6):
            if j["springs"][d] > 0.0:
                reaction[d] = j["springs"][d] * (imposed[d] - disp[d])
        out[j["name"]] = {"disp": disp, "reaction": reaction}
    return out


if __name__ == "__main__":
    main("OpenSeesPy", importlib.metadata.version("openseespy"), solve_case)
