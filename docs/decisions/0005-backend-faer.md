# 0005 — Prueba de viabilidad de `faer` y backend por defecto

Fecha: 2026-10-29. Estado: aceptada. Sustituye la ruta por defecto de la decisión 0002; el skyline se conserva como referencia (`--backend skyline`) y como única ruta si se compila sin la feature `faer`.

## Qué se evaluó

`faer` 0.24.4 (versión fijada con `=`), sin features por defecto, solo `std` + `sparse-linalg`, ejecución secuencial (`Par::Seq`). Capacidades comprobadas leyendo el código fuente de la versión, no deducidas de los nombres:

| Capacidad | Resultado |
|---|---|
| Ordenación | AMD propia de faer. |
| Análisis simbólico separado | Sí; elige simplicial o supernodal; da el tamaño del factor **antes** de reservarlo (permite el presupuesto de memoria). |
| LLᵀ dispersa | Se detiene en el primer pivote no positivo (`NonPositivePivot { index }`); no da recuento ni inercia. **No se usa.** |
| LDLᵀ dispersa sin pivoteo + regularización dinámica | Con `signs = None`, un pivote con \|d\| ≤ ε se sustituye por ±δ y continúa. Con ε = 1e-10 y δ = 1e30 (centinela) sobre la matriz escalada, reproduce el salto de pivotes nulos del skyline y permite leer D: pivotes nulos, negativos e inercia. **Es la ruta adoptada.** Un factor con pivotes marcados nunca se usa para resolver. |
| LBLᵀ dispersa | Pivoteo solo dentro de cada supernodo (*intranode*): no es un solver indefinido general. No se usa; F08-004 sigue bloqueado. |
| Autovalores dispersos | No evaluado en esta prueba. |

## Resultados

- Batería de matrices: SPD conocida, mal escalada, singular, cadena libre, indefinida, clases no admitidas, presupuesto. Pasa igual que el skyline.
- Ruta supernodal y simplicial forzadas: la inercia de un Laplaciano 3D 12³ desplazado coincide con el conteo **analítico** de autovalores, y la nulidad del Laplaciano de grafo 10³ es exactamente 1 en ambas rutas. Esta prueba destapó un error mío al indexar los supernodos, ya corregido.
- Casos de verificación: 11/11 con ambos backends (rtol 1e-8).
- Coincidencia con skyline en el edificio de 52 020 ecuaciones: diferencia relativa máxima de desplazamientos 8,2e-11.
- Determinismo: dos ejecuciones dan resultados idénticos bit a bit (sin contar los tiempos).
- Rendimiento (docs/benchmarks.md): factorización 21× más rápida a 52 020 ecuaciones; resuelve 105 840 ecuaciones en 16 s y 225 000 en 68 s (3,7 GB).

## Hallazgo: el recuento de nulidad por pivotes no es fiable (afecta a ambos backends)

Con estructuras de varias barras sin apoyos (6 modos rígidos), los pivotes nulos se convierten por redondeo en negativos o quedan por encima de 1e-10. Recuentos observados: skyline 4+2 negativos, 3, 2 o 1 nulos según el tamaño; faer 3 nulos + 3 negativos. Los casos V-MECH de un solo elemento no lo detectaban. Antes de esta prueba el motor podía:

- clasificar un mecanismo como «no definida positiva / indeterminado»;
- dar un número de modos nulos incorrecto.

Corrección aplicada en `fem-analysis`: la K del análisis lineal de barras es semidefinida positiva por construcción (E, G, A, I, J > 0 validados), así que **todo pivote no positivo se clasifica como mecanismo** y el mensaje dice «nullity >= 1, exact count not determined». Los DOF señalados son indicativos y dependen del orden de eliminación. Se añadió V-MECH-003 (pórtico 3D libre de 54 barras) como regresión.

Riesgo residual: un modelo muy mal condicionado pero estable (p. ej. un elemento 1e10 veces más flexible que el resto) podría marcarse como mecanismo. Pendiente: búsqueda de vectores del espacio nulo con verificación ‖K z‖ (guía P02 paso 5) para dar una cota inferior verificada.

## Consecuencias

- Backend por defecto: `faer-sparse-ldlt`. `--backend skyline` sigue disponible y se prueba en CI (`cli.rs`).
- La distribución sigue siendo Rust puro; dependencia nueva fijada a `=0.24.4`. Actualizarla exige repetir esta prueba.
