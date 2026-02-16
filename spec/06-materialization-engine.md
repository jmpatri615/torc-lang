# 6. The Materialization Engine

## What Materialization Is

Materialization is the process of transforming a Torc program graph into an executable artifact for a specific target. Unlike traditional compilation (a linear pipeline of parse → analyze → optimize → emit), materialization is a **constraint satisfaction process** that operates holistically.

The engine takes three inputs and produces one output:

```
Inputs:
  1. Program Graph (.trc)         — What the program does
  2. Target Platform Model        — What the hardware/OS can do
  3. Optimization Profile         — What to prioritize

Output:
  Executable Artifact             — ELF, PE, raw binary, FPGA bitstream, etc.
```

## The Materialization Pipeline

While materialization is conceptually holistic, the reference implementation structures it as a series of phases that can iterate and backtrack:

### Phase 1: Graph Canonicalization

Normalize the input graph to a canonical form:

- Deduplicate structurally identical subgraphs (via content-addressed hashing)
- Inline trivial subgraphs (single-node regions)
- Flatten unnecessary region nesting
- Resolve module references and link subgraph dependencies
- Validate graph well-formedness (type consistency, linearity, effect soundness)

**Output:** A single, self-contained, canonical program graph.

### Phase 2: Verification

Discharge all proof obligations before proceeding:

- Type consistency proofs (automatically generated)
- Refinement predicate proofs (delegated to SMT solver)
- Resource bound proofs (require target model — see Phase 4 for iterative refinement)
- Termination proofs (for iterative and recursive nodes)
- Linearity proofs (structural, checked on the graph)
- User-specified contract proofs

Proofs are cached and indexed by content hash. If a subgraph hasn't changed, its proofs are reused. The verification phase can operate incrementally, only re-verifying subgraphs affected by changes.

**Gate:** If any proof obligation fails and is not explicitly waived, materialization halts with a diagnostic report. Waivers require a justification string and are recorded in the provenance log.

### Phase 3: Target-Aware Graph Transformation

Transform the abstract program graph into a target-aware intermediate form:

- **Lowering:** Replace high-level nodes with target-appropriate implementations. A `Sort` node might become a merge sort on a system with ample memory or an in-place heapsort on a memory-constrained embedded target.
- **Specialization:** Monomorphize generic nodes for concrete types. A polymorphic `add` becomes a specific `i32.add` or `f64.add`.
- **Scheduling:** Assign a topological execution order respecting data dependencies, resource constraints, and the target's parallelism model. On a single-core embedded target, this produces a single sequential schedule. On a multicore system, it produces a task graph with affinity hints.
- **Memory Layout:** Assign concrete memory locations, register allocations, and stack frames. Linear type information eliminates the need for escape analysis — ownership is known statically.
- **ABI Conformance:** Insert calling convention adapters for FFI boundaries, OS interfaces, and interrupt handlers.

### Phase 4: Resource Fitting

Verify that the scheduled, laid-out program fits within the target's resource constraints:

- **Flash/ROM:** Total code size must fit
- **RAM:** Peak memory usage (static + stack + heap) must fit
- **Stack:** Maximum call depth must fit within stack allocation
- **WCET:** Worst-case execution time of critical paths must meet timing budgets
- **Bandwidth:** Data throughput must meet I/O constraints

If fitting fails, the engine backtracks to Phase 3 and attempts alternative transformations:

1. Choose smaller (slower) algorithm variants
2. Reduce inlining aggressiveness
3. Trade speed for size or vice versa
4. Split computation across time (if timing constraints permit)

If no valid fitting exists, materialization fails with a detailed resource report explaining which constraints are unsatisfiable and by how much.

### Phase 5: Code Emission

Emit the target-specific artifact. The reference implementation uses LLVM as the primary backend:

```
Program Graph → Torc Target IR → LLVM IR → Target Machine Code → Executable
```

The Torc Target IR is an intermediate step that captures target-aware decisions (scheduling, memory layout) in a form that maps cleanly to LLVM IR. This separation allows alternative backends without changing the upper phases.

**Supported emission targets (initial release):**

| Target | Format | Backend |
|--------|--------|---------|
| Linux x86_64 | ELF shared/static | LLVM |
| Linux ARM64 | ELF shared/static | LLVM |
| Linux RISC-V 64 | ELF shared/static | LLVM |
| Windows x86_64 | PE/COFF | LLVM |
| macOS ARM64 | Mach-O | LLVM |
| Bare-metal ARM Cortex-M | ELF / raw binary | LLVM |
| Bare-metal RISC-V 32 | ELF / raw binary | LLVM |
| Bare-metal PowerPC e200 | ELF / raw binary | LLVM (with PPC backend) |
| WebAssembly | .wasm | LLVM |

**Planned backends:**

| Target | Format | Backend |
|--------|--------|---------|
| Xilinx FPGA | Bitstream | Yosys + nextpnr (via HLS) |
| Intel FPGA | Bitstream | Quartus bridge |
| Custom AI accelerator | Binary config | Native Torc backend |

### Phase 6: Post-Materialization Verification

After code emission, run a final verification pass on the materialized artifact:

- **Binary analysis:** Verify the emitted binary matches expected code size, section layout, and symbol table
- **Timing analysis:** Run static WCET analysis on the emitted machine code (using aiT, AbsInt, or built-in analyzer) and compare against contract bounds
- **Memory analysis:** Verify stack usage matches predictions
- **Smoke tests:** Execute the binary against contract-derived test vectors (if a simulation target is available)

## Optimization Profiles

Rather than cryptic compiler flags, Torc uses named optimization profiles with clear semantics:

```toml
[profile.throughput]
description = "Maximize computation throughput"
strategy = "speed"
inlining = "aggressive"
vectorization = "auto"
loop-unrolling = "aggressive"
code-size = "unrestricted"

[profile.minimal-size]
description = "Minimize binary size for flash-constrained targets"
strategy = "size"
inlining = "minimal"
vectorization = "none"
loop-unrolling = "none"
dead-code-elimination = "aggressive"

[profile.deterministic-timing]
description = "Minimize WCET variance for hard real-time systems"
strategy = "predictability"
inlining = "selective"          # Inline only if it reduces WCET variance
vectorization = "fixed-length"  # No data-dependent iteration counts
loop-unrolling = "full"         # Eliminate loop overhead
branch-free = "prefer"          # Use conditional moves over branches
cache-locking = "critical-sections"  # Lock critical code in cache

[profile.balanced]
description = "Balance speed, size, and predictability"
strategy = "balanced"
inlining = "moderate"
vectorization = "auto"
```

## Incremental Materialization

For development workflows, full materialization on every change is unnecessary. The engine supports incremental materialization:

1. Content-addressed graph nodes identify exactly what changed
2. Only affected subgraphs are re-lowered, re-scheduled, and re-emitted
3. Proof cache avoids re-verifying unchanged contracts
4. Link-time incremental updates patch the binary in place when possible

For a typical edit-build-test cycle, incremental materialization should complete in under 2 seconds for localized changes, regardless of total project size.

## Materialization Reports

Every materialization produces a machine-readable report:

```json
{
  "target": "arm-cortex-m4f-168mhz",
  "profile": "deterministic-timing",
  "duration_ms": 4521,
  "resources": {
    "flash": { "used": 48832, "available": 524288, "percent": 9.3 },
    "ram":   { "used": 3412,  "available": 131072, "percent": 2.6 },
    "stack": { "used": 1024,  "available": 4096,   "percent": 25.0 }
  },
  "timing": {
    "main_loop": { "wcet_us": 847, "budget_us": 1000, "margin_percent": 15.3 },
    "isr_adc":   { "wcet_us": 12,  "budget_us": 50,   "margin_percent": 76.0 }
  },
  "verification": {
    "obligations_total": 1247,
    "verified": 1234,
    "waived": 13,
    "waiver_justifications": ["..."]
  },
  "provenance": {
    "generator": "claude-4.5-opus",
    "timestamp": "2026-02-15T14:30:00Z",
    "graph_hash": "sha256:a1b2c3d4..."
  }
}
```
