# 0004 — Formato de casos de verificación y comparador

Fecha: 2026-10-29. Estado: aceptada.

- Cada caso vive en `verification/cases/<ID>/` con `model.json` y `case.json`.
- `case.json` registra `model_sha256` del archivo de modelo exacto. Si el modelo cambia (sección, unidades, cualquier byte), `verify` devuelve `stale_reference` y falla. `.gitattributes` fija LF para que el hash no cambie en Windows.
- Los valores esperados se calculan en `tools/gen_cases.py` con fórmulas cerradas independientes del motor. Prohibido pegar salidas del motor como referencia.
- Tolerancia por magnitud: |v − ref| ≤ atol(unidad) + rtol·|ref|, fijada antes de evaluar (`linear-closed-form-v1`: rtol 1e-8). Además, todo caso completo debe cumplir residuo ≤ 1e-10 y error de equilibrio global ≤ 1e-9.
- El equilibrio se comprueba contra la resultante calculada desde la definición de cargas, no desde el vector nodal equivalente.
- `oracle.sap2000 = oracle_pending` en todos los casos actuales. Ningún caso puede pasar a `verified_sap` sin captura en `verification/oracle/`.
