# 0021 — Láminas de tres nudos y malla de análisis desde S2K (F03-001, F03-002, F03-003, F06-011, F19-002)

Fecha: 2026-11-06. Estado: aceptada.

## Decisión

**Esquema v6:** `Area.joints` admite 3 o 4 nudos. Un archivo v5 con cuatro nudos se migra sin cambios, y un archivo anterior con otro número se rechaza. En los resultados, `corners` tiene tantas entradas como nudos tiene el área.

**Elemento (`shell3.rs`):** membrana de deformación constante con el mismo taladro Hughes-Brezzi de la decisión 0019, y placa Reissner-Mindlin con el cortante asumido MITC3 (Lee y Bathe, 2004). Comparte con `shell4` el marco de 24 grados de libertad: el cuarto nudo repite el tercero, con rigidez, carga y masa cero. La masa va en tercios iguales.

**Lector S2K:** si el archivo trae `OBJECTS AND ELEMENTS - AREAS`, las áreas del modelo son los elementos de esa tabla, con el nombre que tienen allí. Los resultados son por elemento.

- Las cargas de un área objeto se repiten en cada uno de sus elementos.
- Los nudos internos de la malla se añaden libres desde `OBJECTS AND ELEMENTS - JOINTS`. Solo los que usa un área.
- Un objeto de más de 4 nudos solo se acepta con su malla.
- Una barra que el archivo corta en un nudo de la malla se conecta allí con `auto_mesh.at_joints`, según `OBJECTS AND ELEMENTS - FRAMES`. Sin esto, una viga pasaría junto al borde de su losa sin conectarse, y el resultado sería incorrecto sin ningún aviso.

**Se rechaza, con causa:** un objeto sin elementos, elementos que no cubren su objeto (1 %, porque se perderían cargas), un elemento orientado al revés, un nudo de malla que no está en la tabla o que tiene otro nombre como objeto, una barra con offsets conectada a la malla, un nudo fuera del eje de la barra (1e-6 de su longitud), y un modelo con barras y malla sin la tabla de barras.

Esto **lee** una malla hecha por otro programa. Struktal sigue sin mallar áreas por sí mismo (F03-008).

## Precio del triángulo

Una membrana de deformación constante es rígida en flexión en su plano. Se prefirió no enriquecerla por ahora: es simple y exacta en sus estados constantes. En muros hace falta una malla mucho más fina que con el elemento de 4 nudos: 120 × 20 × 2 elementos para quedar en 0,64 %, frente a 24 × 4 con 0,31 %. **Para muros conviene usar cuatro nudos siempre que se pueda.**

## Verificación

Referencias independientes en `tools/gen_cases.py`. Las tolerancias son las mismas de los elementos de 4 nudos; donde no se alcanzaban, se afinó la malla sin ampliarlas.

| Caso | Qué comprueba | Error |
|---|---|---|
| V-TRI-001 | tracción y flexión pura en malla distorsionada, tira mixta con 3 y 4 nudos, reparto de carga en planos horizontal e inclinado | exacto (tolerancia 1e-7) |
| V-TRI-002 | muro con cortante en punta, malla 120 × 20 × 2, viga de Timoshenko | −0,64 % |
| V-TRI-003 | placas apoyada y empotrada, cuarto de modelo, 24 × 24 × 2, Navier y Timoshenko-Woinowsky | −0,16 % y +0,11 % |
| V-TRI-004 | cubierta de Scordelis-Lo, 40 × 40 × 2, referencia 0,3024 | −0,73 % |
| V-TRI-005 | período de un triángulo sobre resortes: masa en tercios | exacto (tolerancia de modos) |
| V-MECH-010 | triángulo libre | nulidad 6 exacta |

La convergencia observada es de orden h²: cubierta −3,97 % con 16, −1,11 % con 32 y −0,73 % con 40; muro −3,75 % con 48 × 8, −1,00 % con 96 × 16 y −0,64 % con 120 × 20.

El lector tiene pruebas de cada rechazo, y `verification/s2k/meshed-strip.s2k`, escrito a mano, se importa y resuelve hasta la solución cerrada de una tira en tracción, en triángulos y cuadriláteros con nudos internos.

## Comparación con otros motores (decisión 0013)

Con OpenSeesPy 3.8 y su `ASDShellT3`. Pynite no tiene lámina de tres nudos: n/a.

- V-TRI-001: flexión y reparto de cargas coinciden. **En tracción hay DIFF, en los casos TEN y MIX:** con los mismos nudos, OpenSees se aparta de la solución cerrada en los desplazamientos y da momentos MZ en el apoyo. Struktal la reproduce, y por eso el resultado de referencia queda de su lado. La causa no se ha investigado; la sospecha es su término de taladro.
- V-TRI-002, 003 y 004: FORM-DIFF, de formulaciones distintas en mallas finitas. Frente a las referencias:

| Caso | Struktal | OpenSees `ASDShellT3` |
|---|---|---|
| V-TRI-002, muro con cortante | −0,64 % | −0,05 % |
| V-TRI-003, placa apoyada | −0,16 % | −0,09 % |
| V-TRI-003, placa empotrada | +0,11 % | +0,36 % |
| V-TRI-004, Scordelis-Lo | −0,73 % | −0,38 % |

**En muro y cubierta, a estas mallas, el triángulo de OpenSees es más preciso que el nuestro:** su membrana no es de deformación constante pura. Es una mejora posible para Struktal, a condición de no perder la exactitud de V-TRI-001, que `ASDShellT3` no tiene.

- V-TRI-005: n/a, porque los modos con láminas no se traducen.

## Límites

- **No hay ningún archivo real con una malla de áreas en esta máquina.** Los nombres de campos vienen de cómo los lee PAS, y una malla solo se ha probado con archivos escritos a mano. *Corrección del 2026-11-06:* aquí se decía que ningún S2K local tenía áreas, pero solo se habían mirado 12 de ellos. `ejemplo_lineal` tiene 2 áreas con su tabla de elementos, al parecer un elemento por objeto; se resuelve en una copia sin sus tablas de sismo (decisión 0023).
- Una barra cortada en la malla se corta también en cualquier otro nudo del modelo que esté sobre su eje. El archivo podría no hacerlo; es un caso raro y no se detecta.
- Los ejes locales de los elementos salen de su propio plano, sin contrastar con SAP2000.

## Pendiente

- Una membrana de tres nudos más precisa en flexión en su plano.
- Mallado propio de áreas (F03-008).
- Casos modales y de espectro desde S2K, y masa desde patrones: hechos en la decisión 0022.
- Contraste con SAP2000 cuando se revise la licencia.
