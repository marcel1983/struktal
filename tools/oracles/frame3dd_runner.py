"""Solves a Struktal model with Frame3DD. External reference only.

Frame3DD takes the general cross-section head on — `Ax Asy Asz Jx Iy Iz E G roll
density` per element — so it covers the frames ccx cannot: a general section with
A, I22, I33 and J. It is a third implementation, independent of both OpenSees and
CalculiX.

Covered: 3D frames with elastic general sections, joint loads, uniform, point and
trapezoidal frame loads, and restraints on any global component.

Not translated: shells, end releases, rigid end offsets, partial fixity, joint
springs, general constraints, imposed displacements, modal and response
spectrum. Each one is `Unsupported`, because a difference caused by a translation
this runner invented would be counted against the engine.

Units are SI throughout (N, m, kg), the same `common.load_model` hands over, so
the results come back in N, m and radians and need no conversion.

Two conventions have to be translated, and both are checked against the bank:

* Struktal's local 2 is the global Z projected and its local 3 is -Y; Frame3DD
  keeps the global Y as close to its local 2 as the member allows. Its frame is
  Struktal's turned 90 degrees about the member, so a vector (a, b, c) in
  Struktal's local axes is (a, -c, b) for Frame3DD; see `_local`.
* The same turn applies to the section: its local 2 is Struktal's local 3 and
  its 3 is Struktal's 2, so the deck swaps I22 and I33 and leaves the roll alone.

Frame3DD prints six decimals, so `compare_engines.py` allows the printing step
on top of the relative tolerance (`ENGINE_ATOL`): the last digits are not in the
output.

Checked against the bundled cases (2026-11-15, Frame3DD 20140514+):

    V-LIN-001 LC1   match, 18 values
    V-LIN-002 LC1   match, 18 values
    V-LIN-004 LC1   match, 36 values
    V-LIN-005 LC1   match, 18 values
    V-TRAP-001 LC1  match, 66 values
    V-TRAP-002 LC1  match, 54 values
    V-TRAP-003 LC1  match, 18 values

Everything else is refused with a cause: releases, rigid offsets, constraints,
joint springs, interior moments, a frame the mesh translator will not split.

Three cases are known to differ and are *not* refused, so `compare_engines`
reports them as DIFF and not as n/a:

* V-TIM-001, three disconnected cantilevers: Frame3DD factorises them to NaN
  (RMS equilibrium -1.#IO), while a single cantilever from the same model solves.
* V-MOD-001 NONE, a free-free bar with no load: Struktal reports a mechanism,
  Frame3DD returns a rigid-body displacement of 2.0.
* V-SET-001 Q, a settlement case: Frame3DD stops while reading the reactions.
"""
import importlib.metadata
import math
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from common import Unsupported, dot, main  # noqa: E402

FRAME3DD = os.environ.get("FRAME3DD", "frame3dd")


def version():
    try:
        return importlib.metadata.version("frame3dd")
    except importlib.metadata.PackageNotFoundError:
        return "unknown"


def _fmt(value):
    return f"{value:.12g}"


def check_supported(m, loads):
    if m.get("constraints"):
        raise Unsupported("no general constraints")
    if not m.get("frames"):
        raise Unsupported("no frames to solve")

    for j in m["joints"]:
        if any(j["springs"]):
            raise Unsupported(f"joint {j['name']}: no grounded spring")
    for name in loads["imposed"]:
        raise Unsupported(f"joint {name}: no imposed displacement")
    for fr in m["frames"]:
        if any(fr["rel_i"]) or any(fr["rel_j"]):
            raise Unsupported(f"frame {fr['orig']}: no end releases")
        if any(fr["off_i"]) or any(fr["off_j"]):
            raise Unsupported(f"frame {fr['orig']}: no rigid end offsets")
        if fr["sec"].get("shape"):
            raise Unsupported(f"frame {fr['orig']}: parametric sections")
        span = loads["frames"][fr["name"]]
        if span["moments"] or span["moment_segments"]:
            # Frame3DD has no concentrated interior moment card: it would be
            # dropped without a word, and a missing load is a wrong result.
            raise Unsupported(f"frame {fr['orig']}: no concentrated interior moment")


def _local(load, axes):
    """A global vector per unit length or a global force, in Frame3DD coordinates.

    Struktal's local 2 is the global Z projected (the "Z up" convention) and its
    local 3 is then -Y. Frame3DD keeps the global Y as close to its local 2 as
    the member allows, so its frame is Struktal's turned 90 degrees about the
    member: (a, b, c) in Struktal's local axes is (a, -c, b) for Frame3DD.
    Verified by agreement with Struktal on V-TRAP-001..003 and V-LIN-004.
    """
    return [dot(load, axes[0]), -dot(load, axes[2]), dot(load, axes[1])]


def deck(m, loads):
    """The Frame3DD input file, as text."""
    nodes = {j["name"]: index + 1 for index, j in enumerate(m["joints"])}
    elements = {fr["name"]: index + 1 for index, fr in enumerate(m["frames"])}
    lines = ["struktal model", str(len(m["joints"]))]
    for j in m["joints"]:
        lines.append(f"{nodes[j['name']]} {_fmt(j['xyz'][0])} {_fmt(j['xyz'][1])} {_fmt(j['xyz'][2])} 0")

    fixed = [j for j in m["joints"] if any(j["restraint"])]
    lines.append(str(len(fixed)))
    for j in fixed:
        flags = " ".join("1" if r else "0" for r in j["restraint"])
        lines.append(f"{nodes[j['name']]} {flags}")

    lines.append(str(len(m["frames"])))
    for fr in m["frames"]:
        sec, mat = fr["sec"], fr["mat"]
        # Frame3DD refuses a non-positive density even for a static run; a
        # placeholder keeps the deck readable and statics never uses it.
        density = mat.get("mass_density") or 0.0
        lines.append(
            " ".join(
                [
                    str(elements[fr["name"]]),
                    str(nodes[fr["i"]]),
                    str(nodes[fr["j"]]),
                    _fmt(sec["area"]),
                    _fmt(sec.get("as2") or 0.0),
                    _fmt(sec.get("as3") or 0.0),
                    _fmt(sec["j"]),
                    # Frame3DD's local 2 is Struktal's local 3 and its 3 is
                    # Struktal's 2, so the two inertias swap places.
                    _fmt(sec["i33"]),
                    _fmt(sec["i22"]),
                    _fmt(mat["e"]),
                    _fmt(mat["g"]),
                    _fmt(fr.get("angle_deg", 0.0)),
                    _fmt(density if density > 0.0 else 1.0),
                ]
            )
        )

    # Shear and geometric stiffness are Struktal's: Timoshenko when As > 0.
    shear = any(fr["sec"].get("as2") or fr["sec"].get("as3") for fr in m["frames"])
    lines.append("1" if shear else "0")
    lines.append("0")  # no geometric stiffness in a linear static run
    lines.append("50.0")
    lines.append("1.0")
    lines.append("-1")  # skip internal force stations: results are per joint

    # The runner is called once per case with that case's loads, so the deck
    # always carries exactly one load case.
    lines.append("1")
    lines.append("0 0 0")  # self weight reaches the elements as a uniform load
    lines.append(str(len(loads["joints"])))
    for name, values in sorted(loads["joints"].items()):
        if name in nodes:
            lines.append(f"{nodes[name]} " + " ".join(_fmt(v) for v in values))

    uniform, points, segments = [], [], []
    for fr in m["frames"]:
        span = loads["frames"][fr["name"]]
        axes = fr["axes"]
        u = _local(span["uniform"], axes)
        if any(v != 0.0 for v in u):
            uniform.append(f"{elements[fr['name']]} " + " ".join(_fmt(v) for v in u))
        for rel, g in span["points"]:
            local = _local(g, axes)
            points.append(
                f"{elements[fr['name']]} "
                + " ".join(_fmt(v) for v in local)
                + f" {_fmt(rel * fr['length'])}"
            )
        for start, end, va, vb in span["segments"]:
            a, b = _local(va, axes), _local(vb, axes)
            for axis in range(3):
                # Only the first line carries the element number: Frame3DD reads
                # the next two as continuation lines of the same element.
                head = f"{elements[fr['name']]} " if axis == 0 else ""
                segments.append(
                    f"{head}{_fmt(start * fr['length'])} {_fmt(end * fr['length'])} "
                    f"{_fmt(a[axis])} {_fmt(b[axis])}"
                )
    lines.append(str(len(uniform)))
    lines.extend(uniform)
    # nW counts elements, and each one writes three lines (one per local axis).
    lines.append(str(len(segments) // 3))
    lines.extend(segments)
    lines.append(str(len(points)))
    lines.extend(points)
    lines.append("0")  # temperature
    lines.append("0")  # prescribed displacements
    lines.append("0")  # dynamic modes
    return "\n".join(lines) + "\n", nodes


def _blocks(text, title):
    """Rows after every `title` line, in order, as lists of floats."""
    out = []
    for chunk in text.split(title)[1:]:
        rows = []
        for line in chunk.splitlines():
            parts = line.split()
            if not parts:
                if rows:
                    break
                continue
            try:
                rows.append([float(v) for v in parts])
            except ValueError:
                if rows:
                    break
        out.append(rows)
    return out


def _parse(out, nodes):
    """Displacements and reactions per joint, in global axes."""
    disp = _blocks(out, "N O D E   D I S P L A C E M E N T S")
    react = _blocks(out, "R E A C T I O N S")
    by_node = {number: name for name, number in nodes.items()}
    cases = {}
    for index in range(max(len(disp), len(react))):
        # Start from every joint: Frame3DD prints no line for a node with no
        # element, and a node with no element does not move.
        joints = {name: {"disp": [0.0] * 6, "reaction": [0.0] * 6} for name in nodes}
        for rows, keys in ((disp[index] if index < len(disp) else [], "disp"),
                           (react[index] if index < len(react) else [], "reaction")):
            for row in rows:
                if len(row) < 7:
                    continue
                name = by_node.get(int(row[0]))
                if name is None:
                    continue
                joints[name][keys] = row[1:7]
        cases[index] = joints
    return cases


def solve_case(m, loads):
    check_supported(m, loads)
    text, nodes = deck(m, loads)
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        (work / "model.3dd").write_text(text, encoding="utf-8")
        try:
            run = subprocess.run(
                [FRAME3DD, "model.3dd", "out.txt"], cwd=work, capture_output=True, text=True, timeout=900
            )
        except FileNotFoundError:
            raise Unsupported("frame3dd is not on PATH: set FRAME3DD or install Frame3DD") from None
        except subprocess.TimeoutExpired:
            raise Unsupported("frame3dd did not finish") from None
        out = work / "out.txt"
        if not out.exists():
            tail = (run.stdout or run.stderr or "").strip().splitlines()[-1:] or ["no output"]
            raise RuntimeError(f"frame3dd produced no output: {tail[0][:200]}")
        # One load case per deck, so the first block is the answer.
        return _parse(out.read_text(encoding="utf-8", errors="replace"), nodes).get(0, {})



if __name__ == "__main__":
    main("frame3dd", version(), solve_case)
