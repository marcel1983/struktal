# Struktal

Motor de elementos finitos en Rust con el objetivo, **todavía no verificado**, de cubrir el perfil `sap2000-ultimate-27.1.0-target` (ver `spec/sap_profile.yaml`).

La versión 0.1.0 es un subconjunto: análisis estático lineal, modos propios y espectro de respuesta de pórticos 3D (Timoshenko, o Euler–Bernoulli si el área de cortante es 0) y láminas de 4 nudos, con liberaciones, offsets rígidos, restricciones (diafragma, body, equal), resortes de nudo, asientos y diagnóstico de mecanismos. No es equivalente a SAP2000. Cobertura y discrepancias en `docs/compatibility-report.md`; estado para retomar el trabajo en `docs/agent-state.md`.

## Uso

```bash
cargo build --release
./target/release/struktal validate verification/cases/V-LIN-001/model.json
./target/release/struktal solve verification/cases/V-LIN-003/model.json --stations 5 -o results.json
./target/release/struktal solve verification/cases/V-LIN-003/model.json --backend skyline   # backend de referencia
./target/release/struktal import modelo.s2k -o modelo.json
./target/release/struktal capabilities
./target/release/struktal verify verification/cases/*/case.json
```

Códigos de salida: 0 éxito; 1 verificación fallida o caso no completado; 2 entrada inválida; 3 E/S.

## Estructura

| Ruta | Contenido |
|---|---|
| `crates/struktal-core` | Modelo canónico (esquema v3, migra v1 y v2), unidades, validación, hash de snapshot |
| `crates/struktal-math` | Tripletas/CSC, contrato de solver; backends `faer-sparse-ldlt` (por defecto) y `skyline-ldlt` (referencia) |
| `crates/struktal-elements` | Barra 3D con cortante, liberaciones, offsets, ejes locales, cargas equivalentes, acciones en estaciones |
| `crates/struktal-analysis` | Estática lineal, restricciones por eliminación, combinaciones, resultados, manifiesto, estados por caso |
| `crates/struktal-io-s2k` | Lector S2K estricto: solo barras, y rechaza lo que no puede representar |
| `crates/struktal-cli` | Binario `struktal` y comparador de verificación |
| `spec/` | Perfil, inventario de requisitos, convenciones, cobertura S2K/API, capacidades de solver |
| `verification/` | Casos con referencia analítica independiente, tolerancias, oráculos externos |
| `docs/` | Decisiones, informe de compatibilidad, benchmarks, estado del agente |
| `tools/` | Generadores de casos y modelos de rendimiento; comparación opcional con otros motores (decisión 0013) |

## Marcas y afiliación

SAP2000, ETABS, SAFE y CSiBridge son marcas de Computers and Structures, Inc. (CSI). SAP es marca de SAP SE. Struktal es un proyecto independiente: no está afiliado a esas empresas, ni respaldado o patrocinado por ellas. Esos nombres se usan solo para describir formatos de archivo y objetivos de compatibilidad. Ver `NOTICE`.

## Licencia

GPL-3.0 (ver `LICENSE`). Las dependencias usadas (faer, serde, sha2, clap) tienen licencias MIT/Apache-2.0, compatibles con GPL-3.0.
