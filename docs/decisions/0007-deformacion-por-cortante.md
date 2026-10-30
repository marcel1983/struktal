# 0007 — Deformación por cortante (Timoshenko) en barras

Fecha: 2026-10-30. Estado: aceptada. Cierra la decisión pendiente 3 de `agent-state.md` y amplía la decisión 0003.

## Decisión

- La sección general gana `as2` (área de cortante para V2, flexión en el plano 1-2 con I33) y `as3` (para V3, plano 1-3 con I22), en unidades de área.
- **Por defecto se incluye la deformación por cortante cuando la sección tiene área de cortante.** `as = 0` la desprecia en ese plano: es el comportamiento Euler–Bernoulli. Según la documentación de SAP2000, un área de cortante nula desactiva la deformación por cortante; **pendiente de confirmar con oráculo**.
- No se inventa un área de cortante (p. ej. 5/6·A) cuando falta: la sección general no conoce su forma. Se calculará automáticamente cuando existan las secciones paramétricas (F02-002).
- Esquema del modelo v2. Los archivos v1 se migran con `as2 = as3 = 0`, conservando su significado. Un archivo v1 que traiga `as2`/`as3` se rechaza, en lugar de darle semántica v2 en silencio.

## Formulación

- Rigidez exacta de barra prismática de Timoshenko (Przemieniecki, *Theory of Matrix Structural Analysis*, 1968, §5.6), con Φ = 12 E I / (G As L²) por plano; con Φ = 0 se recupera Euler–Bernoulli. G = E/(2(1+ν)) salvo `g` explícito.
- Los giros nodales son giros de la sección (no pendientes).
- **Cargas nodales equivalentes = −(reacciones de empotramiento exactas)**, calculadas por flexibilidad del voladizo empotrado en I con el término de cortante. Sustituye las fórmulas de Hermite: con As = 0 las reproduce (prueba `euler_bernoulli_limit_matches_hermite_loads`), y con As > 0 son exactas para cargas uniformes y puntuales. Por tanto, los desplazamientos nodales siguen siendo exactos.
- Las acciones en estaciones siguen saliendo del equilibrio y no cambian.

## Verificación

- Elemento: la flexibilidad del voladizo obtenida invirtiendo K da L³/3EI + L/GAs, L²/2EI y L/EI en ambos planos; momento de empotramiento uniforme qL²/12 independiente de Φ; resultantes de cargas conservadas; modos rígidos sin fuerzas con As > 0.
- V-TIM-001: voladizos de 1 m y 10 m con As2 ≠ As3, cargas en ambos planos, y uno con As = 0 (exactamente Euler–Bernoulli).
- V-TIM-002: viga biempotrada corta (Φ ≈ 0,16) con carga puntual en vano y uniforme, en un elemento y en dos. La referencia es el método de las fuerzas con trabajos virtuales e integración de Simpson en `tools/gen_cases.py`, un método distinto del implementado. Errores relativos ≤ 5e-14. Aquí el cortante casi duplica la flecha bajo la carga (8,46e-5 m frente a 4,56e-5 m con Euler–Bernoulli) y reduce M_A de 21 367 a 20 800 N·m.
- Los 13 casos v1 previos pasan sin cambios a través de la migración.

## Pendiente

- Cálculo automático de As a partir de la forma (F02-002) y comparación con los valores que SAP2000 asigna a cada perfil (oráculo).
- Modificadores de propiedades (F02-004), que en SAP2000 también afectan a las áreas de cortante.
