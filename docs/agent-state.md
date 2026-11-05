# Estado del agente (para retomar sin el contexto de la conversación)

Actualizado: 2026-10-29. Guía de origen: `GUIA_AGENTE_MOTOR_FEM_RUST_COMPATIBILIDAD_SAP2000.md` v1 (subida por el usuario, no incluida en el repo).

## Repositorio

- Ruta: `struktal/`. Esquema de modelo v2 (v1 se migra). Git local inicializado, sin remoto. SHA: ver `git log -1`.
- Toolchain: rustc/cargo 1.97.0. Plataforma probada: Linux x86_64. Windows: no probado.

## Entregas

| Paquete | Estado | Evidencia |
|---|---|---|
| P00 | Parcial. Perfil, inventario (123 req.), convenciones, matrices S2K/API, tolerancias, formato de casos y comparador creados. Falta instalación de SAP2000, build exacto, manuales, y congelar inventario. | `spec/`, `verification/`, decisión 0004 |
| P01 | Hecho para el subconjunto: modelo v1 serializable, validación todo-o-nada, rechazo de esquema futuro, CLI (`validate`, `solve`, `capabilities`, `verify`, `compare`), manifiesto y estados por caso. Migración: solo gancho. | `struktal-core`, `struktal-cli`, tests |
| P02 | Parcial. Tripletas→CSC determinista; backends `faer-sparse-ldlt` (por defecto, AMD, supernodal) y `skyline-ldlt` (referencia) tras el mismo contrato; residuo; presupuesto de memoria antes de reservar; espacio nulo verificado con cotas y hueco espectral (F08-007). Falta: backend para indefinidas (MUMPS u otro), ensamblaje simbólico reutilizable. | `struktal-math`, decisiones 0002, 0005 y 0006 |
| P03 | No iniciado (no hay mallado ni subdivisión). | — |
| P04 | Parcial. Barra 3D Timoshenko/Euler–Bernoulli (As2, As3; decisión 0007), sección general, cargas nodales/uniformes/puntuales/peso propio con fuerzas de empotramiento exactas, suma lineal. Falta: secciones paramétricas (y As automático) y variables, cargas trapezoidales/térmicas, envolventes, importación S2K mínima. | `struktal-elements`, `struktal-analysis`, V-LIN-001..005 |
| P05 | Parcial: liberaciones y fijación parcial con condensación de K y cargas, combinaciones inestables rechazadas (decisión 0008). Falta: offsets/zonas rígidas, restricciones (equalDOF, diafragmas), masas. | `struktal-elements/src/release.rs`, V-REL-001/002, V-MECH-006 |
| P06–P16 | No iniciados. | `spec/features.csv` |

## Último resultado de pruebas (ejecutado)

- `cargo fmt --all -- --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 74 pruebas, 0 fallos. `cargo test -p struktal-math --no-default-features` (solo skyline): 20, 0 fallos.
- `struktal verify --backend {faer,skyline} verification/cases/*/case.json`: 18/18 `pass` con ambos; todos `oracle_pending` para SAP2000.
- Benchmarks: docs/benchmarks.md.

## Decisiones pendientes

1. Backend para matrices indefinidas (P-Delta, pandeo, multiplicadores): MUMPS por FFI u otra opción; faer LBLᵀ intranodal descartado.
2. Convención de signos de salida compatible con SAP2000: requiere oráculo.

## Siguiente paso concreto

1. P05: offsets y zonas rígidas (F04-004), con transformación cinemática y brazos; después restricciones (F04-005) y masas (F07).
2. P13 mínimo: parser S2K con AST para las tablas de `spec/s2k-table-coverage.csv`, en modo estricto.

## Comandos de reproducción

```bash
cargo test --workspace
cargo run -q -p struktal-cli -- verify verification/cases/*/case.json                    # faer
cargo run -q -p struktal-cli -- verify --backend skyline verification/cases/*/case.json
cargo build --release && python3 tools/bench.py /tmp/bench faer
python3 tools/gen_cases.py   # solo si se cambia un caso a propósito; revisar el diff de case.json
```
