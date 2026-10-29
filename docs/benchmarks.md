# Mediciones de rendimiento

Equipo: contenedor Linux x86_64, 4 vCPU, 16 GB RAM (no es la PC Windows objetivo). Binario `--release`, 1 hilo, backend skyline-ldlt 0.1.0. Modelo: `tools/gen_grid.py NX NY PISOS` (vanos de 6 m, plantas de 3 m, columnas empotradas, 2 casos lineales). Tiempo = proceso completo (lectura, validación, ensamblaje, factorización, 2 soluciones, escritura JSON). Una sola ejecución por tamaño: sin mediana ni dispersión todavía.

| Modelo | Nudos | Barras | Ecuaciones | Perfil (entradas) | Factor | Residuo rel. | Error equilibrio | Tiempo | RSS pico |
|---|---|---|---|---|---|---|---|---|---|
| 4×4×5 | 150 | 325 | 750 | 70 341 | 0,6 MB | 3,9e-13 | 3,1e-14 | 0,02 s | 11 MiB |
| 8×8×10 | 891 | 2 250 | 4 860 | 1 469 070 | 11,9 MB | 3,0e-12 | 1,1e-12 | 0,39 s | 33 MiB |
| 12×12×20 | 3 549 | 9 620 | 20 280 | 14 688 924 | 118 MB | 2,1e-11 | 1,3e-11 | 7,75 s | 194 MiB |
| 16×16×30 | 8 959 | 24 990 | 52 020 | 67 358 502 | 540 MB | 7,6e-11 | 6,2e-11 | 56,3 s | 718 MiB |
| 20×20×40 | — | — | — | — | 1,71 GB requeridos | — | — | rechazado | — |

El último caso fue rechazado por el presupuesto de memoria (1 GiB por defecto) **antes** de reservar el factor, con estado `error / indeterminate` por caso.

Conclusiones:
- El perfil skyline crece aproximadamente como n^1,6 en estos edificios; es el cuello de botella. Ver decisión 0002.
- El residuo empeora con el tamaño (7,6e-11 a 52k ecuaciones); el umbral de verificación 1e-10 solo se aplica a los casos de `verification/`.
- Pendiente: medir etapas por separado (ensamblaje, ordenación, factorización, E/S), mediana de varias ejecuciones y la PC Windows de referencia.
