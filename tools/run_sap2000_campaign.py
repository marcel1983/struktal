#!/usr/bin/env python3
"""Runs the contrast lot through SAP2000's API, on the machine with the licence.

    python tools/run_sap2000_campaign.py LOT_FOLDER
    python tools/run_sap2000_campaign.py LOT_FOLDER --dry-run
    python tools/run_sap2000_campaign.py --list-api

For every `*.s2k` in the folder it opens a new model, imports the file, runs
every load case, and writes `<stem>-sap2000.s2k` (the model with its results, as
text) beside it. Those are the files `tools/check_contrast_lot.py` compares.

It needs SAP2000 with the OAPI (the API is installed with the program) and
`comtypes`:

    pip install comtypes

The ProgID is `SAP2000v1.Helper` from version 21 on; `--progid` overrides it.
`--list-api` prints the methods the helper actually offers, which is the fastest
way to adapt to a build whose factory is called something else: this script tries
`CreateObjectProgModel` (the documented name) and `CreateObjectProgID`, and if
neither exists it says so and lists what is there instead of dying with an
AttributeError.

Every call's return code is checked (0 is success) and printed, because a silent
failure is the one thing that would make the campaign look complete when it is
not. This script has not been run against SAP2000 in the repository's CI: the
licence and the program are not there.
"""
from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

DEFAULT_PROGID = "SAP2000v1.Helper"
# eFileType: 1 = SAP2000 database (.sdb), 3 = text (.s2k).
E_FILE_SDB = 1
E_FILE_S2K = 3


class SapError(RuntimeError):
    """The API refused something."""


def _call(label: str, ret, failures: list[str]) -> None:
    """0 is success; anything else is a failure to report."""
    if ret:
        failures.append(f"{label}: ret={ret}")
        print(f"    FALLO {label}: ret={ret}")


def connect(progid: str):
    """(helper, comtypes module) from the COM API."""
    try:
        import comtypes.client
    except ImportError:
        raise SapError("falta comtypes: pip install comtypes") from None
    try:
        raw = comtypes.client.CreateObject(progid)
    except Exception as error:  # noqa: BLE001 - COM raises OSError and friends
        raise SapError(f"no pude crear {progid}: {error}") from None
    gen = getattr(comtypes.gen, progid.split(".")[0], None)
    if gen is None:
        raise SapError(f"no pude cargar el modulo COM de {progid}")
    helper = raw.QueryInterface(gen.cHelper)
    return helper, gen


def new_model(helper, gen):
    """A new model object, whatever this build calls the factory.

    `CreateObjectProgModel` is the documented name; some builds expose
    `CreateObjectProgID` instead. Try both and, if neither exists, name what the
    interface does offer instead of raising AttributeError.
    """
    errors = []
    attempts = (
        ("CreateObjectProgModel", ("",)),
        ("CreateObjectProgID", (DEFAULT_PROGID.split(".")[0] + ".SAP2000",)),
        ("CreateObjectProgID", ("",)),
    )
    for name, args in attempts:
        method = getattr(helper, name, None)
        if method is None:
            continue
        try:
            obj = method(*args)
        except Exception as error:  # noqa: BLE001 - report and try the next
            errors.append(f"{name}{args}: {error}")
            continue
        try:
            return obj.QueryInterface(gen.cSapModel)
        except Exception:  # noqa: BLE001 - it may already be the model
            return obj
    available = sorted(m for m in dir(helper) if not m.startswith("_"))
    raise SapError(
        "no pude crear el modelo ("
        + "; ".join(errors)
        + "). Metodos del helper: "
        + ", ".join(available)
    )


def open_model(helper, gen, s2k: Path, out: Path):
    """A new model with `s2k` imported and every case solved."""
    sap = new_model(helper, gen)
    failures: list[str] = []
    _call("File.NewFile", sap.File.NewFile(), failures)
    _call(f"File.ImportS2kTextFile({s2k.name})", sap.File.ImportS2kTextFile(str(s2k)), failures)
    if failures:
        raise SapError("; ".join(failures))
    started = time.time()
    _call("Analyze.RunAnalysis", sap.Analyze.RunAnalysis(), failures)
    if failures:
        raise SapError("; ".join(failures))
    print(f"    resuelto en {time.time() - started:.1f} s")
    _call(f"File.SaveAs({out.name})", sap.File.SaveAs(str(out), E_FILE_S2K), failures)
    if failures:
        raise SapError("; ".join(failures))
    return sap


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("lot", nargs="?", help="folder with the models from tools/gen_contrast_lot.py")
    ap.add_argument("--progid", default=DEFAULT_PROGID, help=f"COM ProgID (default {DEFAULT_PROGID})")
    ap.add_argument("--glob", default="*.s2k", help="models to run (default *.s2k)")
    ap.add_argument("--dry-run", action="store_true", help="list what would run and exit")
    ap.add_argument("--list-api", action="store_true", help="print the helper's methods and exit")
    args = ap.parse_args()

    if args.list_api:
        try:
            helper, _ = connect(args.progid)
        except SapError as error:
            print(f"error: {error}", file=sys.stderr)
            return 2
        for name in sorted(m for m in dir(helper) if not m.startswith("_")):
            print(name)
        return 0

    if not args.lot:
        ap.error("falta LOT_FOLDER (o --list-api)")

    lot = Path(args.lot)
    models = sorted(p for p in lot.glob(args.glob) if not p.stem.endswith("-sap2000"))
    if not models:
        print(f"{lot}: no hay modelos {args.glob}", file=sys.stderr)
        return 2

    print(f"{len(models)} modelo(s) en {lot}")
    for model in models:
        print(f"  {model.name}")
    if args.dry_run:
        return 0

    try:
        helper, gen = connect(args.progid)
    except SapError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    bad = 0
    for model in models:
        out = model.with_name(model.stem + "-sap2000.s2k")
        print(f"{model.name} -> {out.name}")
        try:
            sap = open_model(helper, gen, model, out)
        except SapError as error:
            print(f"    error: {error}", file=sys.stderr)
            bad += 1
            continue
        if not out.exists():
            print(f"    error: SAP2000 no escribio {out.name}", file=sys.stderr)
            bad += 1
            continue
        try:
            sap.File.CloseFile()
        except Exception:  # noqa: BLE001 - closing is best effort
            pass
    print(f"{len(models) - bad} de {len(models)} modelo(s) exportados")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())