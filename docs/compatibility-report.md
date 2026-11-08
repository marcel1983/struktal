# Informe de compatibilidad — perfil sap2000-ultimate-27.1.0-target

Fecha: 2026-11-06. Motor struktal 0.1.0. **Estado del perfil: specified_not_verified.**

No se ha ejecutado SAP2000. Ningún requisito está `verified_sap`. No se afirma compatibilidad con SAP2000 en ningún contrato.

## Inventario (spec/features.csv, no congelado: P00 sigue abierto hasta disponer de la instalación)

Denominador: 125 requisitos atómicos, 20 familias (+1 en esta revisión: F01-013).

| Estado global | Requisitos |
|---|---|
| verified_independent | 36 |
| implemented | 3 |
| partial | 15 |
| specified | 69 |
| blocked | 2 |
| verified_sap | 0 |

Cobertura verificada independientemente: 36/125 (29 %). Cobertura contrastada con SAP2000: 0/125 (0 %); un primer contraste puntual con un archivo real queda en la decisión 0024. El inventario crecerá en P00; el porcentaje bajará cuando se añadan requisitos descubiertos, y eso es lo esperado.

## Lo que existe (subconjunto H0/H1 parcial)

- Pórtico 3D lineal con deformación por cortante (Timoshenko) cuando la sección tiene áreas de cortante, Euler–Bernoulli si son 0; liberaciones de extremo y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragmas, body y equal por eliminación exacta (decisión 0011); resortes de nudo y desplazamientos impuestos (decisión 0012): barras, apoyos, cargas nodales, uniformes, trapezoidales y parciales, puntuales, momentos en vano (decisión 0014) y peso propio; casos lineales, combinaciones de suma lineal y envolventes con término gobernante (decisión 0016).
- Backend por defecto `faer-sparse-ldlt` (decisión 0005); `skyline-ldlt` como referencia.
- Diagnóstico de mecanismo con vectores del espacio nulo verificados: nulidad exacta cuando hay hueco espectral, modos con DOF dominantes y nudos afectados (decisión 0006); rechazo explícito de clases de matriz no admitidas.
- CLI con manifiesto, hash de snapshot y estados por caso.
- Láminas de 3 y 4 nudos para losas y muros, con taladro sin rigidez ficticia (decisiones 0019 y 0021).
- Masas concentradas, fuente de masa con elementos, masas añadidas y patrones, modos propios y espectro de respuesta con CQC y SRSS (decisiones 0020 y 0022).
- Barras «solo carga» para cargas lineales sobre otros elementos (decisión 0024).
- Sismo automático por coeficiente de usuario, con las masas del análisis modal (decisión 0026).
- Banco de 65 casos, 64 analíticos y 1 contra valores publicados, todos pasan con ambos backends (rtol 1e-8, o la que fija cada caso).

## Discrepancias y riesgos conocidos frente a SAP2000

| Tema | Situación |
|---|---|
| Convención de signos de V2, V3, M2, M3 | Propia (decisión 0003); mapeo SAP pendiente de oráculo. |
| Ejes locales de barras verticales/casi verticales | Implementado según la ayuda [S06]; umbral sin confirmar. |
| Deformación por cortante | Implementada (decisión 0007). Con una forma, As es automática por reglas de ingeniería (decisión 0017), sin contrastar con los valores de SAP2000; una sección general sin As se comporta como Euler–Bernoulli. |
| Láminas | Sin mallado propio de áreas; MITC4 y MITC3 en lugar de una formulación delgada propia; el triángulo es rígido en flexión en su plano y pide mallas finas en muros (decisión 0021); taladro y membrana distintos de lo que pueda usar SAP2000; ejes locales y signos sin confirmar. |
| Dinámica | Masa concentrada; la masa desde patrones reparte por fuerzas de empotramiento, con g y direcciones sin confirmar (decisión 0022); sin masa faltante, combinación direccional ni excentricidad accidental; sin ajuste de la curva por amortiguamiento; sin recuento de Sturm. |
| Secciones paramétricas | J de pared delgada y As por reglas de ingeniería; sin acuerdos de perfiles laminados, sin T, canal, ángulo ni tablas. |
| Carga uniforme en barras inclinadas | Por longitud real; las opciones de proyección no están implementadas. |
| Malla analítica | Subdivisión de barras por número de tramos y en nudos (decisión 0015); sin longitud máxima, y la tolerancia frente a SAP2000 está sin confirmar. |
| Offsets | Solo vectores globales; sin puntos cardinales ni factor de zona rígida. Zonas rígidas sin peso: sin confirmar. |
| Restricciones | Un apoyo sobre una componente atada se rechaza; body sin selección de componentes; locales y weld no implementados. |
| Desplazamientos impuestos | Un valor sobre una componente sin apoyo ni resorte es un error en Struktal; se cree que SAP2000 lo ignora. Sin confirmar. |
| Resortes de nudo | Solo lineales y en ejes globales. |
| Intercambio S2K/OAPI | Lector S2K estricto de barras y áreas, con su malla de análisis, fuente de masa, modos y espectros (decisiones 0018, 0021 y 0022), probado con una malla real (placa gruesa de 576 láminas, 0,18 % frente a los desplazamientos de SAP2000, decisión 0024); primer edificio real (decisión 0026): peso, cortante basal y reacciones idénticos, periodos principales dentro del 0,8 %, desplazamientos entre −0,5 % y −4,7 %; espectro completo real sin probar; sin escritura ni OAPI. |
| Recuento de modos de mecanismo | Exacto con hueco espectral; por encima de ~10⁵ ecuaciones puede quedar como cota inferior (F08-007, decisión 0006). Formato de aviso de SAP2000 no comparado. |

## Bloqueos

- `oracle_pending` en todos los casos: falta instalación de SAP2000 27.1.0 Ultimate (build exacto y manuales).
- F08-004: el backend actual no resuelve matrices indefinidas.
- F19-009: no hay ruta independiente documentada para leer `.sdb`.
