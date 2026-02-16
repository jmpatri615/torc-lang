//! Binary graph serialization format (.trc) for the Torc language.
//!
//! Handles reading and writing Torc Binary Graph files with content-addressed
//! integrity verification.
//!
//! ## File Layout
//!
//! ```text
//! TRC File Layout:
//! ┌──────────────────────────────┐
//! │ Magic: 0x54524300 ("TRC\0") │  4 bytes
//! │ Version: major.minor.patch   │  3 bytes
//! │ Flags                        │  1 byte
//! ├──────────────────────────────┤
//! │ Header                       │
//! │   node_count: u64            │
//! │   edge_count: u64            │
//! │   region_count: u64          │
//! │   payload_length: u64        │
//! ├──────────────────────────────┤
//! │ JSON payload                 │
//! │   (graph data)               │
//! ├──────────────────────────────┤
//! │ Content Hash (SHA-256)       │  32 bytes
//! └──────────────────────────────┘
//! ```

mod format;

pub use format::{TrcError, TrcFile, TrcFlags, TrcVersion};
