//! Human observability layer for the Torc language.
//!
//! Provides projection views (dataflow, contract, resource budget, pseudo-code,
//! provenance) that make binary graph programs comprehensible to humans.

pub mod contract_table;
pub mod dataflow;
pub mod decision_view;
pub mod error;
pub mod format;
pub mod provenance;
pub mod pseudo_code;
pub mod resource_budget;
pub mod view;

pub use contract_table::ContractView;
pub use dataflow::DataflowView;
pub use decision_view::DecisionView;
pub use error::ObserveError;
pub use format::{bar_chart, format_bytes, format_predicate, format_time_ns, node_display_name};
pub use provenance::ProvenanceView;
pub use pseudo_code::PseudoCodeView;
pub use resource_budget::ResourceBudgetView;
pub use view::{available_views, RenderContext, View, ViewFormat, ViewKind, ViewOutput};
