# 0031 — Mallado de áreas para el análisis

Fecha: 2026-11-16. Estado: aceptada.

## Decisión

Un área puede pedir su propio mallado con `auto_mesh: {m, n}` (esquema v9). El
modelo sigue teniendo **un objeto**; el análisis lo sustituye por sus elementos.

- **Cuadrilátero:** `m × n` sub-áreas sobre la parametrización bilineal del
  cuadrilátero, `1 → m` y `2 → n`.
- **Triángulo:** `n` divisiones por lado y `n²` sub-triángulos. `m` no se usa, y
  la validación lo rechaza si `m ≠ n`, porque una malla triangular conformante
  necesita el mismo número por lado.

Los puntos de la malla se insertan como nudos nuevos, salvo cuando su posición ya
existe: dos áreas vecinas comparten los nudos de su borde común, y una barra que
cruza el borde se conecta allí. Un nudo nuevo en el borde hereda **los grados de
libertad que ambos extremos de ese borde tienen restringidos**, así que mallar una
losa no la deja sin apoyo.

La carga del objeto pasa a cada sub-área con la misma intensidad: la resultante se
conserva exactamente.

## Mapeo de resultados

Cada sub-área es un elemento con su nombre (`<objeto>#<n>`), y `AreaParent` dice
qué elementos salieron de cada objeto. Los resultados son por elemento, como ya
ocurría con las áreas importadas de un S2K mallado (decisión 0021).

## Verificación

- Unitarios en `crates/struktal-analysis/src/mesh.rs`: `m × n` elementos y
  `(m+1)(n+1)` nudos; `n²` triángulos; dos áreas vecinas comparten el borde
  (15 nudos, no 18); la carga del objeto llega a cada elemento.
- Extremo a extremo (2026-11-16): placa 2 × 1 m, 0,1 m de espesor, tracción
  uniforme en X, borde empotrado. Con malla 2 × 2 y sin malla:
  - resultante `sum(FX) = -2000 N` en ambos (conservación exacta);
  - `ux` 9,489e-8 frente a 9,775e-8 (**2,9 %**, malla gruesa frente a malla fina).

## Un caso de verificación que no se cierra

Se intentó un *patch test*: placa 1 × 1 m con `u = (ε x, -ν ε y)` impuesto en
los cuatro vértices, malla 2 × 2 y el nodo central libre. Si el elemento reproduce
un estado de deformación constante, el centro debe caer en `(ε/2, -ν ε/2)`.

Cae en **0,633** del campo (`ux = 3,165e-4` frente a `5e-4`, y `uy` en la misma
proporción). Con el borde totalmente empotrado aparece una capa límite de
**taladro** que a malla gruesa no ha decaído en el centro. No es un efecto del
mallado —el campo impuesto es el mismo con y sin malla— sino del elemento de
lámina.

Por eso **no se añadió** como caso de `verification/cases/`: un caso que falla no
puede entrar en el banco. Queda como pregunta abierta, y F03-008 sigue `partial`.

## Límites

- Sin comparación contra SAP2000: el perfil no tiene oráculo para F03-008.
- No se mallan barras con offsets (ya lo hacía la decisión 0015).
- Las cargas de área son uniformes por objeto; un patrón por nudos sobre un área
  mallada no está traducido a los motores externos.