#!/usr/bin/env python3
"""Compares what two readers find in the same S2K file.

The reference is the file itself, read by `fingerprint_from_s2k`: joint
coordinates, frame and area connectivity, total frame length, total area, and
the resultant of every load pattern and every static case. No engine, no
section property and no axes convention goes into it, so a difference means one
of the readers read the file differently.

What it compares:

* `struktal import` (the engine's own reader), through `--struktal`;
* OpenSees Studio's reader, through its own S2K export (`--opensees-studio`:
  it needs `opensees_studio` importable, and only the round trip is checked).

Known asymmetry of that round trip: OpenSees Studio's exporter writes the
object, not its mesh, so a file with `OBJECTS AND ELEMENTS - AREAS` comes back
with one area instead of its elements. The struktal side is the one compared by
default, and it keeps the mesh.

Only what the file states is compared. A row the reference reader does not
understand is counted under `unhandled` and left out; it is never guessed.

    python tools/diff_s2k_import.py verification/s2k/*.s2k \
        --struktal target/debug/struktal

Exit code 1 if any compared field differs, 2 on a file that cannot be read.
"""
from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
import tempfile
from pathlib import Path

# Force and length units SAP2000 writes in CurrUnits, in SI. A unit this reader
# does not know is an error: guessing a factor is how a load becomes ten times
# what the file says.
FORCE = {
    "N": 1.0, "KN": 1e3, "KG": 9.80665, "KGF": 9.80665,
    "TON": 9806.65, "LBF": 4.4482216152605, "KIP": 4448.2216152605,
}
LENGTH = {"M": 1.0, "CM": 0.01, "MM": 0.001, "FT": 0.3048, "IN": 0.0254}

# Tables this reader uses. Anything else is reported, never read.
TABLE_JOINTS = "JOINT COORDINATES"
TABLE_FRAMES = "CONNECTIVITY - FRAME"
TABLE_AREAS = "CONNECTIVITY - AREA"
TABLE_PATTERNS = "LOAD PATTERN DEFINITIONS"
TABLE_JOINT_LOADS = "JOINT LOADS - FORCE"
TABLE_FRAME_LOADS = ("FRAME LOADS - DISTRIBUTED", "FRAME LOADS - POINT")
TABLE_CASES = "LOAD CASE DEFINITIONS"
TABLE_CASE_LOADS = "CASE - STATIC 1 - LOAD ASSIGNMENTS"


class S2kError(RuntimeError):
    """The file cannot be read at all."""


def read_tables(text: str) -> dict[str, list[dict[str, str]]]:
    """Tables of an S2K file: name -> rows of key/value fields.

    A trailing ` _` continues a row on the next line, and `""` inside quotes is
    a literal quote. Rows that are not Key=Value are skipped: a real export
    always carries a header line before the first table.
    """
    tables: dict[str, list[dict[str, str]]] = {}
    current: list[dict[str, str]] | None = None
    pending: tuple[int, str] | None = None
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.rstrip()
        if pending is None:
            lead = line.strip()
            if lead.startswith("TABLE:"):
                name = lead[len("TABLE:"):].strip()
                if name.startswith('"') and name.endswith('"'):
                    name = name[1:-1]
                current = tables.setdefault(name, [])
                continue
            if lead == "END TABLE DATA":
                current = None
                continue
            if current is None:
                continue
        body, continues = (line[:-1], True) if line.endswith(" _") else (line, False)
        pending = (number, body) if pending is None else (pending[0], f"{pending[1]} {body}")
        if continues:
            continue
        start, joined = pending
        pending = None
        row: dict[str, str] = {}
        for field in _split_fields(joined, start):
            key, _, value = field.partition("=")
            if key and key not in row:
                row[key] = _unquote(value)
        current.append(row)
    if pending is not None:
        raise S2kError(f"the file ends inside a row continued at line {pending[0]}")
    return tables


def _unquote(value: str) -> str:
    """Drops the quotes of a quoted field; `""` is already a literal quote."""
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        return value[1:-1]
    return value


def _split_fields(text: str, line: int) -> list[str]:
    """Splits one logical line on whitespace outside quotes."""
    out: list[str] = []
    current = ""
    quoted = False
    index = 0
    while index < len(text):
        char = text[index]
        if char == '"':
            if quoted and index + 1 < len(text) and text[index + 1] == '"':
                current += '"'
                index += 2
                continue
            quoted = not quoted
            current += char
        elif char.isspace() and not quoted:
            if current:
                out.append(current)
                current = ""
        else:
            current += char
        index += 1
    if quoted:
        raise S2kError(f"unterminated quote in the row at line {line}")
    if current:
        out.append(current)
    return out


def unit_scale(units: str) -> tuple[float, float]:
    """`CurrUnits` as (force to N, length to m)."""
    force = next((FORCE[k] for k in _upper(units) if k in FORCE), None)
    length = next((LENGTH[k] for k in _upper(units) if k in LENGTH), None)
    if force is None or length is None:
        raise S2kError(f"unknown CurrUnits {units!r}")
    return force, length


def _upper(units: str) -> list[str]:
    return [part.strip().upper() for part in units.split(",") if part.strip()]


def _number(row: dict[str, str], key: str, default: float = 0.0) -> float:
    raw = row.get(key)
    if raw is None or raw == "":
        return default
    try:
        value = float(raw)
    except ValueError:
        return default
    return value if math.isfinite(value) else default


def _point(row: dict[str, str], keys: tuple[str, str, str], scale: float) -> tuple[float, float, float] | None:
    if any(k not in row for k in keys):
        return None
    return tuple(_number(row, k) * scale for k in keys)  # type: ignore[return-value]


def _area(points: list[tuple[float, float, float]]) -> float:
    """Area of a planar polygon, by Newell's method."""
    if len(points) < 3:
        return 0.0
    total = [0.0, 0.0, 0.0]
    for index, a in enumerate(points):
        b = points[(index + 1) % len(points)]
        total[0] += a[1] * b[2] - a[2] * b[1]
        total[1] += a[2] * b[0] - a[0] * b[2]
        total[2] += a[0] * b[1] - a[1] * b[0]
    return 0.5 * math.sqrt(sum(v * v for v in total))


def _geometry(joints: dict, frames: list[tuple[str, str]], areas: list[list[str]]) -> dict:
    points = list(joints.values())
    bbox = [[min(p[k] for p in points), max(p[k] for p in points)] for k in range(3)] if points else []
    length = sum(math.dist(joints[i], joints[j]) for i, j in frames if i in joints and j in joints)
    area = sum(_area([joints[n] for n in names if n in joints]) for names in areas)
    return {
        "bbox": [[round(v, 9) for v in pair] for pair in bbox],
        "frame_length_m": round(length, 9),
        "area_m2": round(area, 9),
    }


def fingerprint_from_s2k(path: Path) -> dict:
    """What the file itself says, in SI: the reference every reader is compared to."""
    tables = read_tables(path.read_text(encoding="utf-8", errors="replace"))

    def rows(name: str) -> list[dict[str, str]]:
        return tables.get(name, [])

    control = rows("PROGRAM CONTROL")
    force_scale, length_scale = unit_scale(control[0].get("CurrUnits", "") if control else "")

    joints: dict[str, tuple[float, float, float]] = {}
    unhandled = {"coordinates": 0, "curved_frames": 0, "joint_loads": 0}
    for row in rows(TABLE_JOINTS):
        name = row.get("Joint")
        if not name:
            continue
        if row.get("CoordType", "Cartesian") != "Cartesian":
            unhandled["coordinates"] += 1
            continue
        point = _point(row, ("XorR", "Y", "Z"), length_scale)
        if point is not None:
            joints[name] = point

    frames: list[tuple[str, str]] = []
    for row in rows(TABLE_FRAMES):
        i, j = row.get("JointI"), row.get("JointJ")
        if not i or not j:
            continue
        if row.get("IsCurved", "No") == "Yes":
            unhandled["curved_frames"] += 1
            continue
        frames.append((i, j))

    # A file may carry its own mesh (decision 0021): its joints are the elements'
    # joints, and its areas are the elements of OBJECTS AND ELEMENTS - AREAS.
    for row in rows("OBJECTS AND ELEMENTS - JOINTS"):
        name = row.get("JointElem")
        if not name:
            continue
        point = _point(row, ("GlobalX", "GlobalY", "GlobalZ"), length_scale)
        if point is not None:
            joints[name] = point

    areas: list[list[str]] = []
    for row in rows(TABLE_AREAS):
        count = int(_number(row, "NumJoints", 0)) or sum(1 for k in row if k.startswith("Joint"))
        names = [row[f"Joint{k}"] for k in range(1, count + 1) if row.get(f"Joint{k}")]
        if len(names) >= 3:
            areas.append(names)
    mesh = rows("OBJECTS AND ELEMENTS - AREAS")
    if mesh:
        areas = []
        for row in mesh:
            count = sum(1 for k in row if k.startswith("ElemJt"))
            names = [row[f"ElemJt{k}"] for k in range(1, count + 1) if row.get(f"ElemJt{k}")]
            if len(names) >= 3:
                areas.append(names)

    patterns = {r["LoadPat"]: _number(r, "SelfWtMult", 0.0) for r in rows(TABLE_PATTERNS) if r.get("LoadPat")}

    joint_loads: dict[str, dict] = {}
    for row in rows(TABLE_JOINT_LOADS):
        pattern = row.get("LoadPat")
        if not pattern:
            continue
        if row.get("CoordSys", "GLOBAL") != "GLOBAL":
            unhandled["joint_loads"] += 1
            continue
        entry = joint_loads.setdefault(pattern, {"n": 0, "F": [0.0] * 3, "M": [0.0] * 3})
        entry["n"] += 1
        for index, key in enumerate(("F1", "F2", "F3")):
            entry["F"][index] += _number(row, key) * force_scale
        for index, key in enumerate(("M1", "M2", "M3")):
            entry["M"][index] += _number(row, key) * force_scale * length_scale

    frame_loads: dict[str, dict] = {}
    members = {r.get("Frame"): (r.get("JointI"), r.get("JointJ")) for r in rows(TABLE_FRAMES)}
    for name in TABLE_FRAME_LOADS:
        for row in rows(name):
            pattern, frame = row.get("LoadPat"), row.get("Frame")
            if not pattern or not frame or row.get("LoadType", "Force") != "Force":
                continue  # self weight belongs to the sections, not to this table
            ends = members.get(frame)
            a, b = (joints.get(ends[0]), joints.get(ends[1])) if ends else (None, None)
            if a is None or b is None:
                continue
            entry = frame_loads.setdefault(pattern, {"n": 0, "N": 0.0, "Nm": 0.0})
            entry["n"] += 1
            length = math.dist(a, b)
            if row.get("Val1") is not None or row.get("Dist1") is not None:
                start, end = _number(row, "Dist1", 0.0), _number(row, "Dist2", 1.0)
                w1, w2 = _number(row, "Val1"), _number(row, "Val2", _number(row, "Val1"))
                entry["N"] += abs((w1 + w2) / 2.0) * abs(end - start) * length * force_scale
            else:
                entry["N"] += abs(_number(row, "Val")) * force_scale

    kinds = {r.get("Case"): r.get("Type") for r in rows(TABLE_CASES)}
    cases: dict[str, list[list]] = {}
    for row in rows(TABLE_CASE_LOADS):
        case, pattern = row.get("Case"), row.get("LoadName")
        if not case or not pattern or kinds.get(case) not in (None, "LinStatic"):
            continue
        cases.setdefault(case, []).append([pattern, _number(row, "LoadSF", 1.0)])

    return {
        "units": control[0].get("CurrUnits", "") if control else "",
        "counts": {
            "joints": len(joints), "frames": len(frames), "areas": len(areas),
            "load_patterns": len(patterns),
        },
        "geometry": _geometry(joints, frames, areas),
        "patterns": _rounded(patterns),
        "joint_loads": {
            name: {"n": v["n"], "F": _rounded(v["F"]), "M": _rounded(v["M"])}
            for name, v in sorted(joint_loads.items())
        },
        "frame_loads": {
            name: {"n": v["n"], "N": round(v["N"], 9), "Nm": round(v["Nm"], 9)}
            for name, v in sorted(frame_loads.items())
        },
        "cases": {name: sorted(value) for name, value in sorted(cases.items())},
        "unhandled": unhandled,
        "_source": str(path),
    }


def fingerprint_from_struktal(path: Path) -> dict:
    """The same quantities, from a model written by `struktal import`."""
    data = json.loads(path.read_text(encoding="utf-8"))
    units = data.get("units") or {}
    force_scale, length_scale = unit_scale(f"{units.get('force', 'N')}, {units.get('length', 'm')}")

    joints = {
        j["name"]: (j["x"] * length_scale, j["y"] * length_scale, j["z"] * length_scale)
        for j in data.get("joints", [])
    }
    frames = [(f["i"], f["j"]) for f in data.get("frames", [])]
    areas = [a.get("joints", []) for a in data.get("areas", [])]
    patterns = {p["name"]: p.get("self_weight_multiplier", 0.0) for p in data.get("load_patterns", [])}

    joint_loads: dict[str, dict] = {}
    for load in data.get("joint_loads", []):
        entry = joint_loads.setdefault(load["pattern"], {"n": 0, "F": [0.0] * 3, "M": [0.0] * 3})
        values = load["values"]
        entry["n"] += 1
        for index in range(3):
            entry["F"][index] += values[index] * force_scale
            entry["M"][index] += values[index + 3] * force_scale * length_scale

    frame_loads: dict[str, dict] = {}
    members = {f["name"]: (f["i"], f["j"]) for f in data.get("frames", [])}
    for load in data.get("frame_loads", []):
        entry = frame_loads.setdefault(load["pattern"], {"n": 0, "N": 0.0, "Nm": 0.0})
        entry["n"] += 1
        kind = load["load"]
        ends = members.get(load["frame"])
        length = math.dist(joints[ends[0]], joints[ends[1]]) if ends and ends[0] in joints and ends[1] in joints else 0.0
        if kind["kind"] == "Uniform":
            entry["N"] += abs(kind["value"]) * length * force_scale
        elif kind["kind"] == "Trapezoidal":
            span = abs(kind["rel_end"] - kind["rel_start"])
            entry["N"] += abs((kind["value_start"] + kind["value_end"]) / 2.0) * span * length * force_scale
        elif kind["kind"] == "Point":
            entry["N"] += abs(kind["value"]) * force_scale
        elif kind["kind"] == "PointMoment":
            entry["Nm"] += abs(kind["value"]) * force_scale * length_scale
        elif kind["kind"] == "TrapezoidalMoment":
            span = abs(kind["rel_end"] - kind["rel_start"])
            entry["Nm"] += abs((kind["value_start"] + kind["value_end"]) / 2.0) * span * length * force_scale * length_scale

    cases = {
        case["name"]: [[load["pattern"], load["scale"]] for load in case.get("loads", [])]
        for case in data.get("load_cases", []) if case["type"] == "LinearStatic"
    }

    return {
        "units": f"{units.get('force', 'N')}, {units.get('length', 'm')}",
        "counts": {
            "joints": len(joints), "frames": len(frames), "areas": len(areas),
            "load_patterns": len(patterns),
        },
        "geometry": _geometry(joints, frames, areas),
        "patterns": _rounded(patterns),
        "joint_loads": {
            name: {"n": v["n"], "F": _rounded(v["F"]), "M": _rounded(v["M"])}
            for name, v in sorted(joint_loads.items())
        },
        "frame_loads": {
            name: {"n": v["n"], "N": round(v["N"], 9), "Nm": round(v["Nm"], 9)}
            for name, v in sorted(frame_loads.items())
        },
        "cases": {name: sorted(value) for name, value in sorted(cases.items())},
        "unhandled": {},
        "_source": str(path),
    }


def fingerprint_from_opensees_studio(path: Path, workdir: Path) -> dict:
    """OpenSees Studio's reader, seen through its own S2K export.

    Only the round trip is checked: the export writes its own names and its own
    unit system, and the fingerprint is keyed on geometry and on the numbers the
    file states, so a rename is not a difference.
    """
    sys.path.insert(0, str(Path(__file__).resolve().parent / "opensees_studio_vendor" / "src"))
    from opensees_studio.services.sap2000_text import export_s2k, read_s2k, s2k_to_project  # noqa: PLC0415

    project = s2k_to_project(read_s2k(path)).project
    exported = workdir / f"{path.stem}.roundtrip.s2k"
    export_s2k(project, exported)
    return fingerprint_from_s2k(exported)


def _rounded(values):
    """Floats to 12 decimals, so the last bits of a unit conversion are not a
    difference between two readers."""
    if isinstance(values, dict):
        return {name: round(value, 12) for name, value in sorted(values.items())}
    return [round(value, 12) for value in values]


def _close(left, right, rtol: float) -> bool:
    """Numbers with a relative tolerance; everything else exactly."""
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return math.isclose(left, right, rel_tol=rtol, abs_tol=1e-12)
    if isinstance(left, list) and isinstance(right, list):
        return len(left) == len(right) and all(_close(a, b, rtol) for a, b in zip(left, right))
    if isinstance(left, dict) and isinstance(right, dict):
        return left.keys() == right.keys() and all(_close(left[k], right[k], rtol) for k in left)
    return left == right


FIELDS = ("counts", "geometry", "patterns", "joint_loads", "frame_loads", "cases")


def compare(reference: dict, other: dict, *, rtol: float = 1e-6) -> list[str]:
    """Every field that differs, one line each."""
    problems: list[str] = []
    for field in FIELDS:
        left, right = reference.get(field), other.get(field)
        if field in ("counts", "patterns", "cases"):
            if not _close(left, right, rtol):
                problems.append(f"{field}: {json.dumps(left, sort_keys=True)} != {json.dumps(right, sort_keys=True)}")
        elif field == "geometry":
            for key in left:
                if not _close(left[key], (right or {}).get(key), rtol):
                    problems.append(f"geometry.{key}: {left[key]} != {(right or {}).get(key)}")
        else:
            for name in sorted(set(left) | set(right or {})):
                if not _close(left.get(name), (right or {}).get(name), rtol):
                    problems.append(f"{field}.{name}: {left.get(name)} != {(right or {}).get(name)}")
    return problems


def _expand(inputs: list[str]) -> list[Path]:
    """Arguments as files: a glob or a directory becomes its .s2k files."""
    out: list[Path] = []
    for name in inputs:
        path = Path(name)
        if any(char in name for char in "*?["):
            out.extend(sorted(Path().glob(name)))
        elif path.is_dir():
            out.extend(sorted(path.glob("*.s2k")))
        else:
            out.append(path)
    return out


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="diff_s2k_import",
        description="Compares what struktal and the S2K file itself say about a model.",
    )
    parser.add_argument("inputs", nargs="+", help=".s2k files")
    parser.add_argument("--struktal", default="target/debug/struktal", help="path to the struktal binary")
    parser.add_argument(
        "--opensees-studio",
        action="store_true",
        help="also check OpenSees Studio's reader, through its own S2K export",
    )
    parser.add_argument("--workdir", default=None, help="where the OpenSees Studio round trip is written")
    parser.add_argument("--json", action="store_true", help="print the reference fingerprint and exit")
    args = parser.parse_args(argv)

    binary = Path(args.struktal)
    if not binary.exists() and not binary.suffix:
        # Windows names the executable struktal.exe; the default says struktal.
        windows = binary.with_suffix(".exe")
        if windows.exists():
            binary = windows
    if not args.json and not binary.exists():
        print(f"no hay binario de struktal en {binary}: compílalo primero", file=sys.stderr)
        return 2

    workdir = Path(args.workdir) if args.workdir else Path(tempfile.mkdtemp(prefix="struktal-s2k-"))
    workdir.mkdir(parents=True, exist_ok=True)

    bad = 0
    for path in _expand(args.inputs):
        try:
            reference = fingerprint_from_s2k(path)
        except (S2kError, OSError) as error:
            print(f"{path}: no se puede leer el fichero: {error}")
            bad += 1
            continue
        if args.json:
            print(json.dumps(reference, indent=2, sort_keys=True))
            continue

        model = workdir / f"{path.stem}.json"
        run = subprocess.run(
            # --allow-partial: a case the reader cannot represent is reported by
            # the `cases` field instead of stopping the whole comparison.
            [str(binary), "import", "--allow-partial", str(path), "-o", str(model)],
            capture_output=True,
            text=True,
        )
        if run.returncode != 0:
            print(f"{path.name}: struktal refused the file ({run.returncode}): {run.stderr.strip()[:200]}")
            bad += 1
            continue
        problems = compare(reference, fingerprint_from_struktal(model))
        print(f"{path.name}: {'OK' if not problems else f'{len(problems)} difference(s)'}")
        for problem in problems:
            print(f"    {problem}")
        bad += bool(problems)

        if not args.opensees_studio:
            continue
        try:
            other = compare(reference, fingerprint_from_opensees_studio(path, workdir))
        except ImportError:
            print("    (OpenSees Studio no importable: su round trip no se comprueba)")
            continue
        print(f"{path.name} (OpenSees Studio round trip): {'OK' if not other else f'{len(other)} difference(s)'}")
        for problem in other:
            print(f"    {problem}")
        bad += bool(other)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())