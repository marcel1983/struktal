# 0025 — Láminas alabeadas: investigación de la viga torcida y del taladro

Fecha: 2026-11-08. Estado: **investigación; sin cambio de código**. Decisión pendiente del usuario.

## Origen

Archivos del usuario: ejemplo de verificación de la viga torcida de MacNeal–Harder (90° de torsión en 12 in, ancho 1,1 in, t = 0,32 in, E = 29e6, ν = 0,22, malla 12×2, carga unitaria en la punta en el plano y fuera de él), en versiones gruesa y delgada. Valores publicados (MacNeal y Harder, 1985), la referencia independiente: **5,424e-3 in y 1,754e-3 in**. Los resultados de SAP2000 del propio archivo dan 5,402e-3 y 1,760e-3 en la versión gruesa.

Struktal los rechaza al validar: los 24 elementos están alabeados un 1,6 % de su diagonal, por encima del límite del 0,1 % (decisión 0019). El rechazo es correcto con la formulación actual, como muestra lo siguiente.

## Lo que se probó, en copias locales

1. **Corrección de alabeo con brazos rígidos** (proyección al plano medio; u₁′ = u₁ − hθ₂, u₂′ = u₂ + hθ₁, aplicada a rigidez, cargas y esfuerzos). Signo verificado: invertirlo da un resultado mucho más rígido. Resultado en 12×2: **+30,6 % / +26,6 %**, demasiado flexible.
2. **Tira recta con la misma malla:** −0,5 % y −0,6 % frente a la teoría de vigas. El elemento plano es correcto.
3. **Refinamiento** (generado por un script local): con corrección, 24×4 → 192×32 converge a 7,20e-3 (+33 %); sin corrección, 4,75e-3 → 7,14e-3, hacia el mismo límite. **La corrección es consistente: solo acelera la convergencia. El error no viene del alabeo.**
4. **Factor de taladro** (Hughes–Brezzi; Struktal usa G/1000, decisión 0019):

| Taladro | Viga torcida, en el plano / fuera del plano (12×2 y 48×8) | V-SHELL-002, muro en flexión pura (tolerancia 0,05 %) | V-TRI-004, Scordelis–Lo con triángulos (tolerancia 1 %) |
|---|---|---|---|
| G/1000 | +30,6 / +26,6 %; +32,3 / +26,1 % | 0,009 % ✓ | ✓ |
| G/100 | +2,8 / +2,7 %; +3,1 / +2,5 % | 0,087 % ✗ | 1,05 % ✗ |
| G/30 | +0,7 / +0,9 %; +0,9 / +0,8 % | 0,26 % ✗ | 1,08 % ✗ |
| G/10 | −0,1 / +0,1 %; +0,1 / +0,1 % | 0,86 % ✗ | 1,10 % ✗ |
| G | −0,5 / −2,0 %; −0,2 / −0,2 % | ~8 % (decisión 0019) | — |

## Conclusión

Con el taladro débil, **la lámina no converge a la respuesta correcta en superficies alabeadas**: queda un 30 % más flexible incluso con mallas finas. Con un taladro fuerte converge, pero el muro plano en flexión se rigidiza. Ningún valor único del factor cumple todas las tolerancias, que se fijaron antes de este estudio y no se cambian para que pase.

Afecta a superficies **alabeadas**: viga torcida, paraboloides, cáscaras de doble curvatura malladas con cuadriláteros no planos. No se observó en la cubierta cilíndrica de Scordelis–Lo con cuadriláteros planos (0,03 %), ni afecta a muros y losas planos, que es el caso habitual en edificios. Mientras no se resuelva, **se mantiene el rechazo** de áreas alabeadas por encima del 0,1 %: es la opción segura.

## Opciones

- **A. Mantener el rechazo** (estado actual). Seguro; no admite superficies alabeadas.
- **B. Membrana con giro de taladro propio** (tipo Allman o Ibrahimbegovic–Taylor–Wilson), que admite γ ≈ G sin rigidizar los muros, más la corrección de brazos rígidos del punto 1. Es la solución de fondo; es un cambio de formulación con su propia verificación (patch tests, V-SHELL-*, viga torcida con malla 12×2 al 3 %).
- **C. Factor intermedio** (G/30): viga torcida al 1 %, pero rompe V-SHELL-002 y V-TRI-004. Exigiría cambiar tolerancias fijadas, así que se descarta.
