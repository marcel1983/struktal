# 0022 — Fuente de masa y casos modales y de espectro desde S2K (F07-001, F09-001, F10-001, F19-002)

Fecha: 2026-11-06. Estado: aceptada.

## Decisión

**Esquema v7:** `mass_source` opcional, con `elements`, `added_masses` y `patterns` (patrón y factor). Sin ella, la masa sigue siendo la de los elementos y la añadida, como en la decisión 0020. Un archivo anterior a v7 con este campo se rechaza.

**Masa desde patrones:** en cada nudo, la fuerza vertical hacia abajo de los patrones, dividida por g = 9,80665 m/s², se suma a las tres traslaciones.

- Se usan las fuerzas nodales que ya calcula el caso estático, así que incluye cargas de nudo, de barra, de área y peso propio, y respeta subdivisiones, liberaciones y offsets.
- Las fuerzas horizontales y los momentos no dan masa. El mensaje del caso modal dice cuánta carga y cuánta masa salieron, y avisa si había fuerzas horizontales.
- Un nudo con carga neta hacia arriba tendría masa negativa: el caso modal y sus espectros terminan en error y dicen cuál es. Los casos estáticos siguen. No se aplica a un nudo sin traslaciones libres, donde la masa no tiene efecto.

**Lector S2K.** Se leen `MASS SOURCE`, `JOINT ADDED MASS ASSIGNMENTS`, `CASE - MODAL 1 - GENERAL`, `CASE - RESPONSE SPECTRUM 1` y `2`, y `FUNCTION - RESPONSE SPECTRUM - USER`.

- La fuente de masa usada es la marcada `IsDefault=Yes`, y solo se lee si hay un caso modal importado.
- Un espectro se escribe después de los modos aunque el archivo lo liste antes, porque los necesita ya resueltos.
- Si la fuente cuenta los elementos y además un patrón con peso propio, esa masa cuenta dos veces. Se respeta lo que dice el archivo, y una nota lo avisa.

**Un caso sin equivalente se deja fuera con una nota que da la causa**, y el resto del modelo se importa:

- Modos: Ritz, `EigenShift` o `EigenCutoff` distintos de cero, o una fuente de masa que no es la de por defecto.
- Espectro: combinación distinta de CQC o SRSS, más de una dirección, ejes no globales o con ángulo, movimiento distinto de aceleración, amortiguamiento no constante, `GMCf1`/`GMCf2` distintos de 1 y 0, excentricidad, amortiguamiento por modo, función que no es una tabla de usuario, o una función con amortiguamiento distinto del caso.
- Cualquier caso con `InitialCond` distinto de `Zero`, porque partiría del estado de otro caso. Hasta ahora un caso estático así se importaba sin esa rigidez, y era un error silencioso.
- Una combinación que usa un modo o un espectro: no tiene sentido aún (decisión 0020), y la validación la rechazaría.

**Corrección que estos casos hacían necesaria:** `MMod` y `MassMod` no se leían, y un `AMod` cambiaba la masa de la barra. Ahora la masa sigue a su modificador y no al del área, como ya hacía el peso.

Los resultados `JOINT VELOCITIES`, `JOINT ACCELERATIONS`, `ELEMENT STRAINS` y `RESPONSE SPECTRUM MODAL INFORMATION` y las funciones y tablas de casos de otros tipos ya no bloquean la importación: no cambian la estructura.

## Sin confirmar frente a SAP2000 (oracle_pending)

- Que la masa de patrones actúe igual en las tres direcciones, y el valor de g.
- El reparto entre nudos: aquí son fuerzas de empotramiento. La masa total se conserva siempre, pero con una carga no simétrica en una barra, como una puntual fuera del centro, el centro de masa puede diferir algo del que da un reparto por palanca.
- El tratamiento de la carga hacia arriba, que aquí es un error.
- **El amortiguamiento de la función:** se recuerda del manual, sin haberlo podido comprobar, que SAP2000 ajusta la curva cuando difiere del amortiguamiento del caso. Struktal no sabe hacer ese ajuste, y por eso deja el caso fuera en lugar de dar otro resultado.

## Verificación

Referencias independientes en `tools/gen_cases.py`, con la tolerancia de modos:

| Caso | Qué comprueba |
|---|---|
| V-MASS-001 | solo patrones: la densidad y la masa añadida del modelo quedan fuera, y las fuerzas horizontales y momentos no dan masa; tres períodos en forma cerrada |
| V-MASS-002 | en kip y pulgada: elementos, masa añadida, carga uniforme y peso propio |
| V-MASS-003 | carga de área en una losa sobre resortes, y un espectro sobre esa masa (`u = Sa/ω²`) |
| V-MASS-004 | carga hacia arriba: modos en error con su causa, y el caso estático correcto |

El lector tiene pruebas de cada caso que deja fuera. `verification/s2k/modal-spectrum.s2k`, escrito a mano, se importa y resuelve hasta la forma cerrada del período y del espectro.

Comparación con otros motores: no aplica, porque el traductor no pasa la fuente de masa. Los períodos con masas declaradas ya se compararon en la decisión 0020.

## Archivos locales

De los 17 S2K locales, leídos solo por su forma y sin comparar resultados, se importan 6 y se rechazan 11, cada uno con su causa. Ninguno de los 6 tiene un caso modal. El único con un caso de espectro, `Example 1-022`, se rechaza al validar: tiene secciones con I22 y J nulas. Además, su función tiene amortiguamiento 0 y el caso 0,05, y por eso ese espectro quedaría fuera. **Por eso el espectro desde S2K solo se ha resuelto con archivos escritos a mano.** Los nombres de campos vienen de cómo aparecen en estos archivos.

*Corrección del 2026-11-06:* aquí se decía lo mismo de los modos y la fuente de masa, y era engañoso. Los tres archivos de `ejemplo/` tienen caso modal y fuente de masa, pero estaban entre los rechazados. Dos de ellos se resuelven en copias sin sus tablas de sismo (decisión 0023), aunque esos resultados siguen sin compararse. Tampoco se habían contado los 10 archivos `.$2k`.

## Pendiente

- Combinación direccional (F10-002), masa faltante (F10-003), GMC y demás combinaciones.
- Ajuste de la curva por amortiguamiento, cuando se pueda confirmar qué hace SAP2000.
- Funciones de espectro de norma, y tablas leídas desde archivo.
- Sismo automático por coeficiente, que hoy bloquea modelos locales (`AUTO SEISMIC`).
