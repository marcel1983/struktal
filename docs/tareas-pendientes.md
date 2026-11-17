# Tareas pendientes

Fecha: 2026-11-17. Una línea por tarea, con id estable (`T-nnn`), requisito al
que sirve y criterio de cierre. Se marca `[x]` cuando la tarea cumple la puerta
de calidad del plan (`docs/plan-fases-restantes.md`): modelo, cálculo, test
propio, caso independiente y documentación.

Las tareas ya empezadas llevan su estado en la línea. Las cantidades de arriba
son de `spec/features.csv` a 2026-11-17.

| Familia | Requisitos sin verificar | Tareas |
|---|---|---|
| F03 elementos | 5 | T-001 … T-012 |
| F04 restricciones y links | 4 | T-013 … T-018 |
| F05 cables y tendones | 2 | T-019 … T-022 |
| F06 cargas | 5 | T-023 … T-030 |
| F07 amortiguamiento y masa | 2 | T-031 … T-034 |
| F08 análisis lineal | 3 | T-035 … T-038 |
| F09 Ritz | 1 | T-039 … T-040 |
| F10 sismo | 3 | T-041 … T-045 |
| F11 historia | 4 | T-046 … T-053 |
| F12 no lineal | 5 | T-054 … T-062 |
| F13 etapas | 3 | T-063 … T-066 |
| F14 estado estacionario | 5 | T-067 … T-072 |
| F15 móviles | 3 | T-073 … T-076 |
| F16 combinaciones | 3 | T-077 … T-080 |
| F17 diseño | 7 | T-081 … T-089 |
| F18 resultados | 4 | T-090 … T-095 |
| F19 intercambio | 7 | T-096 … T-103 |
| F20 aplicación | 4 | T-104 … T-107 |
| F01–F02 modelo | 11 | T-108 … T-118 |
| Transversal | — | T-119 … T-124 |

## F03 — Elementos

- [x] **T-001** F03-005b — Elemento plano en el ensamblaje: 2 GDL/nudo, `AreaBehavior::Plane` y `Axisymmetric`; ADR sobre los 4 GDL que quedan libres. *Criterio*: el modelo resuelve un caso plano y el resto de GDL no aparece en el sistema. *Depende de*: —
- [ ] **T-002** F03-005c — Núcleo axisimétrico de 4 nudos: peso `2πr`, `εθ = u/r`, 2 GDL/nudo. *Criterio*: *patch test* con `u = c r` y energía positiva. *Depende de*: —
- [ ] **T-003** F03-005c — Axisimétrico en el ensamblaje y cargas (presión, peso propio, velocidad angular). *Criterio*: cilindro de pared gruesa contra Lamé. *Depende de*: T-001, T-002
- [ ] **T-004** F03-005 — Casos de verificación de planos y axisimétricos (franja a tracción, Lamé, disco). *Criterio*: `verification/cases/` con referencia independiente y dos backends. *Depende de*: T-003
- [ ] **T-005** F03-006 — Núcleo de sólido de 8 nudos: `D` 3D isótropa, 2×2×2 Gauss. *Criterio*: *patch test* 3D y 6 modos rígidos. *Depende de*: —
- [ ] **T-006** F03-006 — Sólido en el ensamblaje: 3 GDL/nudo, malla de áreas con volumen, caras y cargas. *Criterio*: un bloque resuelve y las reacciones cierran. *Depende de*: T-005
- [ ] **T-007** F03-006 — Casos: tracción, cortante y Lamé en 3D. *Criterio*: `V-SOL-*` con dos backends. *Depende de*: T-006
- [ ] **T-008** F03-007 — Leyes no lineales de sólidos (bilineal, perfectamente plástica) con commit/revert. *Criterio*: bloque a compresión contra el resorte equivalente. *Depende de*: T-006, T-054
- [ ] **T-009** F03-004 — Acoplamiento `b ≠ 0` en láminas: membrana y placa acopladas. *Criterio*: ADR y caso con una pila asimétrica. *Depende de*: —
- [ ] **T-010** F03-004 — Placa laminada bajo carga con malla: caso de convergencia con `D` de capas. *Criterio*: caso con tolerancia de malla declarada. *Depende de*: —
- [ ] **T-011** F03-001/002 — Decidir si hace falta una formulación delgada (Kirchhoff) propia o basta MITC4/MITC3. *Criterio*: ADR con la comparación medida. *Depende de*: T-120
- [ ] **T-012** F03-001 — Triángulo: rigidez en flexión en su plano y mallas finas en muros. *Criterio*: diagnóstico medido y ADR (o nueva formulación). *Depende de*: —

## F04 — Restricciones, offsets y links

- [ ] **T-013** F04-004 — Puntos cardinales y zonas rígidas por factor. *Criterio*: reacción y desplazamiento de una viga con zona rígida contra la teoría. *Depende de*: —
- [ ] **T-014** F04-005 — Restricciones locales (por excentricidad y dirección). *Criterio*: caso con apoyo inclinado. *Depende de*: —
- [ ] **T-015** F04-005 — Weld (soldadura rígida entre nudos). *Criterio*: dos barras con weld y su equivalente. *Depende de*: —
- [ ] **T-016** F04-007 — Links lineales (rigidez axial, de cortante y rotacional). *Criterio*: dos nudos con link y su resorte equivalente. *Depende de*: —
- [ ] **T-017** F04-008 — Ejes locales de nudo. *Criterio*: cargas y resultados en los ejes del nudo. *Depende de*: —
- [ ] **T-018** F04 — Casos de verificación de todo lo anterior. *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-013 … T-017

## F05 — Cables y tendones

- [ ] **T-019** F05-001 — Núcleo y elemento de cable (geometría por tensión/flecha). *Criterio*: *patch test* y modos rígidos. *Depende de*: —
- [ ] **T-020** F05-001 — Cargas equivalentes del cable y caso (catenaria o parábola, declarando la aproximación). *Criterio*: reacción en los apoyos. *Depende de*: T-019
- [ ] **T-021** F05-002 — Tendones: geometría, anclajes y cargas equivalentes. *Criterio*: par de fuerzas en los anclajes. *Depende de*: —
- [ ] **T-022** F05 — Casos de cable y tendón. *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-020, T-021

## F06 — Cargas

- [ ] **T-023** F06-008 — Temperatura uniforme y gradiente en barras, con `α`. *Criterio*: deformación y fuerza axial libre/impedida. *Depende de*: —
- [ ] **T-024** F06-008 — Deformación impuesta (curvatura y axial). *Criterio*: momento y fuerza equivalentes. *Depende de*: T-023
- [ ] **T-025** F06-009 — Cargas proyectadas sobre el área (presión sobre una superficie inclinada). *Criterio*: resultante y momento contra la proyección. *Depende de*: —
- [ ] **T-026** F06-011 — Transferencia de cargas de área a barras (tracción y flexión locales). *Criterio*: reacciones de una losa sobre sus vigas. *Depende de*: —
- [ ] **T-027** F06-010 — Generador sísmico por norma (peso, altura, cortante basal, distribución). *Criterio*: caso con pesos y alturas conocidos. *Depende de*: T-041
- [ ] **T-028** F06-010 — Generador de viento por norma (presión, altura, gustos). *Criterio*: caso con presión publicada. *Depende de*: —
- [ ] **T-029** F06-012 — Oleaje y otras cargas del perfil (Morison). *Criterio*: fuerza sobre un cilindro contra la fórmula. *Depende de*: —
- [ ] **T-030** F06 — Casos de carga (temperatura, proyectadas, transferencia, oleaje). *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-023 … T-029

## F07 — Amortiguamiento y masa

- [ ] **T-031** F07-003 — Amortiguamiento modal (Zeta por modo). *Criterio*: caso con dos modos y amortiguamientos distintos. *Depende de*: —
- [ ] **T-032** F07-003 — Rayleigh (`α`, `β`) a partir de dos modos. *Criterio*: los dos modos elegidos quedan con el Zeta pedido. *Depende de*: —
- [ ] **T-033** F07-003 — Amortiguamiento por elemento (material y sección). *Criterio*: caso con amortiguamiento propio. *Depende de*: —
- [ ] **T-034** F07-002 — Masa consistente en barras (hoy sólo concentrada). *Criterio*: masa total y centro de masas exactos. *Depende de*: —

## F08 — Análisis lineal

- [ ] **T-035** F08-005 — P-Delta (segundo orden por geometría). *Criterio*: columna con carga axial contra la teoría. *Depende de*: —
- [ ] **T-036** F08-006 — Pandeo lineal (autovalores de carga). *Criterio*: pilar de Euler y marco con su factor. *Depende de*: T-035
- [ ] **T-037** F08 — Casos de P-Delta y pandeo. *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-036
- [ ] **T-038** F08-004 — Sistemas indefinidos y multiplicadores de Lagrange. *Criterio*: backend que resuelva indefinidos. *Depende de*: bloqueado por el backend actual

## F09 — Vectores Ritz

- [ ] **T-039** F09-002 — Vectores Ritz dependientes de carga. *Criterio*: los vectores reproducen la respuesta estática. *Depende de*: —
- [ ] **T-040** F09 — Caso de Ritz. *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-039

## F10 — Sismo

- [ ] **T-041** F10-002 — Combinación direccional (X, Y y diagonal) y escala de aceleración. *Criterio*: caso con las tres direcciones. *Depende de*: —
- [ ] **T-042** F10-003 — Masa faltante y corrección de signos. *Criterio*: caso con masa no representada. *Depende de*: —
- [ ] **T-043** F10-001 — Métodos de combinación modal que faltan (GMC y otros). *Criterio*: caso con dos modos y correlación conocida. *Depende de*: —
- [ ] **T-044** F10 — Casos de sismo (direccional, escala, masa faltante). *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-041, T-042
- [ ] **T-045** F10 — Contraste de signos y de la combinación espectral con SAP2000. *Criterio*: campaña con valores sin diferencias. *Depende de*: T-120

## F11 — Historia en el tiempo

- [ ] **T-046** F11-001 — Historia modal lineal. *Criterio*: oscilador de 1 GDL contra la solución cerrada. *Depende de*: —
- [ ] **T-047** F11-002 — Integración directa lineal (Newmark y HHT). *Criterio*: oscilador sin amortiguar y amortiguado. *Depende de*: T-046
- [ ] **T-048** F11 — Casos de historia lineal (1 y 2 GDL). *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-047
- [ ] **T-049** F11-004 — Aceleración de base y excitación multi-apoyo. *Criterio*: caso con dos apoyos y su diferencia. *Depende de*: T-047
- [ ] **T-050** F11-003 — Integración directa no lineal. *Criterio*: caso con material no lineal. *Depende de*: T-054
- [ ] **T-051** F11 — Persistencia de resultados de historia. *Criterio*: leer un caso largo por bloques. *Depende de*: T-092
- [ ] **T-052** F11 — Casos de aceleración de base y multi-apoyo. *Criterio*: `verification/cases/`. *Depende de*: T-049
- [ ] **T-053** F11 — Casos de historia no lineal. *Criterio*: `verification/cases/`. *Depende de*: T-050

## F12 — No lineal

- [ ] **T-054** F12-001 — Materiales uniaxiales (acero y hormigón) con commit/revert. *Criterio*: tracción, compresión y descarga contra el resorte equivalente. *Depende de*: —
- [ ] **T-055** F12-002 — Rótulas plásticas (momento y axial). *Criterio*: viga con rótula contra el mecanismo plástico. *Depende de*: T-054
- [ ] **T-056** F12-002 — Interacción P-M-M. *Criterio*: sección con su superficie de interacción. *Depende de*: T-055
- [ ] **T-057** F12-003 — Links no lineales: gap y hook. *Criterio*: ciclo de carga con apertura y cierre. *Depende de*: —
- [ ] **T-058** F12-003 — Fricción, aisladores y amortiguadores no lineales. *Criterio*: ciclo con histeresis conocida. *Depende de*: T-057
- [ ] **T-059** F12-004 — Grandes desplazamientos (corotacional). *Criterio*: columna con su pandeo aproximado. *Depende de*: T-035
- [ ] **T-060** F12-005 — Pushover con control de carga y de desplazamiento. *Criterio*: curva capacidad de un marco. *Depende de*: T-055
- [ ] **T-061** F12 — Casos de no linealidad (material, rótula, links). *Criterio*: `verification/cases/`. *Depende de*: T-054 … T-058
- [ ] **T-062** F12 — Casos de grandes desplazamientos y pushover. *Criterio*: `verification/cases/`. *Depende de*: T-059, T-060

## F13 — Etapas y tiempo

- [ ] **T-063** F13-001 — Construcción por etapas (activación de elementos y cargas). *Criterio*: pórtico por etapas contra el análisis lineal equivalente. *Depende de*: —
- [ ] **T-064** F13-002 — Fluencia, retracción y envejecimiento. *Criterio*: una viga con las tres deformaciones. *Depende de*: T-063
- [ ] **T-065** F13-003 — Pérdidas de pretensado (instantáneas y diferidas). *Criterio*: fuerza en el tendón tras las pérdidas. *Depende de*: T-021
- [ ] **T-066** F13 — Casos de etapas y tiempo. *Criterio*: `verification/cases/`. *Depende de*: T-064, T-065

## F14 — Estado estacionario

- [ ] **T-067** F14-001 — Respuesta en estado estacionario (sinusoidal). *Criterio*: oscilador de 1 GDL con la amplitud cerrada. *Depende de*: —
- [ ] **T-068** F14-002 — PSD (densidad espectral de respuesta). *Criterio*: caso con espectro de entrada conocido. *Depende de*: T-067
- [ ] **T-069** F14-003 — FNA (análisis no lineal rápido, FNA). *Criterio*: caso con un link no lineal. *Depende de*: T-058, T-067
- [ ] **T-070** F14-004 — Fuerza objetivo (target force). *Criterio*: fuerza hallada por iteración. *Depende de*: T-067
- [ ] **T-071** F14-005 — Casos hiperestáticos. *Criterio*: caso con redundancia conocida. *Depende de*: T-067
- [ ] **T-072** F14 — Casos de estado estacionario. *Criterio*: `verification/cases/`. *Depende de*: T-068 … T-071

## F15 — Cargas móviles

- [ ] **T-073** F15-001 — Carriles, vehículos y clases. *Criterio*: caso con un camión de peso conocido. *Depende de*: —
- [ ] **T-074** F15-002 — Líneas de influencia (Müller-Breslau). *Criterio*: ordenada y momento de una viga continua. *Depende de*: —
- [ ] **T-075** F15-002 — Búsqueda de extremos sobre la línea. *Criterio*: posición del máximo contra la ordenada. *Depende de*: T-074
- [ ] **T-076** F15 — Casos de móviles. *Criterio*: `verification/cases/`. *Depende de*: T-073 … T-075

## F16 — Combinaciones

- [ ] **T-077** F16-003 — SRSS y suma absoluta. *Criterio*: caso con dos modos. *Depende de*: —
- [ ] **T-078** F16-004 — Rango de resultados y combinaciones con casos no lineales. *Criterio*: caso con una combinación no lineal. *Depende de*: T-077
- [ ] **T-079** F16-005 — Combinaciones normativas automáticas. *Criterio*: caso con los coeficientes de la norma. *Depende de*: T-078
- [ ] **T-080** F16 — Casos de combinaciones. *Criterio*: `verification/cases/`. *Depende de*: T-077 … T-079

## F17 — Diseño

- [ ] **T-081** F17 — Inventario de normas y ediciones (qué se soporta y con qué criterio). *Criterio*: tabla en `docs/`. *Depende de*: —
- [ ] **T-082** F17-001 — Diseño de acero (capacidad y ratios). *Criterio*: ejemplo publicado de la norma. *Depende de*: T-081
- [ ] **T-083** F17-002 — Diseño de hormigón de barras. *Criterio*: ejemplo publicado. *Depende de*: T-081
- [ ] **T-084** F17-003 — Diseño de aluminio. *Criterio*: ejemplo publicado. *Depende de*: T-081
- [ ] **T-085** F17-004 — Conformado en frío. *Criterio*: ejemplo publicado. *Depende de*: T-081
- [ ] **T-086** F17-005 — Diseño de áreas y láminas de hormigón. *Criterio*: ejemplo publicado. *Depende de*: T-081
- [ ] **T-087** F17-006 — Selección automática de secciones con reanálisis. *Criterio*: caso que converge a la sección esperada. *Depende de*: T-082
- [ ] **T-088** F17-007 — Longitudes de pandeo y arriostramientos de diseño. *Criterio*: caso con longitud efectiva conocida. *Depende de*: T-036
- [ ] **T-089** F17 — Casos de diseño. *Criterio*: `verification/cases/` con ejemplos publicados. *Depende de*: T-082 … T-088

## F18 — Resultados

- [ ] **T-090** F18-007 — Tablas de resultados con nombres y formato. *Criterio*: tabla comparable con la salida de referencia. *Depende de*: —
- [ ] **T-091** F18-008 — Section cuts. *Criterio*: cortante y momento de un corte contra la suma de piezas. *Depende de*: —
- [ ] **T-092** F18-010 — Persistencia por bloques de historias largas. *Criterio*: leer y escribir un caso de muchos pasos. *Depende de*: —
- [ ] **T-093** F18-009 — Tensiones y resultantes en láminas y sólidos. *Criterio*: tensiones en un punto de una lámina y de un sólido. *Depende de*: T-005
- [ ] **T-094** F18-003 — Signos de P, V2, V3, T, M2, M3 frente a SAP2000. *Criterio*: campaña sin diferencias. *Depende de*: T-120
- [ ] **T-095** F18 — Casos de resultados (section cuts, tensiones, tablas). *Criterio*: `verification/cases/`. *Depende de*: T-090 … T-093

## F19 — Intercambio

- [ ] **T-096** F19-003 — Exportación S2K desde el modelo. *Criterio*: el archivo se reimporta y da el mismo modelo. *Depende de*: —
- [ ] **T-097** F19-003 — Round-trip ejecutado en SAP2000. *Criterio*: campaña con el build definido. *Depende de*: T-096, T-120
- [ ] **T-098** F19-004 — Puente OAPI en proceso Windows separado. *Criterio*: una llamada de ida y vuelta. *Depende de*: —
- [ ] **T-099** F19-005 — API Python (PyO3). *Criterio*: un caso resuelto desde Python. *Depende de*: —
- [ ] **T-100** F19-006 — Capa compatible con scripts OAPI existentes. *Criterio*: un script de ejemplo sin cambios. *Depende de*: T-098
- [ ] **T-101** F19-007 — IFC. *Criterio*: un modelo de ida y vuelta. *Depende de*: —
- [ ] **T-102** F19-008 — DXF. *Criterio*: un modelo de ida y vuelta. *Depende de*: —
- [ ] **T-103** F19-009 — Lectura directa de `.sdb`. *Criterio*: documentación de la ruta independiente. *Depende de*: bloqueado

## F20 — Aplicación

- [ ] **T-104** F20-002 — Edición gráfica y tabular con deshacer. *Criterio*: una operación que se deshace y rehace. *Depende de*: —
- [ ] **T-105** F20-003 — Proceso de cálculo aislado con cancelación. *Criterio*: un cálculo largo que se cancela sin dejar el proceso colgado. *Depende de*: —
- [ ] **T-106** F20-004 — Visor de resultados e informes. *Criterio*: informe con las tablas de T-090. *Depende de*: T-090
- [ ] **T-107** F20-005 — Instalador en PC limpia. *Criterio*: instalar y ejecutar un caso. *Depende de*: —

## F01–F02 — Modelo

- [ ] **T-108** F01-004 — Sistemas de coordenadas adicionales y rejillas. *Criterio*: caso con un sistema auxiliar. *Depende de*: —
- [ ] **T-109** F01-005 — Grupos y selecciones. *Criterio*: resultados por grupo. *Depende de*: —
- [ ] **T-110** F01-011 — Unidades de masa, tiempo y temperatura. *Criterio*: un modelo en otras unidades da el mismo resultado. *Depende de*: —
- [ ] **T-111** F02-004 — Modificadores de rigidez, masa y peso. *Criterio*: viga con y sin modificadores. *Depende de*: —
- [ ] **T-112** F02-005 — Secciones variables (no prismáticas). *Criterio*: viga cónica contra la teoría. *Depende de*: —
- [ ] **T-113** F02-006 — Secciones definidas por el usuario. *Criterio*: sección general con sus propiedades. *Depende de*: —
- [ ] **T-114** F02-009 — Ejes locales avanzados (vector de referencia y plano). *Criterio*: barra con eje local explícito. *Depende de*: —
- [ ] **T-115** F02-002 — Secciones que faltan (T, canal, ángulo, tablas). *Criterio*: propiedades de la sección contra el cálculo. *Depende de*: T-113
- [ ] **T-116** F02-010 — Biblioteca de materiales y propiedades dependientes de materiales. *Criterio*: un material de la biblioteca en un caso. *Depende de*: —
- [ ] **T-117** F02-007/008 — Ejes locales por defecto y ángulo frente a SAP2000. *Criterio*: campaña sin diferencias. *Depende de*: T-120
- [ ] **T-118** F02 — Casos de todo lo anterior. *Criterio*: `verification/cases/` con dos backends. *Depende de*: T-108 … T-117

## Transversal

- [ ] **T-119** F03-008 — Caso formal del mallado de áreas: un *patch test* que no dependa de la capa límite de taladro. *Criterio*: caso en el banco. *Depende de*: T-010
- [ ] **T-120** — Build exacto de SAP2000 (Help → About) y constancia. *Criterio*: decisión del mantenedor y `verification/oracle/`. *Depende de*: bloqueado por decisión
- [ ] **T-121** — Actualizar `docs/oracle-coverage.md` cada vez que un caso deja de ser `oracle_pending`. *Criterio*: matriz con la casilla pasada. *Depende de*: cada tarea
- [ ] **T-122** — Revisar `docs/compatibility-report.md` al cerrar cada familia. *Criterio*: la familia pasa de "parcial" a lo que toque. *Depende de*: cada tarea
- [ ] **T-123** — Mantener el banco: `gen_cases.py` determinista, `struktal verify` con dos backends y CI verde. *Criterio*: 0 diferencias y 0 fallos. *Depende de*: cada tarea
- [ ] **T-124** — Publicar cada cierre como commit en la rama y PR, con parche en `patches-struktal/`. *Criterio*: PR con CI verde. *Depende de*: cada tarea
