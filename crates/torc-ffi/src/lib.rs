//! Foreign function interface bridge generation for the Torc language.
//!
//! Generates interop bridges between Torc graphs and C code, with
//! runtime contract checking at FFI boundaries.
//!
//! ## Modules
//!
//! - [`trust`] — Trust level classification for foreign functions
//! - [`csig`] — C function signature parser
//! - [`declaration`] — `.ffi.toml` declaration file parsing
//! - [`marshal`] — C ↔ Torc type mapping and marshaling strategies
//! - [`bridge_from_c`] — Generate Torc wrapper graphs for C functions
//! - [`bridge_to_c`] — Generate C headers from Torc graph exports
//! - [`policy`] — Project-level trust policy enforcement

pub mod bridge_from_c;
pub mod bridge_to_c;
pub mod csig;
pub mod declaration;
pub mod error;
pub mod marshal;
pub mod policy;
pub mod trust;

// Re-export key types for convenience
pub use bridge_from_c::generate_bridge;
pub use bridge_to_c::generate_c_header;
pub use csig::{CSignature, CType};
pub use declaration::FfiDeclaration;
pub use error::FfiError;
pub use marshal::MarshalStrategy;
pub use policy::TrustPolicy;
pub use trust::TrustLevel;
