# 0008 — Liberaciones de extremo y fijación parcial (P05, pasos 1–2)

Fecha: 2026-10-30. Estado: aceptada.

## Modelo

`frames[].releases`: `i`, `j` (booleanos por componente local [P, V2, V3, T, M2, M3]) y `i_springs`, `j_springs` (rigidez del resorte de fijación parcial; 0 = liberación total). Los resortes van en fuerza/longitud para P, V2 y V3, y en fuerza·longitud/rad para T, M2 y M3, convertidos a SI en la frontera. Un resorte sobre una componente no liberada es un error de entrada.

Se añadió al esquema v2 sin subir la versión: v2 se creó en esta misma serie de cambios y no se ha publicado. Un lector v2 anterior rechaza el campo, por `deny_unknown_fields`; no lo ignora.

## Formulación (`fem-elements/src/release.rs`)

Cada componente liberada es un DOF interno del extremo del elemento, unido al DOF del nudo por un resorte k. Se forma el sistema aumentado [u (12), e (m)] y se condensa:

- K_c = K_uu − K_ue K_ee⁻¹ K_eu: rigidez sobre los nudos.
- f_c = f_u − K_ue K_ee⁻¹ f_e: **también se condensan las cargas de vano**, como pide la guía (P05 paso 1).
- e = K_ee⁻¹ (f_e − K_eu u): recuperación de los desplazamientos de extremo, para las fuerzas de extremo y las acciones en estaciones.

K_ee se factoriza con Cholesky. Si no es definida positiva, el error es explícito: sin pseudoinversa ni rigidez épsilon (P05 paso 2).

## Combinaciones inestables

Se rechazan en la validación, con mensaje por barra. Las reglas se derivan de los modos rígidos del elemento restringidos a los DOF liberados sin resorte:

- P en ambos extremos;
- T en ambos extremos;
- V2 (o V3) en ambos extremos;
- M3 en ambos extremos con V2 en alguno (y lo mismo para M2 con V3).

Un resorte k > 0 en la componente evita la inestabilidad. Las mismas reglas se comprueban numéricamente en el elemento, como red de seguridad.

## Verificación

- Elemento: rigidez empotrada-articulada 3EI/L³ y 3EI/L; cargas condensadas 5qL/8, qL²/8 y 3qL/8; resorte en serie L/EI + 1/k; resorte muy rígido = unión continua; cercha con solo rigidez axial; bloques inestables rechazados.
- V-REL-001: vigas con deformación por cortante entre nudos empotrados, con carga puntual y uniforme: articulada en J, con resorte rotacional en J (5e6 N·m/rad) y biapoyada por liberación. Referencia: método de las fuerzas por trabajos virtuales (resorte = flexibilidad 1/k) y estática. Errores relativos ≤ 4e-14; momentos en extremos liberados ≤ 5e-11 N·m.
- V-REL-002: cercha de dos barras construida con liberaciones. Referencia: equilibrio de nudo y Σ N n L / EA.
- V-MECH-006: voladizo con M3 liberado en el apoyo: nulidad 1 exacta, «moves 1/2 joints».
- Prueba de CLI: una combinación inestable termina con código 2 y mensaje «frame H1 / releases: …».

## Pendiente

- Equivalencia exacta con SAP2000 (nomenclatura de componentes, unidades de resortes, reglas de inestabilidad que aplica): oracle_pending.
- Offsets / zonas rígidas (F04-004), restricciones (F04-005) y masas (F07): resto de P05.
