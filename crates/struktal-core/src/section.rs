//! Properties of parametric sections (decision 0017).

use std::f64::consts::PI;

use crate::model::SectionShape;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionProperties {
    pub area: f64,
    pub i33: f64,
    pub i22: f64,
    pub j: f64,
    pub as2: f64,
    pub as3: f64,
}

/// Saint-Venant constant of a solid rectangle, a ≥ b, by the exact series:
/// J = a b³/3 [1 − (192/π⁵)(b/a) Σ tanh(nπa/(2b))/n⁵], n odd.
fn rectangle_torsion(p: f64, q: f64) -> f64 {
    let (a, b) = if p >= q { (p, q) } else { (q, p) };
    // Terms fall as 1/n⁵: well below f64 rounding long before n = 199.
    let sum: f64 = (0..100)
        .map(|k| {
            let n = f64::from(2 * k + 1);
            (n * PI * a / (2.0 * b)).tanh() / n.powi(5)
        })
        .sum();
    a * b.powi(3) / 3.0 * (1.0 - 192.0 / PI.powi(5) * (b / a) * sum)
}

impl SectionShape {
    /// Multiplies every dimension.
    pub fn scale(&mut self, f: f64) {
        match self {
            SectionShape::Rectangle { depth, width } => {
                *depth *= f;
                *width *= f;
            }
            SectionShape::Circle { diameter } => *diameter *= f,
            SectionShape::Pipe { diameter, wall } => {
                *diameter *= f;
                *wall *= f;
            }
            SectionShape::Box { depth, width, flange_t, web_t }
            | SectionShape::ISection { depth, width, flange_t, web_t } => {
                *depth *= f;
                *width *= f;
                *flange_t *= f;
                *web_t *= f;
            }
        }
    }

    /// Named dimensions, all of which must be finite and positive.
    pub fn dimensions(&self) -> Vec<(&'static str, f64)> {
        match *self {
            SectionShape::Rectangle { depth, width } => vec![("depth", depth), ("width", width)],
            SectionShape::Circle { diameter } => vec![("diameter", diameter)],
            SectionShape::Pipe { diameter, wall } => vec![("diameter", diameter), ("wall", wall)],
            SectionShape::Box { depth, width, flange_t, web_t }
            | SectionShape::ISection { depth, width, flange_t, web_t } => {
                vec![("depth", depth), ("width", width), ("flange_t", flange_t), ("web_t", web_t)]
            }
        }
    }

    /// What makes the geometry impossible, if anything.
    pub fn geometry_issue(&self) -> Option<String> {
        match *self {
            SectionShape::Pipe { diameter, wall } if 2.0 * wall >= diameter => {
                Some(format!("wall {wall} leaves no hole in diameter {diameter}"))
            }
            SectionShape::Box { depth, width, flange_t, web_t } if 2.0 * flange_t >= depth || 2.0 * web_t >= width => {
                Some("the walls leave no hole in the box".into())
            }
            SectionShape::ISection { depth, width, flange_t, web_t } if 2.0 * flange_t >= depth || web_t >= width => {
                Some("the flanges leave no web, or the web is not narrower than the flange".into())
            }
            _ => None,
        }
    }

    /// Exact area and inertias. Torsion is exact for rectangle, circle and
    /// pipe, and uses the thin-wall result for box and I. Shear areas are
    /// engineering rules, listed in decision 0017.
    pub fn properties(&self) -> SectionProperties {
        match *self {
            SectionShape::Rectangle { depth: h, width: b } => {
                let area = b * h;
                SectionProperties {
                    area,
                    i33: b * h.powi(3) / 12.0,
                    i22: h * b.powi(3) / 12.0,
                    j: rectangle_torsion(h, b),
                    as2: 5.0 / 6.0 * area,
                    as3: 5.0 / 6.0 * area,
                }
            }
            SectionShape::Circle { diameter: d } => {
                let area = PI * d * d / 4.0;
                let i = PI * d.powi(4) / 64.0;
                SectionProperties { area, i33: i, i22: i, j: 2.0 * i, as2: 0.9 * area, as3: 0.9 * area }
            }
            SectionShape::Pipe { diameter: d, wall: t } => {
                let inner = d - 2.0 * t;
                let area = PI * (d * d - inner * inner) / 4.0;
                let i = PI * (d.powi(4) - inner.powi(4)) / 64.0;
                SectionProperties { area, i33: i, i22: i, j: 2.0 * i, as2: 0.5 * area, as3: 0.5 * area }
            }
            SectionShape::Box { depth: h, width: b, flange_t: tf, web_t: tw } => {
                let (hi, bi) = (h - 2.0 * tf, b - 2.0 * tw);
                // Bredt, on the mid-wall rectangle.
                let (hm, bm) = (h - tf, b - tw);
                SectionProperties {
                    area: b * h - bi * hi,
                    i33: (b * h.powi(3) - bi * hi.powi(3)) / 12.0,
                    i22: (h * b.powi(3) - hi * bi.powi(3)) / 12.0,
                    j: 2.0 * tf * tw * bm * bm * hm * hm / (bm * tw + hm * tf),
                    as2: 2.0 * h * tw,
                    as3: 2.0 * b * tf,
                }
            }
            SectionShape::ISection { depth: h, width: b, flange_t: tf, web_t: tw } => {
                let hw = h - 2.0 * tf;
                SectionProperties {
                    area: 2.0 * b * tf + hw * tw,
                    i33: (b * h.powi(3) - (b - tw) * hw.powi(3)) / 12.0,
                    i22: (2.0 * tf * b.powi(3) + hw * tw.powi(3)) / 12.0,
                    // Open thin-wall section, on the mid-line length of the web.
                    j: (2.0 * b * tf.powi(3) + (h - tf) * tw.powi(3)) / 3.0,
                    as2: h * tw,
                    as3: 5.0 / 3.0 * b * tf,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) {
        assert!((a - b).abs() <= rel * a.abs().max(b.abs()), "{a} vs {b}");
    }

    /// Area and inertias of a union of rectangles (y along local 2, z along
    /// local 3), by the parallel-axis theorem about the origin.
    fn from_rectangles(parts: &[(f64, f64, f64, f64)]) -> (f64, f64, f64) {
        let mut out = (0.0, 0.0, 0.0);
        for &(h, b, y, z) in parts {
            let a = h * b;
            out.0 += a;
            out.1 += b * h.powi(3) / 12.0 + a * y * y;
            out.2 += h * b.powi(3) / 12.0 + a * z * z;
        }
        out
    }

    #[test]
    fn i_and_box_match_their_rectangles() {
        let (h, b, tf, tw) = (0.5, 0.2, 0.02, 0.01);
        let hw = h - 2.0 * tf;
        let p = SectionShape::ISection { depth: h, width: b, flange_t: tf, web_t: tw }.properties();
        let d = (h - tf) / 2.0;
        let (a, i33, i22) = from_rectangles(&[(tf, b, d, 0.0), (tf, b, -d, 0.0), (hw, tw, 0.0, 0.0)]);
        close(p.area, a, 1e-14);
        close(p.i33, i33, 1e-14);
        close(p.i22, i22, 1e-14);

        let p = SectionShape::Box { depth: h, width: b, flange_t: tf, web_t: tw }.properties();
        let w = (b - tw) / 2.0;
        let (a, i33, i22) = from_rectangles(&[(tf, b, d, 0.0), (tf, b, -d, 0.0), (hw, tw, 0.0, w), (hw, tw, 0.0, -w)]);
        close(p.area, a, 1e-14);
        close(p.i33, i33, 1e-14);
        close(p.i22, i22, 1e-14);
    }

    #[test]
    fn rectangle_torsion_matches_the_classic_table() {
        // J = β a b³, β from Timoshenko & Goodier for a/b = 1, 2, 10.
        for (ratio, beta) in [(1.0, 0.1406), (2.0, 0.229), (10.0, 0.312)] {
            let j = rectangle_torsion(ratio, 1.0);
            assert!((j / ratio - beta).abs() < 5e-4, "a/b = {ratio}: {}", j / ratio);
        }
        // The order of the sides does not matter.
        assert_eq!(rectangle_torsion(3.0, 1.0), rectangle_torsion(1.0, 3.0));
        // The slender limit is a b³/3.
        close(rectangle_torsion(1e4, 1.0), 1e4 / 3.0, 1e-3);
    }

    #[test]
    fn a_thin_pipe_tends_to_the_thin_wall_formulas() {
        let (d, t) = (1.0, 1e-4);
        let p = SectionShape::Pipe { diameter: d, wall: t }.properties();
        let r = (d - t) / 2.0;
        close(p.area, 2.0 * PI * r * t, 1e-6);
        close(p.i33, PI * r.powi(3) * t, 1e-6);
        close(p.j, 2.0 * PI * r.powi(3) * t, 1e-6);
        // A pipe with a vanishing hole is a circle.
        let hole = SectionShape::Pipe { diameter: d, wall: d / 2.0 * (1.0 - 1e-9) }.properties();
        let solid = SectionShape::Circle { diameter: d }.properties();
        close(hole.i33, solid.i33, 1e-12);
    }

    #[test]
    fn square_box_torsion_is_bredt() {
        // J = 4 A_m² / ∮ds/t for a uniform wall.
        let (s, t) = (1.0, 0.01);
        let p = SectionShape::Box { depth: s, width: s, flange_t: t, web_t: t }.properties();
        let m = s - t;
        close(p.j, 4.0 * (m * m).powi(2) / (4.0 * m / t), 1e-14);
    }

    #[test]
    fn geometry_issues_are_found() {
        assert!(SectionShape::Pipe { diameter: 1.0, wall: 0.5 }.geometry_issue().is_some());
        assert!(SectionShape::Box { depth: 1.0, width: 1.0, flange_t: 0.5, web_t: 0.1 }.geometry_issue().is_some());
        assert!(
            SectionShape::ISection { depth: 1.0, width: 0.1, flange_t: 0.1, web_t: 0.1 }.geometry_issue().is_some()
        );
        assert!(
            SectionShape::ISection { depth: 1.0, width: 0.5, flange_t: 0.1, web_t: 0.05 }.geometry_issue().is_none()
        );
    }
}
