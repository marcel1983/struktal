# 0030 — Frame3DD como oráculo de barras con sección general

Fecha: 2026-11-15. Estado: aceptada.

## Decisión

Se añade `tools/oracles/frame3dd_runner.py` a `tools/compare_engines.py`, con
**alcance de barras**: barras 3D con sección elástica general, cargas de nudo,
cargas distribuidas uniformes, puntuales y trapezoidales, y restricciones en
cualquier componente global.

El motivo es que cubre lo que CalculiX no puede: una sección general con
`A, Asy, Asz, Jx, Iy, Iz`. ccx solo la admite con un elemento de usuario (U1),
así que `frame3dd` es el único motor externo que mide las barras de Struktal tal
como están definidas.

No se traducen: láminas, liberaciones de extremo, offsets rígidos, fijación
parcial, resortes de nudo, restricciones generales, desplazamientos impuestos,
modos y espectro. Cada caso es `Unsupported`, porque una diferencia causada por
una traducción inventada se contaría contra el motor.

## Las dos convenciones

Frame3DD y Struktal usan el mismo esquema de ejes globales (X, Y, Z, con Z
vertical), pero **no** el mismo eje local de barra:

- Struktal: el eje 2 local es el Z global proyectado (y +X en barras verticales);
  el 3 es el 1 × 2.
- Frame3DD: el eje 2 local es el Y global proyectado, lo más cercano posible.

Su marco local es por tanto el de Struktal girado 90° alrededor del eje de la
barra. Eso se traduce en dos sitios:

1. **Cargas**: un vector `(a, b, c)` en ejes locales de Struktal se escribe como
   `(a, -c, b)` (`_local`).
2. **Sección**: su eje 2 es el 3 de Struktal y viceversa, así que el deck
  intercambia `I22` e `I33` y deja el `roll` igual que Struktal.

Poner `roll = angle + 90` parecía lo mismo y no lo era: el `roll` gira también
los ejes de carga, así que las dos correcciones se anulaban y las tablas
distribuidas volvían a aparecer permutadas.

## Verificación

Casos del banco, 2026-11-15, Frame3DD 20140514+ (`frame3dd` de 32 bits, GPL-3.0):

| Caso | Resultado |
|---|---|
| V-LIN-001 LC1 | ✅ match, 18 valores |
| V-LIN-002 LC1 | ✅ match, 18 valores |
| V-LIN-004 LC1 | ✅ match, 36 valores |
| V-LIN-005 LC1 | ✅ match, 18 valores |
| V-TRAP-001 LC1 | ✅ match, 66 valores |
| V-TRAP-002 LC1 | ✅ match, 54 valores |
| V-TRAP-003 LC1 | ✅ match, 18 valores |
| V-MESH-001/002, V-REL-001, V-MOM-002 | n/a: rechazado con su causa |

## Precisión

Frame3DD imprime **seis decimales**. Su redondeo es absoluto, no relativo, y en
un giro de `1e-4` eso son `5e-7`. Por eso `compare_engines.py` añade
`ENGINE_ATOL = {"frame3dd": 1e-6}` a la tolerancia relativa: sin ese término,
casos correctos fallaban por el formato de salida.

## Cobertura frente a los otros orÃ¡culos

Frame3DD no es el Ãºnico orÃ¡culo de barras: OpenSees cubre offsets rÃ­gidos
(V-OFF-001, 54 valores por caso), diafragmas (V-DIA-001, 81) y resortes de nudo
(V-SPR-001, 38). Lo que sigue sin segunda opiniÃ³n son las **liberaciones de
extremo** (V-REL-001), los **momentos en vano** (V-MOM-001/002) y la malla con
offsets (V-MESH-001): Frame3DD no los traduce y OpenSees tampoco.

## Consecuencias

- `python tools/compare_engines.py --struktal <bin> --engine frame3dd=python V-TRAP-001`
  es una comprobación opcional: necesita `FRAME3DD` en el entorno (o `frame3dd`
  en PATH) y no corre en CI.
- Las barras con liberaciones, offsets o resortes siguen sin segunda opinión.
- La corrección de las dos convenciones se apoya en los casos del banco; si
  aparece un caso nuevo con barras verticales o con `roll`, conviene añadirlo
  antes de dar por buena la traducción.