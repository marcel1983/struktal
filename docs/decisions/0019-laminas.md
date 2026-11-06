# 0019 — Láminas de cuatro nudos para losas y muros (F03-001, F03-002, F03-003, F06-011, F18-009)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

- `area_sections[]`: `material`, `thickness`, `bending_thickness` opcional y `behavior`: `Shell`, `Membrane` o `Plate`.
- `areas[]`: cuatro `joints`, en sentido antihorario visto desde el eje local 3.
- `area_loads[]`: fuerza uniforme por unidad de área, en una dirección. El peso propio de un patrón también actúa sobre las áreas.

Se añade al esquema v5, aún sin publicar. Triángulos y mallado de áreas quedan pendientes: hoy cada área es un elemento, plano y convexo. La validación rechaza un área alabeada más del 1e-3 de su diagonal, porque un elemento plano no la representa.

## Formulación (`struktal-elements/src/shell4.rs`)

- **Membrana:** bilineal con los modos incompatibles de Wilson-Taylor en forma QM6, condensados. Pasa el patch test en cuadriláteros distorsionados y flexiona en su plano sin cortante parásito, lo que importa en muros.
- **Placa:** Reissner-Mindlin con el cortante asumido MITC4 (Dvorkin y Bathe, 1984), sin bloqueo en el límite delgado. Cortante con k = 5/6.
- **Giro de taladro:** término de Hughes-Brezzi, que ata r3 al giro del campo de membrana, ω = ½(∂v/∂x − ∂u/∂y).

## Giro de taladro: la decisión

La opción habitual, y la de PAS, es sumar una rigidez ficticia pequeña a r3. Struktal la rechaza (decisión 0008), porque puede ocultar un mecanismo real. Con Hughes-Brezzi **el movimiento rígido no cuesta energía**: el elemento tiene exactamente sus 6 modos rígidos (rango 18 de 24, y V-MECH-009 da nulidad 6 exacta).

Precio: con un campo bilineal, el término rigidiza la flexión en el plano. Con γ = G, el valor habitual y el que usa OpenSees en `ShellMITC4`, un muro en flexión pura sale **un 8 % más rígido**. Por eso se usa γ = G/1000, y ese error baja a 0,009 %. **Es un error pequeño pero no nulo**, y V-SHELL-002 tiene su propia tolerancia por ello. Con γ tan pequeño, r3 se apoya poco en el elemento; no se ha visto un problema de condicionamiento en estos casos.

## Resultados

`areas[]` da en cada área F11, F22, F12, M11, M22, M12, V13 y V23 por unidad de longitud, en el centro y en las esquinas, en ejes locales. M11 positivo es tracción en la cara +3.

Ejes locales: 3 es la normal; 1 = Z × 3 es horizontal; 2 = 3 × 1 apunta hacia arriba; en un área horizontal, 1 = +X. **Convención de ejes y signos frente a SAP2000: oracle_pending.**

## Verificación

| Prueba | Referencia | Error |
|---|---|---|
| Elemento | 6 modos rígidos, rango 18, patch tests en cuadrilátero distorsionado | exacto |
| V-SHELL-001 | tracción y flexión pura en malla distorsionada, con curvatura anticlástica libre | exacto (tolerancia 1e-7) |
| V-SHELL-002 | muro en flexión pura, una fila de elementos | 0,009 % |
| V-SHELL-003 | muro con cortante en punta, malla 24 × 4, viga de Timoshenko | 0,31 % |
| V-SHELL-004 | placas apoyada y empotrada, malla 16 × 16, serie de Navier y Timoshenko-Woinowsky | 0,10 % y 0,18 % |
| V-SHELL-005 | cubierta de Scordelis-Lo, malla 16 × 16, referencia 0,3024 | 0,03 % |
| V-MECH-009 | lámina libre | nulidad 6 exacta |

Las tolerancias `mesh-convergence-v1` (1 %) y `drilling-penalty-v1` son errores de malla o de formulación, no redondeo, y se fijaron antes de evaluar. El 0,03 % de Scordelis-Lo está por debajo de lo que cabría esperar con esa malla, y puede deberse en parte a una compensación de errores.

## Comparación con otros motores (decisión 0013)

Error frente a la referencia de cada caso, en el punto de control:

| Caso | Struktal | OpenSees `ShellMITC4` | Pynite `Quad3D` |
|---|---|---|---|
| V-SHELL-002, muro en flexión pura | −0,01 % | −36,3 % | −32,6 % |
| V-SHELL-003, muro con cortante | −0,31 % | −3,75 % | −3,21 % |
| V-SHELL-004, placa apoyada | −0,10 % | −0,10 % | −0,00 % |
| V-SHELL-004, placa empotrada | +0,18 % | +0,18 % | +1,56 % |
| V-SHELL-005, Scordelis-Lo | −0,03 % | −1,37 % | −1,16 % |

- V-SHELL-001, de estados exactos, coincide con ambos en todos los valores.
- En placas, OpenSees y Struktal dan los mismos valores en toda la malla: las dos son MITC4.
- En muros, las membranas bilineales de los otros motores se bloquean en flexión, como se esperaba de esa formulación con una sola fila de elementos. No significa que esos motores estén mal: necesitan más malla.
- Como cada formulación tiene su error de malla, estas diferencias salen como `FORM-DIFF` y no cuentan como fallo. La referencia de cada caso es lo que decide.
- V-MECH-009: OpenSees devolvió números para una lámina libre, Pynite se detuvo y Struktal dio nulidad 6 exacta.

## Pendiente

- Mallado propio de áreas. Triángulos y lectura de una malla desde S2K: hechos en la decisión 0021.
- Presión en patrones de nudos, temperatura y cargas proyectadas.
- Masa de láminas (hito 3), P-Δ de láminas, modificadores, secciones por capas y ortotropía.
- Envolventes de esfuerzos de área: hoy se avisa de que no se incluyen.
