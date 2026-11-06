# 0020 — Masas, modos y espectro de respuesta (F07, F09, F10)

Fecha: 2026-11-05. Estado: aceptada.

## Modelo

- `materials[].mass_density`, en fuerza·s²/longitud⁴, y `joints[].added_mass`, seis componentes en ejes globales.
- `load_cases[]`: `{"type": "Modal", "name", "modes"}`.
- `{"type": "ResponseSpectrum", "name", "modal_case", "function", "direction", "scale", "combination", "damping"}`, con `response_spectra[]` como tablas de período y aceleración. `scale` convierte los valores en longitud/s² y sigue las unidades del modelo. `Cqc` es la opción por defecto, con amortiguamiento 0,05.

Se añade al esquema v5, aún sin publicar. El modelo declara sus masas: la fuente de masa desde patrones llegó después, en la decisión 0022.

## Masa

Concentrada: la mitad de cada barra en cada extremo y, en las áreas, según lo que aporta cada esquina, siempre en traslaciones. La zona rígida de un offset no tiene masa. Es de la misma familia que la masa por defecto de otros programas, pero la equivalencia con SAP2000 está **oracle_pending**.

Se reduce como la rigidez, M_r = Cᵀ M C. Las masas de los nudos de un diafragma actúan sobre su referencia con su brazo, y por eso el piso tiene inercia polar sin declararla. V-MOD-002 lo comprueba con un modo de torsión.

## Modos

Iteración en subespacios, con matriz de masa semidefinida y la rigidez ya factorizada. Es determinista: su vector de partida no cambia entre ejecuciones.

- **Modos perdidos:** no hay recuento de Sturm, porque los backends solo factorizan matrices definidas positivas. En su lugar, un vector ortogonal a los modos hallados se itera, y su cociente de Rayleigh no puede quedar por debajo del mayor. No es tan fuerte como un recuento de Sturm, pero detecta lo habitual. Si falla, el caso termina en error.
- Si la iteración no converge, o un residuo supera 1e-6, el caso termina en error.
- Si hay menos grados de libertad con masa que modos pedidos, se dan los que existan y se avisa.

## Espectro

`u_i = φ_i Γ_i Sa(T_i)/ω_i²`, y cada resultado (desplazamiento, reacción, acción en estación, esfuerzo de área) se combina por componente con CQC (Der Kiureghian) o SRSS.

- **El resultado es una magnitud, sin signo.** Sus componentes no están en equilibrio entre sí, y por eso no hay `checks`.
- Fuera del rango de la función se usa el último valor, y se avisa.
- El mensaje del caso da la masa captada. **No hay corrección por masa faltante** (F10-003).
- **Con modos repetidos, SRSS no es invariante:** depende de la base que dé el cálculo. V-SPEC-001 lo mostró en un primer intento con columnas cuadradas. CQC sí lo trata bien, porque su correlación vale 1 entre modos iguales, y por eso es la opción por defecto.
- Modos y espectros no pueden entrar en combinaciones: el espectro no tiene signo y haría falta definir qué significa.

## Verificación

- `eigen.rs`: cadena de resortes contra su forma cerrada con ambos backends, ortonormalidad en M, grados de libertad sin masa.
- V-MOD-001: voladizo con masa en punta, flexión y axil.
- V-MOD-002: edificio de dos plantas con diafragmas, modos de traslación y de torsión en forma cerrada.
- V-MOD-003: viga con masa repartida, 16 tramos, contra la solución continua (tolerancia de malla, 0,5 %).
- V-SPEC-001: un modo (`u = Sa/ω²`, `V = M Sa`) y dos modos con SRSS y CQC.
- Comparación con OpenSeesPy de los períodos, con las mismas masas: V-MOD-001, V-MOD-002 y V-SPEC-001 coinciden, con una diferencia relativa de 1e-15 como máximo. V-MOD-003 no es comparable, porque el traductor no subdivide las barras, y eso cambia dónde quedan las masas. Los resultados de espectro no se comparan con otros motores.

## Pendiente

- Masa consistente (F07-002) y amortiguamiento distinto de un valor constante (F07-003).
- Vectores Ritz (F09-002), masa faltante (F10-003), combinación direccional (F10-002) y excentricidad accidental.
- Fuente de masa desde patrones e importación desde S2K: hechas en la decisión 0022.
- Equivalencia con SAP2000: oracle_pending.
