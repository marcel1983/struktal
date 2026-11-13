"""Solves a Struktal model with OpenSeesPy. External reference only: no OpenSees code
is used by the engine. Needs Python 3.12 with openseespy 3.8 (decision 0013)."""
import importlib.metadata
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import openseespy.opensees as ops  # noqa: E402

from common import Unsupported, combine_modes, dot, empty_loads, lumped_mass, main, spectrum_acceleration  # noqa: E402


def is_link(fr):
    """Both ends pinned in bending and torsion released at one: only axial force,
    exactly a truss, whatever its shear areas (its span loads go to the ends)."""
    ri, rj = fr["rel_i"], fr["rel_j"]
    return (all(ri[4:]) and all(rj[4:]) and (ri[3] or rj[3]) and not any(ri[:3]) and not any(rj[:3])
            and not fr["rel_has_springs"])


def to_ends(fr, span):
    """Span loads of a pinned bar as simple-span reactions at its ends, global."""
    if span["segments"] or span["moments"] or span["moment_segments"]:
        raise Unsupported(f"frame {fr['orig']}: only uniform and point loads on a pinned bar")
    ends = [[0.0] * 3, [0.0] * 3]
    for k in range(3):
        ends[0][k] += span["uniform"][k] * fr["length"] / 2
        ends[1][k] += span["uniform"][k] * fr["length"] / 2
        for rel, g in span["points"]:
            ends[0][k] += g[k] * (1 - rel)
            ends[1][k] += g[k] * rel
    return ends


def check_supported(m):
    for fr in m["frames"]:
        if is_link(fr):
            if any(fr["off_i"]) or any(fr["off_j"]):
                raise Unsupported(f"frame {fr['orig']}: a pinned bar with offsets")
            continue
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


def eigen(m, modes):
    """Eigenvalues and joint vectors with the lumped masses of common.lumped_mass."""
    lumped = lumped_mass(m)
    solve_case(m, empty_loads(m))
    tags = {j["name"]: k + 1 for k, j in enumerate(m["joints"])}
    for name, mass in lumped.items():
        if any(mass):
            ops.mass(tags[name], *mass)
    with_mass = sum(1 for j in m["joints"] for d in range(6) if lumped[j["name"]][d] > 0 and not j["restraint"][d])
    values = ops.eigen("-fullGenLapack", min(modes, with_mass))
    vectors = [{j["name"]: list(ops.nodeEigenvector(tags[j["name"]], k + 1)) for j in m["joints"]} for k in range(len(values))]
    return values, vectors


def solve_modes(m, modes):
    return [2 * math.pi / math.sqrt(v) for v in eigen(m, modes)[0]]


def solve_spectrum(m, case):
    """Each mode as the static response to its inertia forces M phi Gamma Sa, which
    gives phi Gamma Sa / w^2 and its reactions; then SRSS or CQC."""
    modal = next(c for c in m["load_cases"] if c["name"] == case["modal_case"])
    function = next(f for f in m["response_spectra"] if f["name"] == case["function"])
    values, vectors = eigen(m, modal["modes"])
    lumped = lumped_mass(m)
    d = "XYZ".index(case["direction"])
    per_mode, omegas = [], []
    for value, phi in zip(values, vectors):
        w = math.sqrt(value)
        omegas.append(w)
        norm = sum(lumped[n][k] * phi[n][k] ** 2 for n in phi for k in range(6))
        gamma = sum(lumped[n][d] * phi[n][d] for n in phi) / norm
        sa = spectrum_acceleration(function["points"], 2 * math.pi / w) * case["scale"]
        loads = empty_loads(m)
        for n in phi:
            free = [not r for r in m["by_joint"][n]["restraint"]]
            f = [lumped[n][k] * phi[n][k] * gamma * sa if free[k] else 0.0 for k in range(6)]
            if any(f):
                loads["joints"][n] = f
        per_mode.append(solve_case(m, loads))
    return combine_modes(per_mode, omegas, case)


def solve_case(m, loads):
    check_supported(m)
    joint_loads = {name: list(v) for name, v in loads["joints"].items()}
    for fr in m["frames"]:
        span = loads["frames"][fr["name"]]
        if is_link(fr):
            for end, f in zip((fr["i"], fr["j"]), to_ends(fr, span)):
                t = joint_loads.setdefault(end, [0.0] * 6)
                joint_loads[end] = [t[k] + (f[k] if k < 3 else 0.0) for k in range(6)]
            continue
        if fr["sec"]["as2"] > 0 and (span["points"] or span["segments"]):
            # Uniform loads work. Any other span load only prints "load type unknown"
            # and the analysis goes on without it, returning success.
            raise Unsupported(f"frame {fr['name']}: ElasticTimoshenkoBeam drops point and partial loads")
        if span["moments"] or span["moment_segments"]:
            raise Unsupported(f"frame {fr['name']}: no span moments in eleLoad")
        if any(a != b for _, _, a, b in span["segments"]):
            # Probed on elasticBeamColumn: end values are accepted and the load is lost.
            raise Unsupported(f"frame {fr['name']}: no linearly varying span loads")

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
        if is_link(fr):
            ops.uniaxialMaterial("Elastic", next_mat, mat["e"])
            ops.element("Truss", next_ele, i, j, s["area"], next_mat)
            next_mat += 1
        elif s["as2"] > 0:
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

    for k, s in enumerate(m.get("area_sections", [])):
        if s.get("bending_thickness") is not None:
            raise Unsupported(f"area section {s['name']}: a separate bending thickness")
        s["tag"] = 1000 + k
        ops.section("ElasticMembranePlateSection", s["tag"], s["mat"]["e"], s["mat"]["nu"], s["thickness"], 0.0)
    for a in m.get("areas", []):
        kind = "ASDShellT3" if len(a["joints"]) == 3 else "ShellMITC4"
        ops.element(kind, next_ele, *[tags[n] for n in a["joints"]], a["sec"]["tag"])
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
    for name, vals in joint_loads.items():
        ops.load(tags[name], *vals)
    for name, vals in loads["imposed"].items():
        j = m["by_joint"][name]
        for d, v in enumerate(vals):
            if v != 0.0:
                target = tags[name] if j["restraint"][d] else ground_of[name]
                ops.sp(target, d + 1, v)
    for fr in m["frames"]:
        if is_link(fr):
            continue
        span = loads["frames"][fr["name"]]
        e1, e2, e3 = fr["axes"]
        if any(v != 0.0 for v in span["uniform"]):
            w = span["uniform"]
            ops.eleLoad("-ele", ele_of[fr["name"]], "-type", "-beamUniform", dot(w, e2), dot(w, e3), dot(w, e1))
        for rel, g in span["points"]:
            ops.eleLoad("-ele", ele_of[fr["name"]], "-type", "-beamPoint", dot(g, e2), dot(g, e3), rel, dot(g, e1))
        for start, end, w, _ in span["segments"]:
            ops.eleLoad("-ele", ele_of[fr["name"]], "-type", "-beamUniform", dot(w, e2), dot(w, e3), dot(w, e1), start, end)

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
    main("OpenSeesPy", importlib.metadata.version("openseespy"), solve_case, solve_modes, solve_spectrum)
