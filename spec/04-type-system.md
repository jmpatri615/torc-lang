# 4. The Type System: Proof-Carrying Types

## Overview

The Torc type system is the most powerful component of the language. It unifies several type-theoretic traditions that human languages typically offer only one of — dependent types, linear types, effect types, and resource types — into a single coherent system. The AI author doesn't need to choose a type paradigm; it uses all of them simultaneously.

The type system serves three roles:

1. **Correctness encoding.** Types carry enough information to express and verify program properties that would otherwise require separate testing or formal analysis.
2. **Resource tracking.** Types track memory ownership, lifetime, aliasing, and usage counts, making memory safety provable rather than convention-dependent.
3. **Proof obligation generation.** Type annotations generate proof obligations that the verification engine must discharge before materialization proceeds.

## The Type Universe

### Primitive Types

```
Void            — The empty type (no values)
Unit            — The singleton type (exactly one value)
Bool            — {true, false}
Int<W, S>       — Integer with width W (1..128 bits) and signedness S
Float<P>        — IEEE 754 floating-point with precision P (16, 32, 64, 128)
Fixed<W, F>     — Fixed-point with W total bits and F fractional bits
```

All numeric types carry their precision explicitly. There is no implicit widening or narrowing — every conversion is an explicit `Conversion` node with a bounds proof.

### Refinement Types

Any type can be refined with a predicate:

```
type PositiveInt    = Int<32, Signed> where value > 0
type Percentage     = Float<64> where value >= 0.0 && value <= 100.0
type NonEmptyVec<T> = Vec<T> where len > 0
type SortedVec<T>   = Vec<T> where forall i in 0..len-1: element(i) <= element(i+1)
type BoundedLatency = Duration where value <= 50.microseconds
```

Refinement predicates are checked statically by the verification engine when possible, and generate runtime checks when static verification is infeasible.

### Composite Types

```
Tuple<T1, T2, ..., Tn>       — Heterogeneous fixed-length product
Record<{name: T, ...}>       — Named-field product type
Variant<{tag: T, ...}>       — Tagged union (sum type, exhaustive matching required)
Array<T, N>                   — Fixed-length homogeneous sequence
Vec<T>                        — Variable-length homogeneous sequence with capacity tracking
```

### Linear and Affine Types

Torc tracks resource ownership through linearity annotations:

```
Linear<T>       — Must be used exactly once (consumed)
Affine<T>       — May be used at most once (consumed or dropped)
Shared<T>       — May be aliased, immutable access only
Unique<T>       — Single owner, mutable access, transferable
Counted<T>      — Reference-counted shared ownership
```

These annotations are enforced structurally in the graph. A `Linear<T>` value must have exactly one outgoing data edge. If a node needs to use a linear value twice, it must explicitly `Copy` (if the type permits) or restructure the computation.

**Why this matters for safety-critical systems:** Use-after-free, double-free, and dangling references are not runtime errors to be caught — they are type errors that cannot be expressed in a well-typed graph. Memory safety is a theorem, not a hope.

### Effect Types

Every computation node declares its effects through the type system:

```
Pure            — No side effects; result depends only on inputs
Alloc<R>        — Allocates memory in region R
IO<D>           — Performs I/O on device descriptor D
Atomic<O>       — Performs atomic operation with ordering O
FFI<ABI>        — Calls foreign code with ABI specification
Diverge         — May not terminate (must carry justification)
Panic           — May abort execution (must carry recovery strategy)
```

Effects compose and propagate: a node that calls a `Pure` node and an `IO<UART1>` node is itself `IO<UART1>`. The materialization engine uses effect information to determine scheduling freedom — pure nodes can be reordered freely, I/O nodes must respect their device ordering constraints.

### Resource Types

Resource types track quantitative properties through the type system:

```
Timed<T, B>     — Value T produced within time bound B
Sized<T, S>     — Value T occupying at most S bytes
Powered<T, E>   — Value T produced within energy budget E
Bandwidth<T, R> — Value T transmitted within throughput R
```

Resource types enable the materialization engine to verify that a program fits within its target's capabilities without runtime measurement.

### Dependent Types

Types may depend on values:

```
// A matrix type where dimensions are part of the type
Matrix<T, Rows: Nat, Cols: Nat>

// Matrix multiplication enforces dimensional compatibility at the type level
matmul: (Matrix<T, M, K>, Matrix<T, K, N>) -> Matrix<T, M, N>

// The output dimension depends on the input dimensions — this is checked
// at graph construction time, not at runtime
```

```
// An array indexing operation where the index is proven in-bounds
safe_index: (arr: Array<T, N>, idx: Int where idx >= 0 && idx < N) -> T

// The caller must provide a proof that idx is in bounds.
// If the proof cannot be constructed, the graph is rejected.
```

### Probability Types

For probabilistic computation:

```
Distribution<T>      — A probability distribution over type T
Posterior<T, E>      — Distribution conditioned on evidence E
Interval<T, C>       — Confidence interval at confidence level C
Approximate<T, Err>  — Value with bounded approximation error Err
```

## Type Inference and Checking

Because Torc programs are constructed by AI rather than parsed from text, there is no traditional type inference problem. The AI author provides fully explicit types on every node and edge. The type checker's role is verification, not inference:

1. **Consistency check.** For every edge, the source port's output type must be compatible with the target port's input type.
2. **Linearity check.** For every linear or affine value, the number of consumers matches the linearity annotation.
3. **Effect check.** Every node's declared effects are a superset of its actual effects (as determined by its children).
4. **Resource check.** Every node's resource bounds are consistent with its children's bounds and the target platform model.
5. **Refinement check.** Every refinement predicate generates a proof obligation that must be discharged.

## Type Composition Example

To illustrate the power of the unified type system, consider a single function contract as it would exist in a safety-critical automotive context:

```
read_sensor_voltage:
    Input:  channel: SensorChannel
                     where channel in {ADC0, ADC1, ADC2, ADC3}
    Output: Timed<
                Sized<
                    Linear<
                        Float<32>
                            where value >= 0.0 && value <= 5.0
                    >,
                    4  // exactly 4 bytes
                >,
                50.microseconds  // WCET on target
            >
    Effects: IO<ADC_PERIPHERAL>
    Failure: {ADC_TIMEOUT -> return 0.0, ADC_OVERRANGE -> clamp(0.0, 5.0)}
```

This single type signature encodes what would require dozens of lines of comments, separate WCET analysis, manual memory accounting, and integration tests in a traditional language. In Torc, it's the type — and it's machine-verified.
