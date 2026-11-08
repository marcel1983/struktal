# 0024 — Barras «solo carga» y primer contraste con un archivo real

Fecha: 2026-11-08. Estado: aceptada.

## Problema

El primer archivo real que aportó el usuario (ejemplo de verificación de placa gruesa, exportado de SAP2000 27 con su malla y resultados) se rechazaba por dos causas:

1. Los materiales uniaxiales (tendón, armadura) escriben `G12=` y `U12=` vacíos. El lector ya los admitía si faltaban, pero no si venían vacíos. Es una **falsa alarma**: esos materiales no los usaba ninguna barra.
2. 96 barras con una sección cuyos **seis modificadores de rigidez, y también los de masa y peso, valen 0**. Es la forma de ese formato de aplicar una carga lineal sobre otros elementos, en este caso el borde de la placa. El lector compensaba el peso dividiendo por el modificador de área, y salían NaN.

## Decisión

- Lector: un `G12`/`U12` vacío equivale a «sin dato». Cualquier otro número vacío sigue siendo un error.
- Modelo, esquema v8: `frames[].load_only`. La barra **no aporta rigidez ni masa** y entrega sus cargas a sus nudos como fuerzas de empotramiento de una barra prismática con su sección. Es el límite exacto de escalar toda su rigidez por ε → 0: con todos los modificadores iguales, las cargas equivalentes no dependen de ε. No es una rigidez ficticia: la barra no entra en K.
  - No admite liberaciones ni offsets, cuyo efecto con rigidez nula no está definido. Un archivo anterior a v8 con el campo se rechaza.
  - En los resultados conserva sus fuerzas de extremo (lo que entrega a los nudos), pero no tiene estaciones: no tiene esfuerzos propios.
  - Un nudo unido solo por barras «solo carga» es un mecanismo (V-MECH-011).
- Lector S2K: una sección con los seis modificadores de rigidez a 0 da barras «solo carga» y conserva sus propiedades sin modificar. Si además tiene masa (MMod ≠ 0), el archivo se rechaza, porque esa masa no tendría dónde ir. Los modificadores por barra sobre una barra «solo carga» también se rechazan.

## Verificación

- V-NULL-001: viga biapoyada con una barra «solo carga» sobre su mitad derecha. Referencia: carga uniforme en la mitad derecha, por trabajos virtuales con Simpson. Se comprueban reacciones, flecha en el centro, giro en el apoyo y momento.
- V-MECH-011: un nudo sostenido solo por una barra «solo carga» da nulidad 6 exacta, «moves 1/3 joints».
- Pruebas del lector (sección nula; masa sin rigidez rechazada; materiales uniaxiales vacíos) y del modelo (v8, sin liberaciones, no se escribe si es falso).

## Primer contraste con resultados de SAP2000

Hecho **localmente**: el archivo y sus resultados no entran en el repositorio (decisión 0018). Son 672 nudos y 576 láminas de 4 nudos, con un caso estático. Desplazamientos de Struktal frente a la tabla `JOINT DISPLACEMENTS` del propio archivo: diferencia máxima **0,18 % en U3 y 0,11 % en R1/R2**, en el punto de máxima respuesta. El archivo da 4 cifras significativas, así que parte de la diferencia es redondeo. El resto es esperable entre formulaciones de placa gruesa distintas (MITC4 en Struktal). Es un único contraste puntual: no basta para marcar ningún requisito como `verified_sap`.
