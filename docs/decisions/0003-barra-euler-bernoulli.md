# 0003 — Barra 3D Euler–Bernoulli y convención de acciones

Fecha: 2026-10-29. Estado: aceptada.

Formulación: elemento de dos nudos, 6 DOF por nudo, axial lineal, torsión de Saint-Venant (G·J/L), flexión cúbica de Hermite en los planos 1-2 (I33) y 1-3 (I22). Sin deformación por cortante, offsets, liberaciones ni rigidez geométrica. G = E/(2(1+ν)) salvo `g` explícito.

Fuente: rigidez clásica de pórtico espacial (p. ej. Przemieniecki, *Theory of Matrix Structural Analysis*, 1968, cap. 5; McGuire, Gallagher y Ziemian, *Matrix Structural Analysis*, 2.ª ed.). Giros: r3 = dv/dx, r2 = −dw/dx.

Cargas: nodales equivalentes consistentes para uniforme y puntual. Acciones en estación por equilibrio del tramo [0, x] con las fuerzas de extremo I y las cargas de vano (no por derivación del campo interpolado), por lo que son exactas para estas cargas.

Convención de signos de estación: ver spec/units-and-conventions.md. Es convención propia; mapeo a SAP2000 pendiente de oráculo.

Pruebas: simetría, seis modos de cuerpo rígido sin fuerzas en barra oblicua, ortonormalidad de ejes, resultantes de cargas equivalentes, casos V-LIN-001..005 (errores relativos ≤ 1,5e-15).

Pendiente: Timoshenko (F02-003), liberaciones con condensación de fuerzas y cargas (F04-003), offsets (F04-004).
