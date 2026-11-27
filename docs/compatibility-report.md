# Informe de compatibilidad — perfil sap2000-ultimate-27.1.0-target

Fecha: 2026-11-27. Motor struktal 0.1.0, licencia AGPL-3.0 (decisión 0026). **Estado del perfil: specified_not_verified.**

No se ha ejecutado SAP2000. Ningún requisito está `verified_sap`. No se afirma compatibilidad con SAP2000 en ningún contrato.

## Inventario (spec/features.csv, no congelado: P00 sigue abierto hasta disponer de la instalación)

Denominador: 125 requisitos atómicos, 20 familias (+1 en esta revisión: F01-014).

| Estado global | Requisitos |
|---|---|
| verified_independent | 36 |
| implemented | 3 |
| partial | 15 |
| specified | 69 |
| blocked | 2 |
| verified_sap | 0 |

Cobertura verificada independientemente: 36/125 (29 %). Cobertura contrastada con SAP2000: 0/125 (0 %). Los contrastes con archivos reales del usuario (abajo) no cambian ningún estado a `verified_sap`: son modelos privados, sin caso reproducible en el repo. El inventario crecerá en P00; el porcentaje bajará cuando se añadan requisitos descubiertos, y eso es lo esperado.

## Lo que existe (subconjunto H0/H1 parcial)

- Pórtico 3D lineal con deformación por cortante (Timoshenko) cuando la sección tiene áreas de cortante, Euler–Bernoulli si son 0; liberaciones de extremo y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragmas, body y equal por eliminación exacta (decisión 0011); resortes de nudo y desplazamientos impuestos (decisión 0012): barras, apoyos, cargas nodales, uniformes, trapezoidales y parciales, puntuales, momentos en vano (decisión 0014) y peso propio; casos lineales, combinaciones de suma lineal y envolventes con término gobernante (decisión 0016).
- Backend por defecto `faer-sparse-ldlt` (decisión 0005); `skyline-ldlt` como referencia.
- Diagnóstico de mecanismo con vectores del espacio nulo verificados: nulidad exacta cuando hay hueco espectral, modos con DOF dominantes y nudos afectados (decisión 0006); rechazo explícito de clases de matriz no admitidas.
- CLI con manifiesto, hash de snapshot y estados por caso.
- Láminas de 3 y 4 nudos para losas y muros, con taladro sin rigidez ficticia (decisiones 0019 y 0021).
- Masas concentradas, fuente de masa con elementos, masas añadidas y patrones, modos propios y espectro de respuesta con CQC y SRSS (decisiones 0020 y 0022).
- Carga sísmica automática por coeficiente de usuario: W con los nudos libres en la dirección y excentricidad accidental por diafragmas (decisiones 0024 y 0029).
- Barras «solo carga», sin rigidez ni masa, desde secciones S2K con modificadores de rigidez nulos (decisión 0027).
- Mecanismos que las liberaciones dejan con solo redondeo en la diagonal, detectados aparte del control de pivotes (decisión 0025).
- Banco de 70 casos, 69 analíticos y 1 contra valores publicados, todos pasan con ambos backends (rtol 1e-8, o la que fija cada caso).

## Discrepancias y riesgos conocidos frente a SAP2000

| Tema | Situación |
|---|---|
| Convención de signos de V2, V3, M2, M3 | Propia (decisión 0003); mapeo SAP pendiente de oráculo. |
| Ejes locales de barras verticales/casi verticales | Implementado según la ayuda [S06]; umbral sin confirmar. |
| Deformación por cortante | Implementada (decisión 0007). Con una forma, As es automática por reglas de ingeniería (decisión 0017), sin contrastar con los valores de SAP2000; una sección general sin As se comporta como Euler–Bernoulli. |
| Láminas | Sin mallado propio de áreas; MITC4 y MITC3 en lugar de una formulación delgada propia; el triángulo es rígido en flexión en su plano y pide mallas finas en muros (decisión 0021); taladro y membrana distintos de lo que pueda usar SAP2000; ejes locales y signos sin confirmar. |
| Dinámica | Masa concentrada; la masa desde patrones reparte por fuerzas de empotramiento, con g y direcciones sin confirmar (decisión 0022); sin masa faltante, combinación direccional ni excentricidad accidental; sin ajuste de la curva por amortiguamiento; sin recuento de Sturm. |
| Sismo automático | Solo coeficiente de usuario. En 4 edificios reales del usuario, las reacciones basales coinciden con SAP2000 en fuerza y momento, con excentricidad por diafragmas. La excentricidad se deja fuera cuando no hay diafragmas, y su sustitución manual se rechaza (decisión 0029). Sin normas. |
| Excentricidad en espectros | No implementada. SAP2000 suma un torsor estático por diafragma en valor absoluto. El tamaño encaja en Y, pero no en X, donde el ancho efectivo no se explica sin el manual (decisión 0029). |
| Láminas alabeadas | Se rechazan. Con el taladro G/1000, una viga torcida es un 30 % más flexible incluso con malla fina. Pendiente de decidir con PAS (decisión 0028). |
| Secciones paramétricas | J de pared delgada y As por reglas de ingeniería; sin acuerdos de perfiles laminados, sin T, canal, ángulo ni tablas. |
| Carga uniforme en barras inclinadas | Por longitud real; las opciones de proyección no están implementadas. |
| Malla analítica | Subdivisión de barras por número de tramos y en nudos (decisión 0015); sin longitud máxima, y la tolerancia frente a SAP2000 está sin confirmar. |
| Offsets | Solo vectores globales; sin puntos cardinales ni factor de zona rígida. Zonas rígidas sin peso: sin confirmar. |
| Restricciones | Un apoyo sobre una componente atada se rechaza; body sin selección de componentes; locales y weld no implementados. |
| Desplazamientos impuestos | Un valor sobre una componente sin apoyo ni resorte es un error en Struktal; se cree que SAP2000 lo ignora. Sin confirmar. |
| Resortes de nudo | Solo lineales y en ejes globales. |
| Intercambio S2K/OAPI | Lector S2K estricto de barras y áreas, con su malla de análisis, fuente de masa, modos, espectros y sismo por coeficiente (decisiones 0018, 0021, 0022 y 0024), contrastado con 4 edificios del usuario y 3 ejemplos de verificación de láminas (5 archivos) (decisiones 0027 y 0029); sin escritura ni OAPI. |
| Recuento de modos de mecanismo | Exacto con hueco espectral; por encima de ~10⁵ ecuaciones puede quedar como cota inferior (F08-007, decisión 0006). Formato de aviso de SAP2000 no comparado. |

## Bloqueos

- `oracle_pending` en todos los casos: falta instalación de SAP2000 27.1.0 Ultimate (build exacto y manuales).
- F08-004: el backend actual no resuelve matrices indefinidas.
- F19-009: no hay ruta independiente documentada para leer `.sdb`.
