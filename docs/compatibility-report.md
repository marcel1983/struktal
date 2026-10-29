# Informe de compatibilidad — perfil sap2000-ultimate-27.1.0-target

Fecha: 2026-10-29. Motor rust-fem 0.1.0. **Estado del perfil: specified_not_verified.**

No se ha ejecutado SAP2000. Ningún requisito está `verified_sap`. No se afirma compatibilidad con SAP2000 en ningún contrato.

## Inventario (spec/features.csv, no congelado: P00 sigue abierto hasta disponer de la instalación)

Denominador: 123 requisitos atómicos iniciales, 20 familias.

| Estado global | Requisitos |
|---|---|
| verified_independent | 20 |
| implemented | 2 |
| partial | 6 |
| specified | 93 |
| blocked | 2 |
| verified_sap | 0 |

Cobertura verificada independientemente: 20/123 (16 %). Cobertura contrastada con SAP2000: 0/123 (0 %). El inventario crecerá en P00; el porcentaje bajará cuando se añadan requisitos descubiertos, y eso es lo esperado.

## Lo que existe (subconjunto H0/H1 parcial)

- Pórtico 3D lineal Euler–Bernoulli: barras, apoyos, cargas nodales, uniformes, puntuales y peso propio; casos lineales y combinaciones de suma lineal.
- Diagnóstico de mecanismo con DOF identificados; rechazo explícito de matrices no SPD.
- CLI con manifiesto, hash de snapshot y estados por caso.
- Banco de 10 casos analíticos, todos pasan (rtol 1e-8; errores observados ≤ 1,5e-15).

## Discrepancias y riesgos conocidos frente a SAP2000

| Tema | Situación |
|---|---|
| Convención de signos de V2, V3, M2, M3 | Propia (decisión 0003); mapeo SAP pendiente de oráculo. |
| Ejes locales de barras verticales/casi verticales | Implementado según la ayuda [S06]; umbral sin confirmar. |
| Deformación por cortante | SAP2000 la incluye por defecto con áreas de cortante; rust-fem v0.1 no. Los resultados de barras cortas diferirán. |
| Carga uniforme en barras inclinadas | Por longitud real; las opciones de proyección no están implementadas. |
| Malla analítica | Sin subdivisión automática; SAP2000 puede discretizar internamente. |
| Intercambio S2K/OAPI | Nada implementado. |

## Bloqueos

- `oracle_pending` en todos los casos: falta instalación de SAP2000 27.1.0 Ultimate (build exacto y manuales).
- F08-004: el backend actual no resuelve matrices indefinidas.
- F19-009: no hay ruta independiente documentada para leer `.sdb`.
