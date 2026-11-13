# 0026 — Licencia AGPL-3.0

Fecha: 2026-11-13. Estado: aceptada.

## Decisión

Struktal pasa de GPL-3.0-only a **AGPL-3.0-only**. Lo decide el usuario como autor único: los demás commits del historial son de dependabot y solo suben versiones de dependencias.

Cambian `LICENSE` (texto oficial de https://www.gnu.org/licenses/agpl-3.0.txt, 34 523 bytes, SHA-256 `0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0`), el campo `license` de `Cargo.toml`, el `README.md` y `CONTRIBUTING.md`.

## Motivos

- **Servicios en red.** La AGPL añade a la GPL su sección 13: quien ofrezca una versión modificada para usarla por red debe ofrecer su código fuente a los usuarios. Con la GPL, un servicio de cálculo basado en Struktal podía mantener cerrados sus cambios.
- **Otros motores AGPL.** Hay motores estructurales abiertos con AGPL-3.0, como stabileo y portico-core. Bajo GPL-3.0 su código no se podía incorporar; bajo AGPL-3.0 sí, conservando sus avisos de copyright y citando el origen.

## Consecuencias

- Usar Struktal en el propio equipo, o modificarlo sin distribuirlo ni ofrecerlo por red, no obliga a nada, igual que antes.
- Las versiones publicadas hasta `de2ee22` siguen disponibles bajo GPL-3.0 para quien las obtuvo así.
- Las dependencias (faer, serde, sha2, clap y las de prueba) son MIT o Apache-2.0, compatibles con AGPL-3.0.
- Algunas empresas evitan el código AGPL, así que Struktal será menos fácil de incorporar en productos cerrados. Es lo buscado.

## Lo que no cambia

- No se ha incorporado código de ningún otro motor. Incorporarlo, por ejemplo de stabileo, es una decisión aparte: exige revisar su licencia y sus avisos, citar el origen y validar lo portado con casos analíticos propios (`CONTRIBUTING.md`), como cualquier otro código.
- La regla de la decisión 0013 sigue: un motor externo es una segunda opinión, no la referencia.

No es asesoría legal: es la lectura habitual de las dos licencias.
