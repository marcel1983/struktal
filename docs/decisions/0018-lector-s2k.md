# 0018 — Lector S2K estricto y lote de contraste (F19-001, F19-002)

Fecha: 2026-11-05. Estado: aceptada.

## Decisión

`struktal import archivo.s2k -o modelo.json`. El crate `struktal-io-s2k` tiene dos capas: `parser` conoce la sintaxis, y `import` el significado.

**Es estricto: lo que no puede representar fielmente es un problema, no se omite.** Se informan todos los problemas juntos, con tabla, línea y causa, y no se escribe ningún modelo parcial.

- Una tabla desconocida con filas es un problema, porque podría cambiar la estructura o las cargas.
- Las tablas sin efecto en este análisis (resultados, pantalla, datos de diseño) se listan en `IGNORED` y se cuentan en el informe.
- Las secciones sin usar no importan: un archivo trae bibliotecas enteras.
- Los casos que no son `LinStatic`, y las combinaciones que dependen de ellos o de otros tipos, no se importan, y se avisa.
- Solo se leen barras. Áreas y sólidos llegan con los hitos siguientes.

El resultado todavía pasa por `validate`, así que, por ejemplo, un asiento sobre un grado de libertad libre es un error (decisión 0012).

## Modificadores

No hay modificadores en el modelo (F02-004), así que el lector los aplica a las propiedades. Un modificador de área no debe cambiar el peso, por lo que una sección modificada usa una copia de su material con el peso específico compensado: peso por longitud = γ · A · `WMod`. Los de una barra (`FRAME PROPERTY MODIFIERS`) crean una sección propia, `SECCIÓN#BARRA`.

## Rechazado por ahora

Cargas proyectadas, temperatura, ejes locales de nudo, puntos cardinales distintos del centroide, offsets en ejes locales, secciones variables, ejes avanzados, sobrescritura de material, body parcial y restricciones en ejes locales.

## Procedencia

Los nombres de tablas y campos son hechos del formato. Se tomaron de cómo los lee PAS y de archivos locales del usuario, solo para ver su forma. No se copió código de PAS: ese lector es tolerante, y este es estricto. Ningún archivo de SAP2000 entra en el repo; el fixture `verification/s2k/portal.s2k` está escrito a mano.

Los archivos locales se han usado solo para probar la lectura, **sin comparar con sus resultados** mientras la licencia no esté revisada. De 12, 6 se importan y 6 se rechazan por las razones de arriba.

## Lote de contraste con SAP2000

`tools/gen_sap_lot.py CARPETA` escribe 4 modelos pequeños y una guía, fuera del repo: signos, ejes locales, propiedades de secciones y opciones. **No se han abierto nunca en SAP2000**, porque aquí no está instalado. Sirven para resolver lo marcado `oracle_pending`. Lo que vuelva de SAP2000 no entra en el repo público.

## Verificación

- Parser: tablas, continuaciones, comillas, repetición y errores con línea.
- Import: tipos de carga, unidades, grados de libertad activos, modificadores sin cambiar el peso, restricciones, liberaciones, malla, combinaciones y cada rechazo.
- CLI: `portal.s2k` en kip-in se importa y resuelve hasta la solución cerrada en SI; un archivo con algo no soportado termina con código 2, sin salida parcial y con todas las causas.

## Pendiente

- Comparar con los resultados de SAP2000, una vez aclarada la licencia.
- Lo rechazado, según lo pidan tus modelos.
- Áreas y malla de análisis (hito 2).
- Escritura S2K (F19-003).
