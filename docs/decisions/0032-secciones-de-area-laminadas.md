# 0032 — Secciones de área laminadas (A, B, D)

Fecha: 2026-11-16. Estado: aceptada.

## Decisión

Una sección de área puede declarar una pila de capas en lugar de un material:

```json
{ "name": "SH", "material": "STEEL", "thickness": 0.2,
  "layers": [ {"thickness": 0.01, "material": "SKIN"},
              {"thickness": 0.18, "material": "CORE"},
              {"thickness": 0.01, "material": "SKIN"} ] }
```

- `crates/struktal-core/src/laminate.rs` calcula la rigidez equivalente por
  unidad de ancho: `A` (extensional), `B` (acoplamiento flexión–tracción) y `D`
  (flexión). Cada capa aporta su `Q` de tensión plana, y se integra
  `A = Σ Q t`, `B = Σ Q (z₁² − z₀²)/2`, `D = Σ Q (z₁³ − z₀³)/3`, con `z` medido
  desde el **plano medio** del espesor total.
- `shell4.rs` y `shell3.rs` toman `A` y `D` cuando la sección es laminada
  (`ShellProps.laminate`): el elemento deja de pedir `E`, `ν` y `t`.
- Esquema v10: `area_sections[].layers`. Un fichero de esquema anterior que
  traiga `layers` se rechaza (`NewerField`), como el resto de campos nuevos.
- El espesor de la sección sigue siendo el total y las capas deben sumarlo.
  `bending_thickness` no aplica a una sección laminada: la rigidez de flexión
  está en las capas.

## Lo que se rechaza

- **`b ≠ 0`.** El elemento plano no tiene acoplamiento flexión–tracción. Una
  pila que lo necesita se rechaza al validar en lugar de resolverse como si
  fuera cero. Es una limitación declarada, no un fallo silencioso.
- Capas que no suman el espesor, espesor no positivo, material desconocido o
  `bending_thickness` junto a `layers`.

## Cómo entra el cortante

La rigidez de cortante transversal es `k ∫ G dz` con `k = 5/6`. Se lleva como un
`G` equivalente (media ponderada por espesor) multiplicado por el espesor total,
así que `g · shear_t` sigue siendo el valor que el elemento espera y no hace
falta un campo nuevo.

## Qué se verificó

- Unitarios del núcleo (`laminate.rs`): una capa reproduce la sección
  homogénea; una pila simétrica no acopla, y **tampoco una uniforme partida en
  capas** (B se mide desde el plano medio); una pila rígida en una cara sí
  acopla; la `A` de dos materiales es la suma; material desconocido y espesor no
  positivo son errores.
- Unitarios del elemento (`shell4`, `shell3`): con una pila piel–núcleo–piel de
  `E` distintas, las resultantes son `A ε` y `D κ`. Ninguna de las dos es
  `E t` ni `E t³/12` de una capa: el reparto entre membrana y flexión es lo que
  se comprueba.
- Caso `V-SHELL-006`: un elemento con desplazamientos impuestos de deformación
  constante y de curvatura anticlástica. Comprueba `F11`, `F22 = F12 = 0`, la
  reacción en el apoyo y `M11 = (1 − ν²) D₁₁ κ` con `M22 = M12 = 0`.
- 193 tests, clippy `-D warnings`, `fmt --check` y 67/67 casos con los dos
  backends.

## Lo que falta

- El acoplamiento `b ≠ 0`: pediría un elemento con membrana y placa acopladas.
- Una placa laminada bajo carga uniforme contra una malla: la lámina plana no
  puede sostener una curvatura constante exacta con un solo elemento (el `w`
  bilineal deja algo de cortante), así que la comparación sería de convergencia,
  no exacta.

## Consecuencias

- F03-004 pasa a `partial` en `spec/features.csv`.
- `laminate_abc` es una función pura y testeada: genera los valores esperados sin
  depender del elemento.
- El resto de secciones de área no cambia: sin `layers`, `ShellProps.laminate`
  es `None` y el camino de siempre se usa tal cual.