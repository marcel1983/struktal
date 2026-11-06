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
  - Una carga puntual solo imprime `load type unknown`; el análisis termina con éxito y sin esa carga. Queda como `n/a`.
  - `-jntOffset` acorta el elemento, pero no aplica el brazo rígido, porque su matriz de transformación no tiene términos de offset: MY = −P L en lugar de −P (L + a). Queda como `n/a`, y por eso esa combinación tiene su propio caso, V-OFF-003.
- **OpenSees con `UmfPack`** devolvió números en 4 modelos con mecanismo (V-MECH-003, 004, 006 y 007), sin aviso. Struktal los diagnostica con su nulidad analítica. No se usa OpenSees para validar mecanismos.
- **Pynite** no tiene deformación por cortante en barras, offsets, restricciones generales ni extremo móvil de resorte. Tiene el eje Y vertical, por lo que el traductor gira el modelo.

## Resultado (2026-11-05)

| Motor | Coinciden | Se detienen ambos | Solo Struktal detecta el mecanismo | `n/a` |
|---|---|---|---|---|
| OpenSeesPy | 31 | 4 | 4 | 4 |
| Pynite | 16 | 6 | 0 | 21 |

Sobre 43 resultados por motor (casos y combinaciones), sin ninguna diferencia.

## Sin cubrir

- Las acciones en estaciones no se comparan todavía, solo desplazamientos y reacciones.
- V-TIM-002, V-REL-001 y V-OFF-003 no tienen ningún motor comparable: solo los valida su solución cerrada.

## Reproducir

```bash
python tools/compare_engines.py --struktal target/debug/struktal.exe --engine opensees=D:/strukcal-tools/opensees-venv/Scripts/python.exe --engine pynite=D:/strukcal-tools/pynite-venv/Scripts/python.exe
```

## Pendiente

- Añadir láminas, modos y P-Δ a los traductores cuando existan en Struktal.
- Candidatos para el espectro, sin instalar ni probar: portico-core (AGPL-3.0, solo como ejecutable externo y sin leer su código para portarlo) y Open-Structures (declara licencia MIT; hay que confirmar si se puede ejecutar sin interfaz gráfica).
