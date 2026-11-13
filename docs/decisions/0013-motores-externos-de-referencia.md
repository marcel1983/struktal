# 0013 — Otros motores como segunda opinión

Fecha: 2026-11-05. Estado: aceptada.

## Decisión

`tools/compare_engines.py` traduce cada `verification/cases/*/model.json` a otros motores y compara desplazamientos y reacciones. Es opcional, no forma parte del motor ni del CI, y necesita cada motor en su propio entorno de Python.

Un motor externo **no es la referencia**. La referencia de un caso sigue siendo su solución cerrada. Si un motor discrepa, se investiga cuál de los dos representa bien el modelo, y una coincidencia no sustituye a la solución cerrada. Por eso esta comparación no cambia ningún estado de `spec/features.csv`.

## Motores

| Motor | Versión | Entorno, fuera del repo | Licencia |
|---|---|---|---|
| OpenSeesPy | 3.8.0.0 | Python 3.12 (su binario enlaza `python312.dll`) | Uso comercial sin revisar |
| Pynite (PyNiteFEA) | 3.2.0, commit `88f62a5` | Python 3.12 | MIT |

No se usa código de estos motores en Struktal. Los traductores en `tools/oracles/` solo llaman a sus API públicas.

## Reglas

- Si un motor no puede representar fielmente un modelo, el traductor lo marca `n/a` en lugar de aproximarlo. Comparar otra cosa daría falsas diferencias o falsas coincidencias.
- Cada traductor calcula por su cuenta unidades, ejes locales y cargas, en Python, sin usar el código en Rust.
- Tolerancia: relativa 1e-6 sobre el mayor de los dos valores, más un suelo de ruido de 1e-9 sobre la escala del caso. Los valores no finitos cuentan como diferencia.

## Limitaciones encontradas en los motores

- **OpenSees `ElasticTimoshenkoBeam`**, comprobado en la versión 3.8.0.0 y en su código fuente (`ElasticTimoshenkoBeam3d.cpp`):
  - La carga uniforme funciona: un voladizo coincide con la solución exacta.
  - Una carga puntual o parcial solo imprime `load type unknown`; el análisis termina con éxito y sin esa carga. Queda como `n/a`.
  - `-jntOffset` acorta el elemento, pero no aplica el brazo rígido, porque su matriz de transformación no tiene términos de offset: MY = −P L en lugar de −P (L + a). Queda como `n/a`, y por eso esa combinación tiene su propio caso, V-OFF-003.
- **OpenSees `elasticBeamColumn`:** la carga parcial uniforme funciona, pero la de valores finales distintos se descarta sin error. Sin momentos de vano en `eleLoad`. Quedan como `n/a`.
- **OpenSees con `UmfPack`** devolvió números en 4 modelos con mecanismo (V-MECH-003, 004, 006 y 007), sin aviso. Struktal los diagnostica con su nulidad analítica. No se usa OpenSees para validar mecanismos.
- **Pynite** no tiene deformación por cortante en barras, offsets, restricciones generales ni extremo móvil de resorte. Tiene el eje Y vertical, por lo que el traductor gira el modelo.

## Ampliación del 2026-11-09

El traductor, en `tools/oracles/common.py`, se ha ampliado con cinco cosas:

- **Malla de barras:** corta las barras en tramos y en los nudos sobre su eje, como la malla de análisis (decisión 0015), y reparte entre los tramos las cargas de vano.
- **Masa concentrada:** reparte la masa como la fuente de masa, incluidas láminas y patrones (decisiones 0020 y 0022). De los patrones solo traduce cargas de nudo, uniformes y de área, sin uniformes sobre barras con liberaciones.
- **Sismo automático:** calcula la carga por coeficiente con la regla de la decisión 0024. Comprueba la estructura, pero no la regla, que es la misma en los dos lados.
- **Espectro de respuesta en OpenSees:** cada modo se resuelve como la respuesta estática a sus fuerzas de inercia, M φ Γ Sa, y después se combina por SRSS o CQC. Los modos y la estática los pone OpenSees, y la combinación es la misma fórmula.
- **Barras articuladas en OpenSees:** una barra con los momentos liberados en los dos extremos y la torsión en uno se traduce como cercha. Es exacto: solo trabaja axialmente, y sus cargas de vano pasan a los extremos como reacciones de viga simplemente apoyada.

`compare_engines.py` también acepta la ruta de cualquier `model.json`, para comparar modelos sin solución cerrada. En un modelo con láminas, las diferencias se marcan `FORM-DIFF`. Un caso que Struktal resuelve y el motor no (`ENG-FAIL`) cuenta como diferencia.

### Resultado

| Motor | Coinciden | Diferencia de formulación (láminas) | `DIFF` | Solo falla el motor | Solo Struktal detecta el mecanismo | Se detienen ambos | `n/a` |
|---|---|---|---|---|---|---|---|
| OpenSeesPy | 60 | 6 | 2 | 1 | 6 | 6 | 12 |
| Pynite | 33 | 4 | 0 | 0 | 0 | 8 | 48 |

Ahora coinciden la fuente de masa, el espectro, el sismo automático, los modos con barras subdivididas y los de láminas. Los dos `DIFF` son los de la decisión 0021.

El caso donde solo falla el motor es **un defecto de Struktal**. En `V-REL-002`, el nudo C no tiene rigidez fuera del plano de las dos barras articuladas. OpenSees lo detecta como sistema singular, y Struktal no lo detecta: con una carga en Y da UY ≈ 7·10¹³ m como resultado válido. El caso pasa porque su carga no tiene componente en Y. La causa es que la diagonal de ese grado de libertad es solo redondeo, y al escalarla por sí misma parece un pivote sano. Se corrigió en la decisión 0025.

**Modelos locales**, sin publicar los modelos ni sus resultados:

- `ejemplo_combos_lineal`: coinciden sus 10 casos, incluidos 12 modos (diferencia máxima 2,5e-12) y el sismo automático.
- `ejemplo_lineal`: con sus 2 láminas, diferencia de formulación, hasta 0,04 % en períodos. Sin ellas coincide en todo lo traducible. El caso `cv` no se compara, porque `ElasticTimoshenkoBeam` descarta cargas puntuales.

## Resultado (2026-11-05)

| Motor | Coinciden | Se detienen ambos | Solo Struktal detecta el mecanismo | `n/a` |
|---|---|---|---|---|
| OpenSeesPy | 31 | 4 | 4 | 9 |
| Pynite | 19 | 6 | 0 | 23 |

Sobre 48 resultados por motor (casos y combinaciones), sin ninguna diferencia.

## Sin cubrir

- Las acciones en estaciones no se comparan todavía, solo desplazamientos y reacciones.
- V-TIM-002, V-REL-001, V-OFF-003, V-TRAP-003 y V-MOM-002 no tienen ningún motor comparable: solo los valida su solución cerrada.

## Reproducir

```bash
python tools/compare_engines.py --struktal target/debug/struktal.exe --engine opensees=D:/strukcal-tools/opensees-venv/Scripts/python.exe --engine pynite=D:/strukcal-tools/pynite-venv/Scripts/python.exe
```

## Pendiente

- Añadir P-Δ a los traductores cuando exista en Struktal. Pynite sigue sin modos.
- Candidatos para el espectro, sin instalar ni probar: portico-core (AGPL-3.0; desde la decisión 0026 su código sería compatible, pero incorporarlo exige una decisión propia) y Open-Structures (declara licencia MIT; hay que confirmar si se puede ejecutar sin interfaz gráfica).
