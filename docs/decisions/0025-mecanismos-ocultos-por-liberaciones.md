# 0025 — Mecanismos que las liberaciones ocultaban al control de pivotes (F04-003, F08-007)

Fecha: 2026-11-12. Estado: aceptada.

## El defecto

Se encontró al comparar con OpenSees (decisión 0013, ampliación del 2026-11-09).

En `V-REL-002`, el nudo C une dos barras articuladas y no tiene rigidez fuera de su plano. OpenSees da el sistema singular. Struktal lo resolvía como válido: con 1000 N en Y, UY ≈ 7·10¹³ m, con estado `complete` y `response_computed`. El caso pasaba solo porque su carga no tenía componente en Y.

**Causa.** Tras condensar las liberaciones, la diagonal de C UY era solo redondeo, del orden de 1e-11 N/m frente a 1e8 en el resto. Los pivotes se juzgan sobre la matriz escalada por su propia diagonal (decisión 0005), y el análisis del espacio nulo también (decisión 0006). Escalada por sí misma, esa diagonal vale 1 y parece un pivote sano.

Ninguno de los dos controles ve un grado de libertad cuya diagonal sea solo ruido. Sí ven un mecanismo que combina grados de libertad con rigidez real, porque ahí el pivote pequeño aparece tras la eliminación.

## Decisión

Antes de factorizar, la diagonal de cada ecuación reducida se compara con la que le dan sus elementos **antes** de condensar las liberaciones, más los resortes, llevada a las ecuaciones reducidas como la rigidez. Es una escala física y no depende de las unidades.

Si la diagonal no pasa de 1e-10 de esa referencia, las liberaciones dejaron ese grado de libertad sin rigidez:

- Todos los casos terminan en error con estado `mechanism`.
- El mensaje nombra los grados de libertad afectados, por ejemplo `joint C UY`.
- Las combinaciones quedan indeterminadas.

Solo se aplica si alguna barra tiene liberaciones, que es lo único que deja una diagonal reducida a redondeo.

**Precio.** Un resorte de liberación por debajo de 1e-10 de la rigidez de su barra se tomaría por un mecanismo. A esa escala ya no aporta rigidez real.

## Casos

- **`V-REL-002`:** era un modelo con un mecanismo oculto, no una cercha plana. Ahora restringe UY en C, con lo que es la cercha plana que su referencia describe. Sus valores esperados no cambian, porque la carga está en el plano.
- **`V-MECH-011`:** conserva el modelo anterior, con una carga en Y, y espera el mecanismo en `joint C UY`.

## Verificación

- Sin el cambio, `V-MECH-011` se resuelve sin aviso. Con él, termina en mecanismo con ambos backends.
- Los demás casos no cambian, y los dos ejemplos locales con diagonales articuladas se siguen resolviendo.
- `V-REL-002` coincide con OpenSees y Pynite. En `V-MECH-011`, OpenSees también se detiene y Pynite devuelve números sin aviso.

## Pendiente

Un mecanismo cuya diagonal sea ruido por otra causa que no sean las liberaciones. No se conoce ninguna, porque los demás elementos no condensan.
