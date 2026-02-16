# 1. Executive Summary

## What Torc Is

Torc is a programming language, ecosystem, and build system designed from first principles for AI authorship and machine execution. It abandons the assumptions that have shaped every programming language to date — that programs are text, that execution is sequential, that types must be simple enough for humans to hold in their heads, and that compilation is a separate phase from verification.

Instead, Torc represents programs as directed semantic graphs with rich type annotations, expresses computation as dataflow rather than control flow, carries formal verification proofs as part of the program structure, and materializes executables through constraint satisfaction against declarative hardware and platform models.

## What Torc Is Not

Torc is not a language humans write directly. It has no syntax in the traditional sense — no keywords, no operator precedence, no formatting conventions. An AI system constructs Torc programs by building and manipulating graph structures through the Torc API. Humans interact with Torc programs through an observability layer that projects the graph into human-comprehensible views: dependency diagrams, property summaries, resource budgets, and when necessary, pseudo-code approximations.

Torc is also not a purely theoretical exercise. It is designed to target real hardware that exists today — x86_64, ARM64, ARMv7, RISC-V, PowerPC — through real platform environments including Linux, Windows, macOS, bare-metal RTOS, and AUTOSAR. It achieves this by using LLVM as its initial materialization backend while maintaining the ability to adopt or develop native backends as the ecosystem matures.

## Why Now

Three converging trends make Torc timely:

1. **AI code generation has reached production quality.** AI systems now write substantial portions of production software. A language designed for AI authorship removes the impedance mismatch between how AI thinks about computation and how current languages force it to express that computation.

2. **Formal verification tools have matured.** SMT solvers, proof assistants, and abstract interpretation frameworks are now fast enough to verify meaningful program properties in practical timeframes. Torc integrates these as first-class infrastructure rather than aftermarket tooling.

3. **Hardware diversity is exploding.** The end of Dennard scaling has driven a proliferation of specialized hardware: GPUs, TPUs, NPUs, FPGAs, RISC-V custom extensions. A language that treats hardware targeting as constraint satisfaction rather than per-target compiler engineering is increasingly necessary.

## The Ecosystem at a Glance

```
torc                          — The unified CLI tool (like rustup + cargo + rustc combined)
├── torc init                 — Initialize a new Torc project
├── torc build                — Materialize for a target (or all targets)
├── torc verify               — Run formal verification passes
├── torc inspect              — Launch human observability interface
├── torc publish              — Publish a graph module to the Torc Registry
├── torc fetch                — Fetch graph modules from the registry
├── torc target add           — Add a target platform model
├── torc target list          — List available and installed target models
├── torc platform describe    — Dump the constraint model for a target
├── torc ffi bridge           — Generate interop bridges to/from C, Rust, etc.
└── torc doctor               — Diagnose toolchain and dependency health

The Torc Registry             — Centralized + federated module hosting
Target Platform Model Library  — Community-maintained hardware/OS models
Observability Studio           — Visual inspection and debugging tool
```

## Design Constraints

Torc is designed under these non-negotiable constraints:

1. **Must produce real executables for real hardware today.** No vaporware. The initial release targets Linux x86_64, Windows x86_64, Linux ARM64, and bare-metal ARM Cortex-M.
2. **Must interoperate with existing code.** C FFI is mandatory. Rust and C++ interop are high priority. Existing libraries must be callable.
3. **Must be formally verifiable.** Every Torc program carries proof obligations. The materialization engine refuses to emit code unless all proofs are discharged or explicitly waived with documented justification.
4. **Must support human oversight.** The observability layer is not optional. Every program must be inspectable by humans through multiple projection views.
5. **Must be open.** The specification, reference implementation, and core tooling are open source. The registry supports both public and private hosting.
