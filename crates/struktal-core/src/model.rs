//! Canonical physical model, schema v1.
//!
//! Entity names are free text (SAP2000 names are not assumed to be integers).
//! Values are stored in the declared `units` and converted to SI by
//! [`Model::to_si`] before validation turns them into an immutable snapshot.

use serde::{Deserialize, Serialize};

use crate::units::{Dim, UnitSystem};

/// v2 (2026-10-30): shear areas `as2`, `as3` on frame sections. v1 files are
/// migrated with both set to 0, which keeps their v1 meaning.
/// v3 (2026-11-05): rigid end offsets on frames, joint constraints, joint
/// springs and imposed joint displacements. Older files are migrated with
/// none of them (decisions 0010, 0011 and 0012).
/// v4 (2026-11-05): linearly varying and partial span loads, span moments.
/// Older files have no such loads, so they migrate unchanged (decision 0014).
/// v5 (2026-11-05): automatic subdivision of frames. Older files have none
/// (decision 0015).
pub const SCHEMA_VERSION: u32 = 5;

pub type Name = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub schema_version: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub units: UnitSystem,
    pub materials: Vec<Material>,
    pub sections: Vec<FrameSection>,
    pub joints: Vec<Joint>,
    #[serde(default)]
    pub frames: Vec<Frame>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub area_sections: Vec<AreaSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<Area>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub load_patterns: Vec<LoadPattern>,
    #[serde(default)]
    pub joint_loads: Vec<JointLoad>,
    #[serde(default)]
    pub joint_displacements: Vec<JointDisplacement>,
    #[serde(default)]
    pub frame_loads: Vec<FrameLoad>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub area_loads: Vec<AreaLoad>,
    #[serde(default)]
    pub load_cases: Vec<LoadCase>,
    #[serde(default)]
    pub combinations: Vec<Combination>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub name: Name,
    /// Young's modulus [force/length²].
    pub e: f64,
    /// Poisson's ratio; G = E / (2 (1 + nu)) unless `g` is given.
    pub nu: f64,
    /// Optional explicit shear modulus [force/length²].
    #[serde(default)]
    pub g: Option<f64>,
    /// Weight per unit volume [force/length³], used by self-weight.
    #[serde(default)]
    pub unit_weight: f64,
}

/// General frame section. Axis naming follows SAP2000: I33 is the inertia
/// about local axis 3 (bending in the 1-2 plane), I22 about local axis 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameSection {
    pub name: Name,
    pub material: Name,
    /// Parametric shape: validation derives every property below from it, so
    /// they must not be given as well (decision 0017).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<SectionShape>,
    /// With a shape: drop its automatic shear areas (Euler–Bernoulli).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub neglect_shear: bool,
    #[serde(default)]
    pub area: f64,
    #[serde(default)]
    pub i33: f64,
    #[serde(default)]
    pub i22: f64,
    /// Torsional constant, independent from the inertias.
    #[serde(default)]
    pub j: f64,
    /// Shear area for shear force V2 (bending in the 1-2 plane, with I33).
    /// 0 neglects shear deformation in that plane (decision 0007).
    #[serde(default)]
    pub as2: f64,
    /// Shear area for V3 (bending in the 1-3 plane, with I22); 0 neglects it.
    #[serde(default)]
    pub as3: f64,
}

/// Parametric section. `depth` is along local axis 2 and `width` along local
/// axis 3, so I33 is the strong-axis inertia of a section deeper than wide.
/// Dimensions in the model's length unit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum SectionShape {
    Rectangle {
        depth: f64,
        width: f64,
    },
    Circle {
        diameter: f64,
    },
    Pipe {
        diameter: f64,
        wall: f64,
    },
    Box {
        depth: f64,
        width: f64,
        flange_t: f64,
        web_t: f64,
    },
    /// Doubly symmetric I section.
    ISection {
        depth: f64,
        width: f64,
        flange_t: f64,
        web_t: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Joint {
    pub name: Name,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Restrained components [UX, UY, UZ, RX, RY, RZ] in global axes.
    #[serde(default)]
    pub restraint: [bool; 6],
    /// Grounded springs [UX, UY, UZ, RX, RY, RZ] in global axes; 0 = none.
    /// Units: force/length for translations, force·length/rad for rotations.
    #[serde(default)]
    pub springs: [f64; 6],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub name: Name,
    pub i: Name,
    pub j: Name,
    pub section: Name,
    /// Rotation of local axes 2 and 3 about local 1 [degrees], right-hand rule.
    #[serde(default)]
    pub angle_deg: f64,
    /// End releases and partial-fixity springs (decision 0008).
    #[serde(default)]
    pub releases: Option<EndReleases>,
    /// Rigid end offsets (decision 0010).
    #[serde(default)]
    pub offsets: Option<EndOffsets>,
    /// Automatic subdivision for analysis (decision 0015).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_mesh: Option<AutoMesh>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AreaBehavior {
    /// Membrane and plate.
    #[default]
    Shell,
    Membrane,
    Plate,
}

/// Homogeneous isotropic area section (decision 0019).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaSection {
    pub name: Name,
    pub material: Name,
    pub thickness: f64,
    /// Thickness for bending and transverse shear, if different.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bending_thickness: Option<f64>,
    #[serde(default)]
    pub behavior: AreaBehavior,
}

/// Flat four-node area, its joints counterclockwise seen from local axis 3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub name: Name,
    pub joints: [Name; 4],
    pub section: Name,
}

/// Uniform force per unit area. Local 3 is the normal; Gravity is global −Z.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaLoad {
    pub pattern: Name,
    pub area: Name,
    pub direction: LoadDirection,
    pub value: f64,
}

/// The frame stays one object in the model and in its results; the analysis
/// uses its pieces.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoMesh {
    /// Equal divisions of the flexible length.
    #[serde(default = "AutoMesh::one")]
    pub segments: u32,
    /// Also split at every joint on the flexible axis, connecting the frame there.
    #[serde(default)]
    pub at_joints: bool,
}

impl AutoMesh {
    fn one() -> u32 {
        1
    }
}

/// Vectors in global axes [length] from joints I and J to the corresponding
/// element ends. The flexible element, its local axes, its length and its
/// span loads lie between the two offset ends; the zones joint-to-end are
/// rigid and weightless.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndOffsets {
    #[serde(default)]
    pub i: [f64; 3],
    #[serde(default)]
    pub j: [f64; 3],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobalAxis {
    X,
    Y,
    #[default]
    Z,
}

impl GlobalAxis {
    pub fn index(self) -> usize {
        match self {
            GlobalAxis::X => 0,
            GlobalAxis::Y => 1,
            GlobalAxis::Z => 2,
        }
    }
}

/// Kinematic joint constraints, imposed by exact elimination (decision 0011).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Constraint {
    /// Rigid in-plane motion normal to `axis`. The first joint is the
    /// internal reference; results do not depend on the order.
    Diaphragm {
        name: Name,
        #[serde(default)]
        axis: GlobalAxis,
        joints: Vec<Name>,
    },
    /// All six components move as one rigid body with the first joint.
    Body { name: Name, joints: Vec<Name> },
    /// The selected global components [UX, UY, UZ, RX, RY, RZ] are equal in
    /// every joint, with no lever arm.
    Equal { name: Name, components: [bool; 6], joints: Vec<Name> },
}

impl Constraint {
    pub fn name(&self) -> &str {
        match self {
            Constraint::Diaphragm { name, .. } | Constraint::Body { name, .. } | Constraint::Equal { name, .. } => name,
        }
    }

    pub fn joints(&self) -> &[Name] {
        match self {
            Constraint::Diaphragm { joints, .. }
            | Constraint::Body { joints, .. }
            | Constraint::Equal { joints, .. } => joints,
        }
    }

    /// Reference rotations [RX, RY, RZ] that act through a lever arm on the
    /// tied translations: u = u_ref + θ × r with only these θ components.
    pub fn lever_rotations(&self) -> [bool; 3] {
        match self {
            Constraint::Diaphragm { axis, .. } => {
                let mut r = [false; 3];
                r[axis.index()] = true;
                r
            }
            Constraint::Body { .. } => [true; 3],
            Constraint::Equal { .. } => [false; 3],
        }
    }

    /// Joint components tied by the constraint [UX, UY, UZ, RX, RY, RZ].
    pub fn tied_components(&self) -> [bool; 6] {
        match self {
            Constraint::Diaphragm { axis, .. } => {
                let n = axis.index();
                let mut c = [true, true, true, false, false, false];
                c[n] = false;
                c[3 + n] = true;
                c
            }
            Constraint::Body { .. } => [true; 6],
            Constraint::Equal { components, .. } => *components,
        }
    }
}

/// Releases in local components [P, V2, V3, T, M2, M3] (= local DOF order
/// u1, u2, u3, r1, r2, r3) at ends I and J. A released component is joined
/// to the joint through a spring of the given stiffness: 0 is a full
/// release; > 0 is partial fixity. Springs on unreleased components are
/// invalid. Units: force/length for P, V2, V3; force·length/rad for T, M2, M3.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndReleases {
    #[serde(default)]
    pub i: [bool; 6],
    #[serde(default)]
    pub j: [bool; 6],
    #[serde(default)]
    pub i_springs: [f64; 6],
    #[serde(default)]
    pub j_springs: [f64; 6],
}

pub const RELEASE_COMPONENTS: [&str; 6] = ["P", "V2", "V3", "T", "M2", "M3"];

impl EndReleases {
    /// Components that are released with no spring, as 12 local DOFs.
    pub fn free(&self) -> [bool; 12] {
        let mut f = [false; 12];
        for c in 0..6 {
            f[c] = self.i[c] && self.i_springs[c] == 0.0;
            f[6 + c] = self.j[c] && self.j_springs[c] == 0.0;
        }
        f
    }

    /// Release combinations that leave the element itself with a rigid-body
    /// motion, so its released block cannot be condensed. Derived from the
    /// element's rigid modes restricted to the freely released DOFs: axial
    /// translation (P at both ends), twist (T at both ends), and in each
    /// bending plane transverse translation (V at both ends) or rotation
    /// (M at both ends with V at either end).
    pub fn unstable_combinations(&self) -> Vec<&'static str> {
        let f = self.free();
        let both = |c: usize| f[c] && f[6 + c];
        let either = |c: usize| f[c] || f[6 + c];
        let mut out = Vec::new();
        if both(0) {
            out.push("P released at both ends");
        }
        if both(3) {
            out.push("T released at both ends");
        }
        if both(1) {
            out.push("V2 released at both ends");
        }
        if both(2) {
            out.push("V3 released at both ends");
        }
        if both(5) && either(1) {
            out.push("M3 released at both ends with V2 released at an end");
        }
        if both(4) && either(2) {
            out.push("M2 released at both ends with V3 released at an end");
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadPattern {
    pub name: Name,
    /// Multiplier on material unit weight × area, acting in global −Z.
    #[serde(default)]
    pub self_weight_multiplier: f64,
}

/// Concentrated joint load in global axes: [FX, FY, FZ, MX, MY, MZ].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointLoad {
    pub pattern: Name,
    pub joint: Name,
    pub values: [f64; 6],
}

/// Imposed displacement in global axes [UX, UY, UZ, RX, RY, RZ], in length
/// and radians, scaled by the load pattern like a load. On a restrained
/// component it is the support movement; on a sprung component, the movement
/// of the grounded end of the spring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointDisplacement {
    pub pattern: Name,
    pub joint: Name,
    pub values: [f64; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadDirection {
    Local1,
    Local2,
    Local3,
    GlobalX,
    GlobalY,
    GlobalZ,
    /// Global −Z.
    Gravity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum FrameLoadKind {
    /// Uniform force per unit member length over the full length.
    Uniform { direction: LoadDirection, value: f64 },
    /// Concentrated force at relative distance `rel` ∈ [0, 1] from end I.
    Point { direction: LoadDirection, rel: f64, value: f64 },
    /// Force per unit member length varying linearly between relative
    /// distances `rel_start` < `rel_end`; zero elsewhere.
    Trapezoidal { direction: LoadDirection, rel_start: f64, rel_end: f64, value_start: f64, value_end: f64 },
    /// Concentrated moment about `direction` at relative distance `rel`.
    PointMoment { direction: LoadDirection, rel: f64, value: f64 },
    /// Moment per unit member length about `direction`, varying linearly.
    TrapezoidalMoment { direction: LoadDirection, rel_start: f64, rel_end: f64, value_start: f64, value_end: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameLoad {
    pub pattern: Name,
    pub frame: Name,
    pub load: FrameLoadKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaledPattern {
    pub pattern: Name,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum LoadCase {
    LinearStatic { name: Name, loads: Vec<ScaledPattern> },
}

impl LoadCase {
    pub fn name(&self) -> &str {
        match self {
            LoadCase::LinearStatic { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaledCase {
    pub case: Name,
    pub scale: f64,
}

/// SRSS, absolute and range combinations are pending (features F16-*).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Combination {
    LinearAdd {
        name: Name,
        cases: Vec<ScaledCase>,
    },
    /// Maximum and minimum of each result among its scaled terms, with the
    /// term that governs each one (decision 0016). Not a state of the structure,
    /// so it cannot be a term of another combination.
    Envelope {
        name: Name,
        cases: Vec<ScaledCase>,
    },
}

impl Combination {
    pub fn name(&self) -> &str {
        match self {
            Combination::LinearAdd { name, .. } | Combination::Envelope { name, .. } => name,
        }
    }

    pub fn terms(&self) -> &[ScaledCase] {
        match self {
            Combination::LinearAdd { cases, .. } | Combination::Envelope { cases, .. } => cases,
        }
    }
}

impl Model {
    /// Returns a copy with every dimensional value converted to SI and
    /// `units` set to SI. The original units are recorded by the caller.
    pub fn to_si(&self) -> Model {
        let u = self.units;
        let l = u.factor(Dim::LENGTH);
        let mut m = self.clone();
        m.units = UnitSystem::default();
        for mat in &mut m.materials {
            mat.e *= u.factor(Dim::STRESS);
            mat.g = mat.g.map(|g| g * u.factor(Dim::STRESS));
            mat.unit_weight *= u.factor(Dim::FORCE_PER_VOLUME);
        }
        for s in &mut m.sections {
            s.area *= u.factor(Dim::AREA);
            s.i33 *= u.factor(Dim::INERTIA);
            s.i22 *= u.factor(Dim::INERTIA);
            s.j *= u.factor(Dim::INERTIA);
            s.as2 *= u.factor(Dim::AREA);
            s.as3 *= u.factor(Dim::AREA);
            if let Some(shape) = &mut s.shape {
                shape.scale(l);
            }
        }
        for s in &mut m.area_sections {
            s.thickness *= l;
            s.bending_thickness = s.bending_thickness.map(|t| t * l);
        }
        for a in &mut m.area_loads {
            a.value *= u.factor(Dim::STRESS);
        }
        for j in &mut m.joints {
            j.x *= l;
            j.y *= l;
            j.z *= l;
            for k in 0..3 {
                j.springs[k] *= u.factor(Dim::FORCE_PER_LENGTH);
                j.springs[k + 3] *= u.factor(Dim::MOMENT);
            }
        }
        for f in &mut m.frames {
            if let Some(r) = &mut f.releases {
                for c in 0..6 {
                    let dim = if c < 3 { Dim::FORCE_PER_LENGTH } else { Dim::MOMENT };
                    r.i_springs[c] *= u.factor(dim);
                    r.j_springs[c] *= u.factor(dim);
                }
            }
            if let Some(o) = &mut f.offsets {
                for k in 0..3 {
                    o.i[k] *= l;
                    o.j[k] *= l;
                }
            }
        }
        for jl in &mut m.joint_loads {
            for k in 0..3 {
                jl.values[k] *= u.factor(Dim::FORCE);
                jl.values[k + 3] *= u.factor(Dim::MOMENT);
            }
        }
        for jd in &mut m.joint_displacements {
            for k in 0..3 {
                jd.values[k] *= l;
            }
        }
        for fl in &mut m.frame_loads {
            match &mut fl.load {
                FrameLoadKind::Uniform { value, .. } => *value *= u.factor(Dim::FORCE_PER_LENGTH),
                FrameLoadKind::Point { value, .. } => *value *= u.factor(Dim::FORCE),
                FrameLoadKind::Trapezoidal { value_start, value_end, .. } => {
                    *value_start *= u.factor(Dim::FORCE_PER_LENGTH);
                    *value_end *= u.factor(Dim::FORCE_PER_LENGTH);
                }
                FrameLoadKind::PointMoment { value, .. } => *value *= u.factor(Dim::MOMENT),
                // Moment per length has the dimension of a force.
                FrameLoadKind::TrapezoidalMoment { value_start, value_end, .. } => {
                    *value_start *= u.factor(Dim::FORCE);
                    *value_end *= u.factor(Dim::FORCE);
                }
            }
        }
        m
    }
}
