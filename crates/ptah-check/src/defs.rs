//! The Luau type definitions for the `ptah` script API.
//!
//! Single source of truth: `.ptah/ptah.d.luau`, embedded at compile
//! time so the emitted definitions always match the installed binary.
//! Consumers: `ptah types` (prints them) and `ptah check`'s luau-lsp
//! pass (typechecks against them).

/// Luau type definitions for the `ptah` script API.
pub const TYPE_DEFINITIONS: &str = include_str!("../../../.ptah/ptah.d.luau");
