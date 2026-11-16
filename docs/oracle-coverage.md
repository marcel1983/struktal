# Cobertura de los oráculos externos

Fecha: 2026-11-16. `struktal 0.1.0`, `openseespy 3.8.0.0`, CalculiX 2.23,
Frame3DD 20140514+. Casos: los 66 de `verification/cases/`. Salida de
`tools/compare_engines.py` sobre cada motor, con la tolerancia del caso.

`match` = todas las magnitudes dentro de tolerancia · `FORM-DIFF` = diferencia
de formulación esperada (mallas que convergen, triángulos) · `n/a` = el motor
no traduce ese caso y lo dice · `DIFF` = diferencia sin explicar · `ENG-FAIL` =
el motor escribió números que no se pueden comprobar.

| Caso | opensees-mitc4 | opensees-dkgq | calculix | frame3dd |
|---|---|---|---|---|
| `V-BODY-001` | match | match | n/a | n/a |
| `V-DIA-001` | match | match | n/a | n/a |
| `V-DIA-002` | match | match | n/a | n/a |
| `V-DIA-003` | match | match | n/a | n/a |
| `V-ENV-001` | match | match | n/a | match |
| `V-EQ-001` | match | match | n/a | n/a |
| `V-LIN-001` | match | match | n/a | match |
| `V-LIN-001-KNMM` | match | match | n/a | match |
| `V-LIN-001-REV` | match | match | n/a | match |
| `V-LIN-001-ROT` | match | match | n/a | match |
| `V-LIN-002` | match | match | n/a | match |
| `V-LIN-003` | match | match | n/a | match |
| `V-LIN-004` | match | match | n/a | match |
| `V-LIN-005` | match | match | n/a | match |
| `V-MASS-001` | match | match | n/a | match |
| `V-MASS-002` | match | match | n/a | n/a |
| `V-MASS-003` | match | match | n/a | n/a |
| `V-MASS-004` | mecanismo | mecanismo | n/a | match |
| `V-MECH-001` | mecanismo | mecanismo | n/a | mecanismo |
| `V-MECH-002` | mecanismo | mecanismo | n/a | mecanismo |
| `V-MECH-003` | mecanismo | mecanismo | n/a | mecanismo |
| `V-MECH-004` | mecanismo | mecanismo | n/a | mecanismo |
| `V-MECH-005` | mecanismo | mecanismo | n/a | mecanismo |
| `V-MECH-006` | mecanismo | mecanismo | n/a | n/a |
| `V-MECH-007` | mecanismo | mecanismo | n/a | n/a |
| `V-MECH-008` | mecanismo | mecanismo | n/a | n/a |
| `V-MECH-009` | mecanismo | mecanismo | mecanismo | n/a |
| `V-MECH-010` | mecanismo | mecanismo | mecanismo | n/a |
| `V-MECH-011` | mecanismo | mecanismo | n/a | n/a |
| `V-MESH-001` | n/a | n/a | n/a | n/a |
| `V-MESH-002` | n/a | n/a | n/a | n/a |
| `V-MOD-001` | match | match | n/a | DIFF |
| `V-MOD-002` | match | match | n/a | n/a |
| `V-MOD-003` | match | match | n/a | n/a |
| `V-MOD-004` | match | match | n/a | n/a |
| `V-MOM-001` | n/a | n/a | n/a | n/a |
| `V-MOM-002` | n/a | n/a | n/a | n/a |
| `V-OFF-001` | match | match | n/a | n/a |
| `V-OFF-002` | match | match | n/a | n/a |
| `V-OFF-003` | n/a | n/a | n/a | n/a |
| `V-REL-001` | n/a | n/a | n/a | n/a |
| `V-REL-002` | match | match | n/a | n/a |
| `V-SEC-001` | parametric | parametric | parametric | parametric |
| `V-SEIS-001` | match | match | n/a | match |
| `V-SEIS-002` | match | match | n/a | match |
| `V-SEIS-003` | mecanismo | mecanismo | mecanismo | mecanismo |
| `V-SET-001` | match | match | n/a | ENG-FAIL |
| `V-SET-002` | match | match | n/a | n/a |
| `V-SHELL-001` | match | DIFF | DIFF | n/a |
| `V-SHELL-002` | FORM-DIFF | FORM-DIFF | match | n/a |
| `V-SHELL-003` | FORM-DIFF | FORM-DIFF | match | n/a |
| `V-SHELL-004` | match | FORM-DIFF | FORM-DIFF | n/a |
| `V-SHELL-005` | FORM-DIFF | FORM-DIFF | FORM-DIFF | n/a |
| `V-SPEC-001` | match | match | n/a | n/a |
| `V-SPR-001` | match | match | n/a | n/a |
| `V-SPR-002` | match | match | n/a | n/a |
| `V-TIM-001` | match | match | n/a | DIFF |
| `V-TIM-002` | n/a | n/a | n/a | DIFF |
| `V-TRAP-001` | n/a | n/a | n/a | match |
| `V-TRAP-002` | n/a | n/a | n/a | match |
| `V-TRAP-003` | n/a | n/a | n/a | match |
| `V-TRI-001` | DIFF | DIFF | DIFF | n/a |
| `V-TRI-002` | FORM-DIFF | FORM-DIFF | FORM-DIFF | n/a |
| `V-TRI-003` | FORM-DIFF | FORM-DIFF | FORM-DIFF | n/a |
| `V-TRI-004` | FORM-DIFF | FORM-DIFF | FORM-DIFF | n/a |
| `V-TRI-005` | match | match | n/a | n/a |

## Recuento

| Estado | opensees-mitc4 | opensees-dkgq | calculix | frame3dd |
|---|---|---|---|---|
| match | 35 | 33 | 2 | 16 |
| FORM-DIFF | 6 | 7 | 5 | 0 |
| DIFF | 1 | 2 | 2 | 3 |
| n/a | 10 | 10 | 53 | 39 |
| ENG-FAIL | 0 | 0 | 0 | 1 |
| mecanismo | 13 | 13 | 3 | 6 |

## Lectura

- **OpenSees (MITC4)** es el oráculo más amplio: barras, láminas, combinaciones,
  diafragmas, resortes, offsets y espectros.
- **OpenSees (DKGQ)** cambia la familia de lámina: coincide donde MITC4 y añade
  una diferencia en RZ (taladro) que es la firma de la otra formulación.
- **CalculiX** solo cubre láminas (S4/S3), y ahí coincide con Struktal en los
  cuatro nudos; en los triángulos la diferencia es la formulación.
- **Frame3DD** cubre barras con sección general, que es lo que ccx no puede tomar.
  Los `DIFF` restantes están explicados en el docstring del runner: cuerpos
  desconectados (V-TIM-001), mecanismos (V-MOD-001) y asientos (V-SET-001 Q).

Ninguno de los cuatro es oráculo de SAP2000: la campaña del 2026-11-15 cubre
11 casos estáticos y su constancia está en `verification/oracle/`. La columna
`sap2000` de `verification/cases/` sigue `oracle_pending` en casi todo.
