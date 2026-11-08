# Estado del agente (para retomar sin el contexto de la conversación)

Actualizado: 2026-11-08. Guía de origen: `GUIA_AGENTE_MOTOR_FEM_RUST_COMPATIBILIDAD_SAP2000.md` v1 (subida por el usuario, no incluida en el repo).

## Repositorio

- Remoto público: https://github.com/marcel1983/struktal. Esquema de modelo v7 (v1 a v6 se migran). SHA: ver `git log -1`.
- Toolchain: rustc/cargo 1.97.0. Plataformas probadas: Linux x86_64 y Windows 10 x86_64 (MSVC); benchmarks solo en Linux.
- Proyecto anterior del usuario, fuente de lo que se porta: PAS (fuera del repo). Portar con casos analíticos propios, sin archivos de SAP2000 ni afirmar como verificado lo que PAS afirma; ver decisiones 0010 y 0011.

## Entregas

| Paquete | Estado | Evidencia |
|---|---|---|
| P00 | Parcial. Perfil, inventario (125 req.: 36 verificados, 15 parciales), convenciones, matrices S2K/API, tolerancias, formato de casos y comparador creados. Falta instalación de SAP2000, build exacto, manuales, y congelar inventario. | `spec/`, `verification/`, decisión 0004 |
| P01 | Hecho para el subconjunto: modelo v1 serializable, validación todo-o-nada, rechazo de esquema futuro, CLI (`validate`, `solve`, `capabilities`, `verify`, `compare`), manifiesto y estados por caso. Migración: solo gancho. | `struktal-core`, `struktal-cli`, tests |
| P02 | Parcial. Tripletas→CSC determinista; backends `faer-sparse-ldlt` (por defecto, AMD, supernodal) y `skyline-ldlt` (referencia) tras el mismo contrato; residuo; presupuesto de memoria antes de reservar; espacio nulo verificado con cotas y hueco espectral (F08-007). Falta: backend para indefinidas (MUMPS u otro), ensamblaje simbólico reutilizable. | `struktal-math`, decisiones 0002, 0005 y 0006 |
| P03 | Parcial: subdivisión de barras con procedencia (decisión 0015). Falta: mallado de áreas. | `struktal-analysis/src/mesh.rs`, V-MESH-* |
| P04 | Parcial. Barra 3D Timoshenko/Euler–Bernoulli (As2, As3; decisión 0007), sección general, cargas nodales/uniformes/trapezoidales/puntuales/momentos en vano/peso propio con fuerzas de empotramiento exactas (decisión 0014), suma lineal. Falta: secciones paramétricas (y As automático) y variables, cargas térmicas, envolventes, importación S2K mínima. | `struktal-elements`, `struktal-analysis`, V-LIN-001..005, V-TRAP-*, V-MOM-* |
| P05 | Parcial: liberaciones y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragma, body y equal por eliminación exacta (decisión 0011); resortes de nudo y asientos (decisión 0012). Falta: puntos cardinales y zonas rígidas por factor, restricciones locales y weld, ejes locales de nudo, masas. | `release.rs`, `frame3d::offset_transformation`, `struktal-analysis/src/constraints.rs`, V-REL-*, V-OFF-*, V-DIA-*, V-BODY-*, V-EQ-*, V-SPR-*, V-SET-*, V-MECH-006..008 |
| P06 (láminas) | Parcial: lámina de 4 nudos, membrana QM6, placa MITC4 y taladro Hughes-Brezzi (decisión 0019); lámina de 3 nudos con membrana constante y placa MITC3 (decisión 0021); cargas uniformes y esfuerzos. Falta: mallado propio. | `shell4.rs`, `shell3.rs`, `areas.rs`, V-SHELL-*, V-TRI-*, V-MECH-009, V-MECH-010 |
| Dinámica | Parcial: masas, fuente de masa, modos y espectro (decisiones 0020 y 0022). Falta: masa faltante, combinación direccional, historia en el tiempo. | `eigen.rs`, `dynamics.rs`, V-MOD-*, V-SPEC-*, V-MASS-* |
| Resto de P07–P16 | No iniciados, salvo lector S2K de P13, con malla de áreas, fuente de masa, modos y espectros (decisiones 0018, 0021 y 0022). La numeración exacta depende de la guía, que no está en el repo. | `spec/features.csv` |

## Último resultado de pruebas (ejecutado)

- `cargo fmt --all -- --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 179 pruebas, 0 fallos. `cargo test -p struktal-math --no-default-features` (solo skyline): 24, 0 fallos.
- `struktal verify --backend {faer,skyline} verification/cases/*/case.json`: 66/66 `pass` con ambos; todos `oracle_pending` para SAP2000.
- `tools/compare_engines.py`: comparación opcional con OpenSeesPy y Pynite (decisión 0013); con triángulos da DIFF en V-TRI-001, explicado en la decisión 0021. CI en `.github/workflows/ci.yml`: `add290a` y `65feef9` fallaron solo en el trabajo de referencias, porque `sin` y `cos` redondean distinto entre Linux y Windows. `tools/check_cases.py` lo corrige en dos pasos, el segundo para el hash de `model.json`; CI en verde desde `199b727`. Benchmarks de los ejemplos de OpenSees: decisión 0023.
- Benchmarks: docs/benchmarks.md.

## Decisiones pendientes

1. Backend para matrices indefinidas (pandeo, multiplicadores): MUMPS por FFI u otra opción; faer LBLᵀ intranodal descartado. Los diafragmas ya no lo necesitan (decisión 0011).
2. Convención de signos de salida compatible con SAP2000: requiere oráculo.
3. Láminas alabeadas (decisión 0025): el taladro débil (G/1000) no converge en superficies alabeadas (viga torcida +30 %), y uno fuerte rigidiza muros. Opciones: A) mantener el rechazo (actual), B) membrana con taladro propio (Allman / Ibrahimbegovic) más corrección de alabeo. Pendiente: el usuario comprobará cómo resuelve PAS la viga torcida (desplazamientos de punta IN/OUT, o su código de láminas como referencia).

## Siguiente paso concreto

Objetivo del usuario: herramienta open source para su propio trabajo, con modelos que suelen tener losas o muros. Orden acordado el 2026-11-05: 1) resto de pórticos, 2) lo que pide un modelo real (secciones paramétricas, cargas, subdivisión, envolventes), 3) S2K, 4) láminas, 5) masas, modos y espectro.

1. Sismo automático por coeficiente de usuario: **hecho** (decisión 0026). Primer edificio real del usuario importado y contrastado localmente: peso, cortante basal y reacciones idénticos; periodos principales dentro del 0,8 %. Segundo archivo con diafragmas: excentricidad hecha, reacciones y torsor idénticos. Tercer archivo con resultados de espectro: cortante basal CQC exacto con los modos de SAP2000 (+0,5 % con los propios). Espectro sin excentricidad contrastado (cuarto archivo): principales dentro del 0,8–2 %. Excentricidad del espectro: forma identificada (par estático por diafragma sumado en valor absoluto, ajuste 0,13 %), magnitud explicada en Y (B = 18 m) pero no en X (B efectivo 22,9 m); falta el manual para implementarla. Faltan también las normas sísmicas.
1. Importador con modelos reales del usuario: placa gruesa con malla (576 láminas), contrastada localmente con sus propios resultados de SAP2000 con un 0,18 % de diferencia máxima; patch test de láminas grueso y delgado: membrana idéntica, flexión limitada por datos de entrada redondeados en el propio `.s2k` (decisión 0024). Falta un modelo con espectro y uno de edificio con muros.
2. Lo demás que bloquea modelos locales: cargas proyectadas, temperatura, ejes locales de nudo, I23 real, secciones con I22 o J nulas, links.
3. Contraste con SAP2000: `tools/gen_contrast_lot.py` genera el lote; pendiente de que el usuario revise la licencia y lo ejecute en su otro equipo.

## Comandos de reproducción

```bash
cargo test --workspace
cargo run -q -p struktal-cli -- verify verification/cases/*/case.json                    # faer
cargo run -q -p struktal-cli -- verify --backend skyline verification/cases/*/case.json
cargo build --release && python3 tools/bench.py /tmp/bench faer
python3 tools/gen_cases.py   # solo si se cambia un caso a propósito; revisar el diff de case.json
```
