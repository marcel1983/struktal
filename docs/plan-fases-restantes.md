# Plan de las fases restantes

Fecha: 2026-11-17. Estado: vivo; se marca al cerrar cada tarea.

Esta es la lista de lo que queda del motor, fase a fase. Cada tarea se cierra
cuando tiene las cinco cosas:

1. **Modelo**: campos de esquema (con versión nueva si el campo no existía) y
   validación que rechaza lo que no puede representar.
2. **Cálculo**: la formulación, con su referencia (guía, texto o decisión).
3. **Test unitario propio**: el núcleo y el elemento, no sólo el caso.
4. **Caso en `verification/cases/`**: referencia independiente (analítica,
   numérica propia o valores publicados), nunca salida del motor.
5. **Documentación**: `docs/compatibility-report.md`, `docs/agent-state.md` y ADR
   si la tarea toma una decisión.

Nada se marca hecho por estar escrito: sólo por estar medido. Un caso que no
pasa no entra al banco (`docs/decisions/0031-mallado-de-areas.md`).

## Orden de trabajo

El orden sigue las dependencias, no el número de la familia:

1. **F03, elementos** (P06/P08): sin elementos no hay nada que resolver. Ahora
   van planos y axisimétricos, luego sólidos de 8 nudos y, al final, las leyes
   no lineales de sólidos (que necesitan F12).
2. **F04–F06, lo que se apoya en el modelo**: restricciones, cables y cargas.
   Desbloquean casos que hoy no se pueden plantear.
3. **F07–F11, análisis**: amortiguamiento, P-Delta y pandeo, Ritz, sismo,
   historia en el tiempo.
4. **F12–F13, no linealidad**: materiales, rótulas, grandes desplazamientos,
   pushover, etapas y fluencia.
5. **F14–F16, otras respuestas y combinaciones**: estado estacionario, PSD,
   móviles, envolventes.
6. **F17–F18, resultados y diseño**: tablas, section cuts, diseño.
7. **F19–F20, intercambio y GUI**: exportación, puentes, API y aplicación.
8. **F01–F02, huecos de modelo**: coordenadas, grupos, unidades, modificadores,
   secciones y materiales.

Las familias F01 y F02 se cierran al final porque casi todo su contenido son
convenios que sólo se pueden fijar con el contraste (SAP2000 no se define por
ahora, decisión del mantenedor del 2026-11-16).

## F03 — Elementos

| Req. | Alcance | Criterio de aceptación | Estado |
|---|---|---|---|
| F03-001/002 | Lámina delgada y gruesa | Ya cubiertas por MITC4/MIT3. Falta una formulación delgada propia sólo si el contraste la exige. | partial |
| F03-004 | Lámina por capas | Hecha con `A` y `D`; falta `b ≠ 0` y una placa laminada con malla. | partial |
| F03-005 | Planos y axisimétricos | Ver tareas siguientes. | specified |
| F03-006 | Sólidos de 8 nudos | Ver tareas siguientes. | specified |
| F03-007 | Leyes no lineales de sólidos | Depende de F12-001. | specified |
| F03-008 | Mallado de áreas | Falta un caso formal (el *patch test* da 0,633 por la capa límite de taladro). | partial |

### F03-005a — Núcleo de tensión plana

- Elementos: CST de 3 nudos y cuadrilátero de 4 nudos (bilineal). Matriz `D` de
  tensión plana; espesor como factor.
- Criterio: *patch test* exacto en cuadrilátero distorsionado, 3 modos de cuerpo
  rígido, y energía positiva.
- ADR: si el plano entra con 2 GDL/nudo o se lleva a los 6 del ensamblaje.

### F03-005b — Plano en el ensamblaje

- 2 GDL/nudo, malla de áreas, cargas de presión y peso propio, resultantes.
- Criterio: caso con estado de tensión plana conocido (placa perforada no; una
  franja a tracción con contracción de Poisson, y un disco con carga diametral).

### F03-005c — Axisimétrico de 4 nudos

- Peso `2πr` en la integración; εθ = u/r.
- Criterio: cilindro de pared gruesa bajo presión interna contra Lamé; disco
  girando; el mismo mallado con un material y con otro.

### F03-006 — Sólidos de 8 nudos

- Hexaedro de 8 nudos con 3 GDL/nudo; `D` isótropa 3D.
- Criterio: *patch test* 3D, modos rígidos (6), cantilever a tracción y cortante,
  y Lamé en 3D (el barril).
- Riesgo: el ensamblaje y los resultados están pensados para 6 GDL/nudo; hay que
  extenderlos o envolver el sólido.

### F03-007 — Leyes no lineales de sólidos

- Depende de F12-001 (materiales uniaxiales) y de F03-006.
- Criterio: un bloque a compresión con ley bilineal contra la solución de un
  resorte equivalente; descarga y reverting.

### Huecos que quedan abiertos

- F03-004: `b ≠ 0` y placa laminada con malla.
- F03-008: caso formal; la capa límite de taladro del triángulo con borde
  empotrado (0,633 del campo) es del elemento, no del mallado.
- Triángulo rígido en flexión en su plano (decisión 0021): si el contraste lo
  pide, revisar la formulación.

## F04 — Restricciones, offsets y links

| Req. | Alcance | Estado |
|---|---|---|
| F04-004 | Puntos cardinales y factor de zona rígida | partial (sólo offsets vectoriales) |
| F04-005 | Restricciones locales y weld | partial (body, diafragma y equal hechos) |
| F04-007 | Links lineales | specified |
| F04-008 | Ejes locales de nudo | specified |

Criterio común: cada restricción se elimina del sistema y se recupera la
reacción; los casos comprueban reacción y equilibrio, no sólo que resuelva.

## F05 — Cables y tendones

| Req. | Alcance | Estado |
|---|---|---|
| F05-001 | Cable con flecha/tensión inicial | specified |
| F05-002 | Tendones de pretensado y sus cargas | specified |

Criterio: cable con su catenaria aproximada (o parábola, declarando la
aproximación) y tendón con las cargas equivalentes en los apoyos.

## F06 — Cargas

| Req. | Alcance | Estado |
|---|---|---|
| F06-008 | Temperatura y deformación impuesta | specified |
| F06-009 | Cargas proyectadas | specified |
| F06-010 | Generadores sísmicos y de viento por norma | partial (sólo coeficiente) |
| F06-011 | Transferencia de cargas de área a barras | partial (uniforme sobre áreas) |
| F06-012 | Oleaje y otras del perfil | specified |

Criterio: cada carga se comprueba por equilibrio (resultante y momento) y, si
hay norma, con un valor publicado.

## F07 — Amortiguamiento

| Req. | Alcance | Estado |
|---|---|---|
| F07-003 | Modal, Rayleigh y de elementos | specified |

## F08 — Análisis lineal

| Req. | Alcance | Estado |
|---|---|---|
| F08-004 | Sistemas indefinidos / Lagrange | blocked |
| F08-005 | P-Delta | specified |
| F08-006 | Pandeo lineal | specified |

Criterio de pandeo: factor de carga contra la fórmula de Euler y contra un caso
con ala; el P-Delta contra el segundo orden de una columna.

## F09 — Vectores Ritz

| Req. | Alcance | Estado |
|---|---|---|
| F09-002 | Vectores Ritz dependientes de carga | specified |

## F10 — Sismo

| Req. | Alcance | Estado |
|---|---|---|
| F10-001 | Espectro con CQC/SRSS/GMC | partial (CQC y SRSS) |
| F10-002 | Combinación direccional y escala de aceleración | specified |
| F10-003 | Masa faltante y signos | specified |

## F11 — Historia en el tiempo

| Req. | Alcance | Estado |
|---|---|---|
| F11-001 | Historia modal lineal | specified |
| F11-002 | Integración directa lineal | specified |
| F11-003 | Integración directa no lineal | specified |
| F11-004 | Aceleración de base y multi-apoyo | specified |

Criterio: un oscilador de 1 GDL con solución cerrada (sin amortiguar,
amortiguado y con excitación senoidal), y un edificio de 2 GDL con la solución
modal.

## F12 — No lineal

| Req. | Alcance | Estado |
|---|---|---|
| F12-001 | Materiales uniaxiales con commit/revert | specified |
| F12-002 | Rótulas plásticas e interacción P-M-M | specified |
| F12-003 | Links no lineales (gap, hook, fricción, aisladores, amortiguadores) | specified |
| F12-004 | Grandes desplazamientos | specified |
| F12-005 | Pushover con control de carga/desplazamiento | specified |

Criterio: cada ley contra su resorte equivalente (un GDL), y el pushover contra
una curva capacidad conocida.

## F13 — Etapas y tiempo

| Req. | Alcance | Estado |
|---|---|---|
| F13-001 | Construcción por etapas | specified |
| F13-002 | Fluencia, retracción y envejecimiento | specified |
| F13-003 | Pérdidas de pretensado | specified |

## F14 — Estado estacionario

| Req. | Alcance | Estado |
|---|---|---|
| F14-001 | Respuesta en estado estacionario | specified |
| F14-002 | PSD | specified |
| F14-003 | FNA | specified |
| F14-004 | Fuerza objetivo | specified |
| F14-005 | Casos hiperestáticos | specified |

## F15 — Cargas móviles

| Req. | Alcance | Estado |
|---|---|---|
| F15-001 | Carriles, vehículos y clases | specified |
| F15-002 | Líneas de influencia y búsqueda de extremos | specified |
| F15-003 | Cargas móviles dinámicas | specified |

## F16 — Combinaciones

| Req. | Alcance | Estado |
|---|---|---|
| F16-003 | SRSS y suma absoluta | specified |
| F16-004 | Rango y combinaciones con casos no lineales | specified |
| F16-005 | Combinaciones normativas automáticas | specified |

## F17 — Diseño

| Req. | Alcance | Estado |
|---|---|---|
| F17-001..007 | Acero, hormigón de barras, aluminio, conformado en frío, losas, selección automática, longitudes de pandeo | specified |

Criterio: cada norma se contrasta con un ejemplo publicado de la propia norma; sin
ejemplo publicado no se marca hecho.

## F18 — Resultados

| Req. | Alcance | Estado |
|---|---|---|
| F18-003 | Acciones P, V2, V3, T, M2, M3 | partial (signos sin confirmar) |
| F18-007 | Tablas con nombres y formato | specified |
| F18-008 | Section cuts | specified |
| F18-009 | Tensiones y resultantes en láminas y sólidos | partial |
| F18-010 | Persistencia por bloques | specified |

## F19 — Intercambio

| Req. | Alcance | Estado |
|---|---|---|
| F19-003 | Exportación S2K y round-trip en SAP2000 | specified |
| F19-004 | Puente OAPI en proceso Windows separado | specified |
| F19-005 | API Python (PyO3) | specified |
| F19-006 | Capa compatible con scripts OAPI | specified |
| F19-007 | IFC | specified |
| F19-008 | DXF | specified |
| F19-009 | Lectura directa de `.sdb` | blocked |

## F20 — Aplicación

| Req. | Alcance | Estado |
|---|---|---|
| F20-002 | Edición gráfica/tabular con deshacer | specified |
| F20-003 | Proceso de cálculo aislado con cancelación | specified |
| F20-004 | Visor de resultados e informes | specified |
| F20-005 | Instalador en PC limpia | specified |

## F01–F02 — Huecos de modelo

| Req. | Alcance | Estado |
|---|---|---|
| F01-004 | Sistemas de coordenadas adicionales y rejillas | specified |
| F01-005 | Grupos y selecciones | specified |
| F01-011 | Unidades de masa, tiempo y temperatura | specified |
| F02-002 | Secciones paramétricas (T, canal, ángulo, tablas) | partial |
| F02-004 | Modificadores de rigidez, masa y peso | specified |
| F02-005 | Secciones variables (no prismáticas) | specified |
| F02-006 | Secciones definidas por el usuario | specified |
| F02-007/008 | Ejes locales por defecto y ángulo | partial |
| F02-009 | Ejes locales avanzados | specified |
| F02-010 | Biblioteca de materiales | specified |

## Preguntas abiertas y bloqueos

- **Build exacto de SAP2000**: no se define por ahora (decisión del mantenedor,
  2026-11-16). Todo lo que dependa del contraste se queda en `oracle_pending`.
- **F08-004**: el backend actual no resuelve sistemas indefinidos.
- **F19-009**: no hay ruta independiente documentada para leer `.sdb`.
- **Capa límite de taladro**: triángulo con borde empotrado, 0,633 del campo en
  el *patch test* (`docs/decisions/0031-mallado-de-areas.md`).
- **Triángulo rígido en flexión en su plano** (decisión 0021): pide mallas
  finas en muros.
- **Láminas con `b ≠ 0`**: sin acoplamiento flexión–tracción (decisión 0032).
- **Signos**: convención propia verificada en estático lineal (11 casos, 276
  valores, 0 diferencias); resta el resto de familias (F18-003, F10-001).
- **Caso formal de F03-008**: pendiente.

## Definición de hecho

Una tarea está hecha cuando:

- [ ] el modelo la acepta y valida lo que no puede representar;
- [ ] el cálculo está testeado con un test propio que no depende del caso;
- [ ] hay un caso en `verification/cases/` con referencia independiente y pasa
      con los dos backends;
- [ ] `spec/features.csv` dice `verified` (o `partial` con la limitación
      escrita);
- [ ] `docs/agent-state.md`, `docs/compatibility-report.md` y la decisión (si
      la hay) están al día;
- [ ] `cargo fmt --check`, `clippy -D warnings` y el banco completo pasan.
