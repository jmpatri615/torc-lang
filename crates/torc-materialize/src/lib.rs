//! Materialization engine for the Torc language.
//!
//! Transforms a Torc program graph into an executable artifact for a specific target
//! through a 6-phase pipeline: canonicalization, verification, transformation,
//! resource fitting, code emission, and post-materialization verification.
