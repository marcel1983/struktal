//! Strict reader of the S2K text format (decision 0018).
//!
//! `parser` only knows the syntax: tables, rows and fields with their line.
//! `import` knows the meaning, and everything it cannot represent faithfully is
//! a reported problem, never dropped: a table it does not know, a field value
//! that changes the structure, an option it does not implement.

pub mod import;
pub mod parser;

pub use import::{Imported, Problem, import};
pub use parser::{ParseError, Row, S2k, Table, parse};
