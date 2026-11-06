# 0016 — Envolventes con caso gobernante (F16-002)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

`combinations[]` admite `{"type": "Envelope", "name", "cases": [{"case", "scale"}]}`. Los términos pueden ser casos o combinaciones lineales.

Se añade al esquema v5 sin subir la versión, porque v5 aún no se ha publicado.

## Resultados

Una envolvente no tiene `joints` ni `frames` propios, sino `envelope`, con el máximo, el mínimo y el término gobernante por componente de:

- desplazamientos de nudo;
- reacciones;
- acciones en cada estación de barra.

Reglas:

- **No es un estado de la estructura:** cada componente puede venir de un término distinto. Por eso no tiene `checks` de equilibrio, y **no puede ser término de otra combinación**.
- Solo cuentan los términos: si todos son positivos, el mínimo es positivo; no se añade el cero.
- En un empate gobierna el primer término de la lista.
- Si un término no terminó, la envolvente falla con la causa, igual que una combinación lineal.

## Verificación

- V-ENV-001: viga biapoyada con cargas hacia abajo, una combinación y una carga hacia arriba escalada, comprobando valores y término gobernante de reacciones, desplazamientos y acciones.
- El comparador acepta `bound` y `governing`: un término gobernante equivocado es un fallo.
- No se compara con otros motores.

## Pendiente

- SRSS, absoluta y rango (F16-003, F16-004).
- Comportamiento de SAP2000 con envolventes anidadas y términos con signo: oracle_pending.
