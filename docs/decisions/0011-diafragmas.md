# 0011 — Diafragmas, body y equal por eliminación exacta (F04-005, parcial)

Fecha: 2026-11-05. Estado: aceptada. Reduce el alcance de la decisión pendiente 1 de `agent-state.md`.

## Modelo

`constraints[]`, entrada `{"kind": "Diaphragm", "name", "axis", "joints"}`. `axis` es la normal del plano (X, Y o Z; Z por defecto). Un diafragma ata las dos traslaciones del plano y el giro alrededor de la normal. La traslación normal y los otros dos giros siguen independientes en cada nudo.

Otros tipos, con la misma eliminación:

- `{"kind": "Body", "name", "joints"}`: las seis componentes se mueven como un sólido rígido. No admite seleccionar componentes.
- `{"kind": "Equal", "name", "components", "joints"}`: las componentes globales elegidas son iguales en todos los nudos, sin brazo. Sirve también para nudos coincidentes.

Las restricciones locales y las de tipo weld siguen pendientes.

## Formulación

Eliminación exacta u = C x, con x los DOF independientes. Para un nudo s con referencia m, normal n y r = x_s − x_m:

- u_s = u_m + θ_m n × r en el plano;
- θ_s = θ_m alrededor de n.

El sistema reducido Cᵀ K C x = Cᵀ f sigue siendo simétrico y semidefinido positivo. Por tanto:

- no hace falta rigidez de penalización ni multiplicadores;
- sirven los backends SPD y el diagnóstico de mecanismos sin cambios;
- **los diafragmas no dependen del backend para matrices indefinidas** (F08-004).

La referencia es el primer nudo de la lista: un detalle interno. La prueba `constraint_results_do_not_depend_on_the_reference_joint` invierte el orden y compara.

## Ruido de redondeo en barras atadas

Una barra con sus extremos en la misma restricción rígida tiene, en teoría, filas exactamente nulas en Cᵀ K C. En coma flotante quedan restos de redondeo. Eso es peligroso: el escalado diagonal los hace parecer rigidez real. V-MECH-008 lo destapó: un sólido libre con 6 modos se informaba como «nulidad = 4 (exacta)». Con mala suerte, un mecanismo podría pasar como resuelto.

Por eso `DofMap::reduced_element` pone a 0 las entradas de la matriz reducida del elemento por debajo de su nivel de redondeo: |v| ≤ 1e-13 · Σ|cₐ c_b| · máx|k|. Solo quita ruido, no añade rigidez, y no se aplica a elementos sin DOF atados. Una rigidez real 13 órdenes por debajo de la mayor del mismo elemento también se anularía, pero ya sería indistinguible del ruido.

## Reglas de validación

- Al menos 2 nudos, todos existentes; un nudo no puede estar en dos restricciones.
- **Se rechaza un apoyo sobre una componente atada.** Un apoyo así sería una restricción lineal entre varios DOF de la referencia, que esta eliminación no resuelve. Se da un error explícito en lugar de ignorarlo. Los apoyos sobre componentes no atadas sí se admiten.
- Un nudo sin elementos es válido dentro de un diafragma, por ejemplo uno en el centro de masas. Sus componentes no atadas no tienen rigidez y deben restringirse: si no, se informa como mecanismo.

## Resultados

- Se devuelven desplazamientos de todos los nudos, atados o no. El equilibrio global se sigue comprobando, porque las fuerzas de restricción se anulan en conjunto.
- El diagnóstico de mecanismos describe los modos con los DOF independientes. `moves N/M joints` no cuenta los nudos que solo se mueven por estar atados.
- `equations` del manifiesto es el número de DOF independientes.

## Verificación

- `DofMap`: filas de C para las tres normales, body y equal; Cᵀ es la traspuesta de C; barra dentro de un body con ceros exactos.
- V-BODY-001: carga oblicua en un nudo atado rígidamente a la punta de un voladizo, con otro nudo solo para comprobar la cinemática.
- V-EQ-001: dos voladizos con UZ atado en nudos coincidentes: la carga se reparte según su rigidez y los giros siguen libres.
- V-MECH-008: barra oblicua dentro de un body sin apoyos: nulidad 6 exacta.
- V-DIA-001: piso rígido sobre cuatro columnas distintas con cargas excéntricas, referencia en un nudo sin elementos. Sistema de 3 DOF desde la energía de las columnas.
- V-DIA-002: edificio de cortante de dos plantas, con cortante y torsión por planta.
- V-DIA-003: la misma forma cerrada con normales X e Y.
- V-MECH-007: columnas articuladas en la base bajo un diafragma: nulidad 2 exacta.
- Referencias independientes del motor en `tools/gen_cases.py`. Pasan con ambos backends.

## PAS no sirve de referencia aquí

La ruta estática de PAS (`EqualConstraint` con unión de DOF) iguala UX, UY y RZ en todos los nudos, sin el término θ n × r. Eso solo es correcto si el piso no gira. Prueba en un piso con carga excéntrica de 40 kN (4 columnas cuadradas iguales de 3 m, planta irregular con nudos en (0,0), (5,0), (5,2) y (0,3) m, carga en X sobre T2):

| Nudo T2 | Exacto | Struktal | PAS |
|---|---|---|---|
| UX (m) | 2,3986e-3 | 2,3986e-3 | 2,2500e-3 |
| UY (m) | −4,9548e-4 | −4,9548e-4 | 0 |
| RZ (rad) | −1,9819e-4 | −1,9819e-4 | 0 |

La cinemática correcta sí aparece en PAS en la ruta dinámica no lineal (`rigid_slave`). Por eso se tomó esa fórmula, comprobada contra la forma cerrada, y no la ruta estática.

## Pendiente

- Restricciones locales y weld; selección de componentes en body.
- Un nudo en dos restricciones a la vez (se rechaza).
- Apoyos sobre componentes atadas: requieren elegir otro DOF dependiente o una reducción general.
- Equivalencia con SAP2000 (referencia, avisos, tratamiento de nudos fuera del plano): oracle_pending.
