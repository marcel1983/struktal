//! Syntax of an S2K file: `TABLE:  "NAME"`, then rows of `Key=Value` pairs. A
//! value may be quoted, a trailing ` _` continues the row on the next line, and
//! a `""` inside quotes is a literal quote. Field names are never positional.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub cause: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.cause)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub line: usize,
    pub fields: Vec<(String, String)>,
}

impl Row {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub name: String,
    pub line: usize,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct S2k {
    pub tables: Vec<Table>,
}

impl S2k {
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// Rows of a table, or none if it is absent.
    pub fn rows(&self, name: &str) -> &[Row] {
        self.table(name).map_or(&[], |t| t.rows.as_slice())
    }
}

/// Splits one logical line into `Key=Value` pairs.
fn split_fields(text: &str, line: usize) -> Result<Vec<(String, String)>, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out: Vec<(String, String)> = Vec::new();
    let error = |cause: String| ParseError { line, cause };
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[start..i].iter().collect();
        if i >= chars.len() || chars[i] != '=' || key.is_empty() {
            return Err(error(format!("expected Key=Value, found '{key}'")));
        }
        i += 1;
        let mut value = String::new();
        if i < chars.len() && chars[i] == '"' {
            i += 1;
            loop {
                if i >= chars.len() {
                    return Err(error(format!("unterminated quote in field '{key}'")));
                }
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        value.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                value.push(chars[i]);
                i += 1;
            }
        } else {
            while i < chars.len() && !chars[i].is_whitespace() {
                value.push(chars[i]);
                i += 1;
            }
        }
        if out.iter().any(|(k, _)| *k == key) {
            return Err(error(format!("field '{key}' appears twice in the row")));
        }
        out.push((key, value));
    }
    Ok(out)
}

/// A table that appears twice continues the first one.
pub fn parse(text: &str) -> Result<S2k, ParseError> {
    let mut s2k = S2k::default();
    let mut current: Option<usize> = None;
    let mut pending: Option<(usize, String)> = None;
    for (k, raw) in text.lines().enumerate() {
        let line = k + 1;
        let trimmed = raw.trim_end();
        if pending.is_none() {
            let lead = trimmed.trim_start();
            if lead.is_empty() {
                continue;
            }
            if let Some(rest) = lead.strip_prefix("TABLE:") {
                let name = rest.trim();
                let name = name
                    .strip_prefix('"')
                    .and_then(|n| n.strip_suffix('"'))
                    .ok_or_else(|| ParseError { line, cause: format!("table name must be quoted, got {name}") })?;
                let at = s2k.tables.iter().position(|t| t.name == name);
                current = Some(at.unwrap_or_else(|| {
                    s2k.tables.push(Table { name: name.to_string(), line, rows: Vec::new() });
                    s2k.tables.len() - 1
                }));
                continue;
            }
            if lead == "END TABLE DATA" {
                current = None;
                continue;
            }
            // Text before the first table, such as the "File ... was saved" line.
            if current.is_none() {
                continue;
            }
        }
        let (body, continues) = match trimmed.strip_suffix(" _") {
            Some(b) => (b, true),
            None => (trimmed, false),
        };
        let (start, mut joined) = pending.take().unwrap_or((line, String::new()));
        joined.push(' ');
        joined.push_str(body);
        if continues {
            pending = Some((start, joined));
            continue;
        }
        let fields = split_fields(&joined, start)?;
        if let Some(t) = current {
            s2k.tables[t].rows.push(Row { line: start, fields });
        }
    }
    if let Some((line, _)) = pending {
        return Err(ParseError { line, cause: "the file ends inside a continued row".into() });
    }
    Ok(s2k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_rows_continuations_and_quotes() {
        let text = concat!(
            "File C:\\x.s2k was saved on m/d/yy at h:mm:ss\n",
            " \n",
            "TABLE:  \"PROGRAM CONTROL\"\n",
            "   ProgramName=X   CurrUnits=\"Kip, in, F\"   SteelCode=AISC _\n",
            "        ConcCode=\"ACI 318\"\n",
            " \n",
            "TABLE:  \"A\"\n",
            "   Name=\"6\"\" bar\"   N=1\n",
            "END TABLE DATA\n",
        );
        let s = parse(text).expect("parse");
        let pc = s.table("PROGRAM CONTROL").expect("table");
        assert_eq!(pc.line, 3);
        assert_eq!(pc.rows.len(), 1);
        assert_eq!(pc.rows[0].line, 4);
        assert_eq!(pc.rows[0].get("CurrUnits"), Some("Kip, in, F"));
        assert_eq!(pc.rows[0].get("ConcCode"), Some("ACI 318"));
        assert_eq!(s.rows("A")[0].get("Name"), Some("6\" bar"));
        assert!(s.rows("NOPE").is_empty());
    }

    #[test]
    fn repeated_table_continues_the_first() {
        let s = parse("TABLE:  \"A\"\n N=1\nTABLE:  \"B\"\n N=2\nTABLE:  \"A\"\n N=3\n").expect("parse");
        assert_eq!(s.tables.len(), 2);
        assert_eq!(s.rows("A").len(), 2);
    }

    #[test]
    fn errors_give_the_line() {
        let e = parse("TABLE:  \"A\"\n N=1\n garbage\n").expect_err("fail");
        assert_eq!(e.line, 3);
        assert!(e.cause.contains("Key=Value"), "{e}");
        assert_eq!(parse("TABLE:  \"A\"\n N=\"open\n").expect_err("fail").line, 2);
        assert_eq!(parse("TABLE:  \"A\"\n N=1 N=2\n").expect_err("fail").line, 2);
        assert_eq!(parse("TABLE:  A\n").expect_err("fail").line, 1);
        assert_eq!(parse("TABLE:  \"A\"\n N=1 _\n").expect_err("fail").line, 2);
    }
}
