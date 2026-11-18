# 0027 — El importador S2K es estricto con los casos que no puede representar

Fecha: 2026-11-15. Estado: aceptada.

## Decisión

Un caso de carga que el lector S2K no puede representar es un **problema**, como
cualquier otra cosa que no lee: el fichero se rechaza, se listan todos los
motivos con su tabla y su línea, y no se escribe ningún modelo.

`struktal import` sigue siendo estricto por omisión. La salida
`--allow-partial` (en la biblioteca, `ImportOptions { allow_partial: true }`)
conserva el comportamiento anterior: el caso se reporta en `notes` y el resto
del fichero se importa.

Se consideran «caso no representable» los cuatro caminos que antes solo
anotaban:

- un caso con `InitialCond` distinto de `Zero`;
- un `LinModal` cuya conversión falla (por ejemplo, sin su tabla
  `CASE - MODAL 1 - GENERAL`);
- un caso de un `Type` que el lector no reconoce;
- un `LinRespSpec` cuya conversión falla (espectro sin función, amortiguamiento
  distinto del de su caso modal, referencia a un caso modal que no se importó).

## Motivos

El contrato del subcomando ya lo decía (`main.rs`: *"Strict: fails, listing
every problem, on anything it cannot represent faithfully"*), y el lector ya era
estricto con las tablas desconocidas. Un caso de carga que desaparece no es
menos grave: el modelo importado resuelve, el proceso acaba con código 0 y el
usuario solo ve una nota suelta entre otras. Es el camino por el que un análisis
incompleto se cuela sin avisar.

Con `--allow-partial` no se pierde el caso de uso que motivó el diseño
original —quien quiere la geometría, las secciones y los casos estáticos puede
pedirlos— pero tiene que pedirlo.

## Consecuencias

- `verification/s2k/portal.s2k` ya no se importa sin `--allow-partial`, porque
  declara un caso `MODAL` sin su tabla. Es lo que hace útil el fichero como
  prueba: el caso estricto y el parcial se prueban los dos.
- Los tests unitarios del importador que construyen sobre `BASE` usan
  `import_with_options(..., allow_partial: true)`: comprueban el modelo, no el
  rechazo.
- `spec/features.csv` F19-002 pasa a cubrir las dos rutas con los tests nuevos
  `an_unrepresentable_case_refuses_the_file_by_default`,
  `a_partial_import_keeps_the_rest_and_reports_the_case` y
  `an_s2k_with_a_case_it_cannot_read_is_refused_unless_partial_is_asked`.
- Quien tenga una cadena de comandos que importe ficheros con casos dinámicos
  tiene que añadir `--allow-partial` o arreglar el fichero de origen.