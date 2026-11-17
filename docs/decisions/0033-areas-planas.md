# 0033 — Áreas planas (tensión plana, 2 GDL por nudo)

Fecha: 2026-11-17. Estado: aceptada.

## Decisión

Una sección de área puede declarar `"behavior": "Plane"`:

```json
{ "name": "P", "material": "STEEL", "thickness": 0.1, "behavior": "Plane" }
```

- `crates/struktal-elements/src/plane.rs` implementa el elemento de tensión
  plana de 4 y 3 nudos (Q4/T3), con sus mismas funciones de forma y su misma
  regla de Gauss, y calcula `σ = D B u`.
- `crates/struktal-analysis/src/areas.rs` monta su matriz en el marco de 24
  GDL de todo elemento de área y la rota con `to_global`, de modo que un área
  de 2 GDL por nudo entra en el ensamblaje sin un camino aparte.
- `crates/struktal-analysis/src/constraints.rs::held_joints` añade a las
  restricciones del modelo las cuatro componentes que el elemento no tiene:
  la traslación normal al plano y las tres rotaciones. **No se escriben en el
  modelo**: son una consecuencia del análisis de un área plana, no una
  decisión de modelado del usuario. Validación ya ha comprobado que el plano
  es paralelo a un plano de coordenadas, así que el normal es una sola
  componente.

## Por qué 2 GDL y no 6

Con 6 GDL por nudo el elemento necesitaría rigidez de flexión y cortante, que
es el otro elemento (lámina). Tensión plana no tiene espesor en su
formulación: solo `E`, `ν` y el espesor como escala. Los 6 GDL con solo
rigidez de membrana dejarían cuatro ecuaciones singulares en cada nudo.

## Restricciones que vienen de ahí

| Restricción | Motivo |
| --- | --- |
| Su plano debe ser paralelo a un plano global (`plane_axis`) | El conjunto de componentes que se sujeta tiene que ser fijo: un plano inclinado necesitaría una restricción cinemática por nudo. |
| No comparte nudos con barras ni con áreas de 3D | El análisis sujeta sus cuatro componentes; eso sujeta también el elemento 3D que use ese nudo. |
| Dos áreas planas que comparten nudo deben estar en el mismo plano | Ese nudo se sujeta dos veces, y las dos sujeciones podrían discrepar. |
| Sin `bending_thickness` ni `layers` | Una sección plana no tiene flexión (decisión 0032). |
| Sin cargas de nudo, desplazamientos impuestos ni resortes en las componentes sujetas | Llegarían a un elemento sin rigidez ahí y se descartarían sin avisar. |
| Sin cargas de área fuera de su plano | Idem: `Local3`, el componente normal, `Gravity` o `GlobalZ` en un área horizontal; `GlobalY` en una pared. |
| Sin peso propio de un área horizontal | La gravedad sale de su plano. Un área vertical sí lleva su peso. |

Todas se rechazan en validación, con el mensaje `… out of the plane of a plane
area … (decision 0033)`.

## Resultados

`AreaResult` de un área plana lleva las **fuerzas de sección por unidad de
largo** `F11, F22, F12 = σ · t` en los ejes locales del área, para que
`F11` signifique lo mismo en lámina y en plano, y ceros en las cinco ranuras
de placa (`M11, M22, M12, V13, V23`), que el elemento no calcula.

## Esquema v11

`area_sections[].behavior: Plane` no lo puede leer un lector antiguo: fallaría
al parsear el valor. Por eso `SCHEMA_VERSION` pasa de 10 a 11 y un fichero de
esquema anterior que lo traiga se rechaza con `NewerField`, como el resto de
campos nuevos.

## Verificado

- Núcleo (`plane.rs`): 7 tests, contra la solución analítica de una placa
  2×1 con tracción en el borde (`σ = p`, `u = px/E`, `v = −νpy/E`), la de
  corte puro y la de un cuadrado biáxial, más simetrías y el tensor completo.
- Validación: 7 tests nuevos (`plane_axis`, plano inclinado, mezcla de
  elementos, cargas fuera de plano, peso propio, nudo/resorte fuera de plano,
  fichero anterior).
- Análisis: `held_joints` (2 tests, plano horizontal y pared) y un test
  extremo a extremo que resuelve la tira 2×1 con 4 ecuaciones: desplazamientos
  exactos, componentes sujetas a cero, `F11 = p t`, reacciones que equilibran
  y residuo < 1e-12.
- `cargo fmt`, `clippy -D warnings` y `cargo test --workspace` (210 tests);
  `struktal verify` en los 67 casos, con `faer` y con `skyline`.

## Pendiente

- **Casos de verificación**: ninguna sección F03 tiene aún su `V-*`; T-004 los
  añade.
- **Importador S2K**: el lector solo mapea `Shell-*`, `Membrane` y `Plate-*`;
  un `Type` distinto se reporta como problema. `Plane` no llega de `.s2k`.
- **Masa fuera de plano**: `assemble_mass` descarta las componentes sujetas,
  así que un área plana aporta solo la masa de sus dos direcciones. En
  dinámica hay que decidir si eso se documenta o se cambia.
- **Plano inclinado**: hoy se rechaza. Requeriría una restricción cinemática
  por nudo (los grados de libertad locales no son una mezcla fija de los
  globales).
