# Contribuir a Struktal

Struktal sigue la guía de desarrollo del proyecto: cada función se entrega completa (modelo, cálculo, prueba independiente, documentación) antes de pasar a la siguiente.

## Reglas

1. **Referencias independientes.** Los valores esperados de `verification/cases/` se calculan con fórmulas cerradas o métodos distintos del implementado (`tools/gen_cases.py`). Nunca se copia la salida de Struktal como referencia.
2. **No inflar la cobertura.** Un requisito aproximado o sin comparar con SAP2000 se marca `partial` u `oracle_pending` en `spec/features.csv`.
3. **Sin rigideces artificiales** ni regularizaciones ocultas para que un cálculo «converja».
4. **Nada de `unsafe`** en el núcleo (`unsafe_code = "forbid"`). Errores tipados; nada de `unwrap()` sobre entradas del usuario.
5. **Cambios de esquema** con versión y migración (`struktal-core/src/validate.rs`).
6. **Decisiones** de formulación o arquitectura en `docs/decisions/NNNN-titulo.md`.
7. **Marcas.** No usar SAP, SAP2000, CSI, ETABS ni otras marcas en nombres de crates, binarios o módulos (ver `NOTICE`).

## Comprobaciones locales

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -q -p struktal-cli -- verify verification/cases/*/case.json
python tools/diff_s2k_import.py "verification/s2k/*.s2k"        # el importador contra el fichero
# Segunda opinion numerica (opcional; necesita ccx en PATH):
python tools/compare_engines.py --struktal target/debug/struktal --engine calculix=python V-SHELL-002
```

Al contribuir aceptas que tu aportación se distribuya bajo AGPL-3.0 (ver `LICENSE`).

## Publicar

Los seis crates llevan los metadatos completos (`repository`, `homepage`, `readme`, `keywords`, `categories` en `[workspace.package]`, heredados con `.workspace = true`), así que se pueden empaquetar tal cual.

1. `cargo package -p struktal-core --list` comprueba que el `README.md` entra en el paquete.
2. `cargo publish --dry-run -p struktal-core`, y después con cada crate en orden: `struktal-core`, `struktal-math`, `struktal-elements`, `struktal-analysis`, `struktal-io-s2k`, `struktal-cli`.
3. Etiqueta y sube: `git tag -a v0.1.0 -m "Struktal 0.1.0"` y `git push origin v0.1.0`.
4. La nota de la release sale del README y de `docs/compatibility-report.md`, que dice qué está verificado y qué no.

Antes del paso 2 hay que comprobar que el nombre `struktal` está libre en crates.io; si no lo está, hay que decidir el nombre del paquete y corregir `repository`/`homepage` con él.