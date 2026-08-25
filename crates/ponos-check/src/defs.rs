//! The Luau type definitions for the `ponos` script API.
//!
//! Single source of truth: `.ponos/ponos.d.luau`, embedded at compile
//! time so the emitted definitions always match the installed binary.
//! Consumers: `ponos types` (prints them) and `ponos check`'s luau-lsp
//! pass (typechecks against them).

/// Luau type definitions for the `ponos` script API.
pub const TYPE_DEFINITIONS: &str = include_str!("../../../.ponos/ponos.d.luau");
