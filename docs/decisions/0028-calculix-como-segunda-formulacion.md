# 0028 — CalculiX como segunda formulación de lámina

Fecha: 2026-11-15. Estado: aceptada.

## Decisión

Se añade `tools/oracles/calculix_runner.py` a `tools/compare_engines.py`, con
**alcance de láminas**: elementos S4 (cuadriláteros) y S3 (triángulos), materiales
elásticos, cargas de nudo (las de área llegan ya como fuerzas nodales desde
`common.py`) y restricciones en cualquier componente.

No se traducen barras. Una sección general (A, I22, I33, J) solo entra en ccx
con un elemento de usuario U1, y este runner no lo construye: una barra se
declara `Unsupported`, no se aproxima con un rectángulo que solo mediría la
aproximación.

Tampoco se reportan rotaciones: ccx escribe traslaciones y reacciones en
`.dat`, y guarda las rotaciones en `.frd` con numeración interna de nodos.
`compare_engines.py` ahora omite la componente que un motor no reporta, en vez
de puntuar un cero contra Struktal.

## Motivos

Struktal usa MITC4 y MITC3, y OpenSees —el otro motor externo— usa MITC4: la
coincidencia entre ambos no prueba nada si los dos comparten formulación. ccx
S4 es una lámina Mindlin de integración completa, con otra teoría y otra
implementación, así que coincidir es evidencia y discrepar es información.

## Verificación

Con ccx 2.23 y los casos del banco (rtol del caso, 2026-11-15):

| Caso | Resultado |
|---|---|
| V-SHELL-001 TEN | 86 valores, peor 0,18 de tolerancia |
| V-SHELL-002 LC1 | 59 valores, peor 0,00076 |
| V-SHELL-003 LC1 | 506 valores, peor 0,78 |
| V-TRI-001 MIX | 165 valores, peor 0,011 |

Dos diferencias quedan escritas en el docstring del runner y no se corrigen:

- **V-SHELL-001 BEN**: ccx es ~16 % más rígido en flexión pura con momentos
  aplicados en nudos. Decide la forma cerrada del caso, que Struktal cumple.
  Probar S4R (integración reducida) fue mucho peor (factor ~20).
- **V-TRI-001 Q**: las reacciones verticales de HA, HB y HC son 1200 N en
  Struktal y 0 en ccx, con los desplazamientos de acuerdo. ccx declara *degrees
  of freedom per node: 3* para láminas y expande los 42 nudos del modelo a 2218;
  la reacción de un nudo de lámina cargado y totalmente restringido no vuelve en
  el `.dat`. Esos tres nudos sostienen el triángulo HT y nada más, por eso el
  resto del modelo no se ve afectado. Es un límite de la lectura de ccx, no un
  resultado comparable.

  Las coincidencias de V-SHELL-002 y V-SHELL-003 no son triviales: 12 y 124
  nudos con desplazamiento no nulo, y 2 y 5 reacciones no nulas, todas dentro de
  tolerancia.

## Consecuencias

- `python tools/compare_engines.py --struktal <bin> --engine calculix=python <caso>`
  es una comprobación opcional: necesita `ccx` en PATH (o `CCX` apuntando al
  binario) y no corre en CI.
- El runner no es un sustituto de los oráculos analíticos: la forma cerrada del
  caso sigue decidiendo (decisión 0013).
- Si algún día se quiere cubrir barras, hace falta un elemento de usuario U1,
  que es una decisión aparte.