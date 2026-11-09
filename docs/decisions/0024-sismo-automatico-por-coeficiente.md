# 0024 — Carga sísmica automática por coeficiente (F06-010, F19-002)

Fecha: 2026-11-09. Estado: aceptada.

## Por qué

Era lo único que impedía importar `ejemplo_lineal` y, con las combinaciones automáticas, `ejemplo_combos_lineal`. Los tres ejemplos locales usan el mismo método: coeficiente de usuario en X o en Y, K = 1, alturas automáticas, excentricidad del 5 o 10 % y ningún diafragma.

## Decisión

**Esquema v8:** un patrón puede tener `auto_seismic`, con `direction` (X o Y), `coefficient` (C) y `exponent` (k). Un archivo anterior a v8 con este campo se rechaza, y los anteriores sin él se migran sin cambios.

**Cálculo**, para un factor 1 del patrón:

- W = g Σ m, con m la masa concentrada de cada nudo en la dirección de la carga, según la fuente de masa (decisiones 0020 y 0022). Incluye la de los nudos de la base.
- V = C W.
- F_i = V m_i h_iᵏ / Σ m h^k, con h medida desde el nudo más bajo del modelo. Un nudo a esa altura no recibe fuerza, ni siquiera con k = 0.
- Las fuerzas van a los nudos de la malla de análisis antes de reducir por restricciones, como cargas de nudo. Una fuerza sobre una componente restringida va a la reacción.
- El mensaje del caso da W y V.

**Errores:**

- Si no hay masa por encima de la base, el caso estático que usa el patrón termina en error y dice por qué; los demás casos siguen.
- Si la masa tiene un problema, como una carga neta hacia arriba en la fuente, también termina en error.
- Un patrón automático no puede formar parte de la fuente de masa: se calcularía desde sí mismo. La validación lo rechaza.

**Lector S2K:**

- Lee `AutoLoad="USER COEFF"` de `LOAD PATTERN DEFINITIONS`, con su fila de `AUTO SEISMIC - USER COEFFICIENT`.
- La fuente de masa se lee también cuando hay un patrón automático, aunque no haya caso modal.
- `AUTO SEISMIC LOADS TO JOINTS` se ignora: son las fuerzas que calculó el programa de origen, y aquí se calculan de nuevo.
- Se rechazan: otra dirección, otro tipo de carga automática, `UserZ=Yes`, y excentricidad sobre un modelo con diafragmas.
- **Sin diafragmas, la excentricidad se deja fuera con una nota.**
- `AUTO COMBINATION OPTION DATA 01 - GENERAL` solo se acepta con `AutoGen=No`, que no crea combinaciones.

## Sin confirmar frente a SAP2000 (oracle_pending)

Todo el método viene de cómo se recuerda la documentación, sin haberla podido consultar:

- Que W incluya la masa de los nudos de la base y de los restringidos.
- Que las alturas se midan desde el nudo más bajo cuando `UserZ=No`.
- Que la carga vaya a cada nudo con masa, y no a los pisos.
- Que la excentricidad no tenga efecto sin diafragmas. Es lo que se recuerda, y por eso se deja fuera con nota en lugar de rechazar el modelo. Si no fuera así, el resultado de esos modelos cambiaría.

Los archivos locales traen `WeightUsed`, `BaseShear` y las fuerzas por nudo calculadas por SAP2000, que servirían para contrastar este método. No se han comparado, porque los resultados de esos archivos no se comparan mientras no se revise la licencia.

## Verificación

Referencias independientes en `tools/gen_cases.py`:

| Caso | Qué comprueba |
|---|---|
| V-SEIS-001 | columna con masas concentradas, en X con k = 1 y en Y con k = 2 y factor 1,5: reacciones y deflexiones en tres niveles, en forma cerrada |
| V-SEIS-002 | masa desde una fuente con un patrón de carga, compartida entre los dos extremos de un brazo, y con la gravedad en el mismo caso |
| V-SEIS-003 | sin masa sobre la base: error con causa, y el otro caso se resuelve |

Además hay pruebas del lector, una por cada rechazo, y de la validación.

Comparación con otros motores: no aplica. Los traductores no calculan la carga automática, y esos casos se marcan como no comparados.

## Archivos locales

Ahora se importan 14 de los 27 S2K locales. `ejemplo_lineal` y `ejemplo_combos_lineal` se importan sin modificarlos y todos sus casos se resuelven, sin comparar resultados.

## Pendiente

- Excentricidad sobre diafragmas.
- Rango de alturas del usuario.
- Coeficientes de norma y viento.
- Contrastar el método con SAP2000 cuando se revise la licencia.
