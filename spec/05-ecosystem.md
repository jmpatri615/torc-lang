# 5. The Torc Ecosystem: `torc`

## Overview

`torc` is the unified command-line interface for the Torc ecosystem. It combines the roles that are split across multiple tools in other language ecosystems: toolchain management (like `rustup`), project management and building (like `cargo`), package management (like `npm`/`crates.io`), and verification (like separate formal verification tool suites).

The guiding principle: **one tool, one command namespace, zero configuration for common workflows.**

## Installation and Toolchain Management

### Bootstrap Installation

```bash
# Unix-like systems
curl -sSf https://torc-lang.org/install.sh | sh

# Windows
irm https://torc-lang.org/install.ps1 | iex

# Package managers
brew install torc
apt install torc-toolchain
winget install Torc.Toolchain
```

The bootstrap installs `torc` itself and the default toolchain (stable channel). `torc` then manages everything else.

### Toolchain Channels

```bash
torc toolchain install stable          # Production-ready releases
torc toolchain install beta            # Preview of next stable
torc toolchain install nightly         # Bleeding edge, may break
torc toolchain install 0.3.2           # Specific version

torc toolchain default stable          # Set system default
torc toolchain override nightly        # Override for current project

torc toolchain list                    # Show installed toolchains
torc update                            # Update all installed toolchains
```

### Component Management

The Torc toolchain is modular. Not every project needs every component:

```bash
torc component add llvm-backend        # LLVM materialization backend (default)
torc component add cranelift-backend   # Alternative lightweight backend
torc component add fpga-backend        # FPGA bitstream materialization
torc component add z3-prover           # Z3 SMT solver for verification
torc component add cvc5-prover         # CVC5 SMT solver (alternative)
torc component add ffi-c               # C interoperability bridge
torc component add ffi-rust            # Rust interoperability bridge
torc component add observability       # Human inspection tools
torc component add studio              # Visual graph exploration IDE

torc component list                    # Show available and installed
torc component remove fpga-backend     # Remove unused component
```

## Project Structure

### Initialization

```bash
torc init my-project                   # Create a new project
torc init my-project --template lib    # Library module
torc init my-project --template bare-metal-arm  # Embedded project with ARM target
torc init my-project --template autosar-swc     # AUTOSAR software component
```

### Project Layout

```
my-project/
├── torc.toml                   # Project manifest
├── targets/                     # Target platform models
│   ├── linux-x86_64.target.toml
│   └── cortex-m4.target.toml
├── graph/                       # Torc graph modules (.trc files)
│   ├── main.trc                 # Entry point graph
│   └── modules/                 # Subgraph modules
│       ├── sensor.trc
│       └── control.trc
├── contracts/                   # Shared contract definitions
│   └── safety.contracts.trc
├── proofs/                      # Cached proof witnesses
│   └── .torc-proofs/
├── ffi/                         # Foreign function interface definitions
│   ├── bindings.h               # C header for FFI bridge
│   └── bridge.trc               # Generated FFI wrapper graph
├── inspect/                     # Observability configuration
│   └── views.toml               # Custom inspection view definitions
└── out/                         # Materialized outputs (gitignored)
    ├── linux-x86_64/
    │   └── my-project
    └── cortex-m4/
        └── my-project.elf
```

### Project Manifest: `torc.toml`

```toml
[project]
name = "motor-controller"
version = "1.2.0"
description = "Brushless DC motor field-oriented control"
authors = ["ai:claude-4.5-opus@anthropic/20260215"]
license = "Apache-2.0"
edition = "2026"

[project.safety]
integrity-level = "ASIL-D"          # ISO 26262 safety integrity level
certification-standard = "iso-26262"
allow-unverified = false             # Refuse to materialize without proofs

[dependencies]
torc-math = "0.4.1"                # Fixed-point math library
torc-pid = "1.0.3"                 # PID controller module
torc-can = "0.8.0"                 # CAN bus protocol module

[dependencies.torc-hal]             # Hardware abstraction layer
version = "0.6.2"
features = ["adc", "pwm", "timer"]

[dev-dependencies]
torc-sim = "0.2.0"                 # Plant model simulation for HIL

[targets]
default = "linux-x86_64"            # Default materialization target

[targets.linux-x86_64]
model = "platform:linux-x86_64-gnu"
backend = "llvm"
optimization = "throughput"

[targets.cortex-m4]
model = "platform:arm-cortex-m4f-168mhz"
backend = "llvm"
optimization = "size-then-speed"
linker-script = "targets/stm32f407.ld"
runtime = "bare-metal"

[targets.cortex-m4.resources]
flash = "512KB"
ram = "128KB"
stack = "4KB"
clock = "168MHz"

[verification]
solver = "z3"
timeout = "300s"                     # Per-proof timeout
parallel-proofs = 8                  # Concurrent proof jobs

[ffi]
c-headers = ["ffi/bindings.h"]
abi = "C"

[registry]
publish-to = "https://registry.torc-lang.org"
```

## Core Commands

### Building (Materialization)

```bash
torc build                            # Materialize for default target
torc build --target cortex-m4         # Materialize for specific target
torc build --all-targets              # Materialize for all defined targets
torc build --release                  # Release optimization level
torc build --profile minimal-size     # Custom optimization profile

# Materialization output
torc build --emit=llvm-ir             # Emit LLVM IR (for debugging)
torc build --emit=asm                 # Emit assembly (for inspection)
torc build --emit=graph-stats         # Emit graph statistics

# Resource budget checking
torc build --target cortex-m4 --check-resources
# Output:
#   Flash: 47,832 / 524,288 bytes (9.1%)
#   RAM:   3,412 / 131,072 bytes (2.6%)
#   Stack: 1,024 / 4,096 bytes (25.0%)
#   WCET main loop: 847μs / 1,000μs budget (84.7%)
```

### Verification

```bash
torc verify                           # Verify all proof obligations
torc verify --module sensor           # Verify specific module
torc verify --contract safety         # Verify specific contract set
torc verify --report human            # Generate human-readable report
torc verify --report iso-26262        # Generate certification-formatted report

# Verification status
torc verify --status
# Output:
#   Total obligations: 1,247
#   Verified:          1,203 (96.5%)
#   Pending:              31 (2.5%)
#   Waived:               13 (1.0%) [justifications required]
#   Failed:                0

# Incremental verification (only re-verify changed subgraphs)
torc verify --incremental

# Exhaustive variant verification
torc verify --all-variants            # Verify across all configuration variants
```

### Inspection (Human Observability)

```bash
torc inspect                          # Launch interactive inspection UI
torc inspect --view dataflow          # Show dataflow graph visualization
torc inspect --view contracts         # Show contract summary table
torc inspect --view resources         # Show resource budget breakdown
torc inspect --view pseudo-code       # Generate pseudo-code projection
torc inspect --view dependencies      # Show module dependency graph
torc inspect --view provenance        # Show creation/edit history
torc inspect --view diff v1.1.0       # Show changes since version

# Targeted inspection
torc inspect --module sensor --view dataflow
torc inspect --node <uuid> --view contract
```

### Dependency Management

```bash
torc add torc-math                   # Add latest compatible version
torc add torc-math@0.4.1            # Add specific version
torc add torc-can --features j1939   # Add with feature flags
torc remove torc-sim                 # Remove dependency
torc update                           # Update all to latest compatible
torc update torc-math                # Update specific dependency

torc tree                             # Show dependency tree
torc audit                            # Check for known issues in dependencies
torc audit --safety                   # Verify safety properties of dependencies
```

### Target Management

```bash
torc target add linux-x86_64          # Install platform model from registry
torc target add arm-cortex-m4f        # Install embedded target model
torc target add riscv32-imac          # Install RISC-V target model
torc target add ppc-e200z7            # Install PowerPC target model
torc target add windows-x86_64       # Install Windows target model

torc target list                      # List installed targets
torc target describe cortex-m4        # Dump target constraint model
torc target validate cortex-m4        # Validate target model consistency

# Custom target creation
torc target init my-custom-board      # Create custom target template
torc target test my-custom-board      # Validate custom target model
```

### Publishing

```bash
torc publish                          # Publish to configured registry
torc publish --dry-run                # Validate without publishing
torc publish --registry private       # Publish to private registry

torc login                            # Authenticate with registry
torc owner add user@example.com       # Add package co-owner
```

### FFI Bridge

```bash
torc ffi bridge --from-c libfoo.h     # Generate Torc wrappers for C library
torc ffi bridge --to-c                # Generate C headers for Torc modules
torc ffi bridge --from-rust Cargo.toml # Generate wrappers for Rust crate
torc ffi bridge --validate            # Verify FFI boundary contracts
```

### Diagnostics

```bash
torc doctor                           # Full toolchain health check
torc doctor --target cortex-m4        # Check target-specific toolchain
torc clean                            # Remove materialized outputs and caches
torc clean --proofs                   # Also remove cached proofs
```

## Environment and Configuration

### Configuration Hierarchy

1. **Built-in defaults** — sensible defaults for all settings
2. **System config** — `~/.torc/config.toml` — user-wide settings
3. **Project config** — `torc.toml` — project-specific settings
4. **Command-line flags** — override everything
5. **Environment variables** — `TORC_*` prefix for CI/CD integration

### Key Environment Variables

```bash
TORC_HOME=~/.torc                    # Torc installation root
TORC_REGISTRY=https://registry.torc-lang.org  # Default registry
TORC_SOLVER=z3                        # Default SMT solver
TORC_JOBS=8                           # Parallel job count
TORC_LOG=info                         # Logging level
TORC_BACKEND=llvm                     # Default materialization backend
```
