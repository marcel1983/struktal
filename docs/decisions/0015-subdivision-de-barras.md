# 0015 — Subdivisión automática de barras (F01-006)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

`frames[].auto_mesh`: `segments` (divisiones iguales de la longitud flexible, 1 por defecto) y `at_joints` (cortar y conectar en cada nudo situado sobre el eje flexible). Esquema v5; los archivos anteriores se migran sin subdivisión, y uno anterior que la use se rechaza.

## Formulación

`struktal-analysis/src/mesh.rs` crea el modelo de análisis antes de ensamblar:

- La barra se sustituye por tramos colineales, con la misma sección y el mismo ángulo. Los offsets y las liberaciones se quedan solo en los extremos exteriores.
- Las cargas se cortan: uniforme en todos los tramos, trapezoidal por solape con los valores interpolados, y concentrada en el tramo que la contiene. Una concentrada justo en un corte va al tramo anterior.
- Cada tramo tiene fuerzas de empotramiento exactas. **Subdividir por sí solo no cambia los resultados**, y `subdivision_changes_nothing_but_the_output_points` lo comprueba con 4 modelos. Sirve para conectar barras en nudos intermedios y para tener puntos de salida.

## Resultados

- Una barra del modelo sigue siendo un solo objeto: una entrada, con su longitud, sus fuerzas de extremo y sus estaciones. Cada estación sale del tramo que la contiene, y una justo en un corte pertenece al tramo anterior.
- Los nudos generados, `F~1`, `F~2`..., aparecen con `generated: true` y dan la deformada dentro de la barra.
- `manifest.analysis_mesh` registra la procedencia: barra, tramos y nudos intermedios.
- `equations` cuenta el modelo de análisis.

## Reglas

- **Tolerancia sobre el eje:** 1e-6 de la longitud. No se mueve ningún nudo para que encaje. Un nudo más lejos, pero a menos de 1e-3 de la longitud, no se conecta y se avisa en el manifiesto, porque suele ser un error de modelado. **El valor que aplica SAP2000: oracle_pending.**
- Con `at_joints`, la regla de nudo sin conectar se aplica sobre el modelo de análisis: un nudo que sigue suelto produce un error en todos los casos, con la causa.
- Un nombre generado que coincide con uno del usuario es un error, no se renombra.

## Verificación

- `mesh.rs`: cortes, prioridad de los nudos, tramo de cada estación, cargas cortadas, aviso de nudo cercano y choque de nombres.
- V-MESH-001: viga continua de dos vanos con una sola barra, deformada en nudos generados, offsets y liberaciones con carga triangular, y carga sobre un corte.
- V-MESH-002: dos barras que se cruzan y quedan conectadas por el corte. Coincide con Pynite (decisión 0013), que también parte sus barras en los nudos.

## Pendiente

- Relleno por longitud máxima y subdivisión de áreas.
- Equivalencia con la auto-malla de SAP2000: oracle_pending.
