# 10. Formal Verification Framework

## Verification Philosophy

In Torc, verification is not a separate activity performed after development. It is woven into the language, the type system, the build process, and the registry. Every Torc program generates proof obligations as a natural consequence of its type annotations and contracts. Materialization cannot proceed until those obligations are discharged.

This inverts the traditional relationship between development and verification. In a traditional workflow, you write code, then try to prove it correct (and usually settle for testing). In Torc, you specify what "correct" means (via contracts), construct a computation graph that claims to satisfy those specifications, and the system verifies the claim before producing an executable.

## Proof Obligation Generation

Proof obligations are generated automatically from five sources:

### 1. Type Refinement Obligations

Every refinement predicate on a type generates an obligation at every point where a value of that type is produced:

```
Type: PositiveInt = Int<32, Signed> where value > 0

Obligation at node "increment":
  Given: input: Int<32, Signed> where value >= 0
  Prove: input + 1 > 0
  // Trivially provable: value >= 0 implies value + 1 >= 1 > 0
```

### 2. Contract Obligations

Preconditions generate obligations at call sites. Postconditions generate obligations at computation nodes:

```
Contract on "safe_divide":
  Pre:  divisor != 0
  Post: result == dividend / divisor

Obligation at call site:
  Prove: the value flowing into the "divisor" port is non-zero
  // May require tracing backwards through the graph to find a range proof

Obligation at the node:
  Prove: the output equals the mathematical division of the inputs
  // Typically proven by construction (the node IS a division)
```

### 3. Resource Bound Obligations

Resource annotations generate obligations that require target-specific analysis:

```
Contract: WCET <= 50μs @ arm-cortex-m4-168mhz

Obligation:
  Prove: the longest execution path through this subgraph,
         when materialized for the specified target,
         completes within 50 microseconds

  // Requires: microarchitecture timing model, pipeline analysis,
  //           memory access timing, interrupt masking assumptions
```

### 4. Linearity Obligations

Linear type annotations generate structural obligations on the graph:

```
Type: Linear<FileHandle>

Obligations:
  - Exactly one node consumes this value (no duplication, no discard)
  - The consuming node is reachable on all execution paths
  - No node reads this value after the consuming node
  // Proven structurally on the graph — no SMT solver needed
```

### 5. Termination Obligations

Iterative and recursive nodes must prove termination:

```
Node: Iterate(initial, bound=1000, step_fn)

Obligation:
  Prove: step_fn reduces a well-founded metric on every iteration
  // For bounded iteration with a literal bound, this is trivial
  // For recursive nodes, may require user-supplied ranking function
```

## Verification Engines

Torc uses a portfolio approach to verification, dispatching obligations to the most appropriate solver:

### SMT Solvers (Z3, CVC5)

Used for: arithmetic properties, range analysis, refinement predicates, data flow properties.

Most contract obligations reduce to satisfiability queries in the theory of bitvectors, integers, reals, and arrays. Z3 is the default solver; CVC5 is available as an alternative.

### Abstract Interpretation

Used for: numeric range analysis, pointer analysis, resource bound estimation.

For obligations involving loops or large data structures, abstract interpretation provides sound over-approximations. If the abstract analysis proves the property, no further verification is needed. If it fails, the obligation is escalated to the SMT solver with additional hints from the abstract analysis.

### Structural Analysis

Used for: linearity, ownership, effect propagation, graph well-formedness.

Many obligations can be discharged by direct analysis of the graph structure without invoking external solvers. These are the fastest to verify and have zero false positives.

### Model Checking (Bounded)

Used for: concurrent properties, protocol correctness, state machine verification.

For programs with concurrency or reactive behavior, bounded model checking explores all reachable states up to a configurable depth. Properties like mutual exclusion, deadlock freedom, and protocol compliance are verified this way.

### WCET Analysis

Used for: timing bound obligations on specific targets.

WCET analysis requires the microarchitecture model and the materialized (or partially materialized) code. It uses a combination of:
- Control flow analysis on the computation graph
- Pipeline analysis using the microarchitecture timing model
- Cache/memory analysis using the memory model
- Path analysis to identify the longest execution path

## Proof Witnesses and Caching

When a proof obligation is discharged, the solver produces a **proof witness** — a compact, machine-checkable certificate that the property holds. Proof witnesses are:

- **Stored** alongside the graph in the proof table of the `.trc` file
- **Cached** in the project's `.torc-proofs/` directory for incremental verification
- **Content-addressed** by the hash of the obligation, so unchanged obligations reuse existing proofs
- **Independently checkable** by a lightweight proof checker that doesn't require the full solver

This means that verification results are reproducible and auditable. A certification authority can re-check proof witnesses without re-running the full solver suite.

## Handling Verification Failures

When an obligation cannot be proven, the system provides structured diagnostics:

```
VERIFICATION FAILED: obligation at node 7a3f...

  Obligation: output in [0.0, 5.0]
  Context:    read_sensor_voltage, output port

  The solver could not prove that the output is always in [0.0, 5.0].

  Counterexample found:
    When input voltage = 5.12V (ADC reads 4096 at 12-bit resolution)
    Scaled output = 5.12, which violates upper bound 5.0

  Suggestions:
    1. Add clamping: clamp(output, 0.0, 5.0)
    2. Strengthen precondition: require ADC raw value <= 4000
    3. Weaken postcondition: output in [0.0, 5.5]
    4. Waive obligation (requires justification)
```

## Waivers

Sometimes a proof obligation cannot be automatically discharged but the engineer has external justification for why the property holds. Torc supports explicit waivers:

```
Waiver {
    obligation: "output in [0.0, 5.0] at node 7a3f...",
    justification: "Hardware voltage divider limits ADC input to 4.8V max.
                    See schematic REF-SCH-042, section 3.2.
                    Validated by hardware test report TR-2026-0015.",
    author: "jeff.engineer@company.com",
    approved_by: "safety-review-board",
    date: "2026-02-15",
    expiration: "2027-02-15",  // Must be re-reviewed annually
    safety_impact: "low — hardware provides the guarantee"
}
```

Waivers are:
- Recorded in provenance with full audit trail
- Reported in all verification summaries
- Flagged with expiration dates for periodic re-review
- Required to have human approval (AI cannot self-waive)
- Counted against safety certification metrics

## Verification Profiles

Different contexts require different verification rigor:

```toml
[verification.profile.development]
# Fast feedback during development
strategy = "incremental"
solver-timeout = "10s"
abstract-interpretation = true
smt-solving = "changed-only"
wcet-analysis = false              # Skip timing analysis in dev

[verification.profile.integration]
# Thorough verification before integration
strategy = "full"
solver-timeout = "60s"
abstract-interpretation = true
smt-solving = "all"
wcet-analysis = true
bounded-model-checking = true
bmc-depth = 100

[verification.profile.certification]
# Maximum rigor for safety certification
strategy = "exhaustive"
solver-timeout = "600s"
abstract-interpretation = true
smt-solving = "all"
wcet-analysis = true
bounded-model-checking = true
bmc-depth = 1000
proof-checking = "independent"     # Re-check all witnesses
variant-verification = "all"       # Verify all configuration variants
regression-check = true            # Verify no properties weakened vs. prior version
```
