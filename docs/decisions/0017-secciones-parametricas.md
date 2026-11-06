# 0017 — Secciones paramétricas (F02-002)

Fecha: 2026-11-05. Estado: aceptada. Completa lo que dejó pendiente la decisión 0007: As automático.

## Modelo

`sections[].shape`, con `depth` a lo largo del eje local 2 y `width` a lo largo del eje local 3, así que una sección más alta que ancha tiene su inercia mayor en I33:

- `Rectangle { depth, width }`
- `Circle { diameter }`
- `Pipe { diameter, wall }`
- `Box { depth, width, flange_t, web_t }`
- `ISection { depth, width, flange_t, web_t }`, doblemente simétrica

Con `shape` no se pueden dar también `area`, `i33`, `i22`, `j`, `as2` ni `as3`. `neglect_shear: true` quita las áreas de cortante automáticas. Se añade al esquema v5, aún sin publicar.

La validación sustituye la forma por sus propiedades: el snapshot, y por tanto su hash, contiene propiedades. Una forma y una sección general con las mismas propiedades son el mismo modelo analítico.

## Fórmulas

| Forma | A, I33, I22 | J | As2 | As3 |
|---|---|---|---|---|
| Rectangle | exactas | exacta (serie de Saint-Venant) | 5/6 A | 5/6 A |
| Circle | exactas | exacta (2 I) | 0,9 A | 0,9 A |
| Pipe | exactas | exacta (2 I) | 0,5 A | 0,5 A |
| Box | exactas | Bredt, pared delgada | 2 h t_w | 2 b t_f |
| ISection | exactas | sección abierta de pared delgada | h t_w | 5/3 b t_f |

**Las áreas de cortante y el J de pared delgada son reglas de ingeniería, no resultados exactos.** Dependen de la distribución de tensiones que se suponga, y otros programas usan valores algo distintos. Son la principal diferencia esperable frente a SAP2000: **oracle_pending, hay que capturarlas en el contraste con SAP2000**. Un perfil laminado real además tiene acuerdos, que estas fórmulas no incluyen.

## Verificación

- Unitaria: I y cajón contra una suma de rectángulos por ejes paralelos; J del rectángulo contra la tabla de Timoshenko y Goodier; tubo delgado contra sus fórmulas de pared delgada; cajón contra Bredt; validación de geometría.
- V-SEC-001: voladizos con cada forma, a axil, flexión en los dos planos con cortante y torsión. La referencia calcula área e inercias por otra vía, y las fija en el caso.
- Lo que este caso **no** prueba: que As y el J de pared delgada coincidan con SAP2000. Solo comprueba que el motor aplica estas reglas.
- No se compara con otros motores: el traductor tendría que repetir las mismas fórmulas.

## Pendiente

- I asimétrica, T, canal, ángulo, tablas de perfiles y section designer (F02-006).
- Secciones variables (F02-005) y modificadores (F02-004).
