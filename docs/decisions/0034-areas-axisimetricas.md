# 0034 — Áreas axisimétricas (2 GDL por nudo, revolución)

Fecha: 2026-11-17. Estado: aceptada.

## Decisión

Una sección de área puede declarar `"behavior": "Axisymmetric"`:

```json
{ "name": "AX", "material": "STEEL", "thickness": 0.1, "behavior": "Axisymmetric" }
```

El elemento (`crates/struktal-elements/src/axisym.rs`) es el cuadrilátero de
4 nudos del plano meridiano, revolucionado alrededor del eje **z** global:

- GDL por nudo: `u` (radial) y `w` (axial). Igual que un área plana, las otras
  cuatro componentes las sujeta el análisis (`constraints.rs::held_joints`,
  decisión 0033): la traslación normal a la sección y las tres rotaciones.
- Deformación: `εr = ∂u/∂r`, `εz = ∂w/∂z`, `γrz = ∂u/∂z + ∂w/∂r` y la deformación
  **circunferencial `εθ = u/r`**, que es la que lo hace axisimétrico.
- Todo integral lleva el peso `2π r` de la revolución, así que **`thickness` no
  significa nada** en una sección axisimétrica: la sección es la revolución
  entera. El campo sigue siendo obligatorio por el esquema y se ignora.

## Geometría

La sección tiene que estar en un plano que **contenga el eje z**, es decir
`x = 0` o `y = 0`, y todo su radio `r ≥ 0`:

| Sección en | Radio | Componentes sueltas |
|---|---|---|
| `y = 0` | `x` | `y` y las rotaciones |
| `x = 0` | `y` | `x` y las rotaciones |

- Un plano horizontal (`z = const`) no sirve: una revolución de él no tiene
  meridiano. Un plano que no pasa por el eje (`y = 5`) tampoco: `r` no sería
  una coordenada de la sección.
- Solo 4 nudos: el núcleo es el cuadrilátero. Un triángulo se rechaza.
- Las reglas de mezcla de la decisión 0033 siguen valiendo: no comparte nudos
  con barras ni con áreas de 3D, y dos áreas que comparten nudo tienen que
  estar en el mismo plano (aquí: en el mismo `x = 0` o `y = 0`).

## Ejes locales

No se usan los ejes geométricos de `local_axes` (el 3 es la normal al orden de
los nudos). Para un área axisimétrica los ejes del elemento son:

1 = radial **hacia fuera** (la dirección horizontal que varía, con `r ≥ 0`),
2 = `+z` (axial), 3 = 1 × 2.

Y las coordenadas del elemento son las **absolutas** `(r, z)`, no centradas en
el centro del área: `εθ = u/r` necesita el radio de verdad.

## Cargas

- **Peso propio**: `Gravity` está **dentro** del plano (es axial), así que un
  área axisimétrica sí lo lleva, al contrario que un área plana (decisión
  0033). Se integra como `unit_weight · ∫ Nᵢ 2π r dA` (fuerza por volumen).
- **Cargas de área**: son fuerzas por **unidad de volumen** de la revolución,
  integradas con `2π r`. Las direcciones en el plano (`Local1` radial,
  `Local2` axial, `GlobalX`/`GlobalY` según el plano, `GlobalZ` y `Gravity`)
  valen; las de fuera del plano se rechazan como en la decisión 0033.
- **Velocidad angular**: nueva dirección `LoadDirection::Centrifugal`, con
  `value = ω` en rad/s. La carga es la fuerza centrífuga por volumen
  `ρ ω² r` en radial, así que no es uniforme y tiene su propio camino en el
  ensamblaje. Pide `mass_density > 0` en el material; en una barra o en un
  área que no es axisimétrica se rechaza.
- **Presión de la pared** (el cilindro de Lamé): en el modelo **no hay carga
  de borde**, así que la presión entra como las resultantes nodales
  consistentes de la cara interior. Con una malla recta y funciones de forma
  lineales en el borde, `p · 2π r · L / 2` en cada extremo es exacto.

## Esquema v12

`AreaBehavior::Axisymmetric` y `LoadDirection::Centrifugal` son valores que un
lector antiguo no puede parsear, así que `SCHEMA_VERSION` pasa de 11 a 12 y un
fichero anterior que los traiga se rechaza con `NewerField`.

## Resultados

`AreaResult` de un área axisimétrica lleva las **tensiones**
`[σr, σz, τrz, σθ]` en las cuatro primeras ranuras y ceros en las demás: no
hay espesor, así que no son fuerzas por unidad de largo. Como los nombres de
`AREA_COMPONENTS` son los de la lámina (`F11, F22, F12, M11, …`), aparece
`AREA_AXISYM_COMPONENTS = ["S11", "S22", "S12", "S33", …]` y el comparador de
casos (`verify`) acepta los dos juegos.

## Resultantes globales

Las fuerzas radiales de un área axisimétrica son **generalizadas**, no
vectoriales: su GDL radial desplaza toda la circunferencia, y en el trabajo
interno entran `σr` y `σθ` a la vez contra la misma `δu`. Un brazo de momento
en el radio `r` tampoco representaría la distribución simétrica real de las
reacciones en tres dimensiones.

`verify` añade a **todo** caso lineal completo una fila de «equilibrio global
(relativa)» que compara `applied_resultant` contra `reaction_resultant`. Para
que no señale un falso fallo, los nudos de un área axisimétrica
(`areas::axisym_joints`) quedan **fuera de ambos lados**: cargas de nudo,
fuerzas sísmicas automáticas, cargas de área uniforme y centrífuga aplicadas,
y en la respuesta las reacciones con sus brazos y su escala (`gross`). Como
`Σ_e (K_e u)_e = 0` y ambos lados se anulan a la vez, el cheque se cumple de
verdad.

Las reacciones **por nudo** siguen reportándose enteras (con la componente
sujeta que responde: en el anillo de prueba, `∮ Nᵢ σz dA` de la cara inferior
y de la superior). Lo que queda en cero es la resultante global de un modelo
axisimétrico, que es exactamente lo que dice: no hay fuerzas vectoriales que
comparar.

## Verificado

- Núcleo (`axisym.rs`, T-002): *patch test* con `u = c r` exacto en un
  cuadrilátero distorsionado, `w = a z` que solo deforma `z`, simetría y
  energía positiva, el único modo rígido (traslación axial), volúmenes nodales
  contra `π (r₂² − r₁²) h`, la integral centrífuga `∫ Nᵢ 2π r² dA` y una
  sección que llega al eje con rigidez finita.
- Validación (T-003): los rechazos de la geometría (3 nudos, plano horizontal,
  plano que no pasa por el eje, radio negativo y orden horario en `(r, z)`), los
  de la dirección centrífuga (área sin sección axisimétrica, `mass_density = 0`,
  `ω < 0`) y `NewerField { found: 11 }` para `behavior: Axisymmetric` y para
  `direction: Centrifugal`.
- Sujeción (`constraints.rs`, T-003):
  `held_joints_hold_the_tangential_dof_of_an_axisymmetric_area`: la sección en
  `x = 0` sujeta `x` y las tres rotaciones de sus cuatro nudos, con 8 ecuaciones.
- Análisis (`linear_static.rs`, T-003):
  - `an_axisymmetric_ring_in_uniform_radial_stress_is_exact`: anillo
    `r ∈ [0.5, 1]`, `p = 1e6 Pa` en las caras interior y exterior como
    resultantes nodales consistentes, todos los nudos sujetos en `z`. Con
    `C = p (1 + ν)(1 − 2 ν) / E = 2.6e-6`, el campo `u = C r` está en el
    espacio bilineal: desplazamientos exactos (n = 4 ecuaciones), centro del
    área `[σr, σz, τrz, σθ] = [1e6, 6e5, ~0, 1e6]` con las ranuras de lámina en
    cero, residuo y equilibrio `< 1e-12`, reacciones axiales
    `−2e5π, −2.5e5π, +2.5e5π, +2e5π` (la integral `∮ Nᵢ σz dA` de cada cara, que
    se cancelan entre caras) y `e = 1e11` que duplica los desplazamientos sin
    tocar las tensiones.
  - `the_weight_of_an_axisymmetric_ring_is_carried_by_its_base`:
    `γ = 1000 N/m³` sobre los `0.75 π m³` de la revolución, sujeto solo en la
    base: la suma de reacciones axiales es `750 π = 2356.194490192345 N` exacta
    (la traslación axial es modo rígido, `1ᵀ K = 0`), solo la base la recibe,
    la cima baja y resultantes y equilibrio quedan en cero.
  - `the_centrifugal_load_is_the_radius_weighted_volume_integral`:
    `r ∈ [1, 2]`, `ρ ω² = 1`, ambos GDL sujetos (`n = 0`): reacciones
    `−11π/12` en `r = 1`, `−17π/12` en `r = 2`, total `−14π/3`, y todo lo demás
    exactamente en cero.

## Pendiente

- **Casos de verificación**: cubiertos por T-004 — `V-AXI-001` (cilindro de
  pared gruesa contra Lamé, con la presión de la bóveda repartida en sus dos
  nudos) y `V-AXI-002` (disco en rotación con `εz = 0` y la carga centrífuga).
  Ambos comparan contra el cerrado con `mesh-convergence-v1`, que gana `Pa`
  en `atol`: las tensiones axisimétricas se dan en pascales y esa unidad no
  estaba en ningún `atol`. En el disco las tensiones se miran en el primer y
  en el elemento central, porque `εr` cambia de signo en el borde libre y una
  malla de 8 no lo resuelve con error relativo. Ningún oráculo externo
  traduce estos casos: `tools/oracles/common.py` declara `unsupported` la
  sección axisimétrica y la carga centrífuga, y salen como `n/a`.
- **Triángulos**: el núcleo es de 4 nudos; un triángulo axisimétrico no
  existe todavía.
- **Carga de borde**: no hay; la presión de la pared entra por nudos.
- **Masa en dinámica**: como en la decisión 0033, las componentes sujetas se
  descartan del `assemble_mass`.
- **Importador S2K**: no mapea ningún `Type` a `Axisymmetric`.
