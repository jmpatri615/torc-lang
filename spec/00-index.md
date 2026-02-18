# Torc: The World's First AI-Native Programming Language

## Complete Language, Ecosystem, and Build System Specification

**Version:** 0.1.0-spec
**Status:** Design Specification
**Date:** February 2026

---

## Table of Contents

1. [Executive Summary](./01-executive-summary.md)
2. [Design Philosophy and Principles](./02-design-philosophy.md)
3. [The Torc Graph Language (TGL)](./03-language-specification.md)
4. [The Type System: Proof-Carrying Types](./04-type-system.md)
5. [The Torc Ecosystem: `torc`](./05-ecosystem.md)
6. [The Materialization Engine](./06-materialization-engine.md)
7. [Target Platform Models](./07-target-platforms.md)
8. [The Torc Registry and Package System](./08-registry.md)
9. [Human Observability Layer](./09-observability.md)
10. [Formal Verification Framework](./10-verification.md)
11. [Interoperability and FFI](./11-interoperability.md)
12. [Example: A Complete Application](./12-example-application.md)
13. [The Specification Interface: Collaborative Intent Resolution](./13-specification-interface.md)
14. [Probabilistic Specification Engine](./14-probabilistic-specification-engine.md)
15. [The Specification Workspace](./15-specification-workspace.md)

---

## The Name

**Torc** — from the ancient Celtic/Roman *torc*, a rigid ring of twisted metal worn as a symbol of power and authority. The name reflects the language's core identity: computation twisted together from multiple strands — dependent types, linear types, effect types, formal proofs — into a single unified structure. Like the ancient torc, a Torc program is a closed ring of interconnected computation where meaning emerges from connections rather than sequences.

## The Tagline

*Computation is connection.*

## At a Glance

| Aspect | Torc | Traditional Languages |
|--------|-------|----------------------|
| Representation | Binary semantic graph | Text source files |
| Execution model | Dataflow (parallel default) | Control flow (sequential default) |
| Type system | Dependent + linear + effect + resource | Varies (usually one paradigm) |
| Build process | Constraint-solving materialization | Compile → link → package |
| Verification | Integrated proof obligations | Separate test suites |
| Target support | Declarative platform models | Per-target compiler backends |
| Primary author | AI systems | Human developers |
| Human interface | Observability projections | Source code reading |
