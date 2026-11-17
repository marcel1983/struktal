# 0030 — Núcleo de lámina laminada (A, B, D)

Fecha: 2026-11-16. Estado: aceptada.

## Decisión

Se añade `crates/struktal-core/src/laminate.rs` con el cálculo de la rigidez
equivalente de una pila de capas, por unidad de ancho:

- `A` (extensional), `B` (acoplamiento flexión–tracción) y `D` (flexión).
- Cada capa aporta su `Q` de tensión plana, y se integra
  `A = Σ Q t`, `B = Σ Q (z₁² − z₀²)/2`, `D = Σ Q (z₁³ − z₀³)/3`,
  con `z` medido desde el **plano medio** del espesor total.

Es el paso previo a F03-004: el elemento de lámina necesita `A` y `D` en lugar
de `E`, `ν` y `t`, y calcularlos a partir de las capas es lo que hay que probar
antes de tocar `shell4.rs`/`shell3.rs`.

## Qué se verificó

Unitarios en `laminate.rs`:

- una sola capa reproduce la sección homogénea (`A = Q t`, `D = Q t³/12`);
- una pila simétrica no tiene acoplamiento, y **tampoco una pila uniforme
  partida en capas**: `B` se mide desde el plano medio, así que dividir un
  material no lo cambia;
- una pila con material rígido en una cara y blando en el núcleo **sí** acopla
  (`B ≠ 0`), y su versión simétrica no;
- la `A` de dos materiales es la suma capa a capa;
- material desconocido o espesor no positivo son errores.

## Lo que falta

- Que el elemento tome `A` y `D` (hoy toma `E`, `ν`, `t`): es el cambio en
  `shell4.rs` y `shell3.rs`.
- El caso `B ≠ 0`: el elemento MITC4 no tiene acoplamiento. Se rechazará con
  `Unsupported` hasta que se decida cómo tratarlo.
- El caso de verificación en `verification/cases/`.

## Consecuencias

- F03-004 sigue `specified` en `spec/features.csv` hasta que el elemento lo use.
- `laminate_abc` es una función pura y testeada: sirve también para generar los
  valores esperados del caso futuro, sin depender del elemento.