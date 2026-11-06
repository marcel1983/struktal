# Instrucciones para agentes

- Lee `docs/agent-state.md` antes de empezar: estado, decisiones pendientes y siguiente paso.
- Sigue `CONTRIBUTING.md`. En especial: referencias independientes, nada de inflar cobertura, decisiones documentadas.
- Antes de dar algo por terminado: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` y `struktal verify` con `--backend faer` y `--backend skyline`.
- Actualiza `spec/features.csv`, `docs/compatibility-report.md` y `docs/agent-state.md` en el mismo cambio.
- Documentación y mensajes al usuario en español; código y comentarios en inglés.
- Nunca uses marcas de terceros (SAP, CSI, ETABS…) en nombres de código.
