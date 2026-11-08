# 0026 — Sismo automático por coeficiente de usuario, y primer edificio real

Fecha: 2026-11-08. Estado: aceptada.

## Origen

Primer modelo de edificio del usuario (`EDF.s2k`: 24 nudos, 30 barras, 3 áreas malladas en 55 láminas, casos DEAD, MODAL, SX y SY, exportado ya analizado). Se rechazaba solo por cuatro tablas: el sismo por coeficiente de usuario, las fuerzas sísmicas generadas, el mallado automático de áreas y las restricciones de borde de áreas.

## Decisión

- **Modelo, esquema v9:** `load_patterns[].auto_seismic`, con `direction` (X o Y), `coefficient` C y `exponent` k (1 por defecto). Un archivo anterior a v9 con el campo se rechaza. La fuente de masa no puede incluir un patrón sísmico automático, porque sería circular.
- **Cálculo:** con las **mismas masas concentradas** que el análisis modal (fuente de masa, antes de restricciones):
  - W = g · Σ m_j de los nudos **no restringidos en la dirección del sismo**;
  - V = C · W;
  - F_i = V · m_i · h_i^k / Σ m_j · h_j^k, con h medida desde el nudo más bajo del modelo.

  Las dos reglas se dedujeron y **comprobaron con el propio archivo**:
  - con las masas de su tabla de masas ensambladas, la regla de reparto reproduce sus 76 fuerzas por dirección con error 1e-15;
  - W es «no restringidos», no «por encima de la base». El modelo tiene nudos de malla libres a cota 0 que cuentan en W sin recibir fuerza, y la regla de «por encima de la base» daba un 3 % menos.

  Cada caso deja en el manifiesto una nota con W y V.
- **Lector S2K:**
  - Lee `AUTO SEISMIC - USER COEFFICIENT`. Se rechazan direcciones distintas de X o Y, `EccOverride`, `UserZ`, otras cargas automáticas (normas) y la excentricidad cuando hay diafragmas. Sin diafragmas, la excentricidad se ignora con una nota, porque solo actúa a través de ellos: en el archivo todos los momentos generados son 0.
  - `AUTO SEISMIC LOADS TO JOINTS` pasa a la lista de ignoradas: son fuerzas generadas por el programa de origen, y Struktal calcula las suyas.
  - `AREA AUTO MESH ASSIGNMENTS` se acepta si el archivo trae la malla de esa área; si no, es un problema.
  - `AREA EDGE CONSTRAINT ASSIGNMENTS` con `Constrained=Yes` se acepta si ningún nudo queda dentro de un borde de elemento, porque entonces no tiene efecto; si queda alguno, es un problema, porque habría que atarlo.

## Verificación

- V-SEIS-001: columna con masas añadidas en dos niveles y una masa en un nudo libre a cota 0, con k = 1 y k = 2. Referencia a mano: reacción, momento en la base y flecha por superposición.
- Pruebas del lector (lectura, excentricidad ignorada con nota, dirección y norma rechazadas, mallado sin malla rechazado, restricción de borde sin efecto aceptada) y del modelo (v9, eje Z, C y k inválidos, fuente de masa).

## Contraste local con el edificio real

Hecho localmente; el archivo y sus resultados no entran en el repositorio (decisión 0018).

| Magnitud | Diferencia frente a SAP2000 |
|---|---|
| Reacciones en la base DEAD, SX y SY (fuerzas y momentos) | idénticas en todas las cifras mostradas |
| Cortante basal | 78 392,98 kgf en ambos |
| Periodos modos 1–4 | −0,77 %, +0,24 %, +0,19 %, −0,24 % |
| Periodos modos 5–12 | entre +0,03 % y −3,2 % |
| Desplazamiento máximo SY (lateral) | −0,5 % |
| Desplazamiento máximo SX | −1,1 % |
| Flecha máxima DEAD | −4,7 % |

Las diferencias de desplazamientos y periodos son de formulación: la lámina delgada del programa de origen frente a MITC4, en una malla de 1,2 m. El peso y las cargas son idénticos.

## Ampliación: excentricidad por diafragmas y espectros de norma (segundo archivo del usuario)

Mismo edificio con dos diafragmas rígidos horizontales, excentricidad del 5 % y dos casos de espectro con una función ASCE 7-22.

- **Excentricidad accidental** (`auto_seismic.eccentricity`). Cada nudo de un diafragma horizontal recibe además **MZ = −e·B·F_i en X y +e·B·F_i en Y**, donde B es el tamaño del diafragma, medido entre sus propios nudos, normal a la carga. Equivale a desplazar la carga +e·B en el eje normal. Los nudos fuera de diafragmas no reciben momento. La regla se dedujo y comprobó con el archivo: los 32 nudos de diafragma sin ningún desajuste, los 120 restantes con MZ = 0, y el MZ total de cada diafragma igual a su tabla de cargas por diafragma. La validación exige al menos un diafragma horizontal si e ≠ 0. Sin diafragmas, el lector la ignora con una nota. **Pendiente de confirmar con otro archivo:** si B es el tamaño de los nudos del diafragma o el de todo el nivel; aquí coinciden. Con un solo patrón se aplica el signo positivo; los patrones de signo opuesto los define el usuario.
- **Corrección encontrada por la verificación:** la resultante aplicada que usa la comprobación de equilibrio no incluía los momentos de excentricidad. Daba un desequilibrio del 5 % en V-SEIS-002. Las cargas sí se aplicaban bien.
- **Espectro de una función de norma:** si la función no está en la tabla de usuario, se lee la curva **tabulada** que el archivo trae para ella (periodo, aceleración), con una nota, sin recalcularla desde los parámetros de la norma.
- V-SEIS-002: dos columnas atadas por un diafragma, con referencia a mano de reacciones, traslación y giro del diafragma (rigidez lateral 3EI/L³ y torsión GJ/L).

Contraste local (sin publicar): reacciones en la base DEAD, SX y SY **idénticas, incluido el torsor por excentricidad** (−325 864,4 y −12 864,0 kgf·m); desplazamientos principales dentro del 1–2 %; periodos principales dentro del 1,4 %, y modos altos hasta −6,5 %. El archivo no trae resultados de los casos de espectro, así que no se pudieron comparar. La función ASCE está en g y los casos usan factor de escala 1: se avisó al usuario de que probablemente debería ser 9,80665.

## Tercer archivo: espectro con resultados (2026-11-08)

Mismo edificio, con factor de escala 9,8 en los espectros y sus resultados. Los dos casos tienen excentricidad en el espectro (`EccenRatio` 0,05 y 0,10), que el lector rechaza. En una copia local con `EccenRatio = 0`:

- **El espectro es correcto.** El cortante basal por CQC con los modos de SAP2000 y la curva tabulada reproduce **exactamente** el de SAP2000 (137 436 y 328 064 kgf); con los modos de Struktal sale +0,5 % y +0,03 %.
- **Los desplazamientos quedan un 6–7 % por debajo** en la dirección del sismo, y falta la respuesta transversal por torsión (U1 en SYD), porque falta la excentricidad del espectro.
- **Hipótesis descartada:** un par estático por diafragma, T = e·B·F con F la fuerza de inercia del nivel combinada por CQC (de los nudos del diafragma o de todo el nivel), sumado al espectro en valor absoluto o por SRSS. Da un desplazamiento transversal 10 veces mayor que el de SAP2000, y no cuadra en la dirección principal. No se implementa ninguna regla sin confirmarla.
- **Pendiente:** la formulación exacta de la excentricidad del espectro, a partir del manual de análisis de la versión, o un archivo igual con `EccenRatio = 0` para contrastar los desplazamientos del espectro solo.

## Cuarto archivo: espectro sin excentricidad, y forma de la excentricidad (2026-11-08)

Mismo edificio con `EccenRatio = 0`. Se importa sin cambios. Desplazamientos del espectro frente a SAP2000:

- SXD: componentes principales −0,78 %, giros entre 0 y −1,3 %.
- SYD: U2 −2,0 %, U3 −0,3 %, R1 −1,3 %, R3 +1,3 %.
- Algunas componentes secundarias muy pequeñas difieren alrededor del 4 %.

**El espectro sin excentricidad queda contrastado con un archivo real.**

Con los resultados de SAP2000 con excentricidad (tercer archivo) y sin ella (este), su diferencia aísla el efecto de la excentricidad con datos solo de SAP2000:

- **Forma confirmada:** el efecto es la respuesta a un par torsor estático por diafragma, **sumada en valor absoluto** a la del espectro. Ajustando un par por diafragma, el desajuste es del 0,13 % en 248 valores por caso. El intento anterior falló por un error de unidades en el script (par en N·m sobre un modelo en kgf·m), no por la forma.
- **Magnitud:** con T_d = s·F_d, F_d la fuerza de inercia de los nudos del diafragma combinada por CQC:
  - SYD: s = 1,811 m = e·18,11 m, con 18 m el ancho del diafragma normal a Y (ajuste 0,39 %);
  - SXD: s = e·22,86 m, que no coincide ni con 6 m (normal a X) ni con 18 m (ajuste 1,46 %).

  Sin explicar SXD no se implementa ninguna regla.
- **Pendiente:** la definición exacta del par (tamaño B y fuerza F), en el manual de análisis de la versión. Con ella, la implementación es directa y ya hay dos archivos para verificarla.
