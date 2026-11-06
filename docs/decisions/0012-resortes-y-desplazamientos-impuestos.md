# 0012 — Resortes de nudo y desplazamientos impuestos (F04-002, F04-006)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

- `joints[].springs`: seis rigideces a tierra en ejes globales [UX, UY, UZ, RX, RY, RZ], en fuerza/longitud y fuerza·longitud/rad. 0 = sin resorte. No hay resortes acoplados ni en ejes locales de nudo.
- `joint_displacements[]`: `{pattern, joint, values}` en ejes globales, en longitud y radianes, escalados por el patrón como una carga.

Se añaden al esquema v3 sin subir la versión: v3 se creó en esta misma serie de cambios y aún no se ha publicado (mismo criterio que la decisión 0008). Un archivo anterior a v3 que los traiga se rechaza.

## Formulación

Con u_p los desplazamientos impuestos en componentes restringidas y u_g los del extremo a tierra de un resorte:

- K incluye la rigidez k de los resortes, a través de C si el nudo está atado (decisión 0011).
- f_efectiva = f − K u_p + k u_g. El sistema se resuelve en las componentes libres, y u = C x + u_p.
- Reacción en una componente restringida: fuerza interna − carga en nudo. En un resorte: k (u_g − u).
- Trabajo externo: ½ (f·u + Σ R·u_imp), que es la energía de deformación también con desplazamientos impuestos.

La factorización no cambia por caso, así que un mismo modelo admite casos con y sin desplazamientos impuestos, y las combinaciones los superponen.

## Reglas de validación

- Un resorte sobre una componente restringida es un error de entrada.
- **Un desplazamiento impuesto no nulo sobre una componente sin apoyo ni resorte es un error**, no se ignora. Según el comportamiento documentado en PAS, SAP2000 lo ignora: **oracle_pending**. Struktal prefiere el error, porque un dato perdido en silencio cambia el resultado sin aviso.
- Un nudo sin elementos pero con resortes es válido. Sus componentes sin rigidez se informan como mecanismo.

## Cambio en la comprobación de equilibrio

La comprobación normalizaba el error con las resultantes de cargas y reacciones. Bajo solo desplazamientos impuestos las dos son nulas, y el error relativo salía 1,0 por puro ruido (lo destapó V-SET-002). Ahora la escala incluye además la mayor reacción individual. Esto hace la comprobación algo menos estricta cuando las reacciones individuales superan mucho a la resultante; el umbral de los casos sigue en 1e-9.

## Verificación

- V-SPR-001: resortes de traslación y de giro en la punta de un voladizo.
- V-SPR-002: piso sin barras, sostenido solo por resortes en nudos de un diafragma.
- V-SET-001: asiento en viga biempotrada: solo, con carga uniforme en el mismo caso y en una combinación.
- V-SET-002: giro impuesto, asiento con deformación por cortante, apoyo central de una viga de dos vanos y extremo de resorte que se mueve.
- Referencias cerradas en `tools/gen_cases.py`. Pasan con ambos backends.

## Pendiente

- Resortes acoplados, en ejes locales y no lineales (solo compresión).
- Ejes locales de nudo (F04-008), para apoyos y asientos inclinados.
- Comparar con SAP2000 el tratamiento de valores impuestos en componentes libres.
