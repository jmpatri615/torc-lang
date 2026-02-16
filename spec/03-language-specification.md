# 3. The Torc Graph Language (TGL) — Core Specification

## The Graph Model

A Torc program is a **Directed Acyclic Graph** (DAG) at the expression level, with explicit cycle structures for iteration and recursion expressed through special-purpose node types. The graph consists of three fundamental elements:

### Nodes

A node represents a unit of computation. Every node has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `UUID` | Globally unique, content-addressed identifier |
| `kind` | `NodeKind` | The category of computation (see Node Kinds below) |
| `contract` | `Contract` | Full behavioral specification |
| `type_sig` | `TypeSignature` | Input and output types with full annotations |
| `provenance` | `Provenance` | Creation metadata and edit history |
| `annotations` | `Map<String, Annotation>` | Extensible metadata (optimization hints, safety class, etc.) |

### Edges

An edge represents a data dependency between nodes. Every edge has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `UUID` | Unique edge identifier |
| `source` | `(NodeID, PortIndex)` | Output port of the producing node |
| `target` | `(NodeID, PortIndex)` | Input port of the consuming node |
| `type` | `EdgeType` | The type of data flowing along this edge |
| `lifetime` | `Lifetime` | When this data is valid and when it can be freed |
| `bandwidth` | `Option<BandwidthConstraint>` | Optional throughput requirement |

### Regions

A region is a subgraph boundary that defines scope, lifetime, and execution constraints:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `UUID` | Unique region identifier |
| `kind` | `RegionKind` | Sequential, parallel, conditional, iterative, atomic |
| `constraints` | `Vec<Constraint>` | Resource and timing constraints for this region |
| `children` | `Vec<NodeID>` | Nodes contained in this region |
| `interfaces` | `Vec<Port>` | Input/output ports visible outside the region |

## Node Kinds

### Primitive Computation Nodes

```
Literal         — Constant value production
Arithmetic      — add, sub, mul, div, mod, pow (integer and floating-point)
Bitwise         — and, or, xor, not, shift_left, shift_right, rotate
Comparison      — eq, ne, lt, le, gt, ge (returns typed boolean with provenance)
Conversion      — Explicitly typed value conversion with bounds checking
```

### Data Structure Nodes

```
Construct       — Build a composite type from components
Destructure     — Extract components from a composite type
Index           — Access element by computed index (with bounds proof)
Slice           — Extract a contiguous sub-range (with bounds proof)
```

### Control Flow Nodes

```
Select          — Conditional data selection (equivalent to phi/mux)
                  Inputs: condition, true_value, false_value
                  No branching — both paths may be evaluated

Switch          — Multi-way selection with exhaustiveness proof
                  Inputs: discriminant, case_values[]
                  All cases must be covered (verified at graph construction)

Iterate         — Bounded iteration with guaranteed termination
                  Inputs: initial_state, bound, step_function (subgraph)
                  The bound must be statically provable or carry a runtime check

Recurse         — Structural recursion with termination metric
                  Inputs: argument, base_case, recursive_case, termination_proof
                  The termination metric must decrease on every recursive call

Fixpoint        — General fixpoint computation with convergence proof
                  Inputs: initial_value, step_function, convergence_criterion
```

### Effect Nodes

```
Allocate        — Request memory with explicit lifetime and region
Deallocate      — Release memory (must match exactly one Allocate)
Read            — Read from an external data source (I/O, sensor, file)
Write           — Write to an external data sink (I/O, actuator, file)
Atomic          — Atomic read-modify-write with memory ordering
Fence           — Memory ordering fence (acquire, release, seq_cst)
Syscall         — Invoke an OS service (platform-model dependent)
FFICall         — Invoke foreign code through interop bridge
```

### Meta Nodes

```
Verify          — Inline proof obligation (assert with proof witness)
Assume          — Assumed property (unproven, flagged in reports)
Measure         — Runtime measurement point (timing, memory, energy)
Checkpoint      — State snapshot for debugging and replay
Annotate        — Attach human-readable documentation to a subgraph
```

### Probabilistic Nodes

```
Sample          — Draw from a probability distribution
Condition       — Bayesian conditioning on observed data
Expectation     — Compute expected value of a distribution
Entropy         — Compute entropy/information content
Approximate     — Compute with bounded approximation error
```

## The Contract Model

Every computation node carries a `Contract` structure:

```
Contract {
    // What must be true before this node executes
    preconditions: Vec<Predicate>,

    // What will be true after this node executes (given preconditions hold)
    postconditions: Vec<Predicate>,

    // Resource consumption bounds
    time_bound: Option<TimeBound>,         // WCET, BCET, average
    memory_bound: Option<MemoryBound>,     // peak, allocated, freed
    energy_bound: Option<EnergyBound>,     // for power-constrained targets
    stack_bound: Option<StackBound>,       // maximum stack depth

    // Effects this node may perform
    effects: EffectSet,                    // {pure, alloc, io, atomic, ffi, ...}

    // Failure modes
    failure_modes: Vec<FailureMode>,       // what can go wrong and how
    recovery_strategy: RecoveryStrategy,   // abort, retry, degrade, propagate

    // Proof status
    proof_status: ProofStatus,             // verified, assumed, pending, waived
    proof_witness: Option<ProofWitness>,   // machine-checkable proof object
}
```

### Predicate Language

Predicates in contracts are expressed in a first-order logic with arithmetic, supporting:

- Value constraints: `output >= 0 && output <= 4095`
- Relational constraints: `output == f(input)` where `f` is a pure reference function
- Temporal constraints: `execution_time <= 50us`
- Resource constraints: `heap_allocated == 0`
- Invariant preservation: `sorted(output) && permutation(output, input)`
- Information flow: `no_leak(secret_input, public_output)`

## Serialization Format

The canonical serialization is **Torc Binary Graph** (`.trc`), a compact binary format:

```
TRC File Layout:
┌──────────────────────────────┐
│ Magic: 0x54524300 ("TRC\0") │  4 bytes
│ Version: major.minor.patch   │  3 bytes
│ Flags                        │  1 byte
├──────────────────────────────┤
│ Header                       │
│   node_count: u64            │
│   edge_count: u64            │
│   region_count: u64          │
│   string_table_offset: u64   │
│   proof_table_offset: u64    │
│   provenance_offset: u64     │
├──────────────────────────────┤
│ Node Table (packed)          │
│   [NodeKind, TypeSig, ...]   │
├──────────────────────────────┤
│ Edge Table (packed)          │
│   [Source, Target, Type, ...]│
├──────────────────────────────┤
│ Region Table (packed)        │
│   [Kind, Constraints, ...]   │
├──────────────────────────────┤
│ Contract Table               │
│   [Pre, Post, Bounds, ...]   │
├──────────────────────────────┤
│ Proof Table                  │
│   [Witness objects]          │
├──────────────────────────────┤
│ String Table                 │
│   [Interned strings]         │
├──────────────────────────────┤
│ Provenance Table             │
│   [Creation records]         │
├──────────────────────────────┤
│ Content Hash (SHA-256)       │  32 bytes
└──────────────────────────────┘
```

Content addressing: the `id` of every node is derived from the SHA-256 hash of its content (kind, type signature, contract, and child references). This means structurally identical subgraphs automatically deduplicate and graph equality is O(1) by comparing root hashes.

## A Textual Projection (For Human Reading Only)

Torc has no syntax. However, for documentation, debugging, and the observability layer, a standard **textual projection** format exists. This is a *lossy, read-only view* — it cannot be parsed back into a graph without information loss.

```torc-projection
// This is what `torc inspect --pseudo-code` might produce for a simple function

region pure compute_checksum {
    contracts {
        pre:  len(data) > 0 && len(data) <= 65535
        post: result in 0x0000..0xFFFF
        time: <= 12μs @ arm-cortex-m4-168mhz
        mem:  stack <= 64 bytes, heap == 0
        effects: pure
    }

    inputs {
        data: &[u8; N] where N in 1..65535    // linear reference, no copy
    }

    outputs {
        result: u16
    }

    // Dataflow (not sequential — these execute when inputs are ready)
    sum = fold(data, 0u32, |acc, byte| acc + widen(byte))
    high = sum >> 16
    low  = sum & 0xFFFF
    combined = high + low
    carry = combined >> 16
    result = truncate_u16(combined + carry)
    result = bitwise_not(result)
}
```

This projection is purely for human consumption. The actual program is the binary graph, and the projection is generated on demand by the observability layer.
