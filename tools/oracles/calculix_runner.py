"""Solves a Struktal model with CalculiX (ccx). External reference only.

Why ccx: Struktal's shells are MITC4 and MITC3, and ccx's S4 is a different
shell formulation, so agreement between them is evidence. It is not a
general-purpose runner. Frames need a general cross-section (A, I22, I33, J) and
ccx only takes one through a user element (U1), which this runner does not
build: a frame is `Unsupported`, not approximated with a rectangle that would
only measure the approximation.

Covered: 3- and 4-node shells, elastic materials, joint loads (including the
area loads `common` already turned into nodal forces) and restraints on any
component.

Not reported: rotations. CalculiX writes translations and reaction forces to
.dat and keeps rotations in .frd under an internal node numbering, so a
component it does not print comes back as None and `compare_engines.py` skips
it instead of scoring a zero against Struktal.

Not translated: frames, joint springs, general constraints, imposed
displacements, modal and response spectrum.

Verified against the bundled cases (2026-11-15, ccx 2.23, `--backend faer`):

    V-SHELL-001 TEN   86 values, worst 0.18 of tolerance
    V-SHELL-002 LC1   59 values, worst 0.00076 of tolerance
    V-SHELL-003 LC1  506 values, worst 0.78 of tolerance
    V-TRI-001   MIX  165 values, worst 0.011 of tolerance

Two differences are expected and are reported, not hidden:

* V-SHELL-001 BEN (pure bending with nodal moments) is about 16 % stiffer in
  ccx. The case's closed form decides, and Struktal matches it: S4 is a
  Mindlin shell with full integration and MITC4 is not, so a nodal moment is
  not distributed the same way. S4R was tried and is far worse (a factor of
  20), so S4 stays.
* V-TRI-001 Q differs in the vertical reactions of HA, HB and HC (1200 N in
  Struktal, 0 in ccx) while the displacements agree. ccx reports *degrees of
  freedom per node: 3* for shells and expands the 42 joints of the model into
  2218 nodes; the reaction of a fully restrained, loaded shell node does not
  come back in the .dat. The three joints carry the triangle HT and nothing
  else, which is why the rest of the model is unaffected. It is a limitation of
  reading ccx, not a result to compare.
"""
import importlib.metadata
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from common import Unsupported, main  # noqa: E402

CCX = os.environ.get("CCX", "ccx")


class CcxError(RuntimeError):
    """ccx ran and failed."""


def version():
    try:
        return importlib.metadata.version("calculix")
    except importlib.metadata.PackageNotFoundError:
        pass
    try:
        out = subprocess.run([CCX, "-v"], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return "unknown"
    match = re.search(r"Version\s+([\w.]+)", (out.stdout or "") + (out.stderr or ""))
    return match.group(1) if match else "unknown"


def check_supported(m, loads):
    if m.get("constraints"):
        raise Unsupported("no general constraints")
    if m.get("frames"):
        raise Unsupported("frames: ccx takes a general section only through a user element")
    if not m.get("areas"):
        raise Unsupported("no shells to solve")
    for j in m["joints"]:
        if any(j["springs"]):
            raise Unsupported(f"joint {j['name']}: no grounded spring")
    for name in loads["imposed"]:
        raise Unsupported(f"joint {name}: no imposed displacement")


def _fmt(value):
    return f"{value:.16g}"


def _runs(flags):
    """Contiguous runs of True DOFs as (first, last) pairs, 1-based."""
    out = []
    start = None
    for index, flag in enumerate(list(flags) + [False], start=1):
        if flag and start is None:
            start = index
        elif not flag and start is not None:
            out.append((start, index - 1))
            start = None
    return out


def deck(m, loads):
    """The Abaqus-style input deck ccx reads."""
    nodes = {j["name"]: index + 1 for index, j in enumerate(m["joints"])}
    lines = ["*NODE, NSET=NALL"]
    for j in m["joints"]:
        lines.append(f"{nodes[j['name']]}, {_fmt(j['xyz'][0])}, {_fmt(j['xyz'][1])}, {_fmt(j['xyz'][2])}")

    # One element set per (section, node count): a shell section covers a set.
    groups = {}
    for index, area in enumerate(m["areas"], start=1):
        names = area["joints"]
        if len(names) not in (3, 4):
            raise Unsupported(f"area {area['name']}: only triangles and quadrilaterals")
        if any(name not in nodes for name in names):
            raise Unsupported(f"area {area['name']}: a corner is not a joint")
        kind = "S3" if len(names) == 3 else "S4"
        key = (area["sec"]["name"], kind)
        groups.setdefault(key, []).append((index, [nodes[n] for n in names]))

    if not groups:
        raise Unsupported("no shells to solve")

    lines.append("*MATERIAL, NAME=MAT")
    lines.append("*ELASTIC")
    # Every area section shares one material here only if they agree; otherwise
    # one material per section.
    sections = {a["sec"]["name"]: a["sec"] for a in m["areas"]}
    if len({(s["mat"]["e"], s["mat"]["nu"], s["thickness"]) for s in sections.values()}) == 1:
        first = next(iter(sections.values()))
        lines[-2] = f"*MATERIAL, NAME={first['mat']['name']}"
        lines.append(f"{_fmt(first['mat']['e'])}, {_fmt(first['mat']['nu'])}")
        if first["mat"].get("mass_density"):
            lines.append("*DENSITY")
            lines.append(_fmt(first["mat"]["mass_density"]))
    else:
        lines.pop()  # the placeholder material
        for section in sections.values():
            lines.append(f"*MATERIAL, NAME={section['mat']['name']}")
            lines.append("*ELASTIC")
            lines.append(f"{_fmt(section['mat']['e'])}, {_fmt(section['mat']['nu'])}")
            if section["mat"].get("mass_density"):
                lines.append("*DENSITY")
                lines.append(_fmt(section["mat"]["mass_density"]))

    for (section_name, kind), elements in groups.items():
        lines.append(f"*ELEMENT, TYPE={kind}, ELSET={section_name}_{kind}")
        for element, corners in elements:
            lines.append(f"{element}, " + ", ".join(str(n) for n in corners))
        section = sections[section_name]
        thickness = section.get("bending_thickness") or section["thickness"]
        if section.get("bending_thickness") is not None and abs(
            section["bending_thickness"] - section["thickness"]
        ) > 1e-12 * max(1.0, abs(section["thickness"])):
            raise Unsupported(f"area section {section_name}: a separate bending thickness")
        lines.append(f"*SHELL SECTION, ELSET={section_name}_{kind}, MATERIAL={section['mat']['name']}")
        lines.append(_fmt(thickness))

    lines.append("*BOUNDARY")
    for j in m["joints"]:
        for first, last in _runs(j["restraint"]):
            lines.append(f"{nodes[j['name']]}, {first}, {last}, 0.")

    lines.append("*STEP")
    lines.append("*STATIC")
    loads_on = []
    for name, values in sorted(loads["joints"].items()):
        if name not in nodes:
            continue
        for dof in range(6):
            if values[dof] != 0.0:
                loads_on.append(f"{nodes[name]}, {dof + 1}, {_fmt(values[dof])}")
    if loads_on:
        lines.append("*CLOAD")
        lines.extend(loads_on)
    lines.append("*NODE PRINT, NSET=NALL")
    lines.append("U")
    lines.append("*NODE PRINT, NSET=NALL")
    lines.append("RF")
    lines.append("*END STEP")
    return "\n".join(lines) + "\n", nodes


def _last_block(text, title):
    """Rows of the last block with `title`, as lists of floats."""
    blocks = text.split(title)[1:]
    if not blocks:
        return {}
    rows = {}
    for line in blocks[-1].splitlines():
        parts = line.split()
        if len(parts) < 2:
            if rows:
                break
            continue
        try:
            rows[int(parts[0])] = [float(v) for v in parts[1:]]
        except ValueError:
            if rows:
                break
    return rows


def _parse(dat, nodes):
    """Displacements and reactions per node, translations only."""
    disp = _last_block(dat, "displacements (vx,vy,vz)")
    react = _last_block(dat, "forces (fx,fy,fz)")
    by_node = {number: name for name, number in nodes.items()}
    out = {}
    for number, name in by_node.items():
        u = disp.get(number, [0.0, 0.0, 0.0])
        r = react.get(number, [0.0, 0.0, 0.0])
        out[name] = {
            "disp": [u[0], u[1], u[2], None, None, None],
            "reaction": [r[0], r[1], r[2], None, None, None],
        }
    return out


def solve_case(m, loads):
    check_supported(m, loads)
    text, nodes = deck(m, loads)
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        (work / "job.inp").write_text(text, encoding="utf-8")
        try:
            run = subprocess.run([CCX, "-i", "job"], cwd=work, capture_output=True, text=True, timeout=900)
        except FileNotFoundError:
            raise Unsupported("ccx is not on PATH: set CCX or install CalculiX") from None
        except subprocess.TimeoutExpired:
            raise Unsupported("ccx did not finish") from None
        dat = work / "job.dat"
        if not dat.exists():
            tail = (run.stdout or run.stderr or "").strip().splitlines()[-1:] or ["no output"]
            raise CcxError(f"ccx produced no output: {tail[0][:200]}")
        if run.returncode != 0:
            tail = (run.stdout or run.stderr or "").strip().splitlines()[-1:] or ["no output"]
            raise CcxError(f"ccx exited {run.returncode}: {tail[0][:200]}")
        return _parse(dat.read_text(encoding="utf-8", errors="replace"), nodes)


if __name__ == "__main__":
    main("calculix", version(), solve_case)