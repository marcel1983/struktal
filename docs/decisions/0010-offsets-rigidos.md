# 0010 — Offsets rígidos de extremo (F04-004, parcial)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

`frames[].offsets`: `i`, `j`, vectores en ejes globales desde cada nudo hasta el extremo del elemento, en unidades de longitud y convertidos a SI en la frontera. Es el mecanismo general: un punto de inserción o una zona rígida se pueden expresar con él. No se implementan todavía los puntos cardinales ni el factor de zona rígida, por eso F04-004 queda parcial.

Esquema v3. Struktal ya es público, así que no se repite la excepción de la decisión 0008: se sube la versión. Los archivos v1 y v2 se migran sin offsets; uno anterior a v3 que traiga `offsets` o `constraints` se rechaza, en lugar de darle semántica v3 en silencio.

## Formulación

Con r el offset, la zona entre nudo y extremo es rígida: u_extremo = u_nudo + θ_nudo × r, θ_extremo = θ_nudo. Si R es esa matriz, T R sustituye a T en todas las transformaciones:

- rigidez en nudos: (T R)ᵀ K (T R);
- cargas en nudos: (T R)ᵀ f;
- desplazamientos locales de extremo: (T R) u.

## Consecuencias

- Longitud, ejes locales, cargas de vano y estaciones se refieren al tramo flexible, entre los dos extremos desplazados. `length` y `rel` de los resultados son los de ese tramo.
- Las liberaciones actúan en el extremo del elemento, después del brazo rígido.
- Las zonas rígidas no tienen peso: el peso propio se aplica solo en el tramo flexible. **Equivalencia con SAP2000 oracle_pending.**
- La validación rechaza offsets no finitos y extremos coincidentes, aunque los nudos no lo sean.

## Verificación

- Elemento: simetría y fuerzas nulas en los seis modos rígidos con offsets oblicuos; offset nulo igual a T.
- V-OFF-001: brazo rígido en punta, el mismo girado 45° en planta, y carga axial excéntrica. En este último, la barra es paralela a la línea de nudos a una distancia e: N = P, M = eP constante y momento de apoyo nulo.
- V-OFF-003: brazo rígido con deformación por cortante.
- V-OFF-002: tramo flexible biempotrado y biarticulado bajo carga uniforme. Los apoyos reciben el momento del elemento más R·a.
- La referencia se calcula en `tools/gen_cases.py` con el voladizo cerrado y la cinemática del brazo. Pasan con ambos backends.

## Procedencia

La matriz R coincide con `frame_offset_matrix` de PAS (proyecto anterior del mismo usuario). Es cinemática de cuerpo rígido; aquí se implementó como T R, de modo que no hace falta una rama aparte para cargas ni resultados. No se copiaron archivos de SAP2000 ni casos de PAS.

## Pendiente

- Puntos cardinales, centroide de sección y factor de zona rígida.
- Peso de las zonas rígidas, si el oráculo lo requiere.
