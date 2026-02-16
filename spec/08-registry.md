# 8. The Torc Registry and Package System

## Architecture

The Torc Registry serves three categories of artifacts:

1. **Graph Modules** — Reusable computation subgraphs (the equivalent of libraries/crates/packages)
2. **Platform Models** — Target hardware and environment descriptions
3. **Proof Libraries** — Reusable proof strategies and lemma collections

All three use the same underlying infrastructure: content-addressed storage, semantic versioning, and cryptographic signing.

## Graph Modules

### What a Module Is

A Torc graph module is a self-contained subgraph with well-defined input/output interfaces, contracts, and proof obligations. It is the unit of reuse, versioning, and distribution.

Unlike traditional packages (which are collections of source files), a module is a single `.trc` binary graph. Dependencies between modules are expressed as subgraph references — edges that cross module boundaries through declared interface ports.

### Module Manifest

Every published module includes a manifest:

```toml
[module]
name = "torc-pid"
version = "1.0.3"
description = "PID controller with anti-windup and bumpless transfer"
authors = ["ai:claude-4.5-opus@anthropic/20260201"]
license = "MIT"
repository = "https://github.com/torc-modules/torc-pid"
keywords = ["control", "pid", "real-time", "safety-critical"]
categories = ["control-systems", "embedded"]

[module.safety]
max-integrity-level = "ASIL-D"     # Highest level this module claims support for
verification-coverage = 100         # All obligations verified, none waived

[module.compatibility]
torc-edition = ">=2026"
min-toolchain = "0.2.0"

[module.interfaces]
# Declared input/output ports — this is the module's public API
inputs = [
    { name = "setpoint", type = "Float<32>", contract = "finite && in_range(min, max)" },
    { name = "measurement", type = "Float<32>", contract = "finite" },
    { name = "dt", type = "Float<32>", contract = "value > 0.0 && value <= 1.0" },
    { name = "config", type = "PIDConfig", contract = "valid_gains" },
]
outputs = [
    { name = "output", type = "Float<32>", contract = "in_range(config.out_min, config.out_max)" },
    { name = "state", type = "PIDState", contract = "bounded_integrator" },
]

[module.resource-bounds]
# Resource bounds that hold regardless of target
stack = "<= 256 bytes"
heap = "0 bytes"
effects = "pure"               # No I/O, no allocation, no side effects

[dependencies]
torc-math = ">=0.3.0, <1.0.0"
```

### Semantic Versioning with Contract Awareness

Torc enforces semantic versioning with an important extension: **contract compatibility is part of the version contract.**

- **Patch version** (1.0.x): Internal changes only. Same interfaces, same contracts, same or better resource bounds. Proofs may be updated but conclusions must be identical.
- **Minor version** (1.x.0): New interfaces may be added. Existing interfaces unchanged. Contracts may be *strengthened* (more guarantees) but never weakened.
- **Major version** (x.0.0): Breaking changes. Interfaces, contracts, or resource bounds may change in any direction.

The registry enforces this automatically: when publishing, it compares the new version's interfaces and contracts against the previous version and rejects the publish if the version increment doesn't match the actual change magnitude.

### Resolution and Compatibility

Dependency resolution uses the same contract-aware approach. When resolving a dependency tree:

1. Version ranges are resolved using standard semver
2. Contract compatibility is verified at module boundaries
3. Resource bounds are aggregated and checked against the target
4. Conflicting dependencies are detected at the graph level (not at link time)

```bash
torc tree
# Output:
# motor-controller v1.2.0
# ├── torc-pid v1.0.3
# │   └── torc-math v0.4.1
# ├── torc-can v0.8.0
# │   └── torc-math v0.4.1 (shared)
# └── torc-hal v0.6.2
#     └── torc-math v0.4.1 (shared)
#
# All contracts compatible. No conflicts.
```

## Registry Infrastructure

### Hosting Model

The Torc Registry supports both centralized and federated hosting:

**Public Registry** (`registry.torc-lang.org`): The default, community-operated registry for open-source modules. Free to publish and fetch.

**Private Registries**: Organizations can host their own registries for proprietary modules. The `torc` tool supports multiple registry sources with priority ordering:

```toml
# ~/.torc/config.toml

[[registries]]
name = "public"
url = "https://registry.torc-lang.org"
priority = 100

[[registries]]
name = "company-internal"
url = "https://torc.internal.example.com"
priority = 200     # Checked first
auth = "token"
```

### Content Addressing and Integrity

Every artifact in the registry is content-addressed:

- Module hash = SHA-256 of the canonical `.trc` binary
- Proof hash = SHA-256 of the proof witness objects
- Platform model hash = SHA-256 of the canonicalized TOML

Publishing is append-only: a version, once published, cannot be modified. It can be *yanked* (hidden from new resolution) but never deleted or altered.

### Signing and Trust

All published artifacts are cryptographically signed:

```bash
torc publish --sign                     # Sign with local key
torc publish --sign --key corporate     # Sign with organization key
torc verify --check-signatures          # Verify all dependency signatures
```

For safety-critical applications, the trust chain matters:

```toml
# torc.toml

[trust]
required-signatures = ["torc-core-team", "company-security"]
reject-unsigned = true
reject-yanked = true
```

## Proof Libraries

Proof libraries contain reusable verification strategies, lemmas, and decision procedures:

```bash
torc add --proof torc-arithmetic-proofs   # Lemmas about integer/float arithmetic
torc add --proof torc-sorting-proofs      # Proof strategies for sorting algorithms
torc add --proof torc-timing-proofs       # WCET analysis strategies
```

These accelerate verification by providing pre-proven building blocks. When the verification engine encounters a proof obligation, it searches installed proof libraries for applicable lemmas before attempting de novo proof synthesis.

## Audit and Safety Compliance

The registry provides audit capabilities for safety-critical supply chains:

```bash
# Full dependency audit
torc audit
# Output:
#   torc-pid v1.0.3: ASIL-D certified, 100% verified, signed by torc-core-team
#   torc-math v0.4.1: ASIL-D certified, 100% verified, signed by torc-core-team
#   torc-can v0.8.0: ASIL-B certified, 98% verified (2 waivers), signed by can-working-group
#   torc-hal v0.6.2: QM only, 95% verified, signed by community

# Export audit report for certification
torc audit --export iso-26262 --output audit-report.pdf

# Check for known vulnerabilities or issues
torc audit --advisories

# Verify complete provenance chain
torc audit --provenance --depth full
```
