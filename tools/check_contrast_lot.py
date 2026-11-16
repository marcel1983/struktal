#!/usr/bin/env python3
"""Compares what SAP2000 reports for the contrast lot with what Struktal solves.

    python tools/check_contrast_lot.py LOT_FOLDER [--struktal PATH] [--report FILE]

`LOT_FOLDER` holds the models `tools/gen_contrast_lot.py` wrote, plus one CSV per
model with what SAP2000 reported, exported on the machine with a licence:

    C01-signos.s2k      C01-signos.csv
    C02-ejes.s2k        C02-ejes.csv
    ...

The CSV is a flat table with a header row. It needs a column naming the joint
(`Joint`, `JointName`, `Name`) and, when the file holds more than one case, one
naming the case (`Case`, `LoadCase`, `Step`). The components are recognised by
name, so `UX/UY/UZ/RX/RY/RZ`, `U1..R3` and `Fx..Mz` all work; use one naming
for displacements and another for reactions in the same file, or two files
(`<stem>-disp.csv` and `<stem>-react.csv`).

    python tools/check_contrast_lot.py --sample SAMPLE_FOLDER

writes a CSV in the accepted format for `V-LIN-001`, to show what to export.

The point is not only to compare numbers: for every component the report says
whether SAP2000 agrees, agrees **with the sign changed**, or disagrees. That is
how the sign convention of V2, V3, M2 and M3 (decision 0003) is settled, and it
is what an oracle cannot settle.

Nothing here is written back into the public repository: the exports and this
report stay on the machine with the licence (decision 0018).
"""
from __future__ import annotations

import argparse
import csv
import io
import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from diff_s2k_import import read_tables as read_s2k_tables  # noqa: E402

COMPONENTS = ["UX", "UY", "UZ", "RX", "RY", "RZ"]
REACTIONS = ["FX", "FY", "FZ", "MX", "MY", "MZ"]

# A column name as SAP2000 and Excel write it, mapped to (kind, component).
ALIASES = {
    "ux": ("disp", 0), "u1": ("disp", 0), "dx": ("disp", 0), "dispx": ("disp", 0), "u1displ": ("disp", 0),
    "uy": ("disp", 1), "u2": ("disp", 1), "dy": ("disp", 1), "dispy": ("disp", 1), "u2displ": ("disp", 1),
    "uz": ("disp", 2), "u3": ("disp", 2), "dz": ("disp", 2), "dispz": ("disp", 2), "u3displ": ("disp", 2),
    "rx": ("disp", 3), "r1": ("disp", 3), "rotx": ("disp", 3),
    "ry": ("disp", 4), "r2": ("disp", 4), "roty": ("disp", 4),
    "rz": ("disp", 5), "r3": ("disp", 5), "rotz": ("disp", 5),
    "fx": ("react", 0), "f1": ("react", 0), "fxsum": ("react", 0),
    "fy": ("react", 1), "f2": ("react", 1), "fysum": ("react", 1),
    "fz": ("react", 2), "f3": ("react", 2), "fzsum": ("react", 2),
    "mx": ("react", 3), "m1": ("react", 3), "mxsum": ("react", 3),
    "my": ("react", 4), "m2": ("react", 4), "mysum": ("react", 4),
    "mz": ("react", 5), "m3": ("react", 5), "mzsum": ("react", 5),
}
CASE_COLUMNS = ("case", "loadcase", "outputcase", "loadpat", "step", "casename")
JOINT_COLUMNS = ("joint", "jointname", "name", "jointno", "n")

RTOL = 1e-4   # SAP2000 prints six significant digits
ATOL = 1e-9


def normalise(name: str) -> str:
    return "".join(ch for ch in name.strip().lower() if ch.isalnum())


def read_table(path: Path) -> list[dict[str, str]]:
    """A CSV or tab/semicolon separated file, as a list of dicts."""
    text = path.read_text(encoding="utf-8-sig", errors="replace")
    for delimiter in (",", "\t", ";"):
        try:
            rows = list(csv.reader(io.StringIO(text), delimiter=delimiter))
        except csv.Error:
            continue
        if rows and len(rows[0]) > 1:
            header = [h.strip() for h in rows[0]]
            out = []
            for row in rows[1:]:
                if not any(cell.strip() for cell in row):
                    continue
                out.append({header[i]: (row[i] if i < len(row) else "") for i in range(len(header))})
            if out:
                return out
    raise SystemExit(f"{path.name}: no pude leer la tabla")


def columns(rows: list[dict[str, str]]) -> tuple[str | None, str | None, dict]:
    """(joint column, case column, {component index: (kind, column name)})."""
    if not rows:
        return None, None, {}
    keys = list(rows[0].keys())
    joint = next((k for k in keys if normalise(k) in JOINT_COLUMNS), None)
    case = next((k for k in keys if normalise(k) in CASE_COLUMNS), None)
    components = {}
    for k in keys:
        alias = ALIASES.get(normalise(k))
        if alias:
            components.setdefault(alias[1], (alias[0], k))
    return joint, case, components


def values(rows: list[dict[str, str]], default_case: str = "") -> dict:
    """{case: {joint: {index: (kind, value)}}}"""
    joint_col, case_col, components = columns(rows)
    if joint_col is None or not components:
        return {}
    out: dict = {}
    for row in rows:
        joint = row.get(joint_col, "").strip()
        if not joint:
            continue
        case = row.get(case_col, "").strip() if case_col else default_case
        for index, (kind, column) in components.items():
            raw = row.get(column, "").strip().replace(",", ".")
            try:
                value = float(raw)
            except ValueError:
                continue
            # Keyed by kind and component: a table may carry UX and FX, and both
            # are component 0.
            out.setdefault(case, {}).setdefault(joint, {})[(kind, index)] = value
    return out


def rows_from_s2k(path: Path) -> list[dict[str, str]]:
    """The joint tables of an S2K file exported by SAP2000, as flat rows.

    A SAP2000 text export carries the results in the same Key=Value tables as
    the model, so the rows go through the same reader and the same column
    aliases as a CSV.
    """
    tables = read_s2k_tables(path.read_text(encoding="utf-8", errors="replace"))
    rows: list[dict[str, str]] = []
    for name in ("JOINT DISPLACEMENTS", "JOINT REACTIONS"):
        rows.extend(tables.get(name, []))
    return rows


def load_engine(struktal: Path, model: Path, work: Path) -> dict:
    """Struktal's results for the model, imported from the original S2K."""
    imported = work / (model.stem + ".json")
    imp = subprocess.run(
        [str(struktal), "import", "--allow-partial", str(model), "-o", str(imported)],
        capture_output=True, text=True,
    )
    if imp.returncode != 0:
        return {"error": imp.stderr.strip()[:300]}
    out = work / (model.stem + ".results.json")
    sol = subprocess.run([str(struktal), "solve", str(imported), "-o", str(out)], capture_output=True, text=True)
    if sol.returncode != 0 or not out.exists():
        return {"error": sol.stderr.strip()[:300]}
    data = json.loads(out.read_text(encoding="utf-8"))
    return {c["name"]: c for c in data.get("cases", [])}


def compare_case(ours: dict, theirs: dict) -> tuple[list[str], dict]:
    """(lines, per-component tally) for one case."""
    lines = []
    tally: dict = {}
    for joint in sorted(set(ours.get("joints_by_name", {})) | set(theirs)):
        mine = ours["joints_by_name"].get(joint)
        if mine is None:
            continue
        disp = mine["displacement"] or [0.0] * 6
        react = mine["reaction"] or [0.0] * 6
        for index, label in enumerate(COMPONENTS):
            b = theirs.get(joint, {}).get(("disp", index))
            if b is None:
                continue
            lines.append(verdict(label, disp[index], b))
            tally.setdefault((label, "disp"), []).append(verdict(label, disp[index], b))
        for index, label in enumerate(REACTIONS):
            b = theirs.get(joint, {}).get(("react", index))
            if b is None:
                continue
            lines.append(verdict(label, react[index], b))
            tally.setdefault((label, "react"), []).append(verdict(label, react[index], b))
    return lines, tally


def verdict(label: str, ours: float, theirs: float) -> str:
    if not (ours == ours and theirs == theirs):  # NaN
        return f"{label}: no numerico"
    if abs(ours - theirs) <= RTOL * max(abs(ours), abs(theirs)) + ATOL:
        return f"{label}: match"
    if abs(ours + theirs) <= RTOL * max(abs(ours), abs(theirs)) + ATOL:
        return f"{label}: signo invertido"
    return f"{label}: difiere ({ours:.6g} vs {theirs:.6g})"


def _is_export(path: Path) -> bool:
    """True when the .s2k carries results, not just the model.

    A SAP2000 export is not an input: the model beside it is.
    """
    try:
        head = path.read_text(encoding="utf-8", errors="replace")[:8000]
    except OSError:
        return False
    return "JOINT DISPLACEMENTS" in head


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("lot", nargs="?", help="folder with the models and the CSVs")
    ap.add_argument("--struktal", default="target/debug/struktal")
    ap.add_argument("--report", default=None, help="write the Markdown report here")
    ap.add_argument("--sample", metavar="FOLDER", help="write an example CSV and exit")
    args = ap.parse_args()

    if args.sample:
        sample = Path(args.sample)
        sample.mkdir(parents=True, exist_ok=True)
        rows = [
            {"Case": "LC1", "Joint": "1", "UX": "0", "UY": "0", "UZ": "0", "RX": "0", "RY": "0", "RZ": "0",
             "FX": "0", "FY": "0", "FZ": "0", "MX": "0", "MY": "0", "MZ": "0"},
            {"Case": "LC1", "Joint": "2", "UX": "0", "UY": "0", "UZ": "-0.0004", "RX": "0", "RY": "0", "RZ": "0",
             "FX": "0", "FY": "0", "FZ": "0", "MX": "0", "MY": "0", "MZ": "0"},
        ]
        with (sample / "ejemplo.csv").open("w", encoding="utf-8", newline="") as fh:
            writer = csv.DictWriter(fh, fieldnames=list(rows[0].keys()))
            writer.writeheader()
            writer.writerows(rows)
        print(f"ejemplo escrito en {sample / 'ejemplo.csv'}")
        return 0

    if not args.lot:
        ap.error("falta LOT_FOLDER (o --sample FOLDER)")
    lot = Path(args.lot)
    struktal = Path(args.struktal)
    if not struktal.exists() and not struktal.with_suffix(".exe").exists():
        print(f"no hay binario de struktal en {struktal}", file=sys.stderr)
        return 2

    models = sorted(
        p
        for p in lot.glob("*.s2k")
        if not p.stem.endswith("-sap2000") and not _is_export(p)
    )
    if not models:
        print(f"{lot}: no hay modelos .s2k", file=sys.stderr)
        return 2

    report = ["# Contraste con SAP2000", ""]
    problems = 0
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for model in models:
            csvs = sorted(lot.glob(model.stem + "*.csv"))
            # SAP2000 writes Save over the file the model came from, so an
            # export may be the model itself.
            exported = lot / (model.stem + "-sap2000.s2k")
            if not exported.exists():
                exported = model
            rows: list[dict[str, str]] = []
            for path in csvs:
                rows.extend(read_table(path))
            if not rows and exported.exists():
                # Lo que escribe tools/run_sap2000_campaign.py: el modelo con
                # sus resultados, en texto.
                rows = rows_from_s2k(exported)
            ours = load_engine(struktal, model, work)
            report.append(f"## `{model.name}`\n")
            if "error" in ours:
                # Esperable: C03 y C04 traen secciones parametricas,
                # liberaciones y offsets que Struktal no traduce.
                report.append(f"Struktul no pudo importar o resolver: `{ours['error']}`\n")
                continue
            if not rows:
                report.append("Sin CSV ni -sap2000.s2k: no comparado.\n")
                continue
            # Un S2K exportado puede no repetir el caso en cada fila.
            default_case = next(iter(ours), "") if len(ours) == 1 else ""
            theirs = values(rows, default_case)
            if not theirs:
                report.append("El fichero no tiene columnas reconocibles.\n")
                problems += 1
                continue
            for case_name, case in sorted(ours.items()):
                if case_name not in theirs:
                    report.append(f"- Caso `{case_name}`: no está en el CSV.\n")
                    continue
                joints = {j["joint"]: j for j in case.get("joints", [])}
                mine = dict(case)
                mine["joints_by_name"] = joints
                lines, tally = compare_case(mine, theirs[case_name])
                summary: dict = {}
                for line in lines:
                    kind = line.split(": ", 1)[1]
                    summary[kind] = summary.get(kind, 0) + 1
                report.append(f"- Caso `{case_name}`: " + ", ".join(f"{k} {v}" for k, v in sorted(summary.items())))
                for line in lines:
                    if not line.endswith(": match"):
                        report.append(f"    - {line}")
                        if line.endswith(": difiere") or ": difiere (" in line:
                            problems += 1
                report.append("")
    text = "\n".join(report) + "\n"
    if args.report:
        Path(args.report).write_text(text, encoding="utf-8")
        print(f"informe: {args.report}")
    print(text)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())