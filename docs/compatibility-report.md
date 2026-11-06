# Informe de compatibilidad — perfil sap2000-ultimate-27.1.0-target

Fecha: 2026-11-05. Motor struktal 0.1.0. **Estado del perfil: specified_not_verified.**

No se ha ejecutado SAP2000. Ningún requisito está `verified_sap`. No se afirma compatibilidad con SAP2000 en ningún contrato.

## Inventario (spec/features.csv, no congelado: P00 sigue abierto hasta disponer de la instalación)

Denominador: 124 requisitos atómicos, 20 familias (+1 en esta revisión: F08-007).

| Estado global | Requisitos |
|---|---|
| verified_independent | 29 |
| implemented | 2 |
| partial | 6 |
| specified | 85 |
| blocked | 2 |
| verified_sap | 0 |

Cobertura verificada independientemente: 29/124 (23 %). Cobertura contrastada con SAP2000: 0/124 (0 %). El inventario crecerá en P00; el porcentaje bajará cuando se añadan requisitos descubiertos, y eso es lo esperado.

## Lo que existe (subconjunto H0/H1 parcial)

- Pórtico 3D lineal con deformación por cortante (Timoshenko) cuando la sección tiene áreas de cortante, Euler–Bernoulli si son 0; liberaciones de extremo y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragmas, body y equal por eliminación exacta (decisión 0011); resortes de nudo y desplazamientos impuestos (decisión 0012): barras, apoyos, cargas nodales, uniformes, trapezoidales y parciales, puntuales, momentos en vano (decisión 0014) y peso propio; casos lineales y combinaciones de suma lineal.
- Backend por defecto `faer-sparse-ldlt` (decisión 0005); `skyline-ldlt` como referencia.
- Diagnóstico de mecanismo con vectores del espacio nulo verificados: nulidad exacta cuando hay hueco espectral, modos con DOF dominantes y nudos afectados (decisión 0006); rechazo explícito de clases de matriz no admitidas.
- CLI con manifiesto, hash de snapshot y estados por caso.
- Banco de 37 casos analíticos, todos pasan con ambos backends (rtol 1e-8).

## Discrepancias y riesgos conocidos frente a SAP2000

| Tema | Situación |
|---|---|
| Convención de signos de V2, V3, M2, M3 | Propia (decisión 0003); mapeo SAP pendiente de oráculo. |
| Ejes locales de barras verticales/casi verticales | Implementado según la ayuda [S06]; umbral sin confirmar. |
| Deformación por cortante | Implementada (decisión 0007), pero el área de cortante debe darse a mano en la sección general; SAP2000 la calcula de la forma del perfil. Hasta F02-002, un modelo sin As se comporta como Euler–Bernoulli. |
| Carga uniforme en barras inclinadas | Por longitud real; las opciones de proyección no están implementadas. |
| Malla analítica | Sin subdivisión automática; SAP2000 puede discretizar internamente. |
| Offsets | Solo vectores globales; sin puntos cardinales ni factor de zona rígida. Zonas rígidas sin peso: sin confirmar. |
| Restricciones | Un apoyo sobre una componente atada se rechaza; body sin selección de componentes; locales y weld no implementados. |
| Desplazamientos impuestos | Un valor sobre una componente sin apoyo ni resorte es un error en Struktal; se cree que SAP2000 lo ignora. Sin confirmar. |
| Resortes de nudo | Solo lineales y en ejes globales. |
| Intercambio S2K/OAPI | Nada implementado. |
| Recuento de modos de mecanismo | Exacto con hueco espectral; por encima de ~10⁵ ecuaciones puede quedar como cota inferior (F08-007, decisión 0006). Formato de aviso de SAP2000 no comparado. |

## Bloqueos

- `oracle_pending` en todos los casos: falta instalación de SAP2000 27.1.0 Ultimate (build exacto y manuales).
- F08-004: el backend actual no resuelve matrices indefinidas.
- F19-009: no hay ruta independiente documentada para leer `.sdb`.
