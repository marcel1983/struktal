# 0014 — Cargas de vano lineales, parciales y momentos (F06-006, F06-007)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

Nuevos tipos en `frame_loads[].load`, con posiciones relativas a la longitud flexible:

- `Trapezoidal`: fuerza por unidad de longitud que varía linealmente entre `rel_start` y `rel_end`, de `value_start` a `value_end`, y es cero fuera de ese tramo.
- `PointMoment`: momento concentrado alrededor de `direction`, en `rel`.
- `TrapezoidalMoment`: momento por unidad de longitud, con variación lineal.

Una carga parcial uniforme es un `Trapezoidal` con los dos valores iguales.

Esquema v4. Como v3 ya está publicado, se sube la versión. Los archivos anteriores se migran sin cambios, y uno anterior a v4 que use estos tipos se rechaza.

## Formulación

- Las cargas nodales equivalentes siguen siendo −(reacciones de empotramiento exactas), por flexibilidad del voladizo. Un tramo lineal se integra con Gauss de 3 puntos, exacto hasta grado 5. Los integrandos son una intensidad lineal por una cúbica como máximo, así que el resultado es exacto, con o sin cortante. Por tanto, los desplazamientos nodales siguen siendo exactos.
- Un momento solo flexiona el tramo [0, a] y no interviene en el cortante.
- Las acciones en estaciones salen del equilibrio, con la misma integración hasta la sección. Una carga concentrada justo en la estación se excluye, como hasta ahora.
- Un momento alrededor de `Gravity` es un error de entrada.

## Cambio en la comprobación de equilibrio

Bajo solo momentos aplicados, todas las fuerzas son ruido de redondeo, y la comprobación las dividía entre sí (lo destapó V-MOM-001). Ahora fuerzas y momentos fijan la escala del otro a través del tamaño del modelo. Completa el cambio de la decisión 0012.

## Verificación

- Elemento: un tramo constante completo igual a la carga uniforme; un tramo cortado en dos da lo mismo; un tramo muy corto tiende a la carga puntual; momento puntual contra las pendientes de las funciones de Hermite; estática con y sin cortante; las acciones se anulan más allá de las cargas.
- V-TRAP-001: carga triangular en viga biapoyada y biempotrada, con resultados clásicos.
- V-TRAP-002 y V-TRAP-003: cargas parciales en voladizos, constante y lineal, gravedad triangular en una barra inclinada y carga lineal con cortante. La referencia integra por Simpson en `tools/gen_cases.py`, otro método distinto del implementado.
- V-MOM-001 y V-MOM-002: momentos concentrados alrededor de los tres ejes, momento repartido y torsor lineal.
- Comparación externa (decisión 0013): ver el resultado de `tools/compare_engines.py`. OpenSees no admite cargas lineales ni momentos de vano en `eleLoad`, y ningún motor comparable tiene momentos repartidos ni cargas parciales con cortante.

## Pendiente

- Cargas proyectadas (F06-009).
- Equivalencia con SAP2000 de nombres y signos de estas cargas: oracle_pending.
