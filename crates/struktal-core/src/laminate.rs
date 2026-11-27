//! Laminated area sections: the equivalent stiffness of a stack of layers.
//!
//! A layered area is `n` layers through the thickness, each with its own

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::Name;

/// One layer of a laminated area, through the thickness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    /// Thickness of the layer, in the model's length unit.
    pub thickness: f64,
    pub material: Name,
}

/// The equivalent stiffness of a laminate, per unit width (decision 0032).
///
/// `a` is the extensional stiffness, `b` the bending–extension coupling and `d`

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaminateAbc {
    pub a: [[f64; 3]; 3],
    pub b: [[f64; 3]; 3],
    pub d: [[f64; 3]; 3],
}

/// The plane-stress reduced stiffness of one layer.
fn plane_stress(e: f64, nu: f64) -> [[f64; 3]; 3] {
    let f = e / (1.0 - nu * nu);
    [[f, f * nu, 0.0], [f * nu, f, 0.0], [0.0, 0.0, f * (1.0 - nu) / 2.0]]
}

/// The equivalent stiffness of the stack, per unit width.
///
/// `stiffness` gives `(E, nu)` for each material. The integration is
/// carried from the mid-plane of the total thickness, so a uniform stack has no
/// coupling however it is split into layers.
pub fn laminate_abc(layers: &[Layer], stiffness: &HashMap<Name, (f64, f64)>) -> Result<LaminateAbc, String> {
    if layers.is_empty() {
        return Err("a laminate needs at least one layer".to_string());
    }
    let total: f64 = layers.iter().map(|l| l.thickness).sum();
    if !total.is_finite() || total <= 0.0 {
        return Err("the total thickness must be positive and finite".to_string());
    }

    let mut a = [[0.0; 3]; 3];
    let mut b = [[0.0; 3]; 3];
    let mut d = [[0.0; 3]; 3];
    let mut z = -total / 2.0;
    for layer in layers {
        if !layer.thickness.is_finite() || layer.thickness <= 0.0 {
            return Err(format!("material {}: thickness must be positive and finite", layer.material));
        }
        let (e, nu) = stiffness.get(&layer.material).ok_or_else(|| format!("unknown material '{}'", layer.material))?;
        let q = plane_stress(*e, *nu);
        let z1 = z + layer.thickness;
        let t = layer.thickness;
        for i in 0..3 {
            for j in 0..3 {
                a[i][j] += q[i][j] * t;
                b[i][j] += q[i][j] * (z1 * z1 - z * z) / 2.0;
                d[i][j] += q[i][j] * (z1 * z1 * z1 - z * z * z) / 3.0;
            }
        }
        z = z1;
    }
    Ok(LaminateAbc { a, b, d })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn a_single_layer_is_the_homogeneous_section() {
        let layers = vec![Layer { thickness: 0.1, material: "S".into() }];
        let materials = HashMap::from([("S".to_string(), (2e11, 0.3))]);
        let abc = laminate_abc(&layers, &materials).expect("abc");
        let q = plane_stress(2e11, 0.3);
        assert!(close(abc.a[0][0], q[0][0] * 0.1));
        // D = Q t^3 / 12.
        assert!(close(abc.d[0][0], q[0][0] * 0.1f64.powi(3) / 12.0));
        // A symmetric single layer has no coupling.
        assert!(abc.b.iter().flatten().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn a_symmetric_stack_has_no_coupling() {
        let layers = vec![
            Layer { thickness: 0.05, material: "S".into() },
            Layer { thickness: 0.1, material: "S".into() },
            Layer { thickness: 0.05, material: "S".into() },
        ];
        let materials = HashMap::from([("S".to_string(), (2e11, 0.3))]);
        let abc = laminate_abc(&layers, &materials).expect("abc");
        assert!(abc.b.iter().flatten().all(|v| v.abs() < 1e-6));
        // A = Q * total thickness.
        let q = plane_stress(2e11, 0.3);
        assert!(close(abc.a[0][0], q[0][0] * 0.2));
    }

    #[test]
    fn a_uniform_stack_has_no_coupling_however_it_is_split() {
        // B is measured from the mid-plane, so a single material never couples:
        // the split into layers is not a material change.
        let layers =
            vec![Layer { thickness: 0.2, material: "S".into() }, Layer { thickness: 0.001, material: "S".into() }];
        let materials = HashMap::from([("S".to_string(), (2e11, 0.3))]);
        let abc = laminate_abc(&layers, &materials).expect("abc");
        assert!(abc.b[0][0].abs() < 1e-6, "{}", abc.b[0][0]);
    }

    #[test]
    fn an_unsymmetric_material_stack_couples_bending_and_extension() {
        // A stiff skin on one face and a soft core: the neutral axis moves, so
        // extension produces curvature. That is the coupling the element cannot
        // take today.
        let layers = vec![
            Layer { thickness: 0.19, material: "CORE".into() },
            Layer { thickness: 0.01, material: "SKIN".into() },
        ];
        let materials = HashMap::from([("CORE".to_string(), (1e10, 0.2)), ("SKIN".to_string(), (2e11, 0.3))]);
        let abc = laminate_abc(&layers, &materials).expect("abc");
        assert!(abc.b[0][0].abs() > 1.0, "expected coupling, got {}", abc.b[0][0]);
        // A symmetric version of the same stack has none.
        let symmetric = vec![
            Layer { thickness: 0.095, material: "CORE".into() },
            Layer { thickness: 0.01, material: "SKIN".into() },
            Layer { thickness: 0.095, material: "CORE".into() },
        ];
        let abc2 = laminate_abc(&symmetric, &materials).expect("abc");
        assert!(abc2.b[0][0].abs() < 1e-6, "{}", abc2.b[0][0]);
    }

    #[test]
    fn a_laminate_of_two_stiffnesses_is_the_sum_of_its_layers() {
        let layers = vec![
            Layer { thickness: 0.05, material: "CORE".into() },
            Layer { thickness: 0.05, material: "COVER".into() },
            Layer { thickness: 0.05, material: "CORE".into() },
            Layer { thickness: 0.05, material: "COVER".into() },
        ];
        let materials = HashMap::from([("CORE".to_string(), (3e10, 0.2)), ("COVER".to_string(), (2e11, 0.3))]);
        let abc = laminate_abc(&layers, &materials).expect("abc");
        // A = sum Q_i t_i, layer by layer.
        let qc = plane_stress(3e10, 0.2);
        let qs = plane_stress(2e11, 0.3);
        let want = 0.05 * (2.0 * qc[0][0] + 2.0 * qs[0][0]);
        assert!(close(abc.a[0][0], want), "{} vs {}", abc.a[0][0], want);
    }

    #[test]
    fn unknown_materials_and_bad_thicknesses_are_errors() {
        let materials = HashMap::from([("S".to_string(), (2e11, 0.3))]);
        assert!(laminate_abc(&[], &materials).is_err());
        assert!(laminate_abc(&[Layer { thickness: 0.0, material: "S".into() }], &materials).is_err());
        assert!(laminate_abc(&[Layer { thickness: 0.1, material: "NOPE".into() }], &materials).is_err());
    }
}
