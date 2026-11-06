# Estado del agente (para retomar sin el contexto de la conversación)

Actualizado: 2026-11-05. Guía de origen: `GUIA_AGENTE_MOTOR_FEM_RUST_COMPATIBILIDAD_SAP2000.md` v1 (subida por el usuario, no incluida en el repo).

## Repositorio

- Remoto público: https://github.com/marcel1983/struktal. Esquema de modelo v5 (v1 a v4 se migran). SHA: ver `git log -1`.
- Toolchain: rustc/cargo 1.97.0. Plataformas probadas: Linux x86_64 y Windows 10 x86_64 (MSVC); benchmarks solo en Linux.
- Proyecto anterior del usuario, fuente de lo que se porta: PAS (fuera del repo). Portar con casos analíticos propios, sin archivos de SAP2000 ni afirmar como verificado lo que PAS afirma; ver decisiones 0010 y 0011.

## Entregas

| Paquete | Estado | Evidencia |
|---|---|---|
| P00 | Parcial. Perfil, inventario (123 req.), convenciones, matrices S2K/API, tolerancias, formato de casos y comparador creados. Falta instalación de SAP2000, build exacto, manuales, y congelar inventario. | `spec/`, `verification/`, decisión 0004 |
| P01 | Hecho para el subconjunto: modelo v1 serializable, validación todo-o-nada, rechazo de esquema futuro, CLI (`validate`, `solve`, `capabilities`, `verify`, `compare`), manifiesto y estados por caso. Migración: solo gancho. | `struktal-core`, `struktal-cli`, tests |
| P02 | Parcial. Tripletas→CSC determinista; backends `faer-sparse-ldlt` (por defecto, AMD, supernodal) y `skyline-ldlt` (referencia) tras el mismo contrato; residuo; presupuesto de memoria antes de reservar; espacio nulo verificado con cotas y hueco espectral (F08-007). Falta: backend para indefinidas (MUMPS u otro), ensamblaje simbólico reutilizable. | `struktal-math`, decisiones 0002, 0005 y 0006 |
| P03 | Parcial: subdivisión de barras con procedencia (decisión 0015). Falta: mallado de áreas. | `struktal-analysis/src/mesh.rs`, V-MESH-* |
| P04 | Parcial. Barra 3D Timoshenko/Euler–Bernoulli (As2, As3; decisión 0007), sección general, cargas nodales/uniformes/trapezoidales/puntuales/momentos en vano/peso propio con fuerzas de empotramiento exactas (decisión 0014), suma lineal. Falta: secciones paramétricas (y As automático) y variables, cargas térmicas, envolventes, importación S2K mínima. | `struktal-elements`, `struktal-analysis`, V-LIN-001..005, V-TRAP-*, V-MOM-* |
| P05 | Parcial: liberaciones y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragma, body y equal por eliminación exacta (decisión 0011); resortes de nudo y asientos (decisión 0012). Falta: puntos cardinales y zonas rígidas por factor, restricciones locales y weld, ejes locales de nudo, masas. | `release.rs`, `frame3d::offset_transformation`, `struktal-analysis/src/constraints.rs`, V-REL-*, V-OFF-*, V-DIA-*, V-BODY-*, V-EQ-*, V-SPR-*, V-SET-*, V-MECH-006..008 |
| P06–P16 | No iniciados. | `spec/features.csv` |

## Último resultado de pruebas (ejecutado)

- `cargo fmt --all -- --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 96 pruebas, 0 fallos. `cargo test -p struktal-math --no-default-features` (solo skyline): 20, 0 fallos.
- `struktal verify --backend {faer,skyline} verification/cases/*/case.json`: 37/37 `pass` con ambos; todos `oracle_pending` para SAP2000.
- `tools/compare_engines.py`: comparación opcional con OpenSeesPy y Pynite (decisión 0013). CI en `.github/workflows/ci.yml`: en verde en GitHub hasta el commit 34ef550.
- Benchmarks: docs/benchmarks.md.

## Decisiones pendientes

1. Backend para matrices indefinidas (pandeo, multiplicadores): MUMPS por FFI u otra opción; faer LBLᵀ intranodal descartado. Los diafragmas ya no lo necesitan (decisión 0011).
2. Convención de signos de salida compatible con SAP2000: requiere oráculo.

## Siguiente paso concreto

Objetivo del usuario: herramienta open source para su propio trabajo, con modelos que suelen tener losas o muros. Orden acordado el 2026-11-05: 1) resto de pórticos, 2) lo que pide un modelo real (secciones paramétricas, cargas, subdivisión, envolventes), 3) S2K, 4) láminas, 5) masas, modos y espectro.

1. Lo que pide un modelo real: envolventes (F16-002) y secciones paramétricas (F02-002).
2. P13 mínimo: parser S2K con AST para las tablas de `spec/s2k-table-coverage.csv`, en modo estricto.

## Comandos de reproducción

```bash
cargo test --workspace
cargo run -q -p struktal-cli -- verify verification/cases/*/case.json                    # faer
cargo run -q -p struktal-cli -- verify --backend skyline verification/cases/*/case.json
cargo build --release && python3 tools/bench.py /tmp/bench faer
python3 tools/gen_cases.py   # solo si se cambia un caso a propósito; revisar el diff de case.json
```
