//! Backend selection behind one factor type.

use serde::Serialize;

use crate::skyline::{self, BackendCapabilities, FactorDiagnostics, MatrixClass, SkylineFactor, SolverError};
use crate::sparse::Csc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Skyline,
    #[cfg(feature = "faer")]
    Faer,
}

impl Default for Backend {
    /// faer when compiled in (decision 0005), otherwise skyline.
    fn default() -> Self {
        #[cfg(feature = "faer")]
        return Backend::Faer;
        #[cfg(not(feature = "faer"))]
        Backend::Skyline
    }
}

impl Backend {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skyline" => Some(Backend::Skyline),
            #[cfg(feature = "faer")]
            "faer" => Some(Backend::Faer),
            _ => None,
        }
    }

    pub fn capabilities(self) -> BackendCapabilities {
        match self {
            Backend::Skyline => skyline::capabilities(),
            #[cfg(feature = "faer")]
            Backend::Faer => crate::faer_backend::capabilities(),
        }
    }

    pub fn all() -> Vec<Backend> {
        vec![
            Backend::Skyline,
            #[cfg(feature = "faer")]
            Backend::Faer,
        ]
    }
}

pub enum Factor {
    Skyline(SkylineFactor),
    #[cfg(feature = "faer")]
    Faer(crate::faer_backend::FaerFactor),
}

pub fn factorize(backend: Backend, a: &Csc, class: MatrixClass, budget_bytes: usize) -> Result<Factor, SolverError> {
    match backend {
        Backend::Skyline => skyline::factorize(a, class, budget_bytes).map(Factor::Skyline),
        #[cfg(feature = "faer")]
        Backend::Faer => crate::faer_backend::factorize(a, class, budget_bytes).map(Factor::Faer),
    }
}

impl Factor {
    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        match self {
            Factor::Skyline(f) => f.solve(b),
            #[cfg(feature = "faer")]
            Factor::Faer(f) => f.solve(b),
        }
    }

    pub fn diagnostics(&self) -> &FactorDiagnostics {
        match self {
            Factor::Skyline(f) => &f.diagnostics,
            #[cfg(feature = "faer")]
            Factor::Faer(f) => &f.diagnostics,
        }
    }
}
