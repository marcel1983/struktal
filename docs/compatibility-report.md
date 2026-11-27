# Informe de compatibilidad — perfil sap2000-ultimate-27.1.0-target

Fecha: 2026-11-15. Motor struktal 0.1.0, licencia AGPL-3.0 (decisión 0026). **Estado del perfil: specified_not_verified.**

Se ha ejecutado SAP2000 una vez, el 2026-11-15, para contrastar el **camino estático lineal** con tres modelos escritos a mano (11 casos, 276 valores, sin diferencias). Ningún requisito pasa a `verified_sap`: la campaña no cubre cada contrato, y no se afirma compatibilidad con SAP2000 más allá de lo que esa constancia dice.

## Inventario (spec/features.csv, no congelado: P00 sigue abierto hasta disponer de la instalación)

Denominador: 124 requisitos atómicos, 20 familias (+1 en esta revisión: F08-007).

| Estado global | Requisitos |
|---|---|
| verified_independent | 35 |
| implemented | 3 |
| partial | 17 |
| specified | 67 |
| blocked | 2 |
| verified_sap | 0 |

Cobertura verificada independientemente: 35/124 (28 %). Cobertura contrastada con SAP2000: **11 comparaciones de casos (276 valores) sobre 3 modelos**, todas dentro de tolerancia — la constancia está en `verification/oracle/2026-11-15-campana-sap2000.md`. No equivale a 11 requisitos verificados: cada comparación cubre una parte del camino estático, no el contrato completo.

El inventario crecerá en P00; el porcentaje bajará cuando se añadan requisitos descubiertos, y eso es lo esperado.

## Lo que la campaña del 2026-11-15 establece

Con SAP2000 27.1.0 Ultimate y los modelos `C01-signos`, `C02-ejes` y `C03-secciones` de `tools/gen_contrast_lot.py`:

- **Geometría y secciones.** Coordenadas, conectividad y propiedades de sección leídas del S2K coinciden con las del programa.
- **Cargas.** Nodales y distribuidas (uniformes, puntuales, trapezoidales) con sus resultantes.
- **Ejes locales.** Ángulo por barra (C02) y barras verticales, inclinadas y oblicuas.
- **Signos de `V2`, `V3`, `M2` y `M3`.** La pregunta que abría la decisión 0003: ningún componente salió con el signo cambiado.
- **Restricciones.** Los apoyos y las reacciones coinciden.

Lo que **no** establece: láminas, modos, espectros, combinaciones no lineales, temperatura, cargas proyectadas, mallas ni diseño. Y nada sobre el resto de los 124 requisitos.

## Lo que existe (subconjunto H0/H1 parcial)

- Pórtico 3D lineal con deformación por cortante (Timoshenko) cuando la sección tiene áreas de cortante, Euler–Bernoulli si son 0; liberaciones de extremo y fijación parcial (decisión 0008); offsets rígidos vectoriales (decisión 0010); diafragmas, body y equal por eliminación exacta (decisión 0011); resortes de nudo y desplazamientos impuestos (decisión 0012): barras, apoyos, cargas nodales, uniformes, trapezoidales y parciales, puntuales, momentos en vano (decisión 0014) y peso propio; casos lineales, combinaciones de suma lineal y envolventes con término gobernante (decisión 0016).
- Backend por defecto `faer-sparse-ldlt` (decisión 0005); `skyline-ldlt` como referencia.
- Diagnóstico de mecanismo con vectores del espacio nulo verificados: nulidad exacta cuando hay hueco espectral, modos con DOF dominantes y nudos afectados (decisión 0006); rechazo explícito de clases de matriz no admitidas.
- CLI con manifiesto, hash de snapshot y estados por caso.
- Láminas de 3 y 4 nudos para losas y muros, con taladro sin rigidez ficticia (decisiones 0019 y 0021).
- Masas concentradas, fuente de masa con elementos, masas añadidas y patrones, modos propios y espectro de respuesta con CQC y SRSS (decisiones 0020 y 0022).
- Carga sísmica automática por coeficiente de usuario, desde la masa del modelo (decisión 0024).
- Mecanismos que las liberaciones dejan con solo redondeo en la diagonal, detectados aparte del control de pivotes (decisión 0025).
- Mallado de áreas para el análisis (decisión 0031): un área pide `auto_mesh {m, n}` y el modelo sigue teniendo un objeto; el análisis lo sustituye por `m × n` sub-áreas (o `n²` triángulos). Los puntos de la malla se reutilizan si la posición ya existe, así que dos áreas vecinas comparten el borde, y un nudo nuevo en un borde hereda los GDL que ambos extremos restrinjan. La carga del objeto pasa a cada sub-área con la misma intensidad: la resultante se conserva exacta (medido: −2000 N con y sin malla).
- Secciones de área laminadas (decisión 0032): una sección declara `layers` y el elemento toma `A` y `D` por unidad de ancho en lugar de `E`, `ν` y `t`; el acoplamiento `b` no está soportado y una pila que lo necesite se rechaza al validar.
- Áreas planas y axisimétricas con 2 GDL por nudo (decisiones 0033 y 0034, esquema v12): `behavior: "Plane"` es tensión plana en un plano paralelo a uno global y `behavior: "Axisymmetric"` revuelve la sección meridiana alrededor del eje z con peso `2π r` (sin espesor). Las otras cuatro componentes de esos nudos las sujeta el análisis sin escribirlas en el modelo, y la validación rechaza lo que usarían ahí: plano inclinado, mezcla con elementos 3D, cargas o resortes fuera del plano, peso propio de un área plana horizontal, radio negativo, orden horario y centrífuga sin densidad. Los resultados salen como tensiones (`F11 = σ·t` en planas, `[σr, σz, τrz, σθ]` en axisimétricas) y sus fuerzas nodales generalizadas quedan fuera de las resultantes globales que compara el cheque de equilibrio. Tres casos de verificación los contrastan con su cerrado: franja a tracción (solución exacta), cilindro de pared gruesa contra Lamé y disco en rotación con los extremos sujetos. Todavía sin triángulos axisimétricos y sin mapeo en el importador S2K.
- Banco de 70 casos, 69 analíticos y 1 contra valores publicados, todos pasan con ambos backends (rtol 1e-8, o la que fija cada caso).

## Discrepancias y riesgos conocidos frente a SAP2000

| Tema | Situación |
|---|---|
| Convención de signos de V2, V3, M2, M3 | Propia (decisión 0003), **verificada en estático lineal** el 2026-11-15: 11 casos, 276 valores, sin diferencias. Resta el resto de casos y la convención de salida. |
| Ejes locales de barras verticales/casi verticales | Implementado según la ayuda [S06]; umbral sin confirmar. Contrastados en C02 para verticales, inclinadas y oblicuas, sin diferencias en los casos probados. |
| Deformación por cortante | Implementada (decisión 0007). Con una forma, As es automática por reglas de ingeniería (decisión 0017), sin contrastar con los valores de SAP2000; una sección general sin As se comporta como Euler–Bernoulli. |
| Láminas | Sin mallado propio de áreas; MITC4 y MITC3 en lugar de una formulación delgada propia; el triángulo es rígido en flexión en su plano y pide mallas finas en muros (decisión 0021); secciones laminadas con `A` y `D`, sin el acoplamiento `b` (decisión 0032); taladro y membrana distintos de lo que pueda usar SAP2000; ejes locales y signos sin confirmar. |
| Dinámica | Masa concentrada; la masa desde patrones reparte por fuerzas de empotramiento, con g y direcciones sin confirmar (decisión 0022); sin masa faltante, combinación direccional ni excentricidad accidental; sin ajuste de la curva por amortiguamiento; sin recuento de Sturm. |
| Sismo automático | Solo coeficiente de usuario. Peso, reparto con la altura y origen de las alturas recordados de la documentación, sin contrastar; la excentricidad se deja fuera cuando no hay diafragmas, y con diafragmas el patrón se rechaza (decisión 0024). |
| Secciones paramétricas | J de pared delgada y As por reglas de ingeniería; sin acuerdos de perfiles laminados, sin T, canal, ángulo ni tablas. C03 contrasta las propiedades de las formas que sí existen. |
| Carga uniforme en barras inclinadas | Por longitud real; las opciones de proyección no están implementadas. |
| Malla analítica | Subdivisión de barras por número de tramos y en nudos (decisión 0015); sin longitud máxima, y la tolerancia frente a SAP2000 está sin confirmar. |
| Offsets | Solo vectores globales; sin puntos cardinales ni factor de zona rígida. Zonas rígidas sin peso: sin confirmar. |
| Restricciones | Un apoyo sobre una componente atada se rechaza; body sin selección de componentes; locales y weld no implementados. |
| Desplazamientos impuestos | Un valor sobre una componente sin apoyo ni resorte es un error en Struktal; se cree que SAP2000 lo ignora. Sin confirmar. |
| Resortes de nudo | Solo lineales y en ejes globales. |
| Intercambio S2K/OAPI | Lector S2K estricto de barras y áreas, con su malla de análisis, fuente de masa, modos, espectros y sismo por coeficiente (decisiones 0018, 0021, 0022, 0024 y 0027): un caso que no puede representar es un problema y solo se deja fuera con `--allow-partial`, declarándolo; **contrastado con SAP2000 en estático lineal** (11 casos), sin probar con una malla o un espectro completos reales; sin escritura ni OAPI. |
| Recuento de modos de mecanismo | Exacto con hueco espectral; por encima de ~10⁵ ecuaciones puede quedar como cota inferior (F08-007, decisión 0006). Formato de aviso de SAP2000 no comparado. |

## Bloqueos

- `oracle_pending` en la mayoría de los casos: la campaña del 2026-11-15 cubre 11 casos estáticos de 3 modelos, no el resto.
- **Build exacto de SAP2000: no se define por ahora** (decisión del mantenedor, 2026-11-16). La campaña queda como evidencia no reproducible en su fecha; cuando se decida, basta con Help → About y actualizar la constancia.
- F08-004: el backend actual no resuelve matrices indefinidas.
- F19-009: no hay ruta independiente documentada para leer `.sdb`.

## Dónde están las capturas

Fuera del repositorio, en el equipo con la licencia. Aquí solo entra la constancia: `verification/oracle/`.