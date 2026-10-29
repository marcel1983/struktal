# Estado del agente (para retomar sin el contexto de la conversación)

Actualizado: 2026-10-29. Guía de origen: `GUIA_AGENTE_MOTOR_FEM_RUST_COMPATIBILIDAD_SAP2000.md` v1 (subida por el usuario, no incluida en el repo).

## Repositorio

- Ruta: `rust-fem/`. Git local inicializado, sin remoto. SHA: ver `git log -1`.
- Toolchain: rustc/cargo 1.97.0. Plataforma probada: Linux x86_64. Windows: no probado.

## Entregas

| Paquete | Estado | Evidencia |
|---|---|---|
| P00 | Parcial. Perfil, inventario (123 req.), convenciones, matrices S2K/API, tolerancias, formato de casos y comparador creados. Falta instalación de SAP2000, build exacto, manuales, y congelar inventario. | `spec/`, `verification/`, decisión 0004 |
| P01 | Hecho para el subconjunto: modelo v1 serializable, validación todo-o-nada, rechazo de esquema futuro, CLI (`validate`, `solve`, `capabilities`, `verify`, `compare`), manifiesto y estados por caso. Migración: solo gancho. | `fem-core`, `fem-cli`, tests |
| P02 | Parcial. Tripletas→CSC determinista, RCM, skyline LDLᵀ escalado, residuo, diagnóstico de mecanismo/indefinida, presupuesto de memoria. Falta: búsqueda de vectores del espacio nulo, backend para indefinidas y grande (faer/MUMPS). | `fem-math`, decisión 0002 |
| P03 | No iniciado (no hay mallado ni subdivisión). | — |
| P04 | Parcial. Barra Euler–Bernoulli 3D, sección general, cargas nodales/uniformes/puntuales/peso propio, suma lineal. Falta: Timoshenko, secciones paramétricas y variables, cargas trapezoidales/térmicas, envolventes, importación S2K mínima. | `fem-elements`, `fem-analysis`, V-LIN-001..005 |
| P05–P16 | No iniciados. | `spec/features.csv` |

## Último resultado de pruebas (ejecutado)

- `cargo fmt --all -- --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 41 pruebas, 0 fallos.
- `rust-fem verify verification/cases/*/case.json`: 10/10 `pass`; todos `oracle_pending` para SAP2000.
- Benchmarks: docs/benchmarks.md.

## Decisiones pendientes

1. Backend definitivo: evaluar `faer` (Cholesky dispersa, LDLᵀ con pivoteo) con el banco de matrices de `fem-math`.
2. Convención de signos de salida compatible con SAP2000: requiere oráculo.
3. Si la deformación por cortante debe estar activa por defecto (como SAP2000) cuando exista Timoshenko.

## Siguiente paso concreto

1. P02: prueba de viabilidad de `faer` como segundo backend detrás del mismo contrato (`MatrixClass`, `SolverError`, diagnósticos); comparar con skyline en los casos y en `tools/gen_grid.py 16 16 30`.
2. P05: liberaciones de extremo con condensación de K **y** de cargas, con caso analítico de viga articulada y resorte (caso 4 de §21 aún no creado).
3. P13 mínimo: parser S2K con AST para las tablas de `spec/s2k-table-coverage.csv`, en modo estricto.

## Comandos de reproducción

```bash
cargo test --workspace
cargo run -q -p fem-cli -- verify verification/cases/*/case.json
python3 tools/gen_cases.py   # solo si se cambia un caso a propósito; revisar el diff de case.json
```
