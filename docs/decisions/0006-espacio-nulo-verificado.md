# 0006 — Vectores del espacio nulo verificados (F08-007)

Fecha: 2026-10-29. Estado: aceptada. Completa la decisión 0005: el análisis ya no afirma solo «nulidad ≥ 1».

## Problema

Los recuentos de pivotes nulos o negativos no revelan el rango. En pórticos 3D sin apoyos (nulidad 6), ambos backends daban entre 1 y 6 pivotes marcados según el tamaño (decisión 0005).

## Método (`fem-math/src/nullspace.rs`)

Todo en coordenadas escaladas (diagonal unidad, K_s = S K S), para que traslaciones y giros sean comparables.

1. **Factorización PSD regularizada**, con ambos backends: todo pivote escalado ≤ 1e-8, nulo o negativo, se sustituye por 1e30. Equivale exactamente a factorizar K + Σ c_j e_j e_jᵀ sobre el conjunto marcado Z. Solo es válido porque K es semidefinida positiva por construcción; no debe usarse con matrices realmente indefinidas.
2. **Candidatos** por cada DOF marcado: K_reg z = e_j (Farhat y Géradin, 1998), seguido de dos pasos de refinamiento y ← y − K_fix⁻¹ (K y)_libre.
3. **Verificación** contra la K original: ‖K_s y‖ ≤ 1e-10 para y unitario. Después, Gram–Schmidt doble: solo cuentan los vectores independientes. Su número es la **cota inferior** verificada.
4. **Cota superior**. K_fix es K con los DOF de Z fijados; como todo vector nulo con componentes nulas en Z es nulo de K_fix, nulidad(K) ≤ |Z| + nulidad(K_fix). Se estiman los menores autovalores de K_fix por iteración inversa por bloques (8 vectores, ≤ 40 iteraciones, arranque determinista) con el factor regularizado y Rayleigh–Ritz con la K verdadera. Los vectores nulos que aparecen ahí se verifican y se suman: esto recupera direcciones nulas cuyos pivotes escaparon al marcado. La cota |Z| + encontrados solo se declara si la iteración converge y hay **hueco espectral**: λ_min(K_fix) ≥ 10 · máx(mayor residuo nulo aceptado, 100 · η · ‖K_s‖), con η el error regresivo medido de las soluciones.
5. Nulidad **exacta** solo cuando ambas cotas coinciden. Si no, el mensaje dice «nullity >= k (upper bound not certified: spectral gap ratio g)».

Coste: un análisis simbólico y numérico más, y unas 34 soluciones en los casos medidos. Límite de recursos: `max_vectors` = 64 candidatos; si se alcanza, solo se informa la cota inferior.

## Lo que la medición obligó a cambiar

- La primera versión basaba la cota superior en el tamaño del menor pivote no marcado (≥ 1e-6). **Dio un resultado falso**: skyline declaró «nulidad = 3 (exacta)» en el pórtico libre de 53 754 ecuaciones, cuya nulidad real es 6, porque tres pivotes nulos valían ~1,6e-6 por redondeo. Se sustituyó por el certificado del paso 4.
- Un margen absoluto para λ_min(K_fix) tampoco sirve: en estructuras grandes y flexibles es legítimamente pequeño (5e-11 a 108 486 ecuaciones). Se usa el hueco relativo.

## Resultados (pórticos libres, nulidad real 6)

| Ecuaciones | Backend | Marcados + iteración | Resultado | Hueco | Tiempo total |
|---|---|---|---|---|---|
| 900 | faer / skyline | 6+0 / 6+0 | 6 exacta | 1,6e6 / 1,3e4 | <0,1 s |
| 5 346 | faer / skyline | 6+0 / 4+2 | 6 exacta | 7,8e3 / 1,5e5 | 0,2 / 0,8 s |
| 53 754 | faer / skyline | 6+0 / 3+3 | 6 exacta | 285 / 9,5e5 | 7 / 121 s |
| 108 486 | faer | 6+0 | ≥ 6, no certificada | 4,3 | 26 s |

Casos de verificación con nulidad analítica, que pasan con ambos backends: V-MECH-001 (6), V-MECH-002 (1, torsión), V-MECH-003 (6, pórtico de 54 barras), V-MECH-004 (3, sujeto en traslación en un nudo) y V-MECH-005 (6, barra flotante: mecanismo local, «moves 2/4 joints»).

## Límite conocido

En doble precisión, el residuo con que las matrices de barra anulan los movimientos rígidos crece con el tamaño (~1e-13 a 900 ecuaciones, ~1e-11 a 108 000), mientras que λ_min(K_fix) decrece. Por encima de ~10⁵ ecuaciones el recuento puede no certificarse; el motor lo dice en lugar de afirmarlo. «Nulo» significa residuo escalado ≤ 1e-10: una dirección estable pero extremadamente flexible podría contarse como mecanismo.

## Salida

`manifest.stability`: cotas, nulidad, hueco, iteraciones, error regresivo y hasta 24 modos. Cada modo trae los DOF dominantes normalizados (traslación máxima = 1), los nudos que se mueven («k/N»: menos de N indica mecanismo local) y la proyección sobre los 6 movimientos rígidos globales. La base del espacio nulo no es única: los modos son una base válida, no «los» modos.
