# 0002 — Backend de solver inicial: skyline LDLᵀ propio

Fecha: 2026-10-29. Estado: aceptada como ruta inicial; **sustitución prevista**.

Contexto: la guía exige contrato explícito, sin densificar K, sin usar SPD sobre indefinidas, con diagnósticos.

Decisión: LDLᵀ de perfil (skyline) en Rust puro, con ordenación RCM, escalado diagonal simétrico y detección de pivotes nulos/negativos. Solo acepta `MatrixClass::Spd`; el resto devuelve `unsupported_backend`. Pivotes nulos → `Singular` con DOF identificados (mecanismo); negativos → `NotPositiveDefinite` con inercia.

Motivo: dependencia cero, comportamiento completamente auditado y diagnóstico de inercia directo para el subconjunto lineal SPD.

Consecuencias medidas (docs/benchmarks.md): el perfil crece de forma superlineal en edificios 3D; con 52 020 ecuaciones el factor ocupa 540 MB y tarda 56 s. No es adecuado para modelos grandes ni para matrices indefinidas.

Siguiente paso: prueba de viabilidad de `faer` (Cholesky dispersa supernodal, LDLᵀ con pivoteo) contra las mismas matrices de prueba (SPD, singular, casi singular, indefinida, no simétrica) antes de adoptarlo. El skyline se conserva como referencia de regresión.
