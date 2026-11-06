## Qué cambia

<!-- Requisitos del inventario afectados (IDs de spec/features.csv). -->

## Verificación

- [ ] Caso(s) en `verification/cases/` con referencia **independiente** (no salida de Struktal)
- [ ] Tolerancias fijadas antes de evaluar; no se modificaron valores esperados para aprobar
- [ ] `cargo fmt`, `clippy -D warnings` y `cargo test --workspace` en verde
- [ ] `struktal verify` pasa con `faer` y `skyline`

## Documentación

- [ ] `spec/features.csv` actualizado (estado por contrato, sin inflar cobertura)
- [ ] Decisión en `docs/decisions/` si cambia formulación, esquema o convención
- [ ] `docs/agent-state.md` y `docs/compatibility-report.md` al día
- [ ] Discrepancias o supuestos sobre SAP2000 marcados `oracle_pending`
