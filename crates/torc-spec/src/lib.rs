//! Specification interface for the Torc language.
//!
//! Implements the Decision State Model from spec section 13: a collaborative
//! system for resolving human design intent into engineering decisions.
//!
//! The core abstraction is the [`DecisionGraph`], which tracks decisions,
//! assumptions, and state transition history. Every design decision occupies
//! one of seven states (Unexplored, Deferred, Exploring, Tentative,
//! Committed, Derived, Conflicted) and transitions between them are
//! recorded with full rationale.

pub mod assumption;
pub mod conflict;
pub mod decision;
pub mod error;
pub mod graph;
pub mod history;
pub mod impact;
pub mod serialize;

pub use assumption::{Assumption, AssumptionId, Confidence, ImpactLevel};
pub use conflict::{blocking_decisions, check_circular_deps, find_conflicts};
pub use decision::{
    verification_mode, Decision, DecisionId, DecisionState, DecisionValue, RevisitTrigger,
    VerificationMode,
};
pub use error::SpecError;
pub use graph::{DecisionGraph, StatusSummary};
pub use history::StateTransition;
pub use impact::{
    analyze_commit_impact, Concern, ConcernSeverity, DerivedConsequence, Exclusion, ImpactReport,
};
pub use serialize::TdgFile;
