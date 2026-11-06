//! Meaning of the S2K tables: from a parsed file to the canonical model.
//!
//! Strict by design. Each problem names table, line and cause, and all of them
//! are reported together. Tables with no effect on the analysis are listed in
//! `IGNORED`; any other table the importer does not know is a problem, because
//! it may change the structure.

use std::collections::{BTreeMap, HashMap};
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
    "COORDINATE SYSTEMS",
    "DATABASE FORMAT TYPES",
    "ELEMENT FORCES",
    "ELEMENT JOINT FORCES",
    "ELEMENT STRESSES",
    "FRAME DESIGN PROCEDURES",
    "FRAME LOAD TRANSFER OPTIONS",
    "FRAME OUTPUT STATION ASSIGNMENTS",
    "FRAME SECTION PROPERTIES 02 - CONCRETE COLUMN",
    "FRAME SECTION PROPERTIES 03 - CONCRETE BEAM",
    "FRAME SECTION PROPERTIES 13 - TIME DEPENDENT",
    "FUNCTION - PLOT FUNCTIONS",
    "GRID LINES",
    "GROUPS ",
    "JOINT DISPLACEMENTS",
    "JOINT PATTERN DEFINITIONS",
    "JOINT REACTIONS",
    "LINK PROPERTY DEFINITIONS 01 - GENERAL",
    "MASS SOURCE",
    "MATERIAL LIST",
    "MATERIAL PROPERTIES 01 - GENERAL",
    "MATERIAL PROPERTIES 03",
    "MATERIAL PROPERTIES 04",
    "MATERIAL PROPERTIES 06",
    "MATERIAL PROPERTIES 09",
    "MODAL ",
    "NAMED SETS",
    "OBJECTS AND ELEMENTS - FRAMES",
    "OBJECTS AND ELEMENTS - JOINTS",
    "OPTIONS - ",
    "OVERWRITES - ",
    "PREFERENCES - ",
    "PROJECT INFORMATION",
    "REBAR SIZES",
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
    "JOINT CONSTRAINT ASSIGNMENTS",
    "JOINT COORDINATES",
    "JOINT LOADS - FORCE",
    "JOINT LOADS - GROUND DISPLACEMENT",
    "JOINT LOCAL AXES ASSIGNMENTS 1 - TYPICAL",
    "JOINT RESTRAINT ASSIGNMENTS",
    "JOINT SPRING ASSIGNMENTS 1 - UNCOUPLED",
    "LOAD CASE DEFINITIONS",
    "LOAD PATTERN DEFINITIONS",
    "MATERIAL PROPERTIES 02 - BASIC MECHANICAL PROPERTIES",
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
}

const YES: &str = "Yes";

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
    /// does not change weight in the source format, so a modified area takes a
    /// material whose unit weight compensates it.
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
            let section = with_weight_factor(section, w_mod / a_mod, materials);
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
            if [fa, f2, f3, fj, fs2, fs3, fw] == [1.0; 7] {
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
            let modified = with_weight_factor(modified, fw / fa, materials);
            sections.insert(own.clone(), (modified, fa));
            frames[f].section = own;
        }
        frames
    }

    /// Areas as they are in the file: objects are elements. A model whose
    /// analysis mesh differs from its objects is refused, because this reader
    /// has no way to pass the loads of an object to its elements.
    fn areas(&mut self) -> (Vec<AreaSection>, Vec<Area>) {
        const C: &str = "CONNECTIVITY - AREA";
        const A: &str = "AREA SECTION ASSIGNMENTS";
        const P: &str = "AREA SECTION PROPERTIES";
        const E: &str = "OBJECTS AND ELEMENTS - AREAS";
        let mut assigned: HashMap<String, String> = HashMap::new();
        for row in self.s2k.rows(A) {
            let (Some(area), Some(section)) = (self.text(A, row, "Area"), self.text(A, row, "Section")) else {
                continue;
            };
            assigned.insert(area, section);
        }
        let mut areas = Vec::new();
        for row in self.s2k.rows(C) {
            let Some(name) = self.text(C, row, "Area") else { continue };
            if row.get("NumJoints").is_some_and(|n| n != "4") {
                self.problem(C, row.line, format!("area {name}: only four joints (triangles are pending)"));
                continue;
            }
            let (Some(a), Some(b), Some(c), Some(d)) = (
                self.text(C, row, "Joint1"),
                self.text(C, row, "Joint2"),
                self.text(C, row, "Joint3"),
                self.text(C, row, "Joint4"),
            ) else {
                continue;
            };
            let Some(section) = assigned.get(&name).cloned() else {
                self.problem(C, row.line, format!("area {name} has no section assignment"));
                continue;
            };
            areas.push(Area { name, joints: [a, b, c, d], section });
        }
        for row in self.s2k.rows(E) {
            let (object, element) = (row.get("AreaObject"), row.get("AreaElem"));
            if let (Some(object), Some(element)) = (object, element)
                && object != element
            {
                self.problem(E, row.line, format!("area {object} is meshed into elements: export the mesh as objects"));
                break;
            }
        }
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
            m.area_loads.push(AreaLoad { pattern, area, direction, value });
        }
        const G: &str = "AREA LOADS - GRAVITY";
        for row in self.s2k.rows(G) {
            let (Some(area), Some(pattern)) = (self.text(G, row, "Area"), self.text(G, row, "LoadPat")) else {
                continue;
            };
            let Some(sec) = m.areas.iter().find(|a| a.name == area).map(|a| a.section.clone()) else {
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
                    m.area_loads.push(AreaLoad {
                        pattern: pattern.clone(),
                        area: area.clone(),
                        direction,
                        value: w * mult,
                    });
                }
            }
        }
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
        for row in self.s2k.rows(D) {
            let Some(name) = self.text(D, row, "Case") else { continue };
            match row.get("Type") {
                Some("LinStatic") => {
                    let case_loads = loads.remove(&name).unwrap_or_default();
                    m.load_cases.push(LoadCase::LinearStatic { name, loads: case_loads });
                }
                other => self.notes.push(format!("case {name} of type {} not imported", other.unwrap_or("unknown"))),
            }
        }
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
        let cases: Vec<String> = m.load_cases.iter().map(|c| c.name().to_string()).collect();
        let mut done: Vec<String> = Vec::new();
        for name in order {
            let Some((kind, terms)) = defs.remove(&name) else { continue };
            let known = |t: &ScaledCase| cases.contains(&t.case) || done.contains(&t.case);
            if !terms.iter().all(known) {
                self.notes.push(format!("combination {name} not imported: it uses a case that was not imported"));
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

/// A material for this section whose unit weight is scaled, if it needs one.
fn with_weight_factor(mut section: FrameSection, factor: f64, materials: &mut Vec<Material>) -> FrameSection {
    if factor == 1.0 {
        return section;
    }
    let base = materials.iter().find(|m| m.name == section.material).cloned();
    if let Some(base) = base {
        let name = format!("{}#{}", base.name, section.name);
        materials.push(Material { name: name.clone(), unit_weight: base.unit_weight * factor, ..base });
        section.material = name;
    }
    section
}

/// Reads a parsed file into a model in its declared units. The result still
/// has to pass `struktal_core::validate`.
pub fn import(s2k: &S2k) -> Result<Imported, Vec<Problem>> {
    let mut im = Importer { s2k, problems: Vec::new(), notes: Vec::new() };
    let ignored_tables = im.check_tables();
    let units = im.units();

    const ACTIVE: &str = "ACTIVE DEGREES OF FREEDOM";
    let active = s2k
        .rows(ACTIVE)
        .first()
        .map_or([true; 6], |row| ["UX", "UY", "UZ", "RX", "RY", "RZ"].map(|k| row.get(k) != Some("No")));

    let mut materials = im.materials();
    let mut sections = im.sections(&mut materials);
    let (joints, _) = im.joints(active);
    let frames = im.frames(&mut sections, &mut materials);

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
    let (area_sections, areas) = im.areas();
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
    fn areas_that_cannot_be_represented_are_refused() {
        for (extra, needle) in [
            (
                "TABLE:  \"CONNECTIVITY - AREA\"\n   Area=T1   NumJoints=3   Joint1=1   Joint2=2   Joint3=3\n",
                "triangles",
            ),
            ("TABLE:  \"OBJECTS AND ELEMENTS - AREAS\"\n   AreaElem=1   AreaObject=A1\n", "meshed"),
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
