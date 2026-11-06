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
```

Al contribuir aceptas que tu aportación se distribuya bajo GPL-3.0 (ver `LICENSE`).
