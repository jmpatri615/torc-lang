# Torc Implementation Plan

**Version:** 0.1.0
**Last Updated:** 2026-02-16
**Status:** Phase 7 Pass 2 — Complete

---

## Overview

This document tracks the implementation of the Torc programming language, ecosystem, and build system as defined in the [specification](./spec/). Implementation is organized into phases, each building on the previous. Each phase has clear deliverables and acceptance criteria.

The reference implementation is written in **Rust**, chosen for its memory safety guarantees, performance characteristics, and strong ecosystem for systems programming (LLVM bindings, SMT solver bindings, binary format handling).

---

## Phase Summary

| Phase | Name | Description | Status |
|-------|------|-------------|--------|
| 0 | Project Setup | Repo structure, CI, workspace layout | **In Progress** |
| 1 | Core Graph Model | Nodes, edges, regions, basic types | **Complete** |
| 2 | Type System | Full type universe implementation | **Complete** |
| 3 | Contract System | Contracts, predicates, proof obligations | **Complete** |
| 4 | TRC Binary Format | Serialization/deserialization of .trc files | **Complete** |
| 5 | Graph Construction API | Programmatic API for building graphs | **Complete** |
| 6 | Verification Framework | SMT integration, structural analysis, proof caching | **Complete** |
| 7 | Materialization Engine | Graph-to-executable pipeline via LLVM | **Pass 2 Complete** |
| 8 | Target Platform Models | ISA, microarchitecture, environment model parsing | **Pass 1 Complete** |
| 9 | CLI Tool (`torc`) | Unified command-line interface | Not Started |
| 10 | Observability Layer | Projection views, pseudo-code generation | Not Started |
| 11 | FFI Bridge | C interop (Rust interop stretch goal) | Not Started |
| 12 | Registry Client | Package fetching and publishing | Not Started |
| 13 | Integration & Examples | End-to-end example applications | Not Started |

---

## Phase 0: Project Setup

**Goal:** Establish the Rust workspace, CI pipeline, and project conventions.

### Tasks

- [x] Initialize git repository
- [x] Deploy specification documents to `spec/`
- [x] Create `.gitignore`
- [x] Create `README.md`
- [x] Create this implementation plan
- [x] Initialize Rust workspace (`Cargo.toml`) with member crates
- [x] Create crate stubs for all major components
- [ ] Set up CI (GitHub Actions: build, test, clippy, fmt)
- [ ] Establish coding conventions (error handling patterns, naming, module structure)
- [x] Add LICENSE file (Apache-2.0)

### Crate Structure

```
torc/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── torc-core/             # Graph model, types, contracts
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── graph/          # Node, Edge, Region
│   │       ├── types/          # Type universe
│   │       ├── contract/       # Contract model
│   │       └── provenance/     # Provenance tracking
│   ├── torc-trc/              # Binary format (.trc)
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── torc-verify/           # Verification framework
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── torc-materialize/      # Materialization engine
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── torc-targets/          # Platform model parsing
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── torc-ffi/              # FFI bridge generation
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── torc-observe/          # Observability layer
│   │   ├── Cargo.toml
│   │   └── src/
│   └── torc-registry/         # Registry client
│       ├── Cargo.toml
│       └── src/
├── cli/
│   └── torc/                    # CLI binary
│       ├── Cargo.toml
│       └── src/
├── spec/                       # Language specification
└── tests/
    └── integration/            # Cross-crate integration tests
```

### Acceptance Criteria

- `cargo build` succeeds for all crates (even if they're mostly empty)
- `cargo test` runs (even with zero tests)
- `cargo clippy` passes with no warnings
- `cargo fmt --check` passes

---

## Phase 1: Core Graph Model

**Goal:** Implement the fundamental graph data structures — nodes, edges, and regions — as defined in spec section 3.

### Tasks

- [x] Define `NodeId` (UUID, content-addressed via SHA-256)
- [x] Define `EdgeId` and `RegionId`
- [x] Implement `NodeKind` enum with all categories:
  - [x] Primitive computation: `Literal`, `Arithmetic`, `Bitwise`, `Comparison`, `Conversion`
  - [x] Data structure: `Construct`, `Destructure`, `Index`, `Slice`
  - [x] Control flow: `Select`, `Switch`, `Iterate`, `Recurse`, `Fixpoint`
  - [x] Effect: `Allocate`, `Deallocate`, `Read`, `Write`, `Atomic`, `Fence`, `Syscall`, `FFICall`
  - [x] Meta: `Verify`, `Assume`, `Measure`, `Checkpoint`, `Annotate`
  - [x] Probabilistic: `Sample`, `Condition`, `Expectation`, `Entropy`, `Approximate`
- [x] Implement `Node` struct with all fields (id, kind, contract, type_sig, provenance, annotations)
- [x] Implement `Edge` struct (id, source, target, type, lifetime, bandwidth)
- [x] Implement `RegionKind` enum: `Sequential`, `Parallel`, `Conditional`, `Iterative`, `Atomic`
- [x] Implement `Region` struct (id, kind, constraints, children, interfaces)
- [x] Implement `Port` (input/output interface points)
- [x] Implement the `Graph` container:
  - [x] Node storage and lookup by ID
  - [x] Edge storage with source/target indexing
  - [x] Region hierarchy (nested regions)
  - [x] Topological ordering / dependency analysis
  - [x] Subgraph extraction
- [x] Implement content-addressed hashing for nodes (SHA-256 of content)
- [x] Implement graph well-formedness validation:
  - [x] No dangling edges
  - [x] Port type/arity consistency
  - [x] Region containment consistency
  - [x] DAG property at expression level (cycles only via Iterate/Recurse)

### Key Design Decisions

- Use `petgraph` or a custom adjacency-list graph? **Decision: Custom.** HashMap-based storage with outgoing/incoming edge indexes, optimized for content-addressing and port-based edges.
- UUID generation: use `uuid` crate with v4 for runtime IDs; SHA-256 content hashing in `torc-core::hash` for content addressing.

### Acceptance Criteria

- Can programmatically construct a graph with nodes, edges, and regions
- Content-addressed IDs are deterministic (same content = same ID)
- Well-formedness validation catches invalid graphs
- All node kinds are representable
- Unit tests for graph construction, validation, and topological ordering

---

## Phase 2: Type System

**Goal:** Implement the full type universe as defined in spec section 4.

### Tasks

- [x] Implement primitive types: `Void`, `Unit`, `Bool`, `Int<W,S>`, `Float<P>`, `Fixed<W,F>`
- [x] Implement refinement types: type + predicate (`where` clause)
- [x] Implement composite types: `Tuple`, `Record`, `Variant`, `Array`, `Vec`
- [x] Implement linear/affine types: `Linear<T>`, `Affine<T>`, `Shared<T>`, `Unique<T>`, `Counted<T>`
- [x] Implement effect types: `Pure`, `Alloc<R>`, `IO<D>`, `Atomic<O>`, `FFI<ABI>`, `Diverge`, `Panic`
- [x] Implement resource types: `Timed<T,B>`, `Sized<T,S>`, `Powered<T,E>`, `Bandwidth<T,R>`
- [x] Implement dependent types: types parameterized by values (e.g., `Matrix<T, Rows, Cols>`)
- [x] Implement probability types: `Distribution<T>`, `Posterior<T,E>`, `Interval<T,C>`, `Approximate<T,Err>`
- [x] Implement `TypeSignature` (input types + output types for nodes)
- [x] Implement type compatibility checking:
  - [x] Consistency check (edge source/target type compatibility)
  - [x] Linearity check (consumer count matches annotation)
  - [x] Effect check (declared effects superset of actual)
  - [x] Resource check (bounds consistency)
  - [x] Refinement check (generate proof obligations)
- [x] Implement type display/formatting for observability

### Key Design Decisions

- Represent the predicate language for refinement types. Need a small expression AST for first-order logic with arithmetic.
- Dependent type values: represent as compile-time constants or symbolic expressions?

### Acceptance Criteria

- All type kinds from the spec are representable
- Type compatibility checking works correctly
- Linearity violations are detected
- Effect propagation works (a node calling IO inherits IO)
- Refinement predicates generate proof obligations (stubs for now)

---

## Phase 3: Contract System

**Goal:** Implement the contract model and predicate language as defined in spec section 3.

### Tasks

- [x] Implement `Contract` struct:
  - [x] `preconditions: Vec<Predicate>`
  - [x] `postconditions: Vec<Predicate>`
  - [x] `time_bound: Option<TimeBound>`
  - [x] `memory_bound: Option<MemoryBound>`
  - [x] `energy_bound: Option<EnergyBound>`
  - [x] `stack_bound: Option<StackBound>`
  - [x] `effects: EffectSet`
  - [x] `failure_modes: Vec<FailureMode>`
  - [x] `recovery_strategy: RecoveryStrategy`
  - [x] `proof_status: ProofStatus`
  - [x] `proof_witness: Option<ProofWitness>`
- [x] Implement the predicate language AST:
  - [x] Value constraints (`output >= 0 && output <= 4095`)
  - [x] Relational constraints (`output == f(input)`)
  - [x] Temporal constraints (`execution_time <= 50us`)
  - [x] Resource constraints (`heap_allocated == 0`)
  - [x] Invariant preservation (`sorted(output) && permutation(output, input)`)
  - [x] Information flow (`no_leak(secret_input, public_output)`)
- [x] Implement `ProofStatus` enum: `Verified`, `Assumed`, `Pending`, `Waived`
- [x] Implement `ProofWitness` structure (opaque proof object, content-addressed)
- [x] Implement `FailureMode` and `RecoveryStrategy` enums
- [x] Implement `EffectSet` (composable effect tracking)
- [x] Implement proof obligation generation from contracts:
  - [x] Type refinement obligations
  - [x] Pre/postcondition obligations
  - [x] Resource bound obligations (stubs — need target model)
  - [x] Linearity obligations
  - [x] Termination obligations
- [x] Implement `Waiver` struct with justification, author, approval, expiration

### Acceptance Criteria

- Contracts can be attached to nodes
- Predicate AST can represent all predicate forms from the spec
- Proof obligations are generated from contracts
- Effect sets compose correctly
- Waivers are representable with full metadata

---

## Phase 4: TRC Binary Format

**Goal:** Implement serialization and deserialization of the .trc binary graph format as defined in spec section 3.

### Tasks

- [x] Define the binary layout:
  - [x] Magic bytes: `0x54524300` ("TRC\0")
  - [x] Version field (major.minor.patch)
  - [x] Flags byte (COMPRESSED, HAS_PROOFS, HAS_PROVENANCE)
  - [x] Header (node/edge/region counts + payload length)
  - [ ] Packed tables (Node, Edge, Region, Contract, Proof, String, Provenance) — deferred to v1.0, using JSON payload for 0.x
- [x] Implement serialization (`Graph -> Vec<u8>`) via JSON payload
- [x] Implement deserialization (`&[u8] -> Graph`) with header count validation
- [x] Implement content-hash verification on load (SHA-256)
- [ ] Implement incremental graph reading — deferred (requires table-based layout)
- [x] Implement graph merging (for module linking): `Graph::merge()` + `merge_trc_files()`
- [x] Add format versioning and migration support
- [x] Implement HAS_PROOFS flag detection from proof witnesses
- [x] Implement header count mismatch detection (`CountMismatch` error)

### Design Decisions

- **JSON payload for 0.x:** The spec defines 7 packed tables, but all core types already derive `Serialize`/`Deserialize`. JSON is correct, debuggable, and simpler. Table-based layout is deferred to v1.0.
- **Deferred items:** Incremental graph reading (requires table-based layout with offset-based random access), COMPRESSED flag (defined but not implemented), packed binary tables.

### Acceptance Criteria

- [x] Round-trip: serialize a graph and deserialize it back to an identical graph
- [x] Content hash is verified on load; corrupted files are rejected
- [x] Rich graphs with all metadata (contracts, provenance, annotations, edge metadata, nested regions) survive round-trip
- [x] Format version is checked; incompatible versions are rejected
- [x] Header counts validated against deserialized graph
- [x] Graph merging works with conflict detection

---

## Phase 5: Graph Construction API

**Goal:** Provide a programmatic Rust API for building Torc graphs, suitable for use by AI systems.

### Tasks

- [x] Design the builder API (`GraphBuilder`):
  - [x] `add_node(kind, type_sig, contract) -> NodeId`
  - [x] `add_edge(source, target, edge_type) -> EdgeId`
  - [x] `begin_region(kind) / end_region() -> RegionId`
  - [x] `add_annotation(node_id, key, value)`
  - [x] `set_provenance(node_id, provenance)`
- [x] Implement validation during construction:
  - [x] Type checking on edge creation
  - [x] Linearity checking
  - [x] Region containment enforcement
- [x] Implement graph manipulation:
  - [x] Replace subgraph
  - [x] Inline region
  - [x] Extract subgraph as module
  - [x] Graph composition (connect two graphs at interface ports)
- [x] Implement a convenience layer for common patterns:
  - [x] Arithmetic expressions
  - [x] Conditional selection
  - [x] Iteration construction
  - [x] FFI call wrapping
- [x] Graph removal primitives (`remove_node`, `remove_edge`, `remove_region`)
- [x] Additional convenience constructors (`add_switch`, `add_iterate`, `add_recurse`, `add_read`, `add_write`)

### Acceptance Criteria

- [x] Can build the Clarke transform example from spec section 12 using the API
- [x] Can build the safety monitor example from spec section 12
- [x] Invalid constructions are rejected with clear error messages
- [x] API is ergonomic for programmatic (non-human) use

---

## Phase 6: Verification Framework

**Goal:** Implement the verification infrastructure as defined in spec section 10.

### Tasks

- [x] Implement proof obligation registry (collect, track, cache)
- [x] Implement structural analysis engine:
  - [x] Linearity verification (graph structure only, no SMT)
  - [x] Effect propagation verification
  - [x] Graph well-formedness
  - [x] Ownership tracking
- [x] Integrate Z3 SMT solver:
  - [x] Rust bindings (via `z3` crate, feature-gated)
  - [x] Translate predicate AST to Z3 assertions
  - [x] Handle solver results (sat/unsat/unknown/timeout)
  - [x] Extract counterexamples on failure
- [x] Implement abstract interpretation (basic):
  - [x] Numeric range analysis (interval domain)
  - [x] Used for pre-screening before SMT
- [x] Implement proof witness generation and storage
- [x] Implement proof caching (content-addressed, incremental)
- [x] Implement verification reporting:
  - [x] Summary statistics
  - [x] Detailed failure diagnostics with counterexamples
  - [x] Suggestion generation (clamp, strengthen pre, weaken post, waive)
- [x] Implement verification profiles:
  - [x] `development` (fast, incremental, skip WCET)
  - [x] `integration` (full, with WCET)
  - [x] `certification` (exhaustive, independent proof checking)
- [x] Implement waiver management

### Key Dependencies

- Z3 or CVC5 must be available at build time (system dependency or vendored)
- Consider making the solver backend pluggable

### Acceptance Criteria

- Can verify simple arithmetic refinement predicates via Z3
- Linearity violations are caught by structural analysis
- Proof results are cached and reused for unchanged subgraphs
- Verification failures produce actionable diagnostics
- Proof witnesses are generated and independently checkable

---

## Phase 7: Materialization Engine

**Goal:** Implement the 6-phase materialization pipeline as defined in spec section 6.

### Sub-phases

#### Phase 7a: Graph Canonicalization (Pass 1 — Complete)
- [x] Subgraph deduplication via content hashing
- [x] Trivial subgraph inlining
- [x] Region flattening (same-kind nesting only)
- [ ] Module reference resolution and linking

#### Phase 7b: Verification Integration (Pass 1 — Complete)
- [x] Wire up Phase 6 verification as a materialization gate
- [x] Implement the "halt on unproven obligation" logic
- [x] Implement waiver-aware gating

#### Phase 7c: Target-Aware Graph Transformation (Pass 1 — Complete)
- [x] Node lowering trait definitions (`NodeLowering`, `GraphTransform`)
- [x] Transform registry with `IdentityTransform` for testing
- [ ] Generic specialization / monomorphization
- [x] Execution scheduling (topological order with parallelism)
- [x] Memory layout estimation (heuristic, no LLVM)
- [ ] ABI conformance (calling convention adaptation)

#### Phase 7d: Resource Fitting (Pass 1 — Complete)
- [x] Flash/ROM size checking
- [x] RAM (static + stack + heap) checking
- [x] Stack depth analysis (heuristic)
- [ ] WCET analysis integration (basic, for known targets)
- [ ] Backtracking on resource constraint violation

#### Phase 7e: LLVM Code Emission (Pass 2 — Complete)
- [x] Integrate LLVM via `inkwell` (Rust LLVM bindings), feature-gated (`--features llvm`)
- [x] Torc Type → LLVM type mapping (primitives, composites, wrappers peel to base)
- [x] Node lowering: Literal, Arithmetic, Bitwise, Comparison, Select, Conversion
- [x] LLVM optimization pass configuration per profile (Debug/Balanced/Throughput/MinimalSize/DeterministicTiming)
- [x] Object file emission for Linux x86_64 via TargetMachine
- [x] Executable linking via system `cc`
- [x] LLVM IR and bitcode emission modes
- [ ] ELF emission for Linux ARM64 (Pass 3+)
- [ ] Bare-metal ELF for ARM Cortex-M (Pass 3+)

#### Phase 7f: Post-Materialization Verification (Pass 2 — Complete)
- [x] Binary size verification against predictions (5x tolerance)
- [ ] Symbol table validation (Pass 3+)
- [ ] Smoke test generation from contracts (stretch, Pass 3+)

#### Pass 1 Summary
- `torc-materialize`: 9 modules (error, canonicalize, gate, transform, schedule, layout, resource, report, pipeline)
- `materialize()` orchestrator: canonicalize → verify gate → transform → schedule + layout + resource fit → report
- 30 tests, 0 clippy warnings

#### Pass 2 Summary
- `codegen/` submodule: 6 files (mod, context, types, lower, emit, profile)
- `postverify` module for binary size verification
- Pipeline extended: emit_code → post-verify stages (feature-gated behind `llvm`)
- MaterializationReport extended with codegen fields
- 32 new tests (28 codegen + 4 postverify), 62 total in torc-materialize, 258 across workspace with llvm
- 0 clippy warnings on both `--workspace` and `--features llvm`

### Key Dependencies

- LLVM libraries (system or vendored)
- `inkwell` crate for Rust-LLVM bindings

### Acceptance Criteria

- Can materialize a trivial Torc graph to a running Linux x86_64 ELF binary
- Resource checking correctly rejects programs that exceed target constraints
- Optimization profiles produce measurably different output characteristics
- Materialization reports are generated with resource utilization data

---

## Phase 8: Target Platform Models

**Goal:** Implement the 3-layer platform model system as defined in spec section 7.

### Tasks

- [ ] Define TOML schema for ISA models
- [ ] Define TOML schema for microarchitecture models
- [ ] Define TOML schema for environment models
- [ ] Implement model parsing and validation (from TOML files)
- [x] Implement model composition (ISA + uarch + env = platform)
- [x] Create reference models:
  - [x] `linux-x86_64` (Platform::generic_linux_x86_64)
  - [ ] `linux-aarch64-gnu`
  - [x] `bare-metal-arm-cortex-m4f` (Platform::stm32f407_discovery)
- [ ] Implement `torc target describe` output
- [x] Implement resource constraint extraction from models (for Phase 7d)

### Pass 1 Summary
- `torc-targets`: 4 modules (isa, microarch, environment, platform)
- 3-layer model: IsaModel + MicroarchModel + EnvironmentModel = Platform
- Built-in constructors: x86_64, ARMv7-M, Cortex-M4, Linux, bare-metal ARM, STM32F407
- ResourceConstraints derived from Platform for materialization
- 9 tests, 0 clippy warnings

### Acceptance Criteria

- Can parse and validate the STM32F407 example model from spec section 7
- Platform models provide all data needed by the materialization engine
- Custom target models can be authored and validated

---

## Phase 9: CLI Tool (`torc`)

**Goal:** Implement the unified CLI as defined in spec section 5.

### Tasks

- [ ] Set up CLI framework (use `clap`)
- [ ] Implement `torc init` — project scaffolding
- [ ] Implement `torc build` — invoke materialization engine
  - [ ] `--target`, `--all-targets`, `--release`, `--profile`
  - [ ] `--emit=llvm-ir`, `--emit=asm`, `--emit=graph-stats`
  - [ ] `--check-resources`
- [ ] Implement `torc verify` — invoke verification framework
  - [ ] `--module`, `--contract`, `--report`, `--status`
  - [ ] `--incremental`
- [ ] Implement `torc inspect` — launch observability views
  - [ ] `--view dataflow|contracts|resources|pseudo-code|provenance|diff`
  - [ ] `--module`, `--node`
- [ ] Implement `torc target` subcommands
  - [ ] `add`, `list`, `describe`, `validate`
- [ ] Implement `torc doctor` — toolchain diagnostics
- [ ] Implement `torc clean`
- [ ] Implement `torc.toml` project manifest parsing
- [ ] Implement configuration hierarchy (defaults, system, project, CLI, env vars)

### Deferred to Later

- [ ] `torc add` / `torc remove` / `torc update` (needs registry — Phase 12)
- [ ] `torc publish` / `torc login` (needs registry — Phase 12)
- [ ] `torc ffi bridge` (needs FFI — Phase 11)
- [ ] `torc toolchain` / `torc component` (needs distribution infrastructure)

### Acceptance Criteria

- `torc init` creates a valid project skeleton
- `torc build` invokes the materialization engine and produces an executable
- `torc verify` runs verification and reports results
- `torc inspect` produces human-readable output for all implemented views
- Error messages are clear and actionable

---

## Phase 10: Observability Layer

**Goal:** Implement human-readable projection views as defined in spec section 9.

### Tasks

- [ ] Implement projection view framework (view trait, rendering pipeline)
- [ ] Implement **pseudo-code view**: generate procedural-style approximation from graph
- [ ] Implement **contract view**: tabular contract summary
- [ ] Implement **resource budget view**: ASCII bar charts of utilization
- [ ] Implement **dataflow view**: text-based graph rendering (for terminal)
  - [ ] Stretch: GraphViz DOT output for visual rendering
- [ ] Implement **provenance view**: creation/edit history display
- [ ] Implement **diff view**: semantic graph diff between versions
- [ ] Implement export formats: JSON, CSV
- [ ] Implement `torc inspect` integration (wire views to CLI)

### Acceptance Criteria

- Pseudo-code view produces readable output for the Clarke transform example
- Contract view produces a table matching the spec section 9 format
- Resource budget view shows bar charts with utilization percentages
- Views are exportable to JSON

---

## Phase 11: FFI Bridge

**Goal:** Implement C interoperability as defined in spec section 11.

### Tasks

- [ ] Implement FFI declaration parsing (`.ffi.toml` files)
- [ ] Implement Torc-to-C bridge generation:
  - [ ] Runtime precondition checks at boundary
  - [ ] ABI adaptation (struct layout, calling convention)
  - [ ] Postcondition validation on return
  - [ ] Result wrapping (null -> Option::None, etc.)
- [ ] Implement C-to-Torc bridge generation:
  - [ ] C header generation from Torc graph interfaces
  - [ ] Export symbol generation
  - [ ] Contract documentation in header comments
- [ ] Implement trust levels: `verified`, `platform`, `audited`, `unsafe`
- [ ] Implement data marshaling:
  - [ ] Primitive types (direct mapping)
  - [ ] Structs (ABI-compatible layout)
  - [ ] Strings (UTF-8 + null terminator)
  - [ ] Arrays (pointer + length)
- [ ] Implement `torc ffi bridge` CLI subcommand

### Stretch Goal

- [ ] Rust interop bridge (leveraging ownership model alignment)

### Acceptance Criteria

- Can declare a C library interface and generate Torc wrapper nodes
- Can export Torc functions with C-compatible headers
- Runtime checks are inserted at FFI boundaries
- Generated C headers include contract documentation

---

## Phase 12: Registry Client

**Goal:** Implement the package registry client as defined in spec section 8.

### Tasks

- [ ] Define registry API protocol (HTTP + content-addressed storage)
- [ ] Implement module manifest parsing
- [ ] Implement dependency resolution (semver with contract awareness)
- [ ] Implement `torc add` / `torc remove` / `torc update`
- [ ] Implement `torc tree` — dependency tree display
- [ ] Implement `torc publish` — package publishing
- [ ] Implement `torc audit` — dependency auditing
- [ ] Implement local module cache
- [ ] Implement content-addressed integrity verification
- [ ] Implement registry authentication

### Deferred

- [ ] Private registry hosting
- [ ] Federated registry resolution
- [ ] Proof library publishing and resolution
- [ ] Cryptographic signing infrastructure

### Acceptance Criteria

- Can publish a module to a local test registry
- Can fetch and resolve dependencies
- Dependency tree is displayed correctly
- Content integrity is verified on fetch

---

## Phase 13: Integration & Examples

**Goal:** Validate the complete system with end-to-end examples from the spec.

### Tasks

- [ ] Implement the checksum example from spec section 3 (textual projection)
- [ ] Implement the Clarke transform from spec section 12
- [ ] Implement the safety monitor from spec section 12
- [ ] Implement the PID controller module
- [ ] Build the complete FOC motor controller example:
  - [ ] Materialize for Linux x86_64 simulation target
  - [ ] Materialize for STM32F407 bare-metal target (if Phase 7e embedded support complete)
- [ ] Produce verification report matching spec section 12 output
- [ ] Produce resource utilization report matching spec section 12 output
- [ ] Document the end-to-end workflow

### Acceptance Criteria

- At least one non-trivial Torc program materializes to a running executable
- Verification produces meaningful results (not just "all trivially pass")
- Observability views produce output matching spec examples
- The workflow from graph construction to executable is fully automated via `torc`

---

## Cross-Cutting Concerns

### Error Handling

- Use `thiserror` for library error types
- Use `anyhow` in the CLI for ergonomic error reporting
- All errors should be actionable: tell the user what went wrong and suggest a fix

### Testing Strategy

- Unit tests in each crate (`#[cfg(test)]` modules)
- Integration tests in `tests/integration/`
- Property-based testing with `proptest` for graph operations and serialization
- Snapshot tests for observability output

### Documentation

- Rustdoc for all public APIs
- Architecture decision records (ADRs) for significant design choices
- Spec cross-references in code comments

### Performance

- Profile critical paths (serialization, verification, materialization)
- Benchmark suite for regression detection
- Target: incremental materialization under 2 seconds for localized changes

---

## Dependencies (External)

| Dependency | Purpose | Phase |
|------------|---------|-------|
| `uuid` | Node/edge/region identifiers | 1 |
| `sha2` | Content-addressed hashing | 1 |
| `serde` / `serde_json` | Serialization infrastructure | 1 |
| `toml` | Config and manifest parsing | 5, 8, 9 |
| `clap` | CLI framework | 9 |
| `z3` / `z3-sys` | SMT solver integration | 6 |
| `inkwell` / `llvm-sys` | LLVM bindings for code emission | 7 |
| `petgraph` | Graph algorithms (evaluation needed) | 1 |
| `proptest` | Property-based testing | 1+ |
| `thiserror` / `anyhow` | Error handling | 1+ |
| `reqwest` | HTTP client for registry | 12 |
| `indicatif` | CLI progress bars | 9 |

---

## Milestone Targets

### M1: "Hello Graph" — Phases 0-4 complete
A Torc graph can be constructed programmatically, serialized to `.trc`, deserialized, and the round-trip verified.

### M2: "Verified Graph" — Phase 6 complete
Proof obligations are generated from contracts and discharged via Z3. Verification reports are produced.

### M3: "First Executable" — Phase 7 (partial) complete
A trivial Torc graph materializes to a running Linux x86_64 ELF binary via LLVM.

### M4: "Developer Preview" — Phases 8-10 complete
The `torc` CLI supports init, build, verify, and inspect. Platform models are parsed. Observability views work.

### M5: "Interop" — Phase 11 complete
Torc programs can call C libraries and be called from C code.

### M6: "Ecosystem" — Phase 12-13 complete
Packages can be published and fetched. The FOC motor controller example works end-to-end.

---

## Open Questions

1. **Graph library choice:** Use `petgraph` as a foundation or build a custom graph structure optimized for content-addressing and port-based edges?
2. **Z3 integration strategy:** Vendor Z3 or require it as a system dependency? The `z3` crate supports both.
3. **LLVM version:** Which LLVM version to target? `inkwell` supports LLVM 14-18. Need to balance feature availability with platform support.
4. **Provenance storage:** Store full provenance in-memory or use a separate on-disk database for large projects?
5. **Incremental materialization strategy:** File-level granularity (re-materialize changed modules) or node-level (patch binary in place)?

---

*This is a living document. Update it as implementation progresses and decisions are made.*
