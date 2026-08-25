//! Pure domain logic: task bookkeeping, turn/tool fold semantics, result
//! contracts, the config model, structured events, ports, and shared
//! error types.
//!
//! Everything here is I/O-free and adapter-free: no filesystem, no
//! process spawning, no socket/channel I/O, and no imports from the
//! adapter modules (`acp`, `render`, `config_fs`, `check`, `script`,
//! `bridge`, `result_wire`) or from `cli`. The dependency arrow points
//! inward only — adapters depend on core, never the reverse.
//!
//! Two dependency notes, settled by the restructure's design:
//!
//! - `task` carries mlua value types (`MultiValue`, `mlua::Error`): task
//!   results *are* Luau values, and the spawn bookkeeping drives mlua
//!   coroutines. Data-level mlua, no interpreter surface of its own.
//! - `turn` folds `agent_client_protocol` schema types: the fold's input
//!   is the ACP update stream. Schema data types only — this module
//!   never opens a connection.
pub mod error;
pub mod task;
pub mod text;
pub mod turn;
