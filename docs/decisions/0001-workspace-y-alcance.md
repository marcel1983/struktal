# 0001 — Workspace y alcance de la primera entrega

Fecha: 2026-10-29. Estado: aceptada.

- Crates creados solo donde hay código real: `fem-core`, `fem-math`, `fem-elements`, `fem-analysis`, `fem-cli`. Los demás de la guía §5 (`fem-materials`, `fem-mesh`, `fem-results`, `fem-design`, `fem-io-sap`, `fem-python`, `fem-backend-ffi`) se crearán con su primera funcionalidad, no como cascarones vacíos.
- `unsafe_code = "forbid"` en todo el workspace; `clippy::unwrap_used` avisa.
- Dependencias: `serde`, `serde_json`, `sha2`, `clap`. Sin backend numérico externo en v0.1 (ver 0002).
- Toolchain fijado en `rust-toolchain.toml` (1.97.0, edición 2024, MSRV declarado 1.85 — no probado en 1.85).
- Desviación de la guía §2.2: tolerancias en `verification/tolerances.json` (no YAML) para leerlas sin dependencia YAML. Los YAML de `spec/` son documentales y no se parsean.
