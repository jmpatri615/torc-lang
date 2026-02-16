# 7. Target Platform Models

## The Model Architecture

A Target Platform Model is a declarative description of everything the materialization engine needs to know about a deployment target. It replaces the implicit knowledge embedded in traditional compiler backends, linker scripts, and board support packages with explicit, inspectable, composable data.

A complete platform model is assembled from three layers:

```
Platform Model = ISA Model + Microarchitecture Model + Environment Model
```

Each layer is independently versioned, published, and composable.

## Layer 1: ISA Model

Describes the instruction set architecture — what operations the hardware can execute.

```toml
# isa/arm-v7m.isa.toml

[isa]
name = "ARMv7-M"
version = "1.0.0"
endianness = "little"
word-size = 32
address-space = 32

[registers]
general-purpose = { count = 13, width = 32, names = ["r0".."r12"] }
stack-pointer = { name = "sp", width = 32, alias = "r13" }
link-register = { name = "lr", width = 32, alias = "r14" }
program-counter = { name = "pc", width = 32, alias = "r15" }
status = { name = "xPSR", width = 32 }
floating-point = { count = 32, width = 32, names = ["s0".."s31"], extension = "vfpv4-sp" }
fp-double = { count = 16, width = 64, names = ["d0".."d15"], overlay = "s0..s31" }

[instructions.arithmetic]
add = { operands = "reg, reg, reg|imm", latency = 1, throughput = 1 }
sub = { operands = "reg, reg, reg|imm", latency = 1, throughput = 1 }
mul = { operands = "reg, reg, reg", latency = 1, throughput = 1 }
sdiv = { operands = "reg, reg, reg", latency = "2..12", throughput = "2..12" }
mla = { operands = "reg, reg, reg, reg", latency = 2, throughput = 1 }

[instructions.simd]
# No NEON on Cortex-M, but DSP extensions available
smlad = { operands = "reg, reg, reg, reg", latency = 1, throughput = 1, extension = "dsp" }
qadd = { operands = "reg, reg, reg", latency = 1, throughput = 1, extension = "dsp" }

[instructions.floating-point]
vadd-f32 = { operands = "sreg, sreg, sreg", latency = 1, throughput = 1, extension = "vfpv4-sp" }
vmul-f32 = { operands = "sreg, sreg, sreg", latency = 1, throughput = 1, extension = "vfpv4-sp" }
vdiv-f32 = { operands = "sreg, sreg, sreg", latency = "14", throughput = "14", extension = "vfpv4-sp" }

[calling-convention.aapcs]
argument-registers = ["r0", "r1", "r2", "r3"]
return-registers = ["r0", "r1"]
callee-saved = ["r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11"]
stack-alignment = 8
```

## Layer 2: Microarchitecture Model

Describes the specific implementation of the ISA — pipeline depth, cache behavior, bus timing. This is what makes WCET analysis possible.

```toml
# uarch/cortex-m4.uarch.toml

[microarchitecture]
name = "ARM Cortex-M4"
version = "1.0.0"
isa = "arm-v7m"
extensions = ["thumb2", "dsp", "vfpv4-sp"]

[pipeline]
stages = 3                    # Fetch, Decode, Execute
branch-penalty = 1            # 1 cycle for taken branch (pipeline refill)
interrupt-latency = 12        # Cycles from interrupt assertion to first ISR instruction
tail-chaining-latency = 6     # Cycles for back-to-back interrupts

[memory]
# Cortex-M4 has no caches by default (implementation-defined)
# Cache behavior is specified in the SoC model, not here
bus-width = 32
flash-wait-states = "implementation-defined"  # Overridden by SoC model
sram-wait-states = 0

[timing-model]
# For WCET analysis: every instruction has a deterministic cycle count
# (one of the advantages of the Cortex-M pipeline)
deterministic = true
pipeline-interlocks = ["mul-result-forwarding", "load-use-hazard"]
load-use-penalty = 1          # 1 extra cycle if using a loaded value immediately

[interrupt-model]
nvic-priorities = 256         # Number of priority levels
nested = true
priority-grouping = "configurable"
```

## Layer 3: Environment Model

Describes the software environment — OS, runtime, ABI, available services.

```toml
# env/bare-metal-arm.env.toml

[environment]
name = "bare-metal-arm"
version = "1.0.0"
type = "bare-metal"

[runtime]
entry-point = "Reset_Handler"
init-sequence = ["SystemInit", "libc_init_array", "main"]
has-os = false
has-heap = false               # Can be enabled with allocator
has-mmu = false
has-mpu = true

[memory-map]
# Defined by linker script / SoC model, but general regions:
flash = { base = "0x08000000", typical-size = "512KB", access = "rx" }
sram = { base = "0x20000000", typical-size = "128KB", access = "rwx" }
peripherals = { base = "0xE0000000", size = "256MB", access = "rw", volatile = true }

[abi]
standard = "arm-eabi"
calling-convention = "aapcs"
endianness = "little"
float-abi = "hard"            # Use FPU registers for float args

[binary-format]
format = "elf32-littlearm"
sections = ["text", "rodata", "data", "bss", "stack", "heap"]
entry-symbol = "Reset_Handler"
vector-table = { address = "0x08000000", entries = 256 }

[startup]
# The materialization engine generates this:
stack-init = true
bss-zero = true
data-copy = true              # Copy .data from flash to SRAM
fpu-enable = true             # Enable FPU via CPACR register

[available-services]
# No OS services — only hardware peripherals
systick = true
nvic = true
scb = true
mpu = true
fpu = true
dwt = true                    # Data watchpoint and trace (for cycle counting)
```

## Composed Platform Model Example

A complete platform model for a specific board combines all three layers plus board-specific details:

```toml
# targets/stm32f407-discovery.target.toml

[platform]
name = "STM32F407 Discovery Board"
version = "1.0.0"
isa = "isa:arm-v7m@1.0"
microarchitecture = "uarch:cortex-m4@1.0"
environment = "env:bare-metal-arm@1.0"

[soc]
manufacturer = "STMicroelectronics"
part-number = "STM32F407VGT6"
clock-max = "168MHz"
flash-size = "1MB"
sram-size = "192KB"              # 128KB main + 64KB CCM
ccm-ram = { base = "0x10000000", size = "64KB", access = "rw", dma-accessible = false }

[soc.flash-timing]
# STM32F407 flash access time depends on clock and voltage
wait-states-at-168mhz = 5
prefetch-buffer = true
instruction-cache = true         # 64 lines × 128 bits
data-cache = true                # 8 lines × 128 bits

[soc.clock-tree]
hse = "8MHz"                     # External crystal
pll-m = 8
pll-n = 336
pll-p = 2                        # SYSCLK = 168MHz
pll-q = 7                        # USB = 48MHz
ahb-prescaler = 1                # HCLK = 168MHz
apb1-prescaler = 4               # APB1 = 42MHz
apb2-prescaler = 2               # APB2 = 84MHz

[peripherals.adc]
count = 3
resolution = 12
max-sample-rate = "2.4MSPS"
channels = 16
dma-capable = true

[peripherals.can]
count = 2
max-bitrate = "1Mbps"
mailboxes = 3
filters = 28

[peripherals.uart]
count = 6
max-baudrate = "10.5Mbps"

[peripherals.timer]
advanced = { count = 2, bits = 16, names = ["TIM1", "TIM8"] }
general = { count = 10, bits = "16 or 32" }
basic = { count = 2, bits = 16, names = ["TIM6", "TIM7"] }

[errata]
# Known silicon errata that affect code generation
"2.1.8" = { description = "SDIO clock divider", workaround = "none-needed-for-torc" }
"2.5.1" = { description = "Cortex-M4 LDRD/STRD may be non-atomic", workaround = "avoid-unaligned-double-word" }

[debug]
interface = "SWD"
trace = "SWO"
breakpoints-hw = 6
watchpoints-hw = 4
```

## Platform Model Registry

Platform models are published to and fetched from the Torc Registry, just like code modules:

```bash
# Fetch a platform model
torc target add stm32f407-discovery

# Browse available models
torc target search "cortex-m4"
torc target search "risc-v" --filter "fpga-capable"

# Inspect a model before using it
torc target describe stm32f407-discovery --detail full

# Validate a custom model
torc target validate my-custom-board.target.toml
```

### Who Creates Platform Models?

- **Silicon vendors** publish official models for their parts (ideal case)
- **Board manufacturers** publish composed models for their boards
- **Community contributors** create and maintain models for popular targets
- **AI systems** can generate draft models from datasheets and validate against real hardware

The Torc project maintains a set of **reference models** for popular targets. These are validated against real hardware and serve as the quality baseline:

| Target | Status | Maintainer |
|--------|--------|------------|
| Linux x86_64 (generic) | Stable | Torc Core Team |
| Windows x86_64 (generic) | Stable | Torc Core Team |
| macOS ARM64 (Apple Silicon) | Stable | Torc Core Team |
| ARM Cortex-M0/M0+ | Stable | Community |
| ARM Cortex-M4/M4F | Stable | Community |
| ARM Cortex-M7 | Stable | Community |
| ARM Cortex-A53/A72 | Stable | Community |
| RISC-V RV32IMAC | Beta | Community |
| RISC-V RV64GC | Beta | Community |
| PowerPC e200z4/z7 | Beta | Community |
| WebAssembly (WASI) | Stable | Torc Core Team |
| Xilinx Zynq 7000 | Experimental | Community |
