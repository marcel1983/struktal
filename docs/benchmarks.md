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

## Comparación skyline / faer (2026-10-29, decisión 0005)

Mismo equipo. `python3 tools/bench.py DIR faer` y `BENCH_MAX_SIZES=4 python3 tools/bench.py DIR skyline`; presupuesto 4096 MiB. Tiempos por etapa tomados del manifiesto. «Entradas del factor» = perfil (skyline) o `len_val` (faer). Una ejecución por tamaño.

| Backend | Modelo | Ecuaciones | Entradas del factor | Factor MB | Ensamblaje s | Factorización s | 2 soluciones s | Total s | RSS pico MiB | Residuo máx. |
|---|---|---|---|---|---|---|---|---|---|---|
| skyline | 4×4×5 | 750 | 70 341 | 0,6 | 0,00 | 0,00 | 0,00 | 0,02 | 12 | 3,9e-13 |
| faer | 4×4×5 | 750 | 65 772 | 0,5 | 0,00 | 0,00 | 0,00 | 0,02 | 12 | 2,1e-13 |
| skyline | 8×8×10 | 4 860 | 1 469 070 | 11,9 | 0,03 | 0,24 | 0,01 | 0,35 | 34 | 3,0e-12 |
| faer | 8×8×10 | 4 860 | 1 103 472 | 8,8 | 0,03 | 0,03 | 0,00 | 0,13 | 34 | 1,0e-12 |
| skyline | 12×12×20 | 20 280 | 14 688 924 | 118,2 | 0,12 | 6,27 | 0,09 | 6,92 | 195 | 2,1e-11 |
| faer | 12×12×20 | 20 280 | 9 018 324 | 72,1 | 0,18 | 0,42 | 0,04 | 1,05 | 160 | 4,5e-12 |
| skyline | 16×16×30 | 52 020 | 67 358 502 | 540,5 | 0,38 | 51,98 | 0,41 | 53,83 | 719 | 7,6e-11 |
| faer | 16×16×30 | 52 020 | 37 084 428 | 296,7 | 0,43 | 2,41 | 0,11 | 3,83 | 511 | 1,0e-11 |
| faer | 20×20×40 | 105 840 | 121 310 316 | 970,5 | 1,00 | 13,52 | 0,46 | 16,19 | 1 379 | 1,9e-11 |
| faer | 24×24×60 | 225 000 | 360 148 212 | 2 881,2 | 4,07 | 60,49 | 1,09 | 68,43 | 3 681 | 4,4e-11 |

- skyline no se ejecutó en los dos mayores: necesitaría 1,7 GB y del orden de 10 min solo para 20×20×40.
- Desplazamientos skyline vs faer en 16×16×30: diferencia relativa máxima 1,6e-12 (DEAD) y 8,2e-11 (WIND).
- Diagnóstico de singularidad a escala: el mismo edificio 20×20×40 sin apoyos se clasifica como mecanismo en 11 s con faer.
- El ensamblaje (4 s a 225 000 ecuaciones) empieza a pesar: usa tripletas y ordenación global; pendiente el ensamblaje simbólico reutilizable.

## Diagnóstico de mecanismo a escala (2026-10-29, decisión 0006)

Edificios de `tools/gen_grid.py` con todos los apoyos retirados (nulidad real 6), presupuesto 4096 MiB. El tiempo es el total de `solve`: factorización estricta fallida, factorización regularizada y análisis del espacio nulo.

| Modelo | Ecuaciones | faer | skyline |
|---|---|---|---|
| 4×4×5 | 900 | 6 exacta, <0,1 s | 6 exacta, <0,1 s |
| 8×8×10 | 5 346 | 6 exacta, 0,2 s | 6 exacta, 0,8 s |
| 16×16×30 | 53 754 | 6 exacta, 7,1 s | 6 exacta, 121 s |
| 20×20×40 | 108 486 | ≥ 6, no certificada (hueco 4,3), 25,5 s | — |
