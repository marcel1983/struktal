# Oráculos externos

**Aquí no entra ningún fichero generado por SAP2000** (ni `.s2k`, ni `.sdb`, ni `.LOG`, ni exportaciones): lo que se guarda es la constancia de la comparación. Las capturas se quedan en el equipo con la licencia (decisión 0018).

## Estado

| Fecha | Qué se comparó | Resultado | Constancia |
|---|---|---|---|
| 2026-11-15 | 11 casos estáticos de 3 modelos escritos a mano, contra SAP2000 27.1.0 | ✅ 276 valores, 0 diferencias | [2026-11-15-campana-sap2000.md](2026-11-15-campana-sap2000.md) |

Sigue habiendo `oracle_pending` en todos los casos de `verification/cases/`: la campaña cubre el camino estático lineal, no cada contrato.

## Plantilla de cada captura

Cuando se registre una comparación nueva, la constancia debe llevar:

- build exacto del programa y fecha;
- SHA-256 del fichero de entrada (y de la malla analítica, si la hay);
- qué tablas y componentes se compararon, y con qué tolerancia;
- el resultado, sin redondear;
- dónde están las capturas (fuera del repositorio).

**Nunca** copiar aquí salidas de Struktal ni ficheros de SAP2000. Un enlace roto es preferible a un binario versionado.