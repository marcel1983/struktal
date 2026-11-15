# 0029 — Dos familias de lámina en el oráculo de OpenSees

Fecha: 2026-11-15. Estado: aceptada.

## Decisión

El runner de OpenSees (`tools/oracles/opensees_runner.py`) puede usar dos familias
de lámina, elegidas con `STRUKTAL_OPENSEES_SHELL`:

| Valor | Cuadrilátero | Triángulo |
|---|---|---|
| `mitc4` (por omisión) | `ShellMITC4` | `ASDShellT3` |
| `dkgq` | `ShellDKGQ` | `ShellDKGT` |

El valor por omisión no cambia: MITC4 es lo que usa Struktal, y comparar con la
misma formulación demuestra poco, pero es lo que se venía comparando y cambiarlo
invalidaría las comparaciones anteriores.

## Motivos

Una segunda opinión solo vale si el código es otro y la formulación también. La
familia DKG (Hughes–Brezzi con taladro) es otra teoría en el mismo código, así
que sirve para separar «lo hace distinto OpenSees» de «lo hace distinto la
formulación».

## Qué se midió

Casos del banco, 2026-11-15, `openseespy 3.8.0.0`, tolerancia del caso.
`FORM-DIFF` es una diferencia esperada en mallas que convergen, no un fallo.

| Caso | MITC4 | DKGQ |
|---|---|---|
| V-SHELL-001 TEN | ✅ match (174 valores) | ❌ DIFF 35/174; `MZ = 6,5e3 N·m` espurio en T0_0 |
| V-SHELL-001 BEN | ✅ match (174) | ✅ match (174) |
| V-SHELL-002 LC1 | FORM-DIFF 38/129 | FORM-DIFF 14/129 (en RZ) |
| V-SHELL-003 LC1 | FORM-DIFF 338/1131 | FORM-DIFF 365/1131 |
| V-TRI-001 TEN | ❌ DIFF 35/334; `MZ = 9,4e3` espurio | ❌ DIFF 35/334; `MZ = 6,2e3` espurio |
| V-TRI-001 BEN | ✅ match (334) | ✅ match (334) |
| V-TRI-001 MIX | ❌ DIFF 35/334 (mismo `MZ`) | ❌ DIFF 35/334 (mismo `MZ`) |
| V-TRI-001 Q | ✅ match (334, 2,8e-11) | ✅ match (334, 2,8e-11) |

## Lo que dice la medición

- **DKGQ no es mejor que MITC4**: arregla nada que MITC4 no resuelva y rompe
  V-SHELL-001 TEN. Se queda como diagnóstico, no como opción por omisión.
- **El artefacto está en los triángulos, no en la familia.** Las dos familias
  producen el mismo momento espurio `MZ` en T0_0 / X0_0 en V-TRI-001 TEN y MIX.
  Un momento axial en un nudo de membrana solo puede venir del **taladro**: el
  triángulo de OpenSees lo genera y Struktal no. Es un dato sobre
  `ASDShellT3` / `ShellDKGT`, relevante porque Struktal usa MITC3 en triángulos.
- **Las láminas de cuatro nudos coinciden** en los dos estados exactos del banco
  (membrana y flexión pura) con MITC4, lo que es una verificación cruzada real.

## Consecuencias

- MITC4 sigue siendo la comparación por omisión.
- DKGQ se documenta como diagnóstico: si un caso de lámina discrepa, probar la
  otra familia separa «otra formulación» de «otro mapeo».
- La diferencia de V-TRI-001 TEN/MIX queda explicada y se puede citar: no es un
  fallo de Struktal, es el taladro del triángulo de OpenSees.