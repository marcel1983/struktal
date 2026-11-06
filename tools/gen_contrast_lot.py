#!/usr/bin/env python3
"""Writes small S2K models to contrast conventions with SAP2000, by hand, on a
machine with a license. They are hand-written, not exports of the program, and
have not been opened in it yet: an import error there is a result too.

    python tools/gen_contrast_lot.py OUTPUT_FOLDER

What comes back from SAP2000 stays out of this public repository (decision 0018).
"""
import sys
from pathlib import Path

HEADER = 'TABLE:  "PROGRAM CONTROL"\n   ProgramName=SAP2000   Version=27.1.0   CurrUnits="N, m, C"\n\n'
MATERIAL = (
    'TABLE:  "MATERIAL PROPERTIES 01 - GENERAL"\n   Material=STEEL   Type=Steel   SymType=Isotropic   TempDepend=No\n\n'
    'TABLE:  "MATERIAL PROPERTIES 02 - BASIC MECHANICAL PROPERTIES"\n'
    "   Material=STEEL   UnitWeight=0   UnitMass=0   E1=200000000000   G12=76923076923.0769   U12=0.3   A1=0\n\n"
)
GENERAL = (
    'TABLE:  "FRAME SECTION PROPERTIES 01 - GENERAL"\n'
    "   SectionName=G1   Material=STEEL   Shape=General   t3=0.5   t2=0.2   Area=0.01   TorsConst=0.0002   I33=0.0002   I22=0.0001   AS2=0   AS3=0\n\n"
)


def table(name, rows):
    return f'TABLE:  "{name}"\n' + "".join(f"   {r}\n" for r in rows) + "\n"


def joints(items):
    return table("JOINT COORDINATES", [f"Joint={n}   CoordSys=GLOBAL   CoordType=Cartesian   XorR={x}   Y={y}   Z={z}" for n, x, y, z in items])


def fixed(names):
    return table("JOINT RESTRAINT ASSIGNMENTS", [f"Joint={n}   U1=Yes   U2=Yes   U3=Yes   R1=Yes   R2=Yes   R3=Yes" for n in names])


def frames(items, section="G1"):
    return table("CONNECTIVITY - FRAME", [f"Frame={f}   JointI={i}   JointJ={j}   IsCurved=No" for f, i, j in items]) + table(
        "FRAME SECTION ASSIGNMENTS", [f"Frame={f}   SectionType=General   AutoSelect=N.A.   AnalSect={section}   MatProp=Default" for f, _, _ in items]
    )


def cases(names):
    return (
        table("LOAD PATTERN DEFINITIONS", [f"LoadPat={n}   DesignType=Other   SelfWtMult=0" for n in names])
        + table("LOAD CASE DEFINITIONS", [f"Case={n}   Type=LinStatic   InitialCond=Zero   RunCase=Yes" for n in names])
        + table("CASE - STATIC 1 - LOAD ASSIGNMENTS", [f'Case={n}   LoadType="Load pattern"   LoadName={n}   LoadSF=1' for n in names])
    )


def force(pattern, joint, values):
    labels = ["F1", "F2", "F3", "M1", "M2", "M3"]
    return f"Joint={joint}   LoadPat={pattern}   CoordSys=GLOBAL   " + "   ".join(f"{k}={v}" for k, v in zip(labels, values))


LOT = {
    # Signs of P, V2, V3, T, M2, M3 and their stations: one load per case.
    "C01-signos.s2k": HEADER + MATERIAL + GENERAL + joints([(1, 0, 0, 0), (2, 4, 0, 0)]) + fixed([1]) + frames([("F1", 1, 2)])
    + cases(["AX", "VZ", "VY", "TX", "MY", "MZ", "UDL", "PT"])
    + table("JOINT LOADS - FORCE", [
        force("AX", 2, [1000, 0, 0, 0, 0, 0]), force("VZ", 2, [0, 0, -1000, 0, 0, 0]), force("VY", 2, [0, 1000, 0, 0, 0, 0]),
        force("TX", 2, [0, 0, 0, 1000, 0, 0]), force("MY", 2, [0, 0, 0, 0, 1000, 0]), force("MZ", 2, [0, 0, 0, 0, 0, 1000])])
    + table("FRAME LOADS - DISTRIBUTED", ["Frame=F1   LoadPat=UDL   CoordSys=GLOBAL   Type=Force   Dir=Gravity   DistType=RelDist   RelDistA=0   RelDistB=1   FOverLA=1000   FOverLB=1000"])
    + table("FRAME LOADS - POINT", ["Frame=F1   LoadPat=PT   CoordSys=Local   Type=Force   Dir=2   DistType=RelDist   RelDist=0.25   Force=1000"]),
    # Default local axes: vertical, nearly vertical, inclined, and with an angle.
    "C02-ejes.s2k": HEADER + MATERIAL + GENERAL
    + joints([(1, 0, 0, 0), (2, 0, 0, 4), (3, 5, 0, 0), (4, 5.001, 0, 4), (5, 10, 0, 0), (6, 13, 0, 4), (7, 15, 0, 0), (8, 15, 0, 4), (9, 20, 0, 0), (10, 23, 4, 2)])
    + fixed([1, 3, 5, 7, 9])
    + frames([("V", 1, 2), ("NV", 3, 4), ("I", 5, 6), ("A", 7, 8), ("O", 9, 10)])
    + table("FRAME LOCAL AXES ASSIGNMENTS 1 - TYPICAL", ["Frame=A   Angle=30   AdvanceAxes=No"])
    + cases(["HX", "HY"])
    + table("JOINT LOADS - FORCE", [force(p, j, v) for j in (2, 4, 6, 8, 10) for p, v in (("HX", [1000, 0, 0, 0, 0, 0]), ("HY", [0, 1000, 0, 0, 0, 0]))]),
    # Parametric sections: what SAP2000 computes for A, I, J, As2 and As3.
    "C03-secciones.s2k": HEADER + MATERIAL
    + table("FRAME SECTION PROPERTIES 01 - GENERAL", [
        "SectionName=RECT   Material=STEEL   Shape=Rectangular   t3=0.5   t2=0.2",
        "SectionName=CIRC   Material=STEEL   Shape=Circle   t3=0.4",
        "SectionName=PIPE   Material=STEEL   Shape=Pipe   t3=0.5   tw=0.01",
        "SectionName=BOX   Material=STEEL   Shape=Box/Tube   t3=0.5   t2=0.2   tf=0.02   tw=0.012",
        'SectionName=ISEC   Material=STEEL   Shape="I/Wide Flange"   t3=0.5   t2=0.2   tf=0.02   tw=0.012   t2b=0.2   tfb=0.02'])
    + joints([(2 * k + 1, 0, 3 * k, 0) for k in range(5)] + [(2 * k + 2, 4, 3 * k, 0) for k in range(5)])
    + fixed([2 * k + 1 for k in range(5)])
    + table("CONNECTIVITY - FRAME", [f"Frame={s}   JointI={2 * k + 1}   JointJ={2 * k + 2}   IsCurved=No" for k, s in enumerate(["RECT", "CIRC", "PIPE", "BOX", "ISEC"])])
    + table("FRAME SECTION ASSIGNMENTS", [f"Frame={s}   AutoSelect=N.A.   AnalSect={s}   MatProp=Default" for s in ["RECT", "CIRC", "PIPE", "BOX", "ISEC"]])
    + cases(["VZ"])
    + table("JOINT LOADS - FORCE", [force("VZ", 2 * k + 2, [0, 0, -1000, 0, 0, 0]) for k in range(5)]),
    # Releases, offsets and springs, plus a settlement on a free component,
    # which Struktal refuses: remove it from the copy Struktal reads.
    "C04-opciones.s2k": HEADER + MATERIAL + GENERAL
    + joints([(1, 0, 0, 0), (2, 4, 0, 0), (3, 0, 3, 0), (4, 4.5, 3, 0), (5, 0, 6, 0), (6, 4, 6, 0)])
    + fixed([1, 2, 3, 5])
    + frames([("REL", 1, 2), ("OFF", 3, 4), ("SPR", 5, 6)])
    + table("FRAME RELEASE ASSIGNMENTS 1 - GENERAL", ["Frame=REL   PI=No   V2I=No   V3I=No   TI=No   M2I=No   M3I=Yes   PJ=No   V2J=No   V3J=No   TJ=No   M2J=No   M3J=No"])
    + table("FRAME INSERTION POINT ASSIGNMENTS", ['Frame=OFF   CardinalPt="10 (Centroid)"   Mirror2=No   StiffTrans=No   CoordSys=GLOBAL   JtOffsetXI=0   JtOffsetYI=0   JtOffsetZI=0   JtOffsetXJ=-0.5   JtOffsetYJ=0   JtOffsetZJ=0'])
    + table("JOINT SPRING ASSIGNMENTS 1 - UNCOUPLED", ["Joint=6   CoordSys=Local   U1=0   U2=0   U3=2000000   R1=0   R2=0   R3=0"])
    + cases(["L1", "SET"])
    + table("JOINT LOADS - FORCE", [force("L1", 4, [0, 0, -1000, 0, 0, 0]), force("L1", 6, [0, 0, -1000, 0, 0, 0])])
    + table("FRAME LOADS - DISTRIBUTED", ["Frame=REL   LoadPat=L1   CoordSys=GLOBAL   Type=Force   Dir=Gravity   DistType=RelDist   RelDistA=0   RelDistB=1   FOverLA=1000   FOverLB=1000"])
    + table("JOINT LOADS - GROUND DISPLACEMENT", ["Joint=4   LoadPat=SET   CoordSys=GLOBAL   U1=0   U2=0   U3=-0.01   R1=0   R2=0   R3=0"]),
}

README = """# Contraste con SAP2000

Modelos pequeños escritos a mano para aclarar convenciones. Antes de usarlos, comprueba que tu licencia permite esta comparación.

No se han abierto nunca en SAP2000: si alguno da error al importarlo, ese error también es un resultado útil.

## Pasos, en el equipo con licencia

1. Abre cada `.s2k` con File > Import > SAP2000 .s2k Text File.
2. Ejecuta todos los casos.
3. Exporta a `.s2k` con File > Export, con las tablas de modelo y de resultados, incluidas `JOINT DISPLACEMENTS`, `JOINT REACTIONS`, `ELEMENT FORCES - FRAMES` y `FRAME SECTION PROPERTIES 01 - GENERAL`.
4. Deja los archivos exportados en esta misma carpeta, y anota el build exacto.

## Qué aclara cada uno

| Archivo | Pregunta |
|---|---|
| `C01-signos.s2k` | Signos de P, V2, V3, T, M2 y M3 |
| `C02-ejes.s2k` | Ejes locales: vertical, casi vertical, inclinada, oblicua y con ángulo |
| `C03-secciones.s2k` | A, I, J, As2 y As3 que calcula SAP2000 para cada forma |
| `C04-opciones.s2k` | Liberaciones, offsets con peso propio nulo, resortes y un asiento sobre un grado de libertad libre |

Estos archivos y sus resultados no van al repositorio público.
"""


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, text in LOT.items():
        (out / name).write_text(text + "END TABLE DATA\n", encoding="utf-8", newline="\n")
    (out / "LEEME.md").write_text(README, encoding="utf-8", newline="\n")
    print(f"{len(LOT)} models written to {out}")


if __name__ == "__main__":
    main()
