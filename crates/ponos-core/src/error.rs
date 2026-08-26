//! Cross-cutting domain errors shared by core consumers.

/// Signals `ponos.exit(code)`: unwinds the run; the code wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitSignal {
    pub code: i32,
}

impl std::fmt::Display for ExitSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ponos.exit({})", self.code)
    }
}

impl std::error::Error for ExitSignal {}
