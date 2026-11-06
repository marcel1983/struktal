//! Meaning of the S2K tables: from a parsed file to the canonical model.
//!
//! Strict by design. Each problem names table, line and cause, and all of them
//! are reported together. Tables with no effect on the analysis are listed in
//! `IGNORED`; any other table the importer does not know is a problem, because
//! it may change the structure.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use struktal_core::model::*;
use struktal_core::units::{ForceUnit, LengthUnit, UnitSystem};

use crate::parser::{Row, S2k};

/// Prefixes of tables that do not affect a linear static analysis of frames:
/// results, display, design data, and definitions nothing here refers to.
const IGNORED: &[&str] = &[
    "ANALYSIS MESSAGES",
    "ANALYSIS OPTIONS",
    "AREA SECTION PROPERTY",
    "ASSEMBLED JOINT MASSES",
    "AUTO WAVE",
    "BASE REACTIONS",
    "CABLE SECTION DEFINITIONS",
    // Only the cases of the types read here use them, and those tables are in READ.
    "CASE - ",
    "COORDINATE SYSTEMS",
    "DATABASE FORMAT TYPES",
    "ELEMENT FORCES",
    "ELEMENT JOINT FORCES",
    "ELEMENT STRAINS",
    "ELEMENT STRESSES",
    "FRAME DESIGN PROCEDURES",
    "FRAME LOAD TRANSFER OPTIONS",
    "FRAME OUTPUT STATION ASSIGNMENTS",
    "FRAME SECTION PROPERTIES 02 - CONCRETE COLUMN",
    "FRAME SECTION PROPERTIES 03 - CONCRETE BEAM",
    "FRAME SECTION PROPERTIES 13 - TIME DEPENDENT",
    "FUNCTION - ",
    "GRID LINES",
    "GROUPS ",
    "JOINT ACCELERATIONS",
    "JOINT DISPLACEMENTS",
    "JOINT PATTERN DEFINITIONS",
    "JOINT REACTIONS",
    "JOINT VELOCITIES",
    "LINK PROPERTY DEFINITIONS 01 - GENERAL",
    "MATERIAL LIST",
    "MATERIAL PROPERTIES 01 - GENERAL",
    "MATERIAL PROPERTIES 03",
    "MATERIAL PROPERTIES 04",
    "MATERIAL PROPERTIES 06",
    "MATERIAL PROPERTIES 09",
    "MODAL ",
    "NAMED SETS",
    "OPTIONS - ",
    "OVERWRITES - ",
    "PREFERENCES - ",
    "PROJECT INFORMATION",
    "REBAR SIZES",
    "RESPONSE SPECTRUM MODAL INFORMATION",
    "SOLID PROPERTY DEFINITIONS",
    "TABLES AUTOMATICALLY SAVED",
    "TENDON SECTION DEFINITIONS",
];

/// Tables this importer reads.
const READ: &[&str] = &[
    "ACTIVE DEGREES OF FREEDOM",
    "AREA LOADS - GRAVITY",
    "AREA LOADS - UNIFORM",
    "AREA SECTION ASSIGNMENTS",
    "AREA SECTION PROPERTIES",
    "CONNECTIVITY - AREA",
    "CASE - MODAL 1 - GENERAL",
    "CASE - RESPONSE SPECTRUM 1 - GENERAL",
    "CASE - RESPONSE SPECTRUM 2 - LOAD ASSIGNMENTS",
    "CASE - STATIC 1 - LOAD ASSIGNMENTS",
    "COMBINATION DEFINITIONS",
    "CONNECTIVITY - FRAME",
    "CONSTRAINT DEFINITIONS - BODY",
    "CONSTRAINT DEFINITIONS - DIAPHRAGM",
    "CONSTRAINT DEFINITIONS - EQUAL",
    "FRAME AUTO MESH ASSIGNMENTS",
    "FRAME INSERTION POINT ASSIGNMENTS",
    "FRAME LOADS - DISTRIBUTED",
    "FRAME LOADS - GRAVITY",
    "FRAME LOADS - POINT",
    "FRAME LOCAL AXES ASSIGNMENTS 1 - TYPICAL",
    "FRAME PROPERTY MODIFIERS",
    "FRAME RELEASE ASSIGNMENTS 1 - GENERAL",
    "FRAME RELEASE ASSIGNMENTS 2 - PARTIAL FIXITY",
    "FRAME SECTION ASSIGNMENTS",
    "FRAME SECTION PROPERTIES 01 - GENERAL",
    "FUNCTION - RESPONSE SPECTRUM - USER",
    "JOINT ADDED MASS ASSIGNMENTS",
    "JOINT CONSTRAINT ASSIGNMENTS",
    "JOINT COORDINATES",
    "JOINT LOADS - FORCE",
    "JOINT LOADS - GROUND DISPLACEMENT",
    "JOINT LOCAL AXES ASSIGNMENTS 1 - TYPICAL",
    "JOINT RESTRAINT ASSIGNMENTS",
    "JOINT SPRING ASSIGNMENTS 1 - UNCOUPLED",
    "LOAD CASE DEFINITIONS",
    "LOAD PATTERN DEFINITIONS",
    "MASS SOURCE",
    "MATERIAL PROPERTIES 02 - BASIC MECHANICAL PROPERTIES",
    "OBJECTS AND ELEMENTS - AREAS",
    "OBJECTS AND ELEMENTS - FRAMES",
    "OBJECTS AND ELEMENTS - JOINTS",
    "PROGRAM CONTROL",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub table: String,
    pub line: usize,
    pub cause: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (line {}): {}", self.table, self.line, self.cause)
    }
}

#[derive(Debug, Clone)]
pub struct Imported {
    pub model: Model,
    /// Tables skipped because they do not affect this analysis.
    pub ignored_tables: Vec<String>,
    /// What was left out on purpose, such as other analysis cases.
    pub notes: Vec<String>,
}

struct Importer<'a> {
    s2k: &'a S2k,
    problems: Vec<Problem>,
    notes: Vec<String>,
    /// Object name to the names of its elements, when the file has a mesh.
    area_elements: HashMap<String, Vec<String>>,
}

const YES: &str = "Yes";
/// The elements of an area must cover it to this fraction: flat elements on a
/// slightly warped object do not add up exactly to its projected area.
const AREA_COVER_TOL_REL: f64 = 0.01;
/// As `ON_AXIS_TOL_REL` of the analysis, which is what connects the joint.
const FRAME_AXIS_TOL_REL: f64 = 1e-6;
/// A spectrum and its case are taken to have the same damping below this.
const DAMPING_TOL: f64 = 1e-9;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// Normal times area of a polygon, by Newell's formula.
fn vector_area(p: &[[f64; 3]]) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (k, a) in p.iter().enumerate() {
        let b = p[(k + 1) % p.len()];
        out[0] += 0.5 * (a[1] * b[2] - a[2] * b[1]);
        out[1] += 0.5 * (a[2] * b[0] - a[0] * b[2]);
        out[2] += 0.5 * (a[0] * b[1] - a[1] * b[0]);
    }
    out
}

impl Importer<'_> {
    fn problem(&mut self, table: &str, line: usize, cause: impl Into<String>) {
        self.problems.push(Problem { table: table.to_string(), line, cause: cause.into() });
    }

    fn text(&mut self, table: &str, row: &Row, key: &str) -> Option<String> {
        let v = row.get(key).map(str::to_string);
        if v.is_none() {
            self.problem(table, row.line, format!("missing field {key}"));
        }
        v
    }

    fn opt_num(&mut self, table: &str, row: &Row, key: &str) -> Option<f64> {
        let raw = row.get(key)?;
        match raw.parse::<f64>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => {
                self.problem(table, row.line, format!("field {key} is not a number: '{raw}'"));
                None
            }
        }
    }

    /// A number with a default that means "no effect".
    fn num_or(&mut self, table: &str, row: &Row, key: &str, default: f64) -> f64 {
        self.opt_num(table, row, key).unwrap_or(default)
    }

    fn need_num(&mut self, table: &str, row: &Row, key: &str) -> f64 {
        if row.get(key).is_none() {
            self.problem(table, row.line, format!("missing field {key}"));
        }
        self.num_or(table, row, key, 0.0)
    }

    fn check_tables(&mut self) -> Vec<String> {
        let mut ignored = Vec::new();
        let names: Vec<(String, usize)> = self.s2k.tables.iter().map(|t| (t.name.clone(), t.line)).collect();
        for (name, line) in names {
            if READ.contains(&name.as_str()) {
                continue;
            }
            if IGNORED.iter().any(|p| name.starts_with(p)) {
                ignored.push(name);
            } else if !self.s2k.rows(&name).is_empty() {
                self.problem(&name, line, "table not supported: it may change the structure or the loads");
            }
        }
        ignored
    }

    fn units(&mut self) -> UnitSystem {
        const T: &str = "PROGRAM CONTROL";
        let Some(row) = self.s2k.rows(T).first() else {
            self.problem(T, 0, "the file has no PROGRAM CONTROL table, so its units are unknown");
            return UnitSystem::default();
        };
        let Some(text) = row.get("CurrUnits") else {
            self.problem(T, row.line, "missing field CurrUnits");
            return UnitSystem::default();
        };
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        let force = match parts.first().copied().unwrap_or("") {
            "N" => Some(ForceUnit::N),
            "KN" | "kN" => Some(ForceUnit::KN),
            "Kgf" | "kgf" => Some(ForceUnit::Kgf),
            "Tonf" | "tonf" | "Ton" => Some(ForceUnit::Tonf),
            "lb" | "Lb" | "lbf" => Some(ForceUnit::Lbf),
            "Kip" | "kip" => Some(ForceUnit::Kip),
            _ => None,
        };
        let length = match parts.get(1).copied().unwrap_or("") {
            "m" => Some(LengthUnit::M),
            "cm" => Some(LengthUnit::Cm),
            "mm" => Some(LengthUnit::Mm),
            "ft" => Some(LengthUnit::Ft),
            "in" => Some(LengthUnit::In),
            _ => None,
        };
        match (force, length) {
            (Some(force), Some(length)) => UnitSystem { force, length },
            _ => {
                self.problem(T, row.line, format!("units not supported: '{text}'"));
                UnitSystem::default()
            }
        }
    }

    fn materials(&mut self) -> Vec<Material> {
        const T: &str = "MATERIAL PROPERTIES 02 - BASIC MECHANICAL PROPERTIES";
        let mut out = Vec::new();
        for row in self.s2k.rows(T) {
            let Some(name) = self.text(T, row, "Material") else { continue };
            let e = self.need_num(T, row, "E1");
            let g = self.opt_num(T, row, "G12");
            // Uniaxial materials such as tendons have neither: a bar using one
            // would need G, so it is made invalid only if a section uses it.
            let nu = self.opt_num(T, row, "U12").unwrap_or(0.0);
            let unit_weight = self.num_or(T, row, "UnitWeight", 0.0);
            let mass_density = self.num_or(T, row, "UnitMass", 0.0);
            out.push(Material { name, e, nu, g, unit_weight, mass_density });
        }
        out
    }

    /// Section properties with their own modifiers applied. A stiffness modifier
    /// does not change weight or mass in the source format, so a modified area
    /// takes a material whose unit weight and mass compensate it.
    fn sections(&mut self, materials: &mut Vec<Material>) -> HashMap<String, (FrameSection, f64)> {
        const T: &str = "FRAME SECTION PROPERTIES 01 - GENERAL";
        // A file lists whole section libraries: only what the frames use matters,
        // whatever the rest contains.
        let assigned: Vec<&str> =
            self.s2k.rows("FRAME SECTION ASSIGNMENTS").iter().filter_map(|r| r.get("AnalSect")).collect();
        let mut out = HashMap::new();
        for row in self.s2k.rows(T) {
            let Some(name) = self.text(T, row, "SectionName") else { continue };
            if !assigned.contains(&name.as_str()) {
                continue;
            }
            let Some(material) = self.text(T, row, "Material") else { continue };
            if self.num_or(T, row, "I23", 0.0) != 0.0 {
                self.problem(
                    T,
                    row.line,
                    format!("section {name}: I23 is not zero, and there is no product of inertia"),
                );
            }
            for (key, label) in [("EccV2", "EccV2"), ("EccV3", "EccV3")] {
                if self.num_or(T, row, key, 0.0) != 0.0 {
                    self.problem(T, row.line, format!("section {name}: shear center offset {label} is not supported"));
                }
            }
            let a_mod = self.num_or(T, row, "AMod", 1.0);
            let w_mod = self.num_or(T, row, "WMod", 1.0);
            let section = FrameSection {
                name: name.clone(),
                material: material.clone(),
                shape: None,
                neglect_shear: false,
                area: self.need_num(T, row, "Area") * a_mod,
                i33: self.need_num(T, row, "I33") * self.num_or(T, row, "I3Mod", 1.0),
                i22: self.need_num(T, row, "I22") * self.num_or(T, row, "I2Mod", 1.0),
                j: self.need_num(T, row, "TorsConst") * self.num_or(T, row, "JMod", 1.0),
                as2: self.num_or(T, row, "AS2", 0.0) * self.num_or(T, row, "A2Mod", 1.0),
                as3: self.num_or(T, row, "AS3", 0.0) * self.num_or(T, row, "A3Mod", 1.0),
            };
            let m_mod = self.num_or(T, row, "MMod", 1.0);
            let section = with_unit_factors(section, w_mod / a_mod, m_mod / a_mod, materials);
            out.insert(name, (section, a_mod));
        }
        out
    }

    fn joints(&mut self, active: [bool; 6]) -> (Vec<Joint>, BTreeMap<String, usize>) {
        const T: &str = "JOINT COORDINATES";
        let mut joints = Vec::new();
        let mut index = BTreeMap::new();
        for row in self.s2k.rows(T) {
            let Some(name) = self.text(T, row, "Joint") else { continue };
            let xyz = if row.get("GlobalX").is_some() {
                [self.need_num(T, row, "GlobalX"), self.need_num(T, row, "GlobalY"), self.need_num(T, row, "GlobalZ")]
            } else {
                if row.get("CoordSys").is_some_and(|c| c != "GLOBAL")
                    || row.get("CoordType").is_some_and(|c| c != "Cartesian")
                {
                    self.problem(
                        T,
                        row.line,
                        format!("joint {name}: only global Cartesian coordinates, or GlobalX/Y/Z"),
                    );
                }
                [self.need_num(T, row, "XorR"), self.need_num(T, row, "Y"), self.need_num(T, row, "Z")]
            };
            index.insert(name.clone(), joints.len());
            joints.push(Joint {
                name,
                x: xyz[0],
                y: xyz[1],
                z: xyz[2],
                restraint: active.map(|a| !a),
                springs: [0.0; 6],
                added_mass: [0.0; 6],
            });
        }

        const R: &str = "JOINT RESTRAINT ASSIGNMENTS";
        for row in self.s2k.rows(R) {
            let Some(name) = self.text(R, row, "Joint") else { continue };
            let Some(&j) = index.get(&name) else {
                self.problem(R, row.line, format!("unknown joint {name}"));
                continue;
            };
            for (d, key) in ["U1", "U2", "U3", "R1", "R2", "R3"].into_iter().enumerate() {
                joints[j].restraint[d] |= row.get(key) == Some(YES);
            }
        }

        const L: &str = "JOINT LOCAL AXES ASSIGNMENTS 1 - TYPICAL";
        for row in self.s2k.rows(L) {
            let turned = ["AngleA", "AngleB", "AngleC"].iter().any(|k| self.num_or(L, row, k, 0.0) != 0.0);
            if turned || row.get("AdvanceAxes") == Some(YES) {
                self.problem(L, row.line, "joint local axes are not supported");
            }
        }

        const S: &str = "JOINT SPRING ASSIGNMENTS 1 - UNCOUPLED";
        for row in self.s2k.rows(S) {
            let Some(name) = self.text(S, row, "Joint") else { continue };
            let Some(&j) = index.get(&name) else {
                self.problem(S, row.line, format!("unknown joint {name}"));
                continue;
            };
            for (d, key) in ["U1", "U2", "U3", "R1", "R2", "R3"].into_iter().enumerate() {
                joints[j].springs[d] = self.num_or(S, row, key, 0.0);
            }
        }

        const M: &str = "JOINT ADDED MASS ASSIGNMENTS";
        for row in self.s2k.rows(M) {
            let Some(name) = self.text(M, row, "Joint") else { continue };
            let Some(&j) = index.get(&name) else {
                self.problem(M, row.line, format!("unknown joint {name}"));
                continue;
            };
            // Turned joint axes are refused above, so its local axes are the global ones.
            if row.get("CoordSys").is_some_and(|c| c != "GLOBAL" && c != "Joint Local") {
                self.problem(M, row.line, format!("joint {name}: added mass only in global axes"));
            }
            for (d, key) in ["Mass1", "Mass2", "Mass3", "MMI1", "MMI2", "MMI3"].into_iter().enumerate() {
                joints[j].added_mass[d] += self.num_or(M, row, key, 0.0);
            }
        }
        (joints, index)
    }

    fn frames(
        &mut self,
        sections: &mut HashMap<String, (FrameSection, f64)>,
        materials: &mut Vec<Material>,
    ) -> Vec<Frame> {
        const C: &str = "CONNECTIVITY - FRAME";
        const A: &str = "FRAME SECTION ASSIGNMENTS";
        let mut assigned: HashMap<String, String> = HashMap::new();
        for row in self.s2k.rows(A) {
            let (Some(f), Some(sec)) = (self.text(A, row, "Frame"), self.text(A, row, "AnalSect")) else { continue };
            if row.get("MatProp").is_some_and(|m| m != "Default") {
                self.problem(A, row.line, format!("frame {f}: material overwrite is not supported"));
            }
            if row.get("NPSectType").is_some_and(|n| n != "Default") {
                self.problem(A, row.line, format!("frame {f}: nonprismatic section is not supported"));
            }
            assigned.insert(f, sec);
        }
        let mut frames: Vec<Frame> = Vec::new();
        let mut index = HashMap::new();
        for row in self.s2k.rows(C) {
            let (Some(name), Some(i), Some(j)) =
                (self.text(C, row, "Frame"), self.text(C, row, "JointI"), self.text(C, row, "JointJ"))
            else {
                continue;
            };
            if row.get("IsCurved") == Some(YES) {
                self.problem(C, row.line, format!("frame {name}: curved frames are not supported"));
            }
            let Some(section) = assigned.get(&name).cloned() else {
                self.problem(C, row.line, format!("frame {name} has no section assignment"));
                continue;
            };
            index.insert(name.clone(), frames.len());
            frames.push(Frame { name, i, j, section, angle_deg: 0.0, releases: None, offsets: None, auto_mesh: None });
        }

        const LA: &str = "FRAME LOCAL AXES ASSIGNMENTS 1 - TYPICAL";
        for row in self.s2k.rows(LA) {
            let Some(f) = self.frame_of(LA, row, &index) else { continue };
            if row.get("AdvanceAxes") == Some(YES) {
                self.problem(LA, row.line, "advanced frame local axes are not supported");
            }
            frames[f].angle_deg = self.num_or(LA, row, "Angle", 0.0);
        }

        const REL: &str = "FRAME RELEASE ASSIGNMENTS 1 - GENERAL";
        const COMPONENTS: [&str; 6] = ["P", "V2", "V3", "T", "M2", "M3"];
        for row in self.s2k.rows(REL) {
            let Some(f) = self.frame_of(REL, row, &index) else { continue };
            let mut r = EndReleases::default();
            for (d, c) in COMPONENTS.into_iter().enumerate() {
                r.i[d] = row.get(&format!("{c}I")) == Some(YES);
                r.j[d] = row.get(&format!("{c}J")) == Some(YES);
            }
            if r != EndReleases::default() {
                frames[f].releases = Some(r);
            }
        }
        const PF: &str = "FRAME RELEASE ASSIGNMENTS 2 - PARTIAL FIXITY";
        for row in self.s2k.rows(PF) {
            let Some(f) = self.frame_of(PF, row, &index) else { continue };
            let mut springs = ([0.0; 6], [0.0; 6]);
            for (d, c) in COMPONENTS.into_iter().enumerate() {
                springs.0[d] = self.num_or(PF, row, &format!("{c}I"), 0.0);
                springs.1[d] = self.num_or(PF, row, &format!("{c}J"), 0.0);
            }
            if springs != ([0.0; 6], [0.0; 6]) {
                let r = frames[f].releases.get_or_insert_with(EndReleases::default);
                (r.i_springs, r.j_springs) = springs;
            }
        }

        const INS: &str = "FRAME INSERTION POINT ASSIGNMENTS";
        for row in self.s2k.rows(INS) {
            let Some(f) = self.frame_of(INS, row, &index) else { continue };
            // The centroid is point 10; any other point needs the section outline.
            if row.get("CardinalPt").is_some_and(|c| !c.starts_with("10")) {
                self.problem(
                    INS,
                    row.line,
                    format!("frame {}: cardinal point other than the centroid", frames[f].name),
                );
            }
            let read = |me: &mut Self, keys: [&str; 3]| keys.map(|k| me.num_or(INS, row, k, 0.0));
            let oi = read(self, ["JtOffsetXI", "JtOffsetYI", "JtOffsetZI"]);
            let oj = read(self, ["JtOffsetXJ", "JtOffsetYJ", "JtOffsetZJ"]);
            if oi != [0.0; 3] || oj != [0.0; 3] {
                if row.get("CoordSys").is_some_and(|c| c != "GLOBAL") {
                    self.problem(INS, row.line, format!("frame {}: joint offsets in local axes", frames[f].name));
                }
                frames[f].offsets = Some(EndOffsets { i: oi, j: oj });
            }
        }

        const MESH: &str = "FRAME AUTO MESH ASSIGNMENTS";
        for row in self.s2k.rows(MESH) {
            let Some(f) = self.frame_of(MESH, row, &index) else { continue };
            if row.get("AutoMesh") != Some(YES) {
                continue;
            }
            let at_joints = row.get("AtJoints") == Some(YES);
            let segments = self.num_or(MESH, row, "NumSegments", 0.0);
            if row.get("AtFrames") == Some(YES) || row.get("AtShell") == Some(YES) {
                self.notes.push(format!("frame {}: mesh at crossing frames or shells is not imported", frames[f].name));
            }
            if at_joints || segments > 1.0 {
                frames[f].auto_mesh = Some(AutoMesh { segments: (segments.max(1.0)) as u32, at_joints });
            }
        }

        const MODS: &str = "FRAME PROPERTY MODIFIERS";
        for row in self.s2k.rows(MODS) {
            let Some(f) = self.frame_of(MODS, row, &index) else { continue };
            let name = frames[f].name.clone();
            let Some((base, _)) = sections.get(&frames[f].section).cloned() else { continue };
            let fa = self.num_or(MODS, row, "AMod", 1.0) * self.num_or(MODS, row, "EAModifier", 1.0);
            let f2 = self.num_or(MODS, row, "I22Mod", 1.0) * self.num_or(MODS, row, "EIModifier", 1.0);
            let f3 = self.num_or(MODS, row, "I33Mod", 1.0) * self.num_or(MODS, row, "EIModifier", 1.0);
            let fj = self.num_or(MODS, row, "JMod", 1.0);
            let fs2 = self.num_or(MODS, row, "AS2Mod", 1.0);
            let fs3 = self.num_or(MODS, row, "AS3Mod", 1.0);
            let fw = self.num_or(MODS, row, "WeightMod", 1.0);
            let fm = self.num_or(MODS, row, "MassMod", 1.0);
            if [fa, f2, f3, fj, fs2, fs3, fw, fm] == [1.0; 8] {
                continue;
            }
            let own = format!("{}#{name}", base.name);
            let modified = FrameSection {
                name: own.clone(),
                area: base.area * fa,
                i33: base.i33 * f3,
                i22: base.i22 * f2,
                j: base.j * fj,
                as2: base.as2 * fs2,
                as3: base.as3 * fs3,
                ..base
            };
            let modified = with_unit_factors(modified, fw / fa, fm / fa, materials);
            sections.insert(own.clone(), (modified, fa));
            frames[f].section = own;
        }
        frames
    }

    /// The elements of the analysis mesh when the file has them, else its objects.
    fn areas(
        &mut self,
        joints: &mut Vec<Joint>,
        index: &mut BTreeMap<String, usize>,
        active: [bool; 6],
    ) -> (Vec<AreaSection>, Vec<Area>) {
        const C: &str = "CONNECTIVITY - AREA";
        const A: &str = "AREA SECTION ASSIGNMENTS";
        const P: &str = "AREA SECTION PROPERTIES";
        let mut assigned: HashMap<String, String> = HashMap::new();
        for row in self.s2k.rows(A) {
            let (Some(area), Some(section)) = (self.text(A, row, "Area"), self.text(A, row, "Section")) else {
                continue;
            };
            assigned.insert(area, section);
        }
        let mut objects = Vec::new();
        for row in self.s2k.rows(C) {
            let Some(name) = self.text(C, row, "Area") else { continue };
            let count = row.get("NumJoints").map_or(Some(4), |n| n.parse::<usize>().ok()).filter(|n| *n >= 3);
            let Some(count) = count else {
                self.problem(C, row.line, format!("area {name}: invalid NumJoints"));
                continue;
            };
            let found: Vec<Option<String>> = (1..=count).map(|k| self.text(C, row, &format!("Joint{k}"))).collect();
            let Some(area_joints) = found.into_iter().collect::<Option<Vec<String>>>() else { continue };
            let Some(section) = assigned.get(&name).cloned() else {
                self.problem(C, row.line, format!("area {name} has no section assignment"));
                continue;
            };
            objects.push(Area { name, joints: area_joints, section });
        }
        let areas = self.area_mesh(objects, joints, index, active);
        let used: Vec<String> = areas.iter().map(|a: &Area| a.section.clone()).collect();
        let mut sections = Vec::new();
        for row in self.s2k.rows(P) {
            let Some(name) = self.text(P, row, "Section") else { continue };
            if !used.contains(&name) {
                continue;
            }
            let behavior = match row.get("Type").unwrap_or("Shell-Thin") {
                "Shell-Thin" | "Shell-Thick" => AreaBehavior::Shell,
                "Membrane" => AreaBehavior::Membrane,
                "Plate-Thin" | "Plate-Thick" => AreaBehavior::Plate,
                other => {
                    self.problem(P, row.line, format!("area section {name}: type {other} is not supported"));
                    continue;
                }
            };
            if row.get("Type").is_some_and(|t| t.ends_with("-Thin")) {
                self.notes.push(format!(
                    "area section {name}: thin formulation read as this element, which includes shear and tends to it when thin"
                ));
            }
            if row.get("AreaType").is_some_and(|t| t != "Shell") {
                self.problem(P, row.line, format!("area section {name}: only shells, membranes and plates"));
            }
            for key in ["F11Mod", "F22Mod", "F12Mod", "M11Mod", "M22Mod", "M12Mod", "V13Mod", "V23Mod", "MMod", "WMod"]
            {
                if self.num_or(P, row, key, 1.0) != 1.0 {
                    self.problem(P, row.line, format!("area section {name}: modifier {key} is not supported"));
                }
            }
            let Some(material) = self.text(P, row, "Material") else { continue };
            if row.get("MatAngle").is_some_and(|a| a.parse::<f64>().is_ok_and(|v| v != 0.0)) {
                self.problem(P, row.line, format!("area section {name}: material angle is not supported"));
            }
            let thickness = self.need_num(P, row, "Thickness");
            let bending = self.opt_num(P, row, "BendThick").filter(|t| *t != thickness);
            sections.push(AreaSection { name, material, thickness, bending_thickness: bending, behavior });
        }
        (sections, areas)
    }

    /// Replaces the objects by their elements. Joints inside a mesh do not exist
    /// as objects, so they are added free, as they are in the file's analysis.
    fn area_mesh(
        &mut self,
        objects: Vec<Area>,
        joints: &mut Vec<Joint>,
        index: &mut BTreeMap<String, usize>,
        active: [bool; 6],
    ) -> Vec<Area> {
        const C: &str = "CONNECTIVITY - AREA";
        const E: &str = "OBJECTS AND ELEMENTS - AREAS";
        if self.s2k.rows(E).is_empty() {
            for o in objects.iter().filter(|o| o.joints.len() > 4) {
                self.problem(
                    C,
                    0,
                    format!("area {} has {} joints: it needs the table {E} of its mesh", o.name, o.joints.len()),
                );
            }
            return objects;
        }

        let mut by_object: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::new();
        for row in self.s2k.rows(E) {
            let (Some(element), Some(object)) = (self.text(E, row, "AreaElem"), self.text(E, row, "AreaObject")) else {
                continue;
            };
            if !objects.iter().any(|o| o.name == object) {
                self.problem(E, row.line, format!("element {element}: unknown area {object}"));
                continue;
            }
            let (Some(a), Some(b), Some(c)) =
                (self.text(E, row, "ElemJt1"), self.text(E, row, "ElemJt2"), self.text(E, row, "ElemJt3"))
            else {
                continue;
            };
            let mut corners = vec![a, b, c];
            corners.extend(row.get("ElemJt4").filter(|d| !d.is_empty()).map(str::to_string));
            by_object.entry(object).or_default().push((element, corners));
        }

        let mut out = Vec::new();
        let mut meshed = 0;
        for o in objects {
            let Some(elements) = by_object.remove(&o.name) else {
                self.problem(E, 0, format!("area {} has no elements in this table", o.name));
                continue;
            };
            for (_, corners) in &elements {
                for name in corners {
                    self.add_mesh_joint(name, joints, index, active);
                }
            }
            let at = |names: &[String]| -> Option<Vec<[f64; 3]>> {
                names
                    .iter()
                    .map(|n| {
                        index.get(n).map(|&k| {
                            let j = &joints[k];
                            [j.x, j.y, j.z]
                        })
                    })
                    .collect()
            };
            if let Some(whole) = at(&o.joints).map(|p| vector_area(&p)) {
                let mut sum = 0.0;
                for (element, corners) in &elements {
                    let Some(part) = at(corners).map(|p| vector_area(&p)) else { continue };
                    sum += norm(part);
                    if dot(part, whole) <= 0.0 {
                        self.problem(E, 0, format!("element {element} faces against its area {}", o.name));
                    }
                }
                // Its loads go by area: a mesh that does not cover the object would lose them.
                if (sum - norm(whole)).abs() > AREA_COVER_TOL_REL * norm(whole) {
                    self.problem(
                        E,
                        0,
                        format!("the elements of area {} cover {sum:e}, and the area is {:e}", o.name, norm(whole)),
                    );
                }
            }
            if elements.len() > 1 {
                meshed += 1;
            }
            self.area_elements.insert(o.name.clone(), elements.iter().map(|(n, _)| n.clone()).collect());
            out.extend(elements.into_iter().map(|(name, corners)| Area {
                name,
                joints: corners,
                section: o.section.clone(),
            }));
        }
        if meshed > 0 {
            self.notes.push(format!(
                "{meshed} areas are meshed: {} elements, named as in the table {E}; loads go to every element, and results are by element",
                out.len()
            ));
        }
        out
    }

    fn add_mesh_joint(
        &mut self,
        name: &str,
        joints: &mut Vec<Joint>,
        index: &mut BTreeMap<String, usize>,
        active: [bool; 6],
    ) {
        const J: &str = "OBJECTS AND ELEMENTS - JOINTS";
        if index.contains_key(name) {
            return;
        }
        let Some(row) = self.s2k.rows(J).iter().find(|r| r.get("JointElem") == Some(name)) else {
            self.problem(J, 0, format!("mesh joint {name} is not in this table"));
            return;
        };
        if row.get("JointObject").is_some_and(|o| !o.is_empty()) {
            self.problem(J, row.line, format!("mesh joint {name} has another name as an object: not supported"));
            return;
        }
        let xyz =
            [self.need_num(J, row, "GlobalX"), self.need_num(J, row, "GlobalY"), self.need_num(J, row, "GlobalZ")];
        index.insert(name.to_string(), joints.len());
        joints.push(Joint {
            name: name.to_string(),
            x: xyz[0],
            y: xyz[1],
            z: xyz[2],
            restraint: active.map(|a| !a),
            springs: [0.0; 6],
            added_mass: [0.0; 6],
        });
    }

    /// A frame the file cuts at a joint of an area mesh is connected there, and
    /// this model connects it by cutting at the joints on its axis. Without this
    /// a beam would pass by the edge of its slab with no connection.
    fn connect_frames_to_mesh(&mut self, frames: &mut [Frame], joints: &[Joint], areas: &[Area]) {
        const E: &str = "OBJECTS AND ELEMENTS - AREAS";
        const T: &str = "OBJECTS AND ELEMENTS - FRAMES";
        if self.s2k.rows(E).is_empty() {
            return;
        }
        let in_mesh: HashSet<&str> = areas.iter().flat_map(|a| a.joints.iter().map(String::as_str)).collect();
        let position = |name: &str| joints.iter().find(|j| j.name == name).map(|j| [j.x, j.y, j.z]);
        if self.s2k.rows(T).is_empty() && !frames.is_empty() {
            self.problem(T, 0, "a model with frames and an area mesh needs this table, to know where they connect");
            return;
        }
        let mut cut = Vec::new();
        for row in self.s2k.rows(T) {
            let (Some(object), Some(i), Some(j)) =
                (self.text(T, row, "FrameObject"), self.text(T, row, "ElemJtI"), self.text(T, row, "ElemJtJ"))
            else {
                continue;
            };
            let Some(f) = frames.iter_mut().find(|f| f.name == object) else {
                self.problem(T, row.line, format!("unknown frame {object}"));
                continue;
            };
            for name in [&i, &j] {
                if *name == f.i || *name == f.j || !in_mesh.contains(name.as_str()) {
                    continue;
                }
                if f.offsets.is_some() {
                    self.problem(T, row.line, format!("frame {object} has offsets and connects to an area mesh"));
                    continue;
                }
                let (Some(p), Some(a), Some(b)) = (position(name), position(&f.i), position(&f.j)) else { continue };
                let axis = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let length = norm(axis);
                let d = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
                let along = dot(d, axis) / length;
                let off = norm([0, 1, 2].map(|k| d[k] - along * axis[k] / length));
                if off > FRAME_AXIS_TOL_REL * length {
                    self.problem(T, row.line, format!("frame {object}: joint {name} is {off:e} off its axis"));
                    continue;
                }
                if !f.auto_mesh.is_some_and(|m| m.at_joints) {
                    let segments = f.auto_mesh.map_or(1, |m| m.segments);
                    f.auto_mesh = Some(AutoMesh { segments, at_joints: true });
                    cut.push(object.clone());
                }
            }
        }
        if !cut.is_empty() {
            self.notes.push(format!("{} frames are cut at the joints of the area mesh, as the file does", cut.len()));
        }
    }

    fn area_loads(&mut self, m: &mut Model) {
        const U: &str = "AREA LOADS - UNIFORM";
        for row in self.s2k.rows(U) {
            let (Some(area), Some(pattern)) = (self.text(U, row, "Area"), self.text(U, row, "LoadPat")) else {
                continue;
            };
            let direction = match (row.get("CoordSys").unwrap_or("GLOBAL"), row.get("Dir").unwrap_or("Gravity")) {
                ("Local", "3" | "Local 3") => LoadDirection::Local3,
                ("Local", "1" | "Local 1") => LoadDirection::Local1,
                ("Local", "2" | "Local 2") => LoadDirection::Local2,
                (_, "X") => LoadDirection::GlobalX,
                (_, "Y") => LoadDirection::GlobalY,
                (_, "Z") => LoadDirection::GlobalZ,
                (_, "Gravity") => LoadDirection::Gravity,
                (cs, dir) => {
                    self.problem(U, row.line, format!("area load direction {dir} in {cs} is not supported"));
                    continue;
                }
            };
            let value = self.need_num(U, row, "UnifLoad");
            for area in self.elements_of(&area) {
                m.area_loads.push(AreaLoad { pattern: pattern.clone(), area, direction, value });
            }
        }
        const G: &str = "AREA LOADS - GRAVITY";
        for row in self.s2k.rows(G) {
            let (Some(area), Some(pattern)) = (self.text(G, row, "Area"), self.text(G, row, "LoadPat")) else {
                continue;
            };
            let elements = self.elements_of(&area);
            let Some(sec) = m.areas.iter().find(|a| a.name == elements[0]).map(|a| a.section.clone()) else {
                self.problem(G, row.line, format!("unknown area {area}"));
                continue;
            };
            let w =
                m.area_sections.iter().find(|s| s.name == sec).and_then(|s| {
                    m.materials.iter().find(|x| x.name == s.material).map(|x| x.unit_weight * s.thickness)
                });
            let Some(w) = w else { continue };
            for (key, direction) in [
                ("MultiplierX", LoadDirection::GlobalX),
                ("MultiplierY", LoadDirection::GlobalY),
                ("MultiplierZ", LoadDirection::GlobalZ),
            ] {
                let mult = self.num_or(G, row, key, 0.0);
                if mult != 0.0 {
                    for element in &elements {
                        m.area_loads.push(AreaLoad {
                            pattern: pattern.clone(),
                            area: element.clone(),
                            direction,
                            value: w * mult,
                        });
                    }
                }
            }
        }
    }

    /// An unknown area stays as it is, and validation reports it.
    fn elements_of(&self, area: &str) -> Vec<String> {
        self.area_elements.get(area).cloned().unwrap_or_else(|| vec![area.to_string()])
    }

    fn frame_of(&mut self, table: &str, row: &Row, index: &HashMap<String, usize>) -> Option<usize> {
        let name = self.text(table, row, "Frame")?;
        let found = index.get(&name).copied();
        if found.is_none() {
            self.problem(table, row.line, format!("unknown frame {name}"));
        }
        found
    }

    fn constraints(&mut self) -> Vec<Constraint> {
        const DIA: &str = "CONSTRAINT DEFINITIONS - DIAPHRAGM";
        const BODY: &str = "CONSTRAINT DEFINITIONS - BODY";
        const EQ: &str = "CONSTRAINT DEFINITIONS - EQUAL";
        const ASG: &str = "JOINT CONSTRAINT ASSIGNMENTS";
        enum Kind {
            Diaphragm(GlobalAxis),
            Body,
            Equal([bool; 6]),
        }
        let mut defs: BTreeMap<String, Kind> = BTreeMap::new();
        for row in self.s2k.rows(DIA) {
            let Some(name) = self.text(DIA, row, "Name") else { continue };
            let axis = match row.get("Axis").unwrap_or("Z") {
                "X" => GlobalAxis::X,
                "Y" => GlobalAxis::Y,
                "Z" => GlobalAxis::Z,
                other => {
                    self.problem(DIA, row.line, format!("constraint {name}: axis {other} is not supported"));
                    continue;
                }
            };
            if row.get("CoordSys").is_some_and(|c| c != "GLOBAL") {
                self.problem(DIA, row.line, format!("constraint {name}: only global axes"));
            }
            defs.insert(name, Kind::Diaphragm(axis));
        }
        let flags = |row: &Row| ["UX", "UY", "UZ", "RX", "RY", "RZ"].map(|k| row.get(k) == Some(YES));
        for row in self.s2k.rows(BODY) {
            let Some(name) = self.text(BODY, row, "Name") else { continue };
            // Absent flags mean a full body; a partial one has no equivalent.
            if row.fields.iter().any(|(k, _)| k == "UX") && flags(row) != [true; 6] {
                self.problem(BODY, row.line, format!("constraint {name}: a body on some components only"));
            }
            defs.insert(name, Kind::Body);
        }
        for row in self.s2k.rows(EQ) {
            let Some(name) = self.text(EQ, row, "Name") else { continue };
            if row.get("CoordSys").is_some_and(|c| c != "GLOBAL") {
                self.problem(EQ, row.line, format!("constraint {name}: only global axes"));
            }
            defs.insert(name, Kind::Equal(flags(row)));
        }
        let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for row in self.s2k.rows(ASG) {
            let (Some(joint), Some(name)) = (self.text(ASG, row, "Joint"), self.text(ASG, row, "Constraint")) else {
                continue;
            };
            if !defs.contains_key(&name) {
                self.problem(ASG, row.line, format!("constraint {name} has no definition this importer supports"));
                continue;
            }
            members.entry(name).or_default().push(joint);
        }
        members
            .into_iter()
            .map(|(name, joints)| match defs[&name] {
                Kind::Diaphragm(axis) => Constraint::Diaphragm { name, axis, joints },
                Kind::Body => Constraint::Body { name, joints },
                Kind::Equal(components) => Constraint::Equal { name, components, joints },
            })
            .collect()
    }

    fn direction(&mut self, table: &str, row: &Row) -> Option<LoadDirection> {
        let dir = row.get("Dir").unwrap_or("Gravity");
        let local = row.get("CoordSys") == Some("Local");
        let parsed = match (local, dir) {
            (true, "1" | "Local 1") => Some(LoadDirection::Local1),
            (true, "2" | "Local 2") => Some(LoadDirection::Local2),
            (true, "3" | "Local 3") => Some(LoadDirection::Local3),
            (false, "X") => Some(LoadDirection::GlobalX),
            (false, "Y") => Some(LoadDirection::GlobalY),
            (false, "Z") => Some(LoadDirection::GlobalZ),
            (false, "Gravity") => Some(LoadDirection::Gravity),
            _ => None,
        };
        if parsed.is_none() {
            self.problem(
                table,
                row.line,
                format!("load direction '{dir}' is not supported (projected loads are pending)"),
            );
        }
        parsed
    }

    fn loads(&mut self, m: &mut Model, frame_exists: &dyn Fn(&str) -> bool) {
        const JF: &str = "JOINT LOADS - FORCE";
        for row in self.s2k.rows(JF) {
            let (Some(joint), Some(pattern)) = (self.text(JF, row, "Joint"), self.text(JF, row, "LoadPat")) else {
                continue;
            };
            if row.get("CoordSys").is_some_and(|c| c != "GLOBAL") {
                self.problem(JF, row.line, "joint loads only in global axes");
            }
            let values = ["F1", "F2", "F3", "M1", "M2", "M3"].map(|k| self.num_or(JF, row, k, 0.0));
            m.joint_loads.push(JointLoad { pattern, joint, values });
        }
        const JD: &str = "JOINT LOADS - GROUND DISPLACEMENT";
        for row in self.s2k.rows(JD) {
            let (Some(joint), Some(pattern)) = (self.text(JD, row, "Joint"), self.text(JD, row, "LoadPat")) else {
                continue;
            };
            if row.get("CoordSys").is_some_and(|c| c != "GLOBAL") {
                self.problem(JD, row.line, "ground displacements only in global axes");
            }
            let values = ["U1", "U2", "U3", "R1", "R2", "R3"].map(|k| self.num_or(JD, row, k, 0.0));
            m.joint_displacements.push(JointDisplacement { pattern, joint, values });
        }
        const FD: &str = "FRAME LOADS - DISTRIBUTED";
        for row in self.s2k.rows(FD) {
            let (Some(frame), Some(pattern)) = (self.text(FD, row, "Frame"), self.text(FD, row, "LoadPat")) else {
                continue;
            };
            if !frame_exists(&frame) {
                self.problem(FD, row.line, format!("unknown frame {frame}"));
                continue;
            }
            let Some(direction) = self.direction(FD, row) else { continue };
            let (a, b) = (self.need_num(FD, row, "RelDistA"), self.need_num(FD, row, "RelDistB"));
            let kind = row.get("Type").unwrap_or("Force");
            let (ka, kb) = match kind {
                "Force" => ("FOverLA", "FOverLB"),
                "Moment" => ("MOverLA", "MOverLB"),
                other => {
                    self.problem(FD, row.line, format!("distributed load type {other} is not supported"));
                    continue;
                }
            };
            let (va, vb) = (self.need_num(FD, row, ka), self.need_num(FD, row, kb));
            let load = match kind {
                "Force" if a == 0.0 && b == 1.0 && va == vb => FrameLoadKind::Uniform { direction, value: va },
                "Force" => {
                    FrameLoadKind::Trapezoidal { direction, rel_start: a, rel_end: b, value_start: va, value_end: vb }
                }
                _ => FrameLoadKind::TrapezoidalMoment {
                    direction,
                    rel_start: a,
                    rel_end: b,
                    value_start: va,
                    value_end: vb,
                },
            };
            m.frame_loads.push(FrameLoad { pattern, frame, load });
        }
        const FP: &str = "FRAME LOADS - POINT";
        for row in self.s2k.rows(FP) {
            let (Some(frame), Some(pattern)) = (self.text(FP, row, "Frame"), self.text(FP, row, "LoadPat")) else {
                continue;
            };
            if !frame_exists(&frame) {
                self.problem(FP, row.line, format!("unknown frame {frame}"));
                continue;
            }
            let Some(direction) = self.direction(FP, row) else { continue };
            let rel = self.need_num(FP, row, "RelDist");
            let load = match row.get("Type").unwrap_or("Force") {
                "Force" => FrameLoadKind::Point { direction, rel, value: self.need_num(FP, row, "Force") },
                "Moment" => FrameLoadKind::PointMoment { direction, rel, value: self.need_num(FP, row, "Moment") },
                other => {
                    self.problem(FP, row.line, format!("point load type {other} is not supported"));
                    continue;
                }
            };
            m.frame_loads.push(FrameLoad { pattern, frame, load });
        }
        const FG: &str = "FRAME LOADS - GRAVITY";
        for row in self.s2k.rows(FG) {
            let (Some(frame), Some(pattern)) = (self.text(FG, row, "Frame"), self.text(FG, row, "LoadPat")) else {
                continue;
            };
            let Some(f) = m.frames.iter().find(|f| f.name == frame) else {
                self.problem(FG, row.line, format!("unknown frame {frame}"));
                continue;
            };
            // Unit weight × area × multiplier, per global component.
            let w = m
                .sections
                .iter()
                .find(|s| s.name == f.section)
                .and_then(|s| m.materials.iter().find(|x| x.name == s.material).map(|x| x.unit_weight * s.area));
            let Some(w) = w else { continue };
            let mut extra = Vec::new();
            for (key, direction) in [
                ("MultiplierX", LoadDirection::GlobalX),
                ("MultiplierY", LoadDirection::GlobalY),
                ("MultiplierZ", LoadDirection::GlobalZ),
            ] {
                let mult = self.num_or(FG, row, key, 0.0);
                if mult != 0.0 {
                    extra.push(FrameLoad {
                        pattern: pattern.clone(),
                        frame: frame.clone(),
                        load: FrameLoadKind::Uniform { direction, value: w * mult },
                    });
                }
            }
            m.frame_loads.extend(extra);
        }
    }

    fn cases(&mut self, m: &mut Model) {
        const D: &str = "LOAD CASE DEFINITIONS";
        const A: &str = "CASE - STATIC 1 - LOAD ASSIGNMENTS";
        let mut loads: BTreeMap<String, Vec<ScaledPattern>> = BTreeMap::new();
        for row in self.s2k.rows(A) {
            let (Some(case), Some(pattern)) = (self.text(A, row, "Case"), self.text(A, row, "LoadName")) else {
                continue;
            };
            if row.get("LoadType").is_some_and(|t| t != "Load pattern") {
                self.problem(A, row.line, format!("case {case}: load type other than a load pattern"));
            }
            let scale = self.num_or(A, row, "LoadSF", 1.0);
            loads.entry(case).or_default().push(ScaledPattern { pattern, scale });
        }
        // Modes first: a spectrum needs its modal case solved before it.
        let mut spectra = Vec::new();
        for row in self.s2k.rows(D) {
            let Some(name) = self.text(D, row, "Case") else { continue };
            if row.get("InitialCond").is_some_and(|c| c != "Zero") {
                self.notes.push(format!("case {name} not imported: it starts from the state of another case"));
                continue;
            }
            match row.get("Type") {
                Some("LinStatic") => {
                    let case_loads = loads.remove(&name).unwrap_or_default();
                    m.load_cases.push(LoadCase::LinearStatic { name, loads: case_loads });
                }
                Some("LinModal") => match self.modal_case(&name) {
                    Ok(modes) => m.load_cases.push(LoadCase::Modal { name, modes }),
                    Err(why) => self.notes.push(format!("case {name} not imported: {why}")),
                },
                Some("LinRespSpec") => spectra.push((name, row.get("ModalCase"))),
                other => self.notes.push(format!("case {name} of type {} not imported", other.unwrap_or("unknown"))),
            }
        }
        for (name, modal_case) in spectra {
            match self.spectrum_case(&name, modal_case, m) {
                Ok(case) => m.load_cases.push(case),
                Err(why) => self.notes.push(format!("case {name} not imported: {why}")),
            }
        }
        // Without modes, the mass has no effect on anything imported.
        if m.load_cases.iter().any(|c| matches!(c, LoadCase::Modal { .. })) {
            m.mass_source = self.mass_source(m);
        }
    }

    /// The number of modes of a case, or why it has no equivalent here.
    fn modal_case(&mut self, name: &str) -> Result<u32, String> {
        const T: &str = "CASE - MODAL 1 - GENERAL";
        let row =
            self.s2k.rows(T).iter().find(|r| r.get("Case") == Some(name)).ok_or(format!("it has no row in {T}"))?;
        if let Some(kind) = row.get("ModeType").filter(|k| *k != "Eigen") {
            return Err(format!("modes of type {kind}"));
        }
        for key in ["EigenShift", "EigenCutoff"] {
            if row.get(key).is_some_and(|v| v.parse::<f64>() != Ok(0.0)) {
                return Err(format!("{key} is not zero"));
            }
        }
        if let Some(own) = row.get("MassSource")
            && own != "Default"
            && Some(own) != self.default_mass_source().as_deref()
        {
            return Err(format!("mass source {own} is not the default of the file"));
        }
        row.get("MaxNumModes")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n > 0)
            .ok_or("MaxNumModes is not a positive integer".to_string())
    }

    fn default_mass_source(&self) -> Option<String> {
        let rows = self.s2k.rows("MASS SOURCE");
        let marked = rows.iter().find(|r| r.get("IsDefault") == Some(YES));
        let name = marked.or(rows.first())?.get("MassSource")?;
        (marked.is_some() || rows.iter().all(|r| r.get("MassSource") == Some(name))).then(|| name.to_string())
    }

    fn mass_source(&mut self, m: &Model) -> Option<MassSource> {
        const T: &str = "MASS SOURCE";
        let rows = self.s2k.rows(T);
        if rows.is_empty() {
            return None;
        }
        let Some(name) = self.default_mass_source() else {
            self.problem(T, rows[0].line, "no mass source is marked as the default");
            return None;
        };
        let own: Vec<&Row> = rows.iter().filter(|r| r.get("MassSource") == Some(name.as_str())).collect();
        // Not every row has to repeat the flags, but one of them must have each.
        let mut flag = |key: &str| {
            if !own.iter().any(|r| r.get(key).is_some()) {
                self.problems.push(Problem {
                    table: T.to_string(),
                    line: own[0].line,
                    cause: format!("mass source {name}: missing field {key}"),
                });
            }
            own.iter().any(|r| r.get(key) == Some(YES))
        };
        let (elements, added_masses, loads) = (flag("Elements"), flag("Masses"), flag("Loads"));
        let mut patterns = Vec::new();
        if loads {
            for row in own.iter().copied() {
                let (Some(pattern), Some(scale)) = (self.text(T, row, "LoadPat"), self.opt_num(T, row, "Multiplier"))
                else {
                    self.problem(T, row.line, format!("mass source {name}: a pattern needs its Multiplier"));
                    continue;
                };
                patterns.push(ScaledPattern { pattern, scale });
            }
        }
        let weighs =
            |p: &ScaledPattern| m.load_patterns.iter().any(|x| x.name == p.pattern && x.self_weight_multiplier != 0.0);
        if elements && patterns.iter().any(weighs) {
            self.notes.push(format!(
                "mass source {name}: it counts the elements and a pattern with self weight, so that mass counts twice"
            ));
        }
        Some(MassSource { elements, added_masses, patterns })
    }

    /// A spectrum case, or why it has no equivalent here.
    fn spectrum_case(&mut self, name: &str, modal_case: Option<&str>, m: &mut Model) -> Result<LoadCase, String> {
        const G: &str = "CASE - RESPONSE SPECTRUM 1 - GENERAL";
        const L: &str = "CASE - RESPONSE SPECTRUM 2 - LOAD ASSIGNMENTS";
        const F: &str = "FUNCTION - RESPONSE SPECTRUM - USER";
        let number = |row: &Row, key: &str, default: Option<f64>| -> Result<f64, String> {
            match row.get(key) {
                Some(v) => v.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or(format!("{key} is not a number")),
                None => default.ok_or(format!("it has no {key}")),
            }
        };
        let modal_case = modal_case.ok_or("it has no ModalCase")?;
        if !m.load_cases.iter().any(|c| matches!(c, LoadCase::Modal { .. }) && c.name() == modal_case) {
            return Err(format!("its modal case {modal_case} was not imported"));
        }
        let row =
            self.s2k.rows(G).iter().find(|r| r.get("Case") == Some(name)).ok_or(format!("it has no row in {G}"))?;
        let combination = match row.get("ModalCombo") {
            Some("CQC") => ModalCombination::Cqc,
            Some("SRSS") => ModalCombination::Srss,
            other => return Err(format!("modal combination {}", other.unwrap_or("not given"))),
        };
        for (key, wanted) in [("MotionType", "Acceleration"), ("DampingType", "Constant")] {
            if let Some(v) = row.get(key).filter(|v| *v != wanted) {
                return Err(format!("{key} {v}"));
            }
        }
        // The rigid part of the response, accidental eccentricity and per-mode damping.
        for (key, plain) in [("GMCf1", 1.0), ("GMCf2", 0.0), ("EccenRatio", 0.0), ("NumOverride", 0.0)] {
            if number(row, key, Some(plain))? != plain {
                return Err(format!("{key} is not {plain}"));
            }
        }
        let damping = number(row, "ConstDamp", None)?;

        let loads: Vec<&Row> = self.s2k.rows(L).iter().filter(|r| r.get("Case") == Some(name)).collect();
        let [load] = loads[..] else {
            return Err(format!("{} directions: only one, without directional combination", loads.len()));
        };
        if let Some(kind) = load.get("LoadType").filter(|k| *k != "Acceleration") {
            return Err(format!("load type {kind}"));
        }
        if load.get("CoordSys").is_some_and(|c| c != "GLOBAL") || number(load, "Angle", Some(0.0))? != 0.0 {
            return Err("only global axes, with no angle".to_string());
        }
        let direction = match load.get("LoadName") {
            Some("U1") => GlobalAxis::X,
            Some("U2") => GlobalAxis::Y,
            Some("U3") => GlobalAxis::Z,
            other => return Err(format!("direction {}", other.unwrap_or("not given"))),
        };
        let scale = number(load, "TransAccSF", None)?;
        let function = load.get("Function").ok_or("it has no Function")?.to_string();

        let rows: Vec<&Row> = self.s2k.rows(F).iter().filter(|r| r.get("Name") == Some(&function)).collect();
        if rows.is_empty() {
            return Err(format!("its function {function} is not a table in {F}"));
        }
        // The curve would have to be adjusted from its damping to the one of the case.
        if let Some(own) = rows.iter().find_map(|r| r.get("FuncDamp"))
            && own.parse::<f64>().ok().is_none_or(|d| (d - damping).abs() > DAMPING_TOL)
        {
            return Err(format!("its function has damping {own}, and the case {damping}"));
        }
        if !m.response_spectra.iter().any(|f| f.name == function) {
            let mut points = Vec::with_capacity(rows.len());
            for r in rows {
                points.push([number(r, "Period", None)?, number(r, "Accel", None)?]);
            }
            m.response_spectra.push(SpectrumFunction { name: function.clone(), points });
        }
        Ok(LoadCase::ResponseSpectrum {
            name: name.to_string(),
            modal_case: modal_case.to_string(),
            function,
            direction,
            scale,
            combination,
            damping,
        })
    }

    fn combinations(&mut self, m: &mut Model) {
        const T: &str = "COMBINATION DEFINITIONS";
        let mut order: Vec<String> = Vec::new();
        let mut defs: HashMap<String, (String, Vec<ScaledCase>)> = HashMap::new();
        for row in self.s2k.rows(T) {
            let Some(name) = self.text(T, row, "ComboName") else { continue };
            let Some(case) = self.text(T, row, "CaseName") else { continue };
            let scale = self.num_or(T, row, "ScaleFactor", 1.0);
            let entry = defs.entry(name.clone()).or_insert_with(|| {
                order.push(name);
                (row.get("ComboType").unwrap_or("Linear Add").to_string(), Vec::new())
            });
            entry.1.push(ScaledCase { case, scale });
        }
        // Modes and spectra have no sign, so they cannot be terms.
        let cases: Vec<String> = m
            .load_cases
            .iter()
            .filter(|c| matches!(c, LoadCase::LinearStatic { .. }))
            .map(|c| c.name().to_string())
            .collect();
        let mut done: Vec<String> = Vec::new();
        for name in order {
            let Some((kind, terms)) = defs.remove(&name) else { continue };
            let known = |t: &ScaledCase| cases.contains(&t.case) || done.contains(&t.case);
            if !terms.iter().all(known) {
                self.notes.push(format!(
                    "combination {name} not imported: it uses a modal or spectrum case, or one that was not imported"
                ));
                continue;
            }
            match kind.as_str() {
                "Linear Add" | "Linear" => {
                    m.combinations.push(Combination::LinearAdd { name: name.clone(), cases: terms })
                }
                "Envelope" => m.combinations.push(Combination::Envelope { name: name.clone(), cases: terms }),
                other => {
                    self.notes.push(format!("combination {name} of type {other} not imported"));
                    continue;
                }
            }
            done.push(name);
        }
    }
}

/// A material for this section whose unit weight and mass are scaled, if it needs one.
fn with_unit_factors(mut section: FrameSection, weight: f64, mass: f64, materials: &mut Vec<Material>) -> FrameSection {
    if weight == 1.0 && mass == 1.0 {
        return section;
    }
    let base = materials.iter().find(|m| m.name == section.material).cloned();
    if let Some(base) = base {
        let name = format!("{}#{}", base.name, section.name);
        materials.push(Material {
            name: name.clone(),
            unit_weight: base.unit_weight * weight,
            mass_density: base.mass_density * mass,
            ..base
        });
        section.material = name;
    }
    section
}

/// Reads a parsed file into a model in its declared units. The result still
/// has to pass `struktal_core::validate`.
pub fn import(s2k: &S2k) -> Result<Imported, Vec<Problem>> {
    let mut im = Importer { s2k, problems: Vec::new(), notes: Vec::new(), area_elements: HashMap::new() };
    let ignored_tables = im.check_tables();
    let units = im.units();

    const ACTIVE: &str = "ACTIVE DEGREES OF FREEDOM";
    let active = s2k
        .rows(ACTIVE)
        .first()
        .map_or([true; 6], |row| ["UX", "UY", "UZ", "RX", "RY", "RZ"].map(|k| row.get(k) != Some("No")));

    let mut materials = im.materials();
    let mut sections = im.sections(&mut materials);
    let (mut joints, mut joint_index) = im.joints(active);
    let mut frames = im.frames(&mut sections, &mut materials);

    // Only what the frames use: a file lists many unused library sections.
    let mut used: Vec<FrameSection> = Vec::new();
    for f in &frames {
        if used.iter().any(|s| s.name == f.section) {
            continue;
        }
        match sections.get(&f.section) {
            Some((s, _)) => used.push(s.clone()),
            None => {
                im.problem("FRAME SECTION ASSIGNMENTS", 0, format!("frame {}: unknown section {}", f.name, f.section))
            }
        }
    }
    let (area_sections, areas) = im.areas(&mut joints, &mut joint_index, active);
    im.connect_frames_to_mesh(&mut frames, &joints, &areas);
    let used_materials: Vec<Material> = materials
        .iter()
        .filter(|m| used.iter().any(|s| s.material == m.name) || area_sections.iter().any(|s| s.material == m.name))
        .cloned()
        .collect();

    const P: &str = "LOAD PATTERN DEFINITIONS";
    let mut load_patterns = Vec::new();
    for row in s2k.rows(P) {
        let Some(name) = im.text(P, row, "LoadPat") else { continue };
        let self_weight_multiplier = im.num_or(P, row, "SelfWtMult", 0.0);
        load_patterns.push(LoadPattern { name, self_weight_multiplier });
    }

    let constraints = im.constraints();
    let frame_names: Vec<String> = frames.iter().map(|f| f.name.clone()).collect();
    let mut model = Model {
        schema_version: SCHEMA_VERSION,
        title: String::new(),
        units,
        materials: used_materials,
        sections: used,
        joints,
        frames,
        area_sections,
        areas,
        constraints,
        load_patterns,
        joint_loads: Vec::new(),
        joint_displacements: Vec::new(),
        frame_loads: Vec::new(),
        area_loads: Vec::new(),
        response_spectra: Vec::new(),
        mass_source: None,
        load_cases: Vec::new(),
        combinations: Vec::new(),
    };
    im.loads(&mut model, &|f| frame_names.iter().any(|n| n == f));
    im.area_loads(&mut model);
    im.cases(&mut model);
    im.combinations(&mut model);

    if im.problems.is_empty() { Ok(Imported { model, ignored_tables, notes: im.notes }) } else { Err(im.problems) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    const BASE: &str = concat!(
        "TABLE:  \"PROGRAM CONTROL\"\n   ProgramName=X   CurrUnits=\"Kip, in, F\"\n",
        "TABLE:  \"MATERIAL PROPERTIES 02 - BASIC MECHANICAL PROPERTIES\"\n",
        "   Material=S   UnitWeight=0.1   E1=29000   G12=11000   U12=0.3\n",
        "   Material=UNUSED   E1=1\n",
        "TABLE:  \"FRAME SECTION PROPERTIES 01 - GENERAL\"\n",
        "   SectionName=B   Material=S   Area=10   TorsConst=20   I33=100   I22=50   I23=0   AS2=5   AS3=4\n",
        "   SectionName=LIBRARY   Material=UNUSED   Area=1   TorsConst=1   I33=1   I22=1\n",
        "TABLE:  \"JOINT COORDINATES\"\n",
        "   Joint=1   CoordSys=GLOBAL   CoordType=Cartesian   XorR=0   Y=0   Z=0   GlobalX=0   GlobalY=0   GlobalZ=0\n",
        "   Joint=2   CoordSys=GLOBAL   CoordType=Cartesian   XorR=100   Y=0   Z=0\n",
        "TABLE:  \"JOINT RESTRAINT ASSIGNMENTS\"\n",
        "   Joint=1   U1=Yes   U2=Yes   U3=Yes   R1=Yes   R2=Yes   R3=Yes\n",
        "TABLE:  \"CONNECTIVITY - FRAME\"\n   Frame=F1   JointI=1   JointJ=2   IsCurved=No\n",
        "TABLE:  \"FRAME SECTION ASSIGNMENTS\"\n   Frame=F1   AnalSect=B   MatProp=Default\n",
        "TABLE:  \"LOAD PATTERN DEFINITIONS\"\n   LoadPat=P1   SelfWtMult=1\n",
        "TABLE:  \"LOAD CASE DEFINITIONS\"\n   Case=C1   Type=LinStatic\n   Case=MODAL   Type=LinModal\n",
        "TABLE:  \"CASE - STATIC 1 - LOAD ASSIGNMENTS\"\n   Case=C1   LoadType=\"Load pattern\"   LoadName=P1   LoadSF=2\n",
    );

    fn run(extra: &str) -> Result<Imported, Vec<Problem>> {
        import(&parse(&format!("{BASE}{extra}")).expect("parse"))
    }

    fn causes(extra: &str) -> String {
        run(extra).expect_err("must fail").iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
    }

    const MODAL: &str = "TABLE:  \"CASE - MODAL 1 - GENERAL\"\n   Case=MODAL   ModeType=Eigen   MaxNumModes=4   EigenShift=0   EigenCutoff=0\n";
    const SPECTRUM: &str = concat!(
        "TABLE:  \"FUNCTION - RESPONSE SPECTRUM - USER\"\n",
        "   Name=F   Period=0   Accel=1   FuncDamp=0.05\n   Name=F   Period=1   Accel=2\n   Name=LIBRARY   Period=1   Accel=x\n",
        "TABLE:  \"CASE - RESPONSE SPECTRUM 1 - GENERAL\"\n",
        "   Case=SP   ModalCombo=SRSS   GMCf1=1   GMCf2=0   MotionType=Acceleration   DampingType=Constant   ConstDamp=0.05   EccenRatio=0   NumOverride=0\n",
        "TABLE:  \"CASE - RESPONSE SPECTRUM 2 - LOAD ASSIGNMENTS\"\n",
        "   Case=SP   LoadType=Acceleration   LoadName=U2   CoordSys=GLOBAL   Function=F   Angle=0   TransAccSF=386.4\n",
    );

    /// BASE, with a spectrum case listed before its modal case.
    fn with_spectrum(extra: &str) -> String {
        let cases = "   Case=SP   Type=LinRespSpec   ModalCase=MODAL\n   Case=C1   Type=LinStatic\n";
        format!("{}{MODAL}{SPECTRUM}{extra}", BASE.replace("   Case=C1   Type=LinStatic\n", cases))
    }

    fn import_text(text: &str) -> Imported {
        import(&parse(text).expect("parse")).expect("ok")
    }

    /// The notes of a file where this text replaces that one.
    fn refused_spectrum(from: &str, to: &str) -> String {
        let text = with_spectrum("");
        assert!(text.contains(from), "{from}");
        let im = import_text(&text.replace(from, to));
        assert!(!im.model.load_cases.iter().any(|c| c.name() == "SP"), "{to}");
        im.notes.join("\n")
    }

    #[test]
    fn modes_and_spectra_are_read_with_the_modes_first() {
        let im = import_text(&with_spectrum(""));
        let m = &im.model;
        struktal_core::validate(m).expect("valid model");
        let names: Vec<&str> = m.load_cases.iter().map(|c| c.name()).collect();
        assert_eq!(names, ["C1", "MODAL", "SP"]);
        assert!(matches!(m.load_cases[1], LoadCase::Modal { modes: 4, .. }));
        let LoadCase::ResponseSpectrum { modal_case, function, direction, scale, combination, damping, .. } =
            &m.load_cases[2]
        else {
            panic!("spectrum")
        };
        assert_eq!(modal_case, "MODAL");
        assert_eq!(function, "F");
        assert_eq!(*direction, GlobalAxis::Y);
        assert_eq!(*scale, 386.4);
        assert_eq!(*combination, ModalCombination::Srss);
        assert_eq!(*damping, 0.05);
        // Only the function in use: the other one could not even be read.
        assert_eq!(m.response_spectra.len(), 1);
        assert_eq!(m.response_spectra[0].points, vec![[0.0, 1.0], [1.0, 2.0]]);
    }

    #[test]
    fn a_spectrum_with_no_equivalent_is_left_out_and_says_why() {
        for (from, to, why) in [
            ("ModalCombo=SRSS", "ModalCombo=GMC", "GMC"),
            ("MotionType=Acceleration", "MotionType=Displacement", "Displacement"),
            ("DampingType=Constant", "DampingType=Interpolated", "Interpolated"),
            ("GMCf2=0", "GMCf2=5", "GMCf2"),
            ("EccenRatio=0", "EccenRatio=0.05", "EccenRatio"),
            ("NumOverride=0", "NumOverride=2", "NumOverride"),
            ("LoadName=U2", "LoadName=R1", "direction R1"),
            ("CoordSys=GLOBAL   Function", "CoordSys=LOCAL   Function", "global axes"),
            ("Angle=0", "Angle=30", "global axes"),
            ("Function=F", "Function=MISSING", "MISSING"),
            ("FuncDamp=0.05", "FuncDamp=0.02", "damping"),
            ("ModalCase=MODAL", "ModalCase=NOPE", "NOPE"),
            ("TransAccSF=386.4", "Other=1", "TransAccSF"),
        ] {
            let notes = refused_spectrum(from, to);
            assert!(notes.contains("SP not imported") && notes.contains(why), "{to}: {notes}");
        }
    }

    #[test]
    fn a_spectrum_in_two_directions_is_left_out() {
        let extra = "   Case=SP   LoadType=Acceleration   LoadName=U1   Function=F   TransAccSF=1\n";
        let notes = import_text(&with_spectrum(extra)).notes.join("\n");
        assert!(notes.contains("2 directions"), "{notes}");
    }

    #[test]
    fn modes_with_no_equivalent_are_left_out_with_their_spectra() {
        for (from, to, why) in [
            ("ModeType=Eigen", "ModeType=Ritz", "Ritz"),
            ("EigenShift=0", "EigenShift=2", "EigenShift"),
            ("ModeType=Eigen", "ModeType=Eigen   MassSource=OTHER", "OTHER"),
        ] {
            let notes = refused_spectrum(from, to);
            assert!(notes.contains("MODAL not imported") && notes.contains(why), "{to}: {notes}");
            assert!(notes.contains("its modal case MODAL was not imported"), "{notes}");
        }
    }

    #[test]
    fn a_case_that_starts_from_another_is_left_out() {
        let text = BASE.replace("Case=C1   Type=LinStatic", "Case=C1   Type=LinStatic   InitialCond=NL");
        let im = import_text(&text);
        assert!(!im.model.load_cases.iter().any(|c| c.name() == "C1"));
        assert!(im.notes.iter().any(|n| n.contains("C1 not imported")), "{:?}", im.notes);
    }

    #[test]
    fn a_combination_with_a_spectrum_is_left_out_and_the_model_stays_valid() {
        let im = import_text(&with_spectrum(concat!(
            "TABLE:  \"COMBINATION DEFINITIONS\"\n",
            "   ComboName=BOTH   ComboType=\"Linear Add\"   CaseName=C1   ScaleFactor=1\n",
            "   ComboName=BOTH   ComboType=\"Linear Add\"   CaseName=SP   ScaleFactor=1\n",
            "   ComboName=ALONE   ComboType=\"Linear Add\"   CaseName=C1   ScaleFactor=1\n",
        )));
        assert_eq!(im.model.combinations.len(), 1);
        assert!(im.notes.iter().any(|n| n.contains("BOTH")), "{:?}", im.notes);
        struktal_core::validate(&im.model).expect("valid model");
    }

    #[test]
    fn the_default_mass_source_is_the_one_read() {
        let im = import_text(&with_spectrum(concat!(
            "TABLE:  \"MASS SOURCE\"\n",
            "   MassSource=A   Elements=Yes   Masses=Yes   Loads=No   IsDefault=No\n",
            "   MassSource=B   Elements=No   Masses=Yes   Loads=Yes   IsDefault=Yes   LoadPat=P1   Multiplier=0.25\n",
        )));
        let source = im.model.mass_source.as_ref().expect("source");
        assert!(!source.elements && source.added_masses);
        assert_eq!(source.patterns, vec![ScaledPattern { pattern: "P1".into(), scale: 0.25 }]);
        struktal_core::validate(&im.model).expect("valid model");
    }

    #[test]
    fn a_mass_source_that_counts_self_weight_twice_says_so() {
        let im = import_text(&with_spectrum(concat!(
            "TABLE:  \"MASS SOURCE\"\n",
            "   MassSource=A   Elements=Yes   Masses=Yes   Loads=Yes   IsDefault=Yes   LoadPat=P1   Multiplier=1\n",
        )));
        assert!(im.notes.iter().any(|n| n.contains("counts twice")), "{:?}", im.notes);
    }

    #[test]
    fn a_mass_source_only_matters_with_modes() {
        let bad = "TABLE:  \"MASS SOURCE\"\n   MassSource=A   Loads=Yes\n   MassSource=B   Loads=Yes\n";
        assert!(run(bad).expect("no modes").model.mass_source.is_none());
        let text = causes_of(&with_spectrum(bad));
        assert!(text.contains("no mass source is marked"), "{text}");
        let one = "TABLE:  \"MASS SOURCE\"\n   MassSource=A   Elements=Yes   Loads=Yes   LoadPat=P1\n";
        let text = causes_of(&with_spectrum(one));
        assert!(text.contains("missing field Masses") && text.contains("Multiplier"), "{text}");
    }

    fn causes_of(text: &str) -> String {
        let found = import(&parse(text).expect("parse")).expect_err("must fail");
        found.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn added_masses_add_up_and_need_a_known_joint_in_global_axes() {
        let im = run(concat!(
            "TABLE:  \"JOINT ADDED MASS ASSIGNMENTS\"\n",
            "   Joint=2   CoordSys=GLOBAL   Mass1=1   Mass2=2   Mass3=3   MMI1=4   MMI2=5   MMI3=6\n",
            "   Joint=2   CoordSys=\"Joint Local\"   Mass1=10\n",
        ))
        .expect("ok");
        assert_eq!(im.model.joints[1].added_mass, [11.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let text = causes(concat!(
            "TABLE:  \"JOINT ADDED MASS ASSIGNMENTS\"\n",
            "   Joint=9   Mass1=1\n   Joint=2   CoordSys=OTHER   Mass1=1\n",
        ));
        assert!(text.contains("unknown joint 9") && text.contains("global axes"), "{text}");
    }

    #[test]
    fn mass_modifiers_reach_the_material_and_compensate_the_area() {
        let text = BASE
            .replace("UnitWeight=0.1", "UnitWeight=0.1   UnitMass=8")
            .replace("I23=0   AS2=5", "I23=0   AMod=2   MMod=3   WMod=5   AS2=5");
        let im = import_text(&text);
        let m = &im.model;
        let material = m.materials.iter().find(|x| x.name == m.sections[0].material).expect("material");
        assert_eq!(m.sections[0].area, 20.0);
        assert_eq!(material.mass_density * m.sections[0].area, 8.0 * 10.0 * 3.0);
        assert_eq!(material.unit_weight * m.sections[0].area, 0.1 * 10.0 * 5.0);

        let frame = "TABLE:  \"FRAME PROPERTY MODIFIERS\"\n   Frame=F1   AMod=2   MassMod=3   WeightMod=1\n";
        let im = import_text(&format!("{}{frame}", BASE.replace("UnitWeight=0.1", "UnitWeight=0.1   UnitMass=8")));
        let m = &im.model;
        let material = m.materials.iter().find(|x| x.name == m.sections[0].material).expect("material");
        assert_eq!(material.mass_density * m.sections[0].area, 8.0 * 10.0 * 3.0);
        assert_eq!(material.unit_weight * m.sections[0].area, 0.1 * 10.0);
    }

    #[test]
    fn minimal_model_keeps_units_and_only_what_is_used() {
        let im = run("TABLE:  \"GRID LINES\"\n   CoordSys=GLOBAL\n").expect("ok");
        let m = &im.model;
        assert_eq!(m.units, UnitSystem { force: ForceUnit::Kip, length: LengthUnit::In });
        assert_eq!(m.sections.len(), 1);
        assert_eq!(m.materials.len(), 1);
        assert_eq!(m.sections[0].as3, 4.0);
        assert_eq!(m.joints[0].restraint, [true; 6]);
        assert_eq!(m.joints[1].x, 100.0);
        assert!(im.ignored_tables.contains(&"GRID LINES".to_string()));
        assert!(im.notes.iter().any(|n| n.contains("MODAL")), "{:?}", im.notes);
        let LoadCase::LinearStatic { loads, .. } = &m.load_cases[0] else { panic!("static case") };
        assert_eq!(loads[0].scale, 2.0);
        struktal_core::validate(m).expect("valid model");
    }

    #[test]
    fn an_unused_section_cannot_block_the_import() {
        let text = BASE.replace(
            "   SectionName=LIBRARY   Material=UNUSED   Area=1   TorsConst=1   I33=1   I22=1\n",
            "   SectionName=LIBRARY   Area=x   I23=7\n",
        );
        assert!(import(&parse(&text).expect("parse")).is_ok());
    }

    #[test]
    fn unknown_table_with_rows_is_a_problem_but_an_empty_one_is_not() {
        let text = causes("TABLE:  \"FRAME LOADS - TEMPERATURE\"\n   Frame=F1   LoadPat=P1   Temp=10\n");
        assert!(text.contains("FRAME LOADS - TEMPERATURE") && text.contains("not supported"), "{text}");
        assert!(run("TABLE:  \"FRAME LOADS - TEMPERATURE\"\n").is_ok());
    }

    #[test]
    fn every_problem_is_reported_together() {
        let text = causes(concat!(
            "TABLE:  \"JOINT LOADS - FORCE\"\n   Joint=2   LoadPat=P1   CoordSys=Local   F1=abc\n",
            "TABLE:  \"FRAME LOADS - DISTRIBUTED\"\n   Frame=F1   LoadPat=P1   CoordSys=GLOBAL   Dir=\"Gravity Proj\"\n",
        ));
        assert!(text.contains("global axes"), "{text}");
        assert!(text.contains("not a number"), "{text}");
        assert!(text.contains("Gravity Proj"), "{text}");
    }

    #[test]
    fn frame_loads_map_to_their_kinds() {
        let im = run(concat!(
            "TABLE:  \"FRAME LOADS - DISTRIBUTED\"\n",
            "   Frame=F1   LoadPat=P1   CoordSys=GLOBAL   Type=Force   Dir=Gravity   RelDistA=0   RelDistB=1   FOverLA=2   FOverLB=2\n",
            "   Frame=F1   LoadPat=P1   CoordSys=Local   Type=Force   Dir=2   RelDistA=0.2   RelDistB=0.8   FOverLA=1   FOverLB=3\n",
            "   Frame=F1   LoadPat=P1   CoordSys=Local   Type=Moment   Dir=1   RelDistA=0   RelDistB=1   MOverLA=1   MOverLB=1\n",
            "TABLE:  \"FRAME LOADS - POINT\"\n",
            "   Frame=F1   LoadPat=P1   CoordSys=GLOBAL   Type=Force   Dir=X   RelDist=0.5   Force=7\n",
            "   Frame=F1   LoadPat=P1   CoordSys=Local   Type=Moment   Dir=3   RelDist=0.25   Moment=9\n",
        ))
        .expect("ok");
        let kinds = &im.model.frame_loads;
        assert!(matches!(kinds[0].load, FrameLoadKind::Uniform { direction: LoadDirection::Gravity, value: 2.0 }));
        assert!(matches!(
            kinds[1].load,
            FrameLoadKind::Trapezoidal { direction: LoadDirection::Local2, value_end: 3.0, .. }
        ));
        assert!(matches!(kinds[2].load, FrameLoadKind::TrapezoidalMoment { direction: LoadDirection::Local1, .. }));
        assert!(matches!(kinds[3].load, FrameLoadKind::Point { direction: LoadDirection::GlobalX, value: 7.0, .. }));
        assert!(matches!(
            kinds[4].load,
            FrameLoadKind::PointMoment { direction: LoadDirection::Local3, value: 9.0, .. }
        ));
    }

    #[test]
    fn modifiers_change_stiffness_but_not_weight() {
        let text = BASE.replace("AS2=5   AS3=4", "AS2=5   AS3=4   AMod=1000   I3Mod=2   WMod=0.5");
        let im = import(&parse(&text).expect("parse")).expect("ok");
        let s = &im.model.sections[0];
        assert_eq!((s.area, s.i33), (10_000.0, 200.0));
        let mat = im.model.materials.iter().find(|m| m.name == s.material).expect("material");
        // Weight per length: original 0.1 × 10 × WMod 0.5.
        assert!((mat.unit_weight * s.area - 0.5).abs() < 1e-12);
    }

    #[test]
    fn frame_property_modifiers_make_a_section_for_that_frame() {
        let im = run("TABLE:  \"FRAME PROPERTY MODIFIERS\"\n   Frame=F1   AMod=2   I33Mod=3   EIModifier=0.5\n")
            .expect("ok");
        let s = im.model.sections.iter().find(|s| s.name == im.model.frames[0].section).expect("section");
        assert_eq!((s.area, s.i33, s.i22), (20.0, 150.0, 25.0));
        assert_eq!(im.model.frames[0].section, "B#F1");
    }

    #[test]
    fn active_dofs_restrain_all_joints() {
        let im = run("TABLE:  \"ACTIVE DEGREES OF FREEDOM\"\n   UX=Yes   UY=No   UZ=Yes   RX=No   RY=Yes   RZ=No\n")
            .expect("ok");
        assert_eq!(im.model.joints[1].restraint, [false, true, false, true, false, true]);
    }

    #[test]
    fn constraints_releases_springs_mesh_and_combinations() {
        let im = run(concat!(
            "TABLE:  \"JOINT SPRING ASSIGNMENTS 1 - UNCOUPLED\"\n   Joint=2   CoordSys=Local   U3=8\n",
            "TABLE:  \"FRAME RELEASE ASSIGNMENTS 1 - GENERAL\"\n   Frame=F1   M3J=Yes   PI=No\n",
            "TABLE:  \"FRAME RELEASE ASSIGNMENTS 2 - PARTIAL FIXITY\"\n   Frame=F1   M3J=50\n",
            "TABLE:  \"FRAME LOCAL AXES ASSIGNMENTS 1 - TYPICAL\"\n   Frame=F1   Angle=30   AdvanceAxes=No\n",
            "TABLE:  \"FRAME AUTO MESH ASSIGNMENTS\"\n   Frame=F1   AutoMesh=Yes   AtJoints=Yes   NumSegments=3\n",
            "TABLE:  \"FRAME INSERTION POINT ASSIGNMENTS\"\n   Frame=F1   CardinalPt=\"10 (Centroid)\"   CoordSys=GLOBAL   JtOffsetZI=2\n",
            "TABLE:  \"CONSTRAINT DEFINITIONS - DIAPHRAGM\"\n   Name=D1   Axis=Z\n",
            "TABLE:  \"JOINT CONSTRAINT ASSIGNMENTS\"\n   Joint=2   Constraint=D1   Type=Diaphragm\n",
            "TABLE:  \"COMBINATION DEFINITIONS\"\n",
            "   ComboName=U1   ComboType=\"Linear Add\"   CaseType=\"Linear Static\"   CaseName=C1   ScaleFactor=1.4\n",
            "   ComboName=E1   ComboType=Envelope   CaseName=U1   ScaleFactor=1\n",
            "   ComboName=X1   ComboType=\"Linear Add\"   CaseName=MODAL   ScaleFactor=1\n",
            "   ComboName=S1   ComboType=SRSS   CaseName=C1   ScaleFactor=1\n",
        ))
        .expect("ok");
        let m = &im.model;
        assert_eq!(m.joints[1].springs[2], 8.0);
        let f = &m.frames[0];
        assert_eq!(f.angle_deg, 30.0);
        let r = f.releases.as_ref().expect("releases");
        assert!(r.j[5] && !r.i[0]);
        assert_eq!(r.j_springs[5], 50.0);
        assert_eq!(f.auto_mesh, Some(AutoMesh { segments: 3, at_joints: true }));
        assert_eq!(f.offsets.expect("offsets").i, [0.0, 0.0, 2.0]);
        assert_eq!(m.constraints.len(), 1);
        assert_eq!(m.combinations.len(), 2);
        assert!(matches!(m.combinations[1], Combination::Envelope { .. }));
        assert!(
            im.notes.iter().any(|n| n.contains("X1")) && im.notes.iter().any(|n| n.contains("S1")),
            "{:?}",
            im.notes
        );
    }

    const AREA: &str = concat!(
        "TABLE:  \"JOINT COORDINATES\"\n",
        "   Joint=3   XorR=100   Y=100   Z=0\n   Joint=4   XorR=0   Y=100   Z=0\n",
        "TABLE:  \"AREA SECTION PROPERTIES\"\n",
        "   Section=SL   Material=S   AreaType=Shell   Type=Shell-Thin   Thickness=5   BendThick=5   F11Mod=1\n",
        "   Section=UNUSED   Material=NOPE   AreaType=Shell   Type=Other   Thickness=x\n",
        "TABLE:  \"CONNECTIVITY - AREA\"\n",
        "   Area=A1   NumJoints=4   Joint1=1   Joint2=2   Joint3=3   Joint4=4\n",
        "TABLE:  \"AREA SECTION ASSIGNMENTS\"\n   Area=A1   Section=SL\n",
    );

    #[test]
    fn areas_and_their_loads_are_read() {
        let im = run(&format!(
            "{AREA}TABLE:  \"AREA LOADS - UNIFORM\"\n   Area=A1   LoadPat=P1   CoordSys=Local   Dir=3   UnifLoad=-0.01\n   Area=A1   LoadPat=P1   CoordSys=GLOBAL   Dir=Gravity   UnifLoad=0.02\nTABLE:  \"AREA LOADS - GRAVITY\"\n   Area=A1   LoadPat=P1   MultiplierZ=-1\n"
        ))
        .expect("ok");
        let m = &im.model;
        assert_eq!(m.area_sections.len(), 1);
        assert_eq!(m.area_sections[0].behavior, AreaBehavior::Shell);
        assert_eq!(m.area_sections[0].bending_thickness, None);
        assert_eq!(m.areas[0].joints[2], "3");
        assert_eq!(m.area_loads[0].direction, LoadDirection::Local3);
        assert_eq!(m.area_loads[1].direction, LoadDirection::Gravity);
        // Multiplier -1 on Z: unit weight 0.1 × thickness 5, downward.
        assert_eq!((m.area_loads[2].direction, m.area_loads[2].value), (LoadDirection::GlobalZ, -0.5));
        assert!(im.notes.iter().any(|n| n.contains("thin formulation")), "{:?}", im.notes);
        struktal_core::validate(m).expect("valid");
    }

    #[test]
    fn a_triangular_object_is_read() {
        let im = run(&format!(
            "{AREA}TABLE:  \"CONNECTIVITY - AREA\"\n   Area=T1   NumJoints=3   Joint1=1   Joint2=2   Joint3=3\nTABLE:  \"AREA SECTION ASSIGNMENTS\"\n   Area=T1   Section=SL\n"
        ))
        .expect("ok");
        assert_eq!(im.model.areas[1].joints, ["1", "2", "3"]);
        struktal_core::validate(&im.model).expect("valid");
    }

    /// The square A1 in two triangles and two quadrilaterals around ~1, ~2 and ~3.
    const MESH: &str = concat!(
        "TABLE:  \"OBJECTS AND ELEMENTS - JOINTS\"\n",
        "   JointElem=1   JointObject=1   GlobalX=0   GlobalY=0   GlobalZ=0\n",
        "   JointElem=~1   GlobalX=50   GlobalY=0   GlobalZ=0\n",
        "   JointElem=~2   GlobalX=50   GlobalY=100   GlobalZ=0\n",
        "   JointElem=~3   GlobalX=50   GlobalY=50   GlobalZ=0\n",
        "   JointElem=~9   GlobalX=7   GlobalY=7   GlobalZ=7\n",
        "TABLE:  \"OBJECTS AND ELEMENTS - AREAS\"\n",
        "   AreaElem=E1   AreaObject=A1   ElemJt1=1   ElemJt2=~1   ElemJt3=~2   ElemJt4=4\n",
        "   AreaElem=E2   AreaObject=A1   ElemJt1=~1   ElemJt2=2   ElemJt3=~3\n",
        "   AreaElem=E3   AreaObject=A1   ElemJt1=2   ElemJt2=3   ElemJt3=~3\n",
        "   AreaElem=E4   AreaObject=A1   ElemJt1=~3   ElemJt2=3   ElemJt3=~2\n",
    );
    const MESH_FRAMES: &str = concat!(
        "TABLE:  \"OBJECTS AND ELEMENTS - FRAMES\"\n",
        "   FrameElem=F1-1   FrameObject=F1   ElemJtI=1   ElemJtJ=~1\n",
        "   FrameElem=F1-2   FrameObject=F1   ElemJtI=~1   ElemJtJ=2\n",
    );

    #[test]
    fn a_mesh_replaces_its_object_and_gets_its_loads() {
        let im = run(&format!(
            "{AREA}{MESH}{MESH_FRAMES}TABLE:  \"AREA LOADS - UNIFORM\"\n   Area=A1   LoadPat=P1   CoordSys=Local   Dir=3   UnifLoad=-0.01\nTABLE:  \"AREA LOADS - GRAVITY\"\n   Area=A1   LoadPat=P1   MultiplierZ=-1\n"
        ))
        .expect("ok");
        let m = &im.model;
        let names: Vec<&str> = m.areas.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["E1", "E2", "E3", "E4"]);
        assert_eq!(m.areas[1].joints, ["~1", "2", "~3"]);
        assert!(m.areas.iter().all(|a| a.section == "SL"));
        // Each load, once per element.
        assert_eq!(m.area_loads.len(), 8);
        assert_eq!(m.area_loads.iter().filter(|l| l.area == "E3").count(), 2);
        assert_eq!(m.area_loads[7].value, -0.5);
        // The joints it needs are added free, and only those.
        let mid = m.joints.iter().find(|j| j.name == "~3").expect("~3");
        assert_eq!((mid.x, mid.y, mid.restraint), (50.0, 50.0, [false; 6]));
        assert!(!m.joints.iter().any(|j| j.name == "~9"));
        // The beam is cut where the file cuts it, so it connects to the mesh.
        assert!(m.frames[0].auto_mesh.expect("mesh").at_joints);
        assert!(im.notes.iter().any(|n| n.contains("meshed")) && im.notes.iter().any(|n| n.contains("cut")));
        struktal_core::validate(m).expect("valid");
    }

    #[test]
    fn a_frame_not_cut_at_the_mesh_stays_whole() {
        let frames =
            "TABLE:  \"OBJECTS AND ELEMENTS - FRAMES\"\n   FrameElem=F1   FrameObject=F1   ElemJtI=1   ElemJtJ=2\n";
        let im = run(&format!("{AREA}{MESH}{frames}")).expect("ok");
        assert!(im.model.frames[0].auto_mesh.is_none());
    }

    #[test]
    fn a_mesh_that_does_not_match_its_object_is_refused() {
        let with_frames = |mesh: &str| causes(&format!("{AREA}{mesh}{MESH_FRAMES}"));
        for (mesh, needle) in [
            // Without E4, a quarter of the area and its load would be lost.
            (MESH.replace("   AreaElem=E4   AreaObject=A1   ElemJt1=~3   ElemJt2=3   ElemJt3=~2\n", ""), "cover"),
            (MESH.replace("ElemJt1=2   ElemJt2=3   ElemJt3=~3", "ElemJt1=3   ElemJt2=2   ElemJt3=~3"), "faces against"),
            (MESH.replace("   JointElem=~3   GlobalX=50   GlobalY=50   GlobalZ=0\n", ""), "not in this table"),
            (MESH.replace("JointElem=~3   GlobalX", "JointElem=~3   JointObject=9   GlobalX"), "another name"),
            (MESH.replace("AreaObject=A1   ElemJt1=~1", "AreaObject=NOPE   ElemJt1=~1"), "unknown area"),
            (
                MESH.replace("JointElem=~1   GlobalX=50   GlobalY=0", "JointElem=~1   GlobalX=50   GlobalY=2"),
                "off its axis",
            ),
        ] {
            let text = with_frames(&mesh);
            assert!(text.contains(needle), "{needle}: {text}");
        }
        let other = "TABLE:  \"CONNECTIVITY - AREA\"\n   Area=A2   NumJoints=3   Joint1=1   Joint2=2   Joint3=3\nTABLE:  \"AREA SECTION ASSIGNMENTS\"\n   Area=A2   Section=SL\n";
        assert!(causes(&format!("{AREA}{other}{MESH}{MESH_FRAMES}")).contains("A2 has no elements"));
        assert!(causes(&format!("{AREA}{MESH}")).contains("to know where they connect"));
    }

    #[test]
    fn areas_that_cannot_be_represented_are_refused() {
        for (extra, needle) in [
            (
                "TABLE:  \"CONNECTIVITY - AREA\"\n   Area=P1   NumJoints=5   Joint1=1   Joint2=2   Joint3=3   Joint4=4   Joint5=2\nTABLE:  \"AREA SECTION ASSIGNMENTS\"\n   Area=P1   Section=SL\n",
                "needs the table",
            ),
            (
                "TABLE:  \"AREA LOADS - UNIFORM\"\n   Area=A1   LoadPat=P1   CoordSys=GLOBAL   Dir=\"Gravity Proj\"   UnifLoad=1\n",
                "Gravity Proj",
            ),
            ("TABLE:  \"AREA LOADS - TEMPERATURE\"\n   Area=A1   LoadPat=P1\n", "not supported"),
        ] {
            let text = causes(&format!("{AREA}{extra}"));
            assert!(text.contains(needle), "{needle}: {text}");
        }
        let modified = AREA.replace("F11Mod=1", "F11Mod=0.5");
        assert!(causes(&modified).contains("F11Mod"));
    }

    #[test]
    fn what_cannot_be_represented_is_refused() {
        for (extra, needle) in [
            (
                "TABLE:  \"FRAME INSERTION POINT ASSIGNMENTS\"\n   Frame=F1   CardinalPt=\"1 (Bottom left)\"\n",
                "cardinal",
            ),
            (
                "TABLE:  \"FRAME LOCAL AXES ASSIGNMENTS 1 - TYPICAL\"\n   Frame=F1   Angle=0   AdvanceAxes=Yes\n",
                "advanced",
            ),
            ("TABLE:  \"JOINT LOCAL AXES ASSIGNMENTS 1 - TYPICAL\"\n   Joint=2   AngleA=30\n", "local axes"),
            ("TABLE:  \"CONSTRAINT DEFINITIONS - BODY\"\n   Name=B1   UX=Yes   UY=No\n", "some components"),
            ("TABLE:  \"JOINT CONSTRAINT ASSIGNMENTS\"\n   Joint=2   Constraint=NOPE\n", "no definition"),
            ("TABLE:  \"FRAME LOADS - POINT\"\n   Frame=NOPE   LoadPat=P1   RelDist=0.5   Force=1\n", "unknown frame"),
        ] {
            let text = causes(extra);
            assert!(text.contains(needle), "{needle}: {text}");
        }
        let text = BASE.replace("CurrUnits=\"Kip, in, F\"", "CurrUnits=\"Stone, furlong, K\"");
        let errs = import(&parse(&text).expect("parse")).expect_err("fail");
        assert!(errs[0].cause.contains("Stone"), "{errs:?}");
    }
}
