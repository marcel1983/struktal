//! Triplet assembly and compressed sparse column storage.

/// Accumulates (row, col, value) entries. Duplicates are summed on conversion.
#[derive(Debug, Clone, Default)]
pub struct TripletBuilder {
    n: usize,
    entries: Vec<(u32, u32, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SparseError {
    IndexOutOfRange { row: usize, col: usize, n: usize },
    DimensionTooLarge { n: usize },
    NonFinite { row: usize, col: usize },
}

impl std::fmt::Display for SparseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SparseError::IndexOutOfRange { row, col, n } => write!(f, "entry ({row}, {col}) outside {n}x{n}"),
            SparseError::DimensionTooLarge { n } => write!(f, "dimension {n} exceeds u32 index range"),
            SparseError::NonFinite { row, col } => write!(f, "non-finite value at ({row}, {col})"),
        }
    }
}

impl std::error::Error for SparseError {}

impl TripletBuilder {
    pub fn new(n: usize) -> Result<Self, SparseError> {
        if u32::try_from(n).is_err() {
            return Err(SparseError::DimensionTooLarge { n });
        }
        Ok(Self { n, entries: Vec::new() })
    }

    pub fn dim(&self) -> usize {
        self.n
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add(&mut self, row: usize, col: usize, value: f64) -> Result<(), SparseError> {
        if row >= self.n || col >= self.n {
            return Err(SparseError::IndexOutOfRange { row, col, n: self.n });
        }
        if !value.is_finite() {
            return Err(SparseError::NonFinite { row, col });
        }
        // Indices fit: checked against n, which fits in u32.
        self.entries.push((row as u32, col as u32, value));
        Ok(())
    }

    /// Deterministic conversion: entries are sorted by (col, row) and then
    /// summed in insertion order within each position (stable sort), so the
    /// result does not depend on hash order or threading.
    pub fn to_csc(&self) -> Csc {
        let mut e = self.entries.clone();
        e.sort_by_key(|&(r, c, _)| (c, r));
        let mut col_ptr = vec![0usize; self.n + 1];
        let mut row_idx: Vec<u32> = Vec::with_capacity(e.len());
        let mut values: Vec<f64> = Vec::with_capacity(e.len());
        let mut last: Option<(u32, u32)> = None;
        for (r, c, v) in e {
            if last == Some((r, c)) {
                if let Some(x) = values.last_mut() {
                    *x += v;
                }
            } else {
                row_idx.push(r);
                values.push(v);
                col_ptr[c as usize + 1] += 1;
                last = Some((r, c));
            }
        }
        for k in 0..self.n {
            col_ptr[k + 1] += col_ptr[k];
        }
        Csc { n: self.n, col_ptr, row_idx, values }
    }
}

/// Square CSC matrix with sorted row indices within each column.
#[derive(Debug, Clone, PartialEq)]
pub struct Csc {
    pub n: usize,
    pub col_ptr: Vec<usize>,
    pub row_idx: Vec<u32>,
    pub values: Vec<f64>,
}

impl Csc {
    pub fn nnz(&self) -> usize {
        self.values.len()
    }

    pub fn memory_bytes(&self) -> usize {
        self.values.len() * std::mem::size_of::<f64>()
            + self.row_idx.len() * std::mem::size_of::<u32>()
            + self.col_ptr.len() * std::mem::size_of::<usize>()
    }

    pub fn column(&self, c: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        let (a, b) = (self.col_ptr[c], self.col_ptr[c + 1]);
        self.row_idx[a..b].iter().zip(&self.values[a..b]).map(|(&r, &v)| (r as usize, v))
    }

    pub fn get(&self, r: usize, c: usize) -> f64 {
        let (a, b) = (self.col_ptr[c], self.col_ptr[c + 1]);
        match self.row_idx[a..b].binary_search(&(r as u32)) {
            Ok(k) => self.values[a + k],
            Err(_) => 0.0,
        }
    }

    pub fn diagonal(&self) -> Vec<f64> {
        (0..self.n).map(|i| self.get(i, i)).collect()
    }

    /// y = A x
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; self.n];
        for (c, &xc) in x.iter().enumerate().take(self.n) {
            for (r, v) in self.column(c) {
                y[r] += v * xc;
            }
        }
        y
    }

    /// Largest |A_ij - A_ji| relative to the largest |A_ij|.
    pub fn symmetry_error(&self) -> f64 {
        let mut max_abs = 0.0_f64;
        let mut max_diff = 0.0_f64;
        for c in 0..self.n {
            for (r, v) in self.column(c) {
                max_abs = max_abs.max(v.abs());
                max_diff = max_diff.max((v - self.get(c, r)).abs());
            }
        }
        if max_abs == 0.0 { 0.0 } else { max_diff / max_abs }
    }

    pub fn all_finite(&self) -> bool {
        self.values.iter().all(|v| v.is_finite())
    }
}

pub fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// ‖A x − b‖₂ / max(‖b‖₂, ‖A‖·‖x‖ scale), measured against the original operator.
pub fn relative_residual(a: &Csc, x: &[f64], b: &[f64]) -> f64 {
    let ax = a.mul_vec(x);
    let r: Vec<f64> = ax.iter().zip(b).map(|(p, q)| p - q).collect();
    let denom = norm2(b).max(norm2(&ax));
    if denom == 0.0 { norm2(&r) } else { norm2(&r) / denom }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicates_summed_and_sorted() {
        let mut t = TripletBuilder::new(3).expect("n");
        t.add(2, 0, 1.0).expect("ok");
        t.add(0, 0, 2.0).expect("ok");
        t.add(2, 0, 3.0).expect("ok");
        t.add(1, 2, 5.0).expect("ok");
        let a = t.to_csc();
        assert_eq!(a.col_ptr, vec![0, 2, 2, 3]);
        assert_eq!(a.row_idx, vec![0, 2, 1]);
        assert_eq!(a.values, vec![2.0, 4.0, 5.0]);
        assert_eq!(a.get(2, 0), 4.0);
        assert_eq!(a.get(1, 1), 0.0);
    }

    #[test]
    fn out_of_range_and_nan_rejected() {
        let mut t = TripletBuilder::new(2).expect("n");
        assert!(t.add(2, 0, 1.0).is_err());
        assert!(t.add(0, 0, f64::NAN).is_err());
    }

    #[test]
    fn matvec() {
        let mut t = TripletBuilder::new(2).expect("n");
        for (r, c, v) in [(0, 0, 2.0), (0, 1, 1.0), (1, 0, 1.0), (1, 1, 3.0)] {
            t.add(r, c, v).expect("ok");
        }
        let a = t.to_csc();
        assert_eq!(a.mul_vec(&[1.0, 2.0]), vec![4.0, 7.0]);
        assert_eq!(a.symmetry_error(), 0.0);
    }
}
