# 12. Example: A Complete Application

## The Application

To ground the entire Torc specification in something concrete, we'll walk through a complete application: a **brushless DC motor controller with field-oriented control (FOC)**, targeting both a Linux x86_64 simulation environment and a bare-metal STM32F407 Cortex-M4 deployment target.

This example is chosen deliberately because it combines:

- Hard real-time requirements (current control loop at 20kHz)
- Safety-critical behavior (motor runaway protection)
- Mixed computation (trigonometric math, PID control, state machines)
- Hardware I/O (ADC sensing, PWM generation, CAN communication)
- Multi-target deployment (simulation + embedded)
- Integration with existing code (C HAL drivers on the embedded target)

## Project Setup

```bash
torc init foc-controller --template bare-metal-arm
cd foc-controller
torc target add linux-x86_64
torc target add stm32f407-discovery
torc add torc-math --features "trig, fixed-point"
torc add torc-pid
torc add torc-can --features "raw"
torc add torc-hal --features "adc, pwm, timer, gpio"
```

## Project Manifest

```toml
[project]
name = "foc-controller"
version = "0.1.0"
description = "Brushless DC motor field-oriented control"
authors = ["ai:claude-4.5-opus@anthropic/20260215"]
edition = "2026"

[project.safety]
integrity-level = "ASIL-B"
certification-standard = "iso-26262"
allow-unverified = false

[dependencies]
torc-math = { version = "0.4.1", features = ["trig", "fixed-point"] }
torc-pid = "1.0.3"
torc-can = { version = "0.8.0", features = ["raw"] }
torc-hal = { version = "0.6.2", features = ["adc", "pwm", "timer", "gpio"] }

[dev-dependencies]
torc-sim = "0.2.0"
torc-motor-model = "0.1.0"

[targets.linux-x86_64]
model = "platform:linux-x86_64-gnu"
backend = "llvm"
optimization = "throughput"
purpose = "simulation"

[targets.stm32f407]
model = "platform:stm32f407-discovery"
backend = "llvm"
optimization = "deterministic-timing"
purpose = "deployment"

[targets.stm32f407.resources]
flash = "512KB"
ram = "128KB"
stack = "4KB"
clock = "168MHz"

[targets.stm32f407.timing-budgets]
current-loop = { period = "50μs", wcet-budget = "40μs" }    # 20kHz
speed-loop = { period = "1000μs", wcet-budget = "800μs" }   # 1kHz
can-loop = { period = "10000μs", wcet-budget = "5000μs" }   # 100Hz
```

## The Program Graph (Conceptual Description)

Since Torc programs are binary graphs, we describe the structure conceptually. An AI system would construct this graph through the Torc API.

### Top-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ foc-controller                                                   │
│                                                                   │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ Current Loop  │    │ Speed Loop   │    │ CAN Loop     │       │
│  │ (20kHz ISR)   │    │ (1kHz task)  │    │ (100Hz task) │       │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘       │
│         │                    │                    │               │
│         ▼                    ▼                    ▼               │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ Clarke/Park   │    │ Speed PID    │    │ CAN TX/RX    │       │
│  │ Transform     │    │ Controller   │    │ Protocol     │       │
│  └──────┬───────┘    └──────────────┘    └──────────────┘       │
│         │                                                         │
│         ▼                                                         │
│  ┌──────────────┐    ┌──────────────┐                            │
│  │ d/q Current   │    │ Safety       │                            │
│  │ PID Control   │    │ Monitor      │                            │
│  └──────┬───────┘    └──────┬───────┘                            │
│         │                    │                                     │
│         ▼                    ▼                                     │
│  ┌──────────────┐    ┌──────────────┐                            │
│  │ Inverse Park  │    │ State        │                            │
│  │ + SVPWM       │    │ Machine      │                            │
│  └──────┬───────┘    └──────────────┘                            │
│         │                                                         │
│         ▼                                                         │
│  ┌──────────────┐                                                │
│  │ PWM Output    │                                                │
│  │ (3-phase)     │                                                │
│  └──────────────┘                                                │
└─────────────────────────────────────────────────────────────────┘
```

### Key Subgraph: Clarke Transform

The Clarke transform converts three-phase currents (Ia, Ib, Ic) into a two-axis stationary reference frame (Iα, Iβ). Here's how it looks as pseudo-code projection:

```torc-projection
region pure clarke_transform {
    contracts {
        pre:  is_finite(ia) && is_finite(ib) && is_finite(ic)
              && ia + ib + ic ~= 0.0 (tolerance: 0.01)  // balanced 3-phase
        post: is_finite(i_alpha) && is_finite(i_beta)
              && magnitude(i_alpha, i_beta) <= max(abs(ia), abs(ib), abs(ic)) * 1.16
        time: <= 2μs @ arm-cortex-m4f-168mhz
        mem:  stack <= 32 bytes, heap == 0
        effects: pure
    }

    inputs {
        ia: Float<32> where abs(value) <= 50.0    // max 50A phase current
        ib: Float<32> where abs(value) <= 50.0
        ic: Float<32> where abs(value) <= 50.0
    }

    outputs {
        i_alpha: Float<32>
        i_beta:  Float<32>
    }

    // Dataflow computation (all independent operations execute in parallel)
    // Using simplified Clarke (assumes Ia + Ib + Ic = 0):
    i_alpha = ia
    i_beta  = (ia + 2.0 * ib) * ONE_OVER_SQRT3
    // where ONE_OVER_SQRT3 = 0.57735026919... (compile-time constant)
}
```

### Key Subgraph: Safety Monitor

```torc-projection
region safety_monitor {
    contracts {
        pre:  true  // Always callable
        post: state in {NORMAL, WARNING, FAULT, SHUTDOWN}
              && (state == SHUTDOWN implies pwm_disabled == true)
        time: <= 10μs @ arm-cortex-m4f-168mhz
        mem:  stack <= 128 bytes, heap == 0
        effects: IO<GPIO_FAULT_PIN>
    }

    inputs {
        phase_currents: (Float<32>, Float<32>, Float<32>)
        dc_bus_voltage: Float<32>
        motor_temp: Float<32>
        controller_state: ControllerState
        config: SafetyConfig
    }

    outputs {
        safety_state: SafetyState
        pwm_enabled: Bool
        fault_code: Option<FaultCode>
    }

    // Overcurrent check (all three checks execute in parallel)
    ia_overcurrent = abs(phase_currents.0) > config.current_limit
    ib_overcurrent = abs(phase_currents.1) > config.current_limit
    ic_overcurrent = abs(phase_currents.2) > config.current_limit
    any_overcurrent = ia_overcurrent || ib_overcurrent || ic_overcurrent

    // Overvoltage / undervoltage
    overvoltage = dc_bus_voltage > config.voltage_max
    undervoltage = dc_bus_voltage < config.voltage_min

    // Overtemperature
    overtemp = motor_temp > config.temp_limit

    // State machine transition
    safety_state = select {
        any_overcurrent => SHUTDOWN,
        overvoltage     => SHUTDOWN,
        overtemp        => FAULT,
        undervoltage    => WARNING,
        _               => NORMAL,
    }

    pwm_enabled = safety_state != SHUTDOWN
    fault_code = select {
        any_overcurrent => Some(OVERCURRENT),
        overvoltage     => Some(OVERVOLTAGE),
        overtemp        => Some(OVERTEMPERATURE),
        undervoltage    => Some(UNDERVOLTAGE),
        _               => None,
    }

    // Hardware fault pin (active low, direct GPIO for fastest response)
    write_gpio(FAULT_PIN, pwm_enabled)
}
```

## Materialization for Both Targets

### Simulation Target (Linux x86_64)

```bash
torc build --target linux-x86_64
```

The materialization engine:
1. Resolves the `torc-hal` ADC/PWM nodes to the simulation backend (software model instead of hardware registers)
2. Links against `torc-motor-model` for plant simulation
3. Uses `f64` arithmetic internally (no reason to constrain to `f32` on desktop)
4. Emits a standard Linux ELF binary
5. Skips WCET analysis (not meaningful on Linux)

### Deployment Target (STM32F407)

```bash
torc build --target stm32f407
```

The materialization engine:
1. Resolves `torc-hal` nodes to STM32F407 hardware register access (memory-mapped I/O)
2. Configures interrupt priorities: current loop as highest-priority ISR, speed loop as medium, CAN as low
3. Uses `f32` with hardware FPU (VFPv4 single-precision)
4. Generates vector table, startup code, and clock configuration
5. Runs full WCET analysis against the Cortex-M4 timing model
6. Verifies flash/RAM/stack fit within resource constraints
7. Emits bare-metal ELF binary

### Build Output

```bash
torc build --target stm32f407 --check-resources

# Output:
# Materializing foc-controller v0.1.0 for stm32f407-discovery...
#
# Verification: 847/847 obligations verified (0 waived)
# 
# Resources:
#   Flash:  31,244 / 524,288 bytes  (6.0%)
#   RAM:     2,108 / 131,072 bytes  (1.6%)
#   Stack:     892 /   4,096 bytes  (21.8%)
#
# Timing (worst-case @ 168MHz):
#   Current loop ISR:   28.4μs /  40.0μs budget  (71.0% — 11.6μs margin)
#     ├─ ADC read:        2.1μs
#     ├─ Clarke:          1.8μs
#     ├─ Park:            2.3μs
#     ├─ d-axis PID:      3.2μs
#     ├─ q-axis PID:      3.2μs
#     ├─ Inverse Park:    2.1μs
#     ├─ SVPWM:           4.8μs
#     ├─ Safety monitor:  6.9μs
#     └─ PWM update:      2.0μs
#   Speed loop:        142.0μs / 800.0μs budget  (17.8%)
#   CAN loop:          384.0μs / 5000.0μs budget  (7.7%)
#
# Materialized: out/stm32f407/foc-controller.elf (31,244 bytes)
```

## Human Inspection

```bash
torc inspect --target stm32f407 --view contracts --module safety_monitor
```

The observability layer shows the safety monitor's full contract set, verification status, and traceable links to the safety requirements. A safety engineer can review the behavioral guarantees, verify that all overcurrent, overvoltage, and overtemperature conditions are handled, and confirm that the SHUTDOWN state always disables PWM output — all without reading a single line of code.

```bash
torc inspect --view diff v0.0.9..v0.1.0 --module current_loop
```

Shows what changed in the current loop between versions: perhaps the PID gains were adjusted, or the SVPWM algorithm was changed from a sector-based lookup to a direct computation. The diff operates on the semantic graph, so it shows *what changed in behavior* rather than *what lines of text changed*.

## Testing the Simulation

```bash
torc build --target linux-x86_64
./out/linux-x86_64/foc-controller --sim-config sim/step-response.toml

# Runs the FOC controller against a simulated motor model
# Outputs telemetry data for analysis
# Validates that contracts hold during dynamic operation
```

## What This Example Demonstrates

1. **Single program, multiple targets.** The same computation graph materializes to both a Linux simulation and a bare-metal embedded deployment.
2. **Contracts replace tests.** The 847 proof obligations provide stronger guarantees than any test suite could, and they're verified automatically.
3. **Resource visibility.** Flash, RAM, stack, and WCET budgets are visible and verified before the code ever touches hardware.
4. **Safety integration.** The safety monitor's behavior is formally specified and verified, with traceable links to requirements.
5. **Incremental adoption.** The embedded target uses FFI bridges to existing HAL drivers — Torc doesn't require rewriting everything.
6. **Human oversight.** Engineers can inspect every aspect of the system through the observability layer without needing to read the binary graph.

This is a realistic, buildable application — not a toy example. It represents the kind of safety-critical embedded control system that consumes enormous engineering effort today and could be dramatically accelerated by AI-native development with formal verification.
