# Unidades y convenciones (perfil v1)

Estado: convenciones de rust-fem definidas y verificadas analíticamente. La correspondencia con SAP2000 está marcada como `oracle_pending` mientras no haya instalación de referencia.

## Unidades

- Internas: N, m, s, rad. Toda conversión ocurre una sola vez en `Model::to_si` (crate `fem-core`), usando exponentes de fuerza y longitud (`Dim`).
- Entrada (esquema v1): fuerza `N, kN, MN, kgf, tonf, lbf, kip`; longitud `m, cm, mm, ft, in`. kgf/tonf usan g₀ = 9,80665 m/s².
- La unidad original se registra en el manifiesto (`input_units`); los resultados se emiten siempre en SI (`result_units`).
- Pendiente: masa, tiempo y temperatura (absoluta, incremento y gradiente por separado) — F01-011.

## Ejes globales y grados de libertad

- Ejes globales dextrógiros X, Y, Z; Z vertical hacia arriba. Gravedad = −Z.
- Orden por nudo: `UX, UY, UZ, RX, RY, RZ`; reacciones `FX, FY, FZ, MX, MY, MZ`. Giros positivos por la regla de la mano derecha.

## Ejes locales de barra

- Eje 1: de I a J.
- Barra no vertical: el plano 1-2 contiene Z global y el eje 2 tiene componente Z positiva.
- Barra vertical (sen del ángulo con Z < 1e-3): eje 2 = +X global.
- Eje 3 = 1 × 2. `angle_deg` gira 2 y 3 alrededor de 1 (mano derecha).
- Fuente: ayuda SAP2000 de ejes locales [S06]. **Umbral de verticalidad y signo del ángulo: oracle_pending.**
- Propiedades: I33 es la inercia respecto al eje 3 (flexión en el plano 1-2), I22 respecto al eje 2. As2 es el área de cortante para V2 (plano 1-2, junto con I33) y As3 para V3 (plano 1-3, junto con I22); 0 desprecia la deformación por cortante en ese plano (decisión 0007, equivalencia con SAP2000 oracle_pending).
- Los giros nodales son giros de la sección; con deformación por cortante no coinciden con la pendiente de la elástica.

## Fuerzas de extremo y acciones de sección

- `end_forces_local`: fuerzas que los nudos ejercen sobre el elemento, en ejes locales, `[I: F1 F2 F3 M1 M2 M3, J: …]`.
- Acciones de estación `[P, V2, V3, T, M2, M3]` (decisión 0003): componentes del vector fuerza/momento sobre la cara de corte positiva (normal +1), ejercidas por la parte más allá de x. P > 0 es tracción. Una carga puntual situada exactamente en x se excluye (valor inmediatamente antes).
- Consecuencia ejemplo: en una viga horizontal según +X (2 = +Z, 3 = −Y), un momento positivo M3 produce tracción en la fibra inferior (flexión positiva).
- **El mapeo a las convenciones de signo de SAP2000 para V2, V3, M2, M3 está pendiente de oráculo.** No se debe aplicar un cambio de signo sin un caso de referencia ejecutado.

## Liberaciones de extremo

- Componentes en ejes locales y en el orden [P, V2, V3, T, M2, M3] = DOF [u1, u2, u3, r1, r2, r3] de cada extremo.
- Resortes de fijación parcial: fuerza/longitud (P, V2, V3) y fuerza·longitud/rad (T, M2, M3); 0 = liberación total.
- `end_forces_local` y las acciones en estaciones se refieren a los extremos del elemento (después del resorte): en una componente liberada sin resorte valen 0. Las reacciones y el equilibrio de nudos usan la rigidez y las cargas condensadas.

## Cargas en barras

- Uniforme: por unidad de longitud real del miembro, en toda la longitud. La opción proyectada es F06-009.
- Las cargas en ejes globales se proyectan a ejes locales con la matriz de ejes de la barra.
- Cargas nodales equivalentes = −(reacciones de empotramiento exactas), con o sin deformación por cortante; los desplazamientos nodales son exactos para cargas uniformes y puntuales.
