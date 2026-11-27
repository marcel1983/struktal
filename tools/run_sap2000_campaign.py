#!/usr/bin/env python3
"""Runs the contrast lot through SAP2000's API, on the machine with the licence.

    python tools/run_sap2000_campaign.py LOT_FOLDER
    python tools/run_sap2000_campaign.py LOT_FOLDER --dry-run
    python tools/run_sap2000_campaign.py --list-api

For every `*.s2k` in the folder it opens the file, runs every load case, and
writes `<stem>-sap2000.s2k` (the model with its results, as text) beside it.
Those are the files `tools/check_contrast_lot.py` compares.

It needs SAP2000 with the OAPI (the API is installed with the program) and
`comtypes`:

    pip install comtypes

The sequence is the one in the official example (`CSI_OAPI_Documentation.chm`,
`Example_Code/Example_7_(Python).htm`):

    helper = comtypes.client.CreateObject('SAP2000v1.Helper')
    helper = helper.QueryInterface(comtypes.gen.SAP2000v1.cHelper)
    mySapObject = helper.CreateObjectProgID("CSI.SAP2000.API.SapObject")
    mySapObject.ApplicationStart()
    SapModel = mySapObject.SapModel
    SapModel.File.OpenFile("model.s2k")     # .s2k and .$2k are imported as text
    SapModel.Analyze.RunAnalysis()

`File.OpenFile` is what reads the text file: the API has no `ImportS2kTextFile`.
`--program PATH` uses `helper.CreateObject(PATH)` instead of the ProgID, which
starts the interpreter at a given path (useful for a version other than the
latest installed).

The results come back through `File.Save`: the API has no table export, so the
script writes the solved model as `.s2k` and checks that the file really is text
before accepting it.

Every call's return code is checked (0 is success). This script has not been run
against SAP2000 in the repository's CI: the licence and the program are not
there. `--list-api` prints the methods of the helper, `--probe` creates the
object and prints what came back.
"""
from __future__ import annotations

import argparse
import shutil
import sys
import tempfile
import time
from pathlib import Path

DEFAULT_PROGID = "SAP2000v1.Helper"
SAP_PROGID = "CSI.SAP2000.API.SapObject"
# eFileType: 3 = text (.s2k), 1 = SAP2000 database (.sdb).
E_FILE_S2K = 3


class SapError(RuntimeError):
    """The API refused something."""


def _call(label: str, ret, failures: list[str]) -> bool:
    """0 is success; anything else is a failure to report."""
    if ret:
        failures.append(f"{label}: ret={ret}")
        print(f"    FALLO {label}: ret={ret}")
        return False
    return True


def connect(progid: str):
    """(dynamic helper, comtypes module) from the COM API."""
    try:
        import comtypes.client
    except ImportError:
        raise SapError("falta comtypes: pip install comtypes") from None
    try:
        raw = comtypes.client.CreateObject(progid)
    except Exception as error:  # noqa: BLE001 - COM raises OSError and friends
        raise SapError(f"no pude crear {progid}: {error}") from None
    gen = getattr(comtypes.gen, progid.split(".")[0], None)
    return raw, gen


def _sap_object(helper, program: str | None):
    """The SapObject, started by ProgID or from a program path."""
    if program:
        try:
            return helper.CreateObject(program)
        except Exception as error:  # noqa: BLE001
            raise SapError(f"no pude arrancar {program}: {error}") from None
    method = getattr(helper, "CreateObjectProgID", None)
    if method is None:
        raise SapError("el helper no tiene CreateObjectProgID")
    try:
        obj = method(SAP_PROGID)
    except Exception as error:  # noqa: BLE001
        raise SapError(f"no pude crear {SAP_PROGID}: {error}") from None
    if obj is None:
        raise SapError(f"CreateObjectProgID({SAP_PROGID}) devolvio None")
    return obj


def open_model(helper, program: str | None, s2k: Path, out: Path):
    """A model with `s2k` imported and every case solved."""
    obj = _sap_object(helper, program)
    try:
        obj.ApplicationStart()
    except Exception:  # noqa: BLE001 - it may already be running
        pass
    model = obj.SapModel
    if model is None:
        raise SapError("el SapObject no tiene SapModel")

    # File.Save writes over the file the model came from, whatever name it is
    # given. Work on a copy in a temporary folder: the original is never
    # touched, and the side files SAP2000 writes stay out of the lot.
    with tempfile.TemporaryDirectory(prefix="struktal-sap-") as tmp:
        work = Path(tmp) / out.name
        shutil.copyfile(s2k, work)

        failures: list[str] = []
        if not _call(f"File.OpenFile({work.name})", model.File.OpenFile(str(work)), failures):
            # A brand-new process may need InitializeNewModel before the file.
            model.InitializeNewModel()
            failures.clear()
            if not _call(f"File.OpenFile({work.name})", model.File.OpenFile(str(work)), failures):
                raise SapError("; ".join(failures))

        started = time.time()
        if not _call("Analyze.RunAnalysis", model.Analyze.RunAnalysis(), failures):
            raise SapError("; ".join(failures))
        print(f"    resuelto en {time.time() - started:.1f} s")

        # No name: it saves to the file the model came from, which is the copy.
        if not _call("File.Save", model.File.Save(), failures):
            raise SapError("; ".join(failures))
        if not work.exists():
            raise SapError(f"SAP2000 no escribio {work.name}")
        head = work.read_text(encoding="utf-8", errors="replace")[:64]
        if "TABLE" not in head:
            raise SapError(
                f"{out.name} no es texto (empieza por {head!r}): esta version guarda en binario. "
                "Abrelo en SAP2000 y usa File > Export > Tables."
            )
        shutil.copyfile(work, out)
    return model


def _methods(obj) -> list[str]:
    try:
        return sorted(m for m in dir(obj) if not m.startswith("_"))
    except Exception:  # noqa: BLE001
        return []


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("lot", nargs="?", help="folder with the models from tools/gen_contrast_lot.py")
    ap.add_argument("--progid", default=DEFAULT_PROGID, help=f"COM ProgID (default {DEFAULT_PROGID})")
    ap.add_argument("--program", default=None, help="path to SAP2000.exe (default: the latest installed)")
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
        for name in _methods(helper):
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
        helper, _ = connect(args.progid)
    except SapError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    bad = 0
    for model in models:
        out = model.with_name(model.stem + "-sap2000.s2k")
        print(f"{model.name} -> {out.name}")
        try:
            open_model(helper, args.program, model, out)
        except SapError as error:
            print(f"    error: {error}", file=sys.stderr)
            bad += 1
            continue
    print(f"{len(models) - bad} de {len(models)} modelo(s) exportados")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())