# rust-fem

Motor de elementos finitos en Rust con el objetivo, **todavía no verificado**, de cubrir el perfil `sap2000-ultimate-27.1.0-target` (ver `spec/sap_profile.yaml`).

La versión 0.1.0 es un subconjunto: análisis estático lineal de pórticos 3D Euler–Bernoulli con diagnóstico de mecanismos. No es equivalente a SAP2000. Cobertura y discrepancias en `docs/compatibility-report.md`; estado para retomar el trabajo en `docs/agent-state.md`.

## Uso

```bash
cargo build --release
./target/release/rust-fem validate verification/cases/V-LIN-001/model.json
./target/release/rust-fem solve verification/cases/V-LIN-003/model.json --stations 5 -o results.json
./target/release/rust-fem solve verification/cases/V-LIN-003/model.json --backend skyline   # backend de referencia
./target/release/rust-fem capabilities
./target/release/rust-fem verify verification/cases/*/case.json
```

Códigos de salida: 0 éxito; 1 verificación fallida o caso no completado; 2 entrada inválida; 3 E/S.

## Estructura

| Ruta | Contenido |
|---|---|
| `crates/fem-core` | Modelo canónico v1, unidades, validación, hash de snapshot |
| `crates/fem-math` | Tripletas/CSC, contrato de solver; backends `faer-sparse-ldlt` (por defecto) y `skyline-ldlt` (referencia) |
| `crates/fem-elements` | Barra 3D Euler–Bernoulli, ejes locales, cargas equivalentes, acciones en estaciones |
| `crates/fem-analysis` | Estática lineal, combinaciones, resultados, manifiesto, estados por caso |
| `crates/fem-cli` | Binario `rust-fem` y comparador de verificación |
| `spec/` | Perfil, inventario de requisitos, convenciones, cobertura S2K/API, capacidades de solver |
| `verification/` | Casos con referencia analítica independiente, tolerancias, oráculos externos |
| `docs/` | Decisiones, informe de compatibilidad, benchmarks, estado del agente |
| `tools/` | Generadores de casos y modelos de rendimiento |
