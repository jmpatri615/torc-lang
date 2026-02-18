# 14. Probabilistic Specification Engine

## Extending the Decision Model

Document 13 introduced four response modes for human decisions: defer, explore, tentative, and commit. These capture the human's *intent* regarding a decision, but they don't fully capture the *structure* of the human's knowledge. Real engineering decisions involve richer forms of uncertainty that the specification engine must model explicitly.

## Expanded Decision Response Modes

### The Original Four

| Mode | Human Says | System Response |
|------|-----------|----------------|
| **Defer** | "Park it for now" | Provisional default, track for revisit |
| **Explore** | "Show me options" | Structured analysis with recommendation |
| **Tentative** | "Probably Y, uncertain" | Soft constraint, flag if contradicted |
| **Commit** | "It shall be X" | Hard constraint, decision impact report |

### Six Additional Modes

#### Constrained Set

**Human says:** "It's one of these: {A, B, C}."

The human can enumerate the possibilities but cannot choose between them. This is distinct from Explore (where the human doesn't know the options) and from Tentative (where they have a preferred answer). The human is providing a discrete probability distribution over known alternatives.

**Typical situation:** "The motor is either the ABC-1234 or the XYZ-5678 — procurement hasn't finalized the vendor." Or: "The communication bus will be CAN or LIN — the vehicle platform team decides, not us."

**System response:** The AI explores all branches in parallel and produces a divergence report:

```
Constrained Set: Motor = {ABC-1234, XYZ-5678}

Analysis across both options:

  Invariant (same regardless of choice):
    • Control topology: FOC in both cases
    • Safety architecture: unchanged
    • Communication protocol: unaffected
    • Flash usage: within budget for both (<48KB)

  Divergent:
    • Current sensing gain: 0.033 V/A (ABC) vs 0.028 V/A (XYZ)
      → Requires different ADC scaling. Both are within 12-bit resolution.
    • Maximum speed: 6000 RPM (ABC) vs 8000 RPM (XYZ)
      → If XYZ is selected, the speed loop bandwidth needs revision.
    • Connector pinout: Different. Affects PCB layout.
      → Hardware team should be informed of both possibilities.

  Recommendation: Design for XYZ-5678 envelope (the harder case) and
  constrain to ABC-1234 parameters via configuration. This eliminates
  redesign risk regardless of procurement outcome.
```

The system computes which properties hold universally across the constrained set and which are contingent. Verification can proceed on the universal properties immediately.

#### Bounded Range

**Human says:** "Between X and Y."

The human has bounds but not a point estimate. The uncertainty is continuous over a known interval.

**Typical situation:** "The pump flow rate will be between 5 and 15 liters per minute — the exact requirement depends on the thermal modeling that another team hasn't finished." Or: "The operating temperature range is at least -20°C to +85°C, but may extend to -40°C to +105°C for the automotive grade."

**System response:** The AI optimizes for robustness across the range rather than for a specific point. It identifies which design choices are sensitive to the parameter's value within the range and which are invariant:

```
Bounded Range: Flow rate = [5, 15] L/min

Sensitivity analysis:
  • Motor power rating: Scales linearly with flow rate.
    At 5 L/min: 120W sufficient. At 15 L/min: 340W required.
    → Motor selection CANNOT be committed until this range narrows.

  • Control loop bandwidth: Insensitive within this range.
    Current loop at 20kHz is adequate for all flow rates.
    → Control architecture can proceed.

  • Pipe diameter: At 15 L/min, flow velocity exceeds 2 m/s in
    12mm pipe, which may cause cavitation.
    → Flag for hydraulic engineering review.
```

#### Volatile

**Human says:** "X for now, but it may change."

The human is confident in the current answer but aware that external factors — a customer requirement change, a supply chain disruption, a regulatory update — could force a revision. This is distinct from Tentative, where the human doubts their own answer. Here the answer is currently correct but the world might change.

**Typical situation:** "The target MCU is the STM32F407, but there's a chance the supply chain team will switch us to the STM32G474." Or: "The safety standard is ISO 26262:2018, but the 2026 revision is in draft and may change the decomposition rules."

**System response:** The AI assesses the *cost of change* and actively prefers designs that are resilient to the anticipated disruption:

```
Volatile: Target MCU = STM32F407 (may change to STM32G474)

Change impact assessment:
  If STM32F407 → STM32G474:
    • Pin-compatible: Yes (same LQFP-100 package)
    • Peripheral compatibility: CAN→FDCAN (protocol change required)
    • FPU: Single-precision → Single+Double (no negative impact)
    • Flash: 512KB → 512KB (same)
    • Clock: 168MHz → 170MHz (negligible difference)
    • ADC: 12-bit → 12-bit with hardware oversampling (improvement)

  Estimated redesign cost: LOW
    Software: Regenerate CAN driver wrappers (2 modules affected)
    Hardware: None (pin-compatible)
    Verification: Re-verify CAN-related proofs only (estimated 4 of 847)

  Resilience strategy: I've abstracted the CAN peripheral interface so
  that switching between CAN 2.0B and FDCAN requires only a configuration
  change, not a structural redesign.
```

#### Blocked

**Human says:** "This is someone else's decision, and I'm waiting on it."

A decision exists, the human can't make it, and they know who can. This is distinct from Defer (which is voluntary). Blocked is involuntary — it's a dependency on an external party.

**Typical situation:** "The vehicle network architect determines the CAN bus speed and message IDs. I've requested the information but haven't received it." Or: "The safety assessment team is doing the HARA. Until they assign the ASIL level, I can't finalize the safety architecture."

**System response:** The system tracks the external dependency with an owner and an expected resolution date. It continues to explore the design space around the blocked decision, identifying what can proceed and what truly requires the blocked information:

```
Blocked: CAN message IDs (owner: Vehicle Network Architect, ETA: March 1)

Work that can proceed without this decision:
  • All control loop development (no CAN dependency)
  • Safety monitoring architecture
  • CAN physical layer configuration (baud rate is already committed)

Work that is blocked:
  • CAN transmit message packing
  • CAN receive message parsing
  • Integration testing with vehicle network

Mitigation: I've defined a CAN message abstraction layer with a
placeholder message database. When the actual IDs arrive, only the
database configuration changes — no structural redesign required.

⏰ Reminder scheduled: If unresolved by February 25 (5 days before ETA),
   I'll prompt you to follow up with the Vehicle Network Architect.
```

#### Challenge

**Human says:** "I say X, but prove me wrong."

The human has a strong opinion but suspects they might be biased by experience, habit, or incomplete information. They want the AI to actively attempt to find a better answer. This is a committed decision with an explicit request for adversarial analysis.

**Typical situation:** "I always use 20kHz PWM because that's what I've used for ten years. But maybe there's a reason to do something different on this hardware." Or: "I'm going to use a PI controller because PID derivative kick is always a problem. But tell me if I'm leaving performance on the table."

**System response:** The AI performs the commitment (so the design proceeds) but simultaneously launches an adversarial analysis that actively tries to find configurations where a different choice would be superior. It reports back with evidence:

```
Challenge: PWM frequency = 20kHz

Adversarial analysis complete. I found a better option.

Your choice of 20kHz is defensible but not optimal for your specific target:

  The STM32F407's ADC has a 12-bit conversion time of 3μs. At 20kHz (50μs period),
  you can sample 3 channels sequentially in 9μs, leaving 41μs for computation.

  At 16kHz (62.5μs period), the same 3 channels take 9μs, leaving 53.5μs — but
  more importantly, 16kHz aligns with the ADC's DMA burst transfer size, eliminating
  a 2.3μs software copy that occurs at 20kHz due to buffer misalignment.

  Net WCET improvement at 16kHz: 4.7μs per control cycle.

  Trade-off: 16kHz is within the audible range for young adults (hearing
  threshold ~17kHz). For your pump application with ambient mechanical noise
  of 65dB, the PWM tone at 16kHz would be masked above 40% duty cycle.
  Below 40% duty cycle (i.e., near-idle pump operation), it may be faintly
  audible in a quiet environment.

  Verdict: 16kHz is measurably better. The acoustic concern is real but
  likely irrelevant for your application. Recommend switching.

  [Accept 16kHz] [Keep 20kHz] [Explore further]
```

#### Exclude

**Human says:** "Anything but X." Or: "Never Y."

An anti-constraint. The human doesn't know what the answer should be but knows what it must *not* be. This constrains the solution space without specifying a direction.

**Typical situation:** "I don't care what communication protocol you use, but it cannot be proprietary — must be an open standard." Or: "Do not use dynamic memory allocation anywhere in the safety-critical partition." Or: "No Texas Instruments parts — our supply chain has a moratorium on that vendor."

**System response:** The AI adds a hard exclusion constraint and immediately evaluates its impact:

```
Exclude: No dynamic memory allocation in safety-critical partition

Impact: This eliminates the following patterns from the solution space:
  • Dynamic CAN message buffer allocation (will use static pools instead)
  • Runtime-sized FFT for vibration analysis (will use fixed-size buffers)
  • String formatting for diagnostic messages (will use pre-allocated templates)

  No currently committed decisions are affected.
  3 exploring decisions now have reduced option sets.

  This constraint is consistent with ASIL-B requirements and is actually
  already implied by your MISRA compliance commitment. I've recorded both
  the explicit exclusion and the MISRA derivation as independent justifications.
```

## The Probabilistic Inference Framework

With these richer decision modes, the specification engine maintains not just states but *probability distributions* over the entire decision space. This enables powerful analytical capabilities.

### The Decision Space as a Bayesian Network

The decision graph is formally a Bayesian network. Each decision node carries a probability distribution over its possible values:

- **Committed** nodes have a point distribution (probability 1.0 at the committed value)
- **Tentative** nodes have a peaked distribution centered on the stated value with spread proportional to uncertainty
- **Bounded Range** nodes have a distribution (often uniform) over the stated interval
- **Constrained Set** nodes have a discrete distribution over the enumerated options
- **Exploring** nodes have a distribution shaped by the AI's analysis
- **Deferred** and **Blocked** nodes carry a prior distribution based on domain defaults
- **Excluded** nodes have zero probability at excluded values

Conditional dependencies between decisions are represented as edges with conditional probability tables (for discrete decisions) or conditional density functions (for continuous decisions).

### Bayesian Inference Operations

#### Posterior Propagation

When the human commits a decision, the system performs Bayesian inference to update the posterior distributions on all connected decisions:

```
Event: Human commits "Control topology = FOC"

Prior → Posterior updates:
  P(position_sensor_required):  0.6 → 0.95  (FOC almost always needs position)
  P(computational_budget > 500_cycles):  0.4 → 0.85  (FOC is compute-intensive)
  P(phase_current_sensing = shunt):  0.5 → 0.7  (FOC benefits from precise sensing)
  P(torque_ripple < 5%):  0.6 → 0.92  (FOC excels at ripple reduction)
```

The human sees this propagation reflected in the Decision Map — downstream nodes shift their visual state to reflect narrowed uncertainty.

#### Expected Value of Information (EVI)

At any point, the system can compute which unmade decision would most reduce overall uncertainty if resolved. This is formally the Expected Value of Information — a decision-theoretic quantity that answers: "Where should the human focus next?"

```
Information Value Ranking (top 5):

  1. Motor selection          EVI = 34.2 bits  (affects 14 downstream decisions)
  2. Safety integrity level   EVI = 22.8 bits  (affects 11 downstream decisions)
  3. Target MCU              EVI = 18.4 bits  (affects 9 downstream decisions)
  4. Communication protocol   EVI = 12.1 bits  (affects 6 downstream decisions)
  5. PWM frequency           EVI =  4.3 bits  (affects 3 downstream decisions)

  Recommendation: Resolving the motor selection would eliminate the most
  uncertainty from the design. If motor selection is blocked on procurement,
  the safety integrity level is the next highest-value decision.
```

This dynamically optimizes the human's attention — instead of working through a fixed priority list, the system continuously recomputes where attention is most valuable based on the current state of knowledge.

#### Sensitivity Analysis

For tentative and ranged decisions, the system computes how sensitive the overall design quality is to each uncertain parameter:

```
Sensitivity Analysis: PWM Frequency (tentative: ~20kHz)

  If actual optimum is in [15kHz, 25kHz]:
    Impact on committed requirements: NEGLIGIBLE
    All safety properties: SATISFIED
    WCET margin varies by ±3μs (within budget)

  If actual optimum is below 10kHz:
    Torque ripple requirement: VIOLATED (ripple exceeds 5% below 12kHz)
    Audible noise: LIKELY (below 15kHz is clearly audible)

  If actual optimum is above 30kHz:
    Switching losses increase by ~15% (efficiency impact)
    ADC sampling becomes limiting factor (conversion time budget)

  Conclusion: Your tentative 20kHz is in the insensitive region.
  The exact value doesn't matter much between 15-25kHz. This decision
  is LOW RISK — don't spend more time on it unless other constraints change.
```

This tells the human which uncertainties are dangerous (motor selection, where the sensitivity is high) and which are safe to live with (PWM frequency, where the design is robust within a wide range).

### Monte Carlo Decision Space Exploration

For complex systems with many interacting uncertain decisions, the system uses Monte Carlo sampling to explore the joint decision space:

1. Sample from the joint distribution of all uncertain decisions
2. Materialize each sample (or evaluate key properties without full materialization)
3. Analyze the population of resulting designs statistically

This reveals:

**Robust properties:** "In 99.2% of 10,000 sampled configurations, flash usage is below 48KB. This is nearly certain regardless of outstanding decisions."

**Fragile properties:** "WCET compliance passes in 73% of samples. The failures concentrate in the region where motor pole count is high AND control loop frequency is at the lower end of its range. These two decisions interact — consider them together."

**Unexpected correlations:** "Motor selection and CAN bus loading are correlated in a non-obvious way: high-pole-count motors require faster current loop updates, which generate more frequent CAN telemetry messages, pushing bus loading above 80% in some configurations."

**Pareto frontiers:** For multi-objective optimization, the system visualizes the achievable trade-off envelope across the uncertain decision space. As decisions are committed, the frontier narrows, giving the human an increasingly precise picture of what's achievable.

### Information Entropy as Progress Metric

Shannon entropy provides a natural, quantitative measure of specification completeness. Each decision contributes entropy proportional to its remaining uncertainty:

- A committed decision: 0 bits
- A binary uncertain decision: 1 bit
- A uniform choice among 8 options: 3 bits
- A continuous uncertain parameter: entropy of its distribution (differential entropy)

Total specification entropy captures "how decided is this design?" in a single number:

```
Specification Entropy Over Time:

  Session 1 (Feb 15):  847 bits  ████████████████████████████████████  100%
  Session 2 (Feb 16):  412 bits  ████████████████████░░░░░░░░░░░░░░░   49%
  Session 3 (Feb 18):  198 bits  █████████░░░░░░░░░░░░░░░░░░░░░░░░░   23%
  Session 4 (Feb 20):   34 bits  ██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    4%

  Remaining entropy by domain:
    Motor/mechanical:    12 bits  (motor selection tentative)
    Communication:        8 bits  (protocol deferred, waiting on network team)
    Software:             6 bits  (current sensing topology in exploration)
    Safety:               4 bits  (minor decisions with reasonable defaults)
    Hardware:             4 bits  (PCB-related, blocked on mechanical design)
```

The rate of entropy decrease measures specification velocity. When the rate stalls, it signals that the remaining decisions need external input, deeper analysis, or organizational action — not more thinking by the human.

### Kalman-Style Estimation for Evolving Specifications

The specification process can be modeled as a state estimation problem analogous to Kalman filtering:

**State vector:** The estimated values of all decisions — a high-dimensional vector mixing continuous parameters (PWM frequency, current limit) with discrete choices (control topology, MCU selection).

**Covariance matrix:** The uncertainty in each decision and, critically, the correlations between uncertainties. This captures coupled uncertainties: "If the motor turns out to need more current than expected, the thermal design probably also needs revision."

**Process model:** How the specification is expected to evolve. Early in the project, decisions change frequently. As the design matures, they stabilize. The process model captures this trajectory — it's an engineering project's equivalent of a dynamical system model.

**Measurement model:** How the human's statements map to state updates. A committed decision is a low-noise measurement. A tentative decision is a higher-noise measurement. A bounded range is a partial observation.

Each human input is a measurement update. The engine fuses it with the prior estimate, weighted by the human's stated confidence. If the human says "probably 20kHz" with low confidence, and the AI's analysis suggests 16kHz is optimal with high confidence, the posterior estimate reflects both sources of information — and the system surfaces the tension for resolution.

**Prediction step:** Based on project history and the pattern of decisions made so far, the engine predicts which decisions are likely to be revisited and pre-computes the impact of probable changes. "Based on the rate at which thermal requirements have been tightening, there's a 60% chance the temperature limit decision will need revision within two weeks. I'm pre-computing design variants that accommodate tighter limits so we're prepared."

## Verification Under Uncertainty

The probabilistic framework changes how verification operates:

**Committed regions:** Full proof obligations, discharged completely. Same as standard Torc verification.

**Tentative regions:** Conditional proofs. "If the PWM frequency is in [15kHz, 25kHz], then all timing properties hold." The proof is valid as long as the condition holds. If the tentative value is later committed within the proven range, no re-verification is needed.

**Ranged regions:** Universally quantified proofs. "For ALL flow rates in [5, 15] L/min, the safety properties hold." This is stronger than point verification — it guarantees correctness across the entire range.

**Constrained Set regions:** Finite case analysis. "For motor = ABC-1234, properties hold. For motor = XYZ-5678, properties hold. Therefore properties hold for all possible motor selections." Complete enumeration of the constrained set.

**Deferred/Blocked regions:** The engine identifies the maximal set of properties that can be proven regardless of the deferred decision (universal properties) and those that cannot (contingent properties). The human sees exactly how many properties are waiting on each unresolved decision.

This means the human gets meaningful, progressively stronger verification feedback throughout the specification process — not a binary pass/fail at the end.
