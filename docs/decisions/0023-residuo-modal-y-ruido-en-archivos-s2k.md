# 0023 — Parada del solver modal, I23 con ruido y nudos huérfanos (F09-001, F19-002)

Fecha: 2026-11-06. Estado: aceptada.

Tres defectos encontrados al probar Struktal con benchmarks publicados y con los S2K locales.

## Decisión

**Solver modal.** La iteración de subespacio paraba cuando los valores propios cambiaban menos de 1e-11. Un valor propio converge el doble de rápido que su vector, así que el vector podía quedar sin converger. En un marco de 10 vanos y 9 pisos (Bathe y Wilson, 1972), el caso modal terminaba en error con residuo 1,57e-6, sobre el límite de 1e-6, aunque los valores estaban bien.

Ahora, con los valores estables, sigue hasta que el residuo baja de 1e-8, o hasta que deja de bajar, porque entonces ya está en el piso del redondeo. El límite de aceptación sigue en 1e-6.

**I23 en el lector S2K.** Se exigía exactamente 0, y se rechazaban secciones simétricas con valores de ruido como 1,4e-20. Ahora un |I23| de hasta 1e-9 de la mayor inercia de la sección se toma como 0, sin nota. Por encima sigue siendo un rechazo, porque no hay producto de inercia.

**Nudos huérfanos en el lector S2K.** Un nudo sin conexión hacía fallar todos los casos del modelo. Ahora el lector lo deja fuera con una nota si cumple todo esto:

- Ninguna barra, área o restricción lo usa, y no está sobre el eje de una barra que se corta en nudos.
- No tiene carga, asiento, resorte ni masa.
- No está totalmente restringido. Si lo está, es válido y se queda.

Si algo actúa sobre él, se queda y el solver lo rechaza, porque esa carga se perdería sin aviso. En un modelo nativo tampoco cambia nada: el rechazo del solver sigue igual.

## Sin confirmar frente a SAP2000 (oracle_pending)

Que SAP2000 ignore un nudo huérfano. El archivo que lo mostró le da masa cero, y eso es todo lo que se sabe. No afecta al resultado de la estructura, porque ese nudo no está conectado a ella.

## Verificación

- **V-MOD-004:** el marco de Bathe y Wilson, contra sus valores publicados 0,589541, 5,52695 y 16,5878. Es el primer caso con referencia `published`. Su tolerancia, `modes-published-v1`, es la que permiten esas seis cifras, 1e-5, y no mide el error del solver. Sin el cambio, este caso termina en error.
- Prueba de residuo en `eigen.rs`, y pruebas del lector para el ruido, el I23 real, el nudo huérfano, el nudo con carga, el restringido y el que está sobre el eje de una barra.

## Comparación con los ejemplos de OpenSees

Comparación puntual, con scripts fuera del repositorio. Los modelos se reconstruyeron siguiendo `EXAMPLES/verification` de OpenSees, y no se copió ninguno. Sigue siendo una segunda opinión (decisión 0013), y no cambia ningún estado de `spec/features.csv`.

| Benchmark | Struktal frente a OpenSeesPy 3.8 |
|---|---|
| Cercha de 3 barras, parte lineal | unas 15 cifras, igual a la solución exacta |
| 3 resortes en serie, como barras axiales | unas 15 cifras |
| Pórtico de 7 pisos, períodos y carga estática | unas 12 cifras |
| Marco de Bathe y Wilson | unas 13 cifras |
| Cilindro pellizcado, malla 32 × 32 | −1,0 % frente a la referencia; `ShellMITC4` −1,2 %, `ASDShellQ4` −1,1 % |
| 9 muros de corte, 16 × 16 por vano y piso | hasta 0,19 % de `quad`, siempre entre `quad` y `SSPquad` |

En el cilindro con 64 × 64, los tres motores superan la referencia 164,24 P/(E t) en 0,6 a 0,7 %. Podría indicar que ese valor es algo bajo, pero no se ha comprobado. Por eso solo el marco pasó a ser un caso de verificación.

Los ejemplos con materiales no lineales, análisis transitorio o P-Δ no se pueden comparar: Struktal no tiene esos análisis.

## Efecto en los S2K locales

De los 27 archivos locales (17 `.s2k` y 10 `.$2k`) se siguen importando 12. Con estos cambios, `ejemplo_lineal` y `ejemplo_combos_lineal` solo quedan bloqueados por el sismo automático y, el segundo, por las opciones de combinación automática. En copias temporales sin esas tablas, ambos se importan y se resuelven, incluido su caso modal. En esas copias, los casos sísmicos se resuelven sin carga, y los resultados no se compararon con los del archivo.

## Pendiente

- Sismo automático: `AUTO SEISMIC`.
- Una malla real de áreas y un caso de espectro importable, que estos archivos no tienen.
