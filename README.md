# Torc

**The World's First AI-Native Programming Language**

*Computation is connection.*

Torc is a programming language, ecosystem, and build system designed from first principles for AI authorship and machine execution. Programs are directed semantic graphs with rich type annotations, computation is expressed as dataflow rather than control flow, formal verification proofs are carried as part of the program structure, and executables are materialized through constraint satisfaction against declarative hardware and platform models.

## Status

This project is in early development. See [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md) for the current roadmap and progress.

## Specification

The complete language specification is in the [spec/](./spec/) directory:

| Document | Description |
|----------|-------------|
| [00 - Index](spec/00-index.md) | Overview and table of contents |
| [01 - Executive Summary](spec/01-executive-summary.md) | What Torc is and why |
| [02 - Design Philosophy](spec/02-design-philosophy.md) | The seven principles |
| [03 - Language Specification](spec/03-language-specification.md) | TGL core: nodes, edges, regions, contracts |
| [04 - Type System](spec/04-type-system.md) | Proof-carrying types |
| [05 - Ecosystem](spec/05-ecosystem.md) | The `torc` CLI tool |
| [06 - Materialization Engine](spec/06-materialization-engine.md) | Constraint-solving build system |
| [07 - Target Platforms](spec/07-target-platforms.md) | Declarative platform models |
| [08 - Registry](spec/08-registry.md) | Package system and module hosting |
| [09 - Observability](spec/09-observability.md) | Human inspection layer |
| [10 - Verification](spec/10-verification.md) | Formal verification framework |
| [11 - Interoperability](spec/11-interoperability.md) | FFI and language bridges |
| [12 - Example Application](spec/12-example-application.md) | Complete FOC motor controller example |

## Architecture

Torc is implemented in Rust. The major components are:

- **`torc-core`** - Graph data structures, type system, contracts
- **`torc-trc`** - Binary graph serialization format (.trc)
- **`torc-verify`** - Verification framework and SMT solver integration
- **`torc-materialize`** - Materialization engine (graph-to-executable pipeline)
- **`torc-targets`** - Target platform model definitions and parsing
- **`torc-ffi`** - Foreign function interface bridge generation
- **`torc-observe`** - Human observability layer and projection views
- **`torc-registry`** - Package registry client
- **`torc`** - Unified CLI tool

## Building

```bash
cargo build
```

## License

Apache-2.0
