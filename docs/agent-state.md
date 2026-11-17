# Estado del agente (para retomar sin el contexto de la conversación)

Actualizado: 2026-11-16. Guía de origen: `GUIA_AGENTE_MOTOR_FEM_RUST_COMPATIBILIDAD_SAP2000.md` v1 (subida por el usuario, no incluida en el repo).

## Repositorio

- Remoto público: https://github.com/marcel1983/struktal. Licencia AGPL-3.0-only desde el 2026-11-13, antes GPL-3.0 (decisión 0026); no se ha incorporado código de otros motores. Esquema de modelo v8 (v1 a v7 se migran). SHA: ver `git log -1`.
- Toolchain: rustc/cargo 1.97.0. Plataformas probadas: Linux x86_64 y Windows 10 x86_64 (MSVC); benchmarks solo en Linux.
- Proyecto anterior del usuario, fuente de lo que se porta: PAS (fuera del repo). Portar con casos analíticos propios, sin archivos de SAP2000 ni afirmar como verificado lo que PAS afirma; ver decisiones 0010 y 0011.

## Entregas

| Paquete | Estado | Evidencia |
|---|---|---|
| P00 | Parcial. Perfil, inventario (124 req.), convenciones, matrices S2K/API, tolerancias, formato de casos y comparador creados. **SAP2000 ya se ha ejecutado** (campaña del 2026-11-15, 11 casos); falta el build exacto — que **no se define por ahora** —, congelar el inventario y la instalación permanente. | `spec/`, `verification/`, decisión 0004 |
| P01 | Hecho para el subconjunto: modelo v1 serializable, validación todo-o-nada, rechazo de esquema futuro, CLI (`validate`, `solve`, `capabilities`, `verify`, `compare`), manifiesto y estados por caso. Migración: solo gancho. | `struktal-core`, `struktal-cli`, tests |
| P02 | Parcial. Tripletas→CSC determinista; backends `faer-sparse-ldlt` (por defecto, AMD, supernodal) y `skyline-ldlt` (referencia) tras el mismo contrato; residuo; presupuesto de memoria antes de reservar; espacio nulo verificado con cotas y hueco espectral (F08-007). Falta: backend para indefinidas (MUMPS u otro), ensamblaje simbólico reutilizable. | `struktal-math`, decisiones 0002, 0005 y 0006 |
| P03 | Parcial: subdivisión de barras con procedencia (decisión 0015). Falta: mallado de áreas. | `struktal-analysis/src/mesh.rs`, V-MESH-* |
| P04 | Parcial. Barra 3D Timoshenko/Euler–Bernoulli (As2, As3; decisión 0007), sección general, cargas nodales/uniformes/trapezoidales/puntuales/momentos en vano/peso propio con fuerzas de empotramiento exactas (decisión 0014), suma lineal. Falta: secciones paramétricas (y As automático) y variables, cargas térmicas, envolventes, importación S2K mínima. | `struktal-elements`, `struktal-analysis`, V-LIN-001..005, V-TRAP-*, V-MOM-* |
| P05 | Parcial: liberaciones y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragma, body y equal por eliminación exacta (decisión 0011); resortes de nudo y asientos (decisión 0012). Falta: puntos cardinales y zonas rígidas por factor, restricciones locales y weld, ejes locales de nudo, masas. | `release.rs`, `frame3d::offset_transformation`, `struktal-analysis/src/constraints.rs`, V-REL-*, V-OFF-*, V-DIA-*, V-BODY-*, V-EQ-*, V-SPR-*, V-SET-*, V-MECH-006..008 |
| P06 (láminas) | Parcial: lámina de 4 nudos, membrana QM6, placa MITC4 y taladro Hughes-Brezzi (decisión 0019); lámina de 3 nudos con membrana constante y placa MITC3 (decisión 0021); cargas uniformes y esfuerzos. Falta: mallado propio. | `shell4.rs`, `shell3.rs`, `areas.rs`, V-SHELL-*, V-TRI-*, V-MECH-009, V-MECH-010 |
| Dinámica | Parcial: masas, fuente de masa, modos y espectro (decisiones 0020 y 0022); carga sísmica por coeficiente de usuario (decisión 0024). Falta: masa faltante, combinación direccional, historia en el tiempo, coeficientes de norma. | `eigen.rs`, `dynamics.rs`, V-MOD-*, V-SPEC-*, V-MASS-* |
| Resto de P07–P16 | No iniciados, salvo lector S2K de P13, con malla de áreas, fuente de masa, modos y espectros (decisiones 0018, 0021 y 0022). La numeración exacta depende de la guía, que no está en el repo. | `spec/features.csv` |

## Último resultado de pruebas (ejecutado)

- `cargo fmt --all -- --check`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: 180 pruebas, 0 fallos. `cargo test -p struktal-math --no-default-features` (solo skyline): 24, 0 fallos.
- `struktal verify --backend {faer,skyline} verification/cases/*/case.json`: 66/66 `pass` con ambos; todos `oracle_pending` para SAP2000.
- `tools/compare_engines.py`: comparación opcional con OpenSeesPy y Pynite (decisión 0013); con triángulos da DIFF en V-TRI-001, explicado en la decisión 0021. CI en `.github/workflows/ci.yml`: `add290a` y `65feef9` fallaron solo en el trabajo de referencias, porque `sin` y `cos` redondean distinto entre Linux y Windows. `tools/check_cases.py` lo corrige en dos pasos, el segundo para el hash de `model.json`; CI en verde desde `199b727`. Benchmarks de los ejemplos de OpenSees: decisión 0023. Comparación ampliada con OpenSees (malla, masa, sismo, espectro): decisión 0013.
- Benchmarks: docs/benchmarks.md.
- Núcleo de lámina laminada (decisión 0032, primer paso de F03-004): `struktal-core/src/laminate.rs` calcula `A`, `B` y `D` por unidad de ancho a partir de una pila de capas, con 6 tests. Falta que el elemento tome `A`/`D` en vez de `E`/`ν`/`t`, y el caso de verificación.
- Mallado de áreas (decisión 0031, F03-008): `Area.auto_mesh {m, n}` en el esquema v9; el análisis sustituye el objeto por sus elementos, comparte los nudos de los bordes entre áreas vecinas y reparte la carga. 184 tests (4 nuevos) y 66/66 casos de verificación. Falta un caso de verificación formal en `verification/cases/` y el contraste con SAP2000.
- Campaña de contraste con SAP2000 (2026-11-15): 11 casos estáticos de 3 modelos escritos a mano, 276 valores, 0 diferencias. Constancia en `verification/oracle/2026-11-15-campana-sap2000.md` y `docs/compatibility-report.md`. Ningún fichero de SAP2000 entra al repo. Levanta el bloqueo de signos (decisión 0003) en estático lineal; el build exacto **no se define por ahora** (decisión del mantenedor, 2026-11-16).
- `tools/run_sap2000_campaign.py`: lanza el lote de contraste por la OAPI de SAP2000 (comtypes). Abre cada `.s2k`, resuelve y escribe `<modelo>-sap2000.s2k`. Comprueba cada código de retorno. No se ha ejecutado contra SAP2000 en CI (no hay licencia): está escrito contra la API documentada y `--dry-run` lista los modelos.
- `tools/check_contrast_lot.py`: colector de la campaña de contraste con SAP2000. Lee un CSV por modelo (Case, Joint, UX..RZ, FX..MZ) y dice por componente si SAP2000 coincide, coincide con el signo cambiado o difiere. Es lo que resuelve la convención de signos (decisión 0003), que ningún oráculo open source puede decidir. Los modelos salen de `tools/gen_contrast_lot.py`.
- `docs/oracle-coverage.md`: matriz de los 66 casos frente a OpenSees (MITC4 y DKGQ), CalculiX y Frame3DD (2026-11-16). match: 35 / 33 / 2 / 16. Sirve para ver de un vistazo qué cubre cada oráculo; DKGQ queda por detrás de MITC4, como dice la decisión 0029.
- Importador S2K: desde la decisión 0027 un caso de carga que no puede representar es un problema (el fichero se rechaza y no se escribe modelo); `--allow-partial` conserva el comportamiento anterior. Probado con `verification/s2k/portal.s2k`, que declara un caso `MODAL` sin su tabla.
- `tools/oracles/opensees_runner.py`: `STRUKTAL_OPENSEES_SHELL=dkgq` cambia la familia de lámina a ShellDKGQ/ShellDKGT (decisión 0029). Medido: DKGQ no es mejor que MITC4, solo diagnóstico; las dos familias dan el mismo `MZ` espurio en los triángulos de V-TRI-001, así que ese artefacto es del taladro del triángulo de OpenSees.
- `tools/oracles/frame3dd_runner.py`: oráculo de barras con sección general (decisión 0030). Frame3DD imprime 6 decimales, así que `compare_engines.py` suma `ENGINE_ATOL = 1e-6`. Coincide en V-LIN-001/002/004/005 y V-TRAP-001/002/003; rechaza con causa lo demás. Dos convenciones traducidas: ejes locales 90° (cargas `(a,-c,b)`) e `I22`/`I33` intercambiados.
- `tools/oracles/calculix_runner.py`: segunda formulación de lámina para `tools/compare_engines.py` (decisión 0028). Solo láminas: ccx no toma secciones generales de barra sin un elemento de usuario. Verificado con V-SHELL-001 TEN, V-SHELL-002, V-SHELL-003 y V-TRI-001 MIX; quedan dos diferencias escritas en el docstring (V-SHELL-001 BEN ~16 % y V-TRI-001 Q en tres reacciones). Necesita `ccx` en PATH o `CCX`.
- `tools/diff_s2k_import.py`: huella física del S2K (geometría, longitud de barras, área y resultantes de carga) leída del propio fichero, comparada con lo que importa struktal. Sin motor de por medio. Con `--opensees-studio` compara además el round trip de OpenSees Studio, que exporta el objeto y no su malla: un fichero con `OBJECTS AND ELEMENTS - AREAS` vuelve con un área en vez de sus elementos.

## Decisiones pendientes

1. Backend para matrices indefinidas (pandeo, multiplicadores): MUMPS por FFI u otra opción; faer LBLᵀ intranodal descartado. Los diafragmas ya no lo necesitan (decisión 0011).
2. Convención de signos de salida compatible con SAP2000: la de `V2/V3/M2/M3` quedó verificada en estático lineal (2026-11-15); falta la convención de salida de resultados (`V2/V3/M2/M3` de los diagramas) y el resto de casos.

## Siguiente paso concreto

Objetivo del usuario: herramienta open source para su propio trabajo, con modelos que suelen tener losas o muros. Orden acordado el 2026-11-05: 1) resto de pórticos, 2) lo que pide un modelo real (secciones paramétricas, cargas, subdivisión, envolventes), 3) S2K, 4) láminas, 5) masas, modos y espectro.

1. Contraste con SAP2000 de `ejemplo_lineal` y `ejemplo_combos_lineal`, que ya se importan y resuelven sin modificarlos (decisión 0024): **hecho para estático** (campaña 2026-11-15); falta modos, fuente de masa y sismo automático.
1. Probar el importador con un modelo real del usuario, con losas o muros, su malla y un espectro: mallas y espectros completos solo se han probado con archivos escritos a mano (decisiones 0021 y 0022).
2. Lo demás que bloquea modelos locales: cargas proyectadas, temperatura, ejes locales de nudo, I23 real, secciones con I22 o J nulas, links.
3. Contraste con SAP2000: **hecho** para el lote estático (11 casos, 276 valores). Siguiente: repetir con modelos reales del usuario (losas o muros) y ampliar a modos y espectros.

## Comandos de reproducción

```bash
cargo test --workspace
cargo run -q -p struktal-cli -- verify verification/cases/*/case.json                    # faer
cargo run -q -p struktal-cli -- verify --backend skyline verification/cases/*/case.json
cargo build --release && python3 tools/bench.py /tmp/bench faer
python3 tools/gen_cases.py   # solo si se cambia un caso a propósito; revisar el diff de case.json
```
