# 13. The Specification Interface: Collaborative Intent Resolution

## The Problem

When a human works with an AI agent to develop software, two failure modes dominate:

**Under-specification.** The human says "build me a motor controller" and carries an enormous mental model of implicit assumptions — that the controller should fail safe, that current limits follow from the motor datasheet, that the CAN bus protocol is J1939, that the PWM frequency shouldn't be audible. None of this gets stated because the human doesn't recognize these as *decisions*. They're invisible context — the water the fish doesn't see. The AI, lacking that tacit knowledge, makes different choices. The result technically meets the specification but violates the intent.

**Over-specification.** The human, perhaps burned by under-specification, starts dictating implementation: "Use 20kHz PWM. Use a PI controller with Kp=0.5 and Ki=0.02. Use trapezoidal integration." Now the AI has no room to discover that a different topology would be more robust, that the integration method introduces unnecessary phase lag, or that decoupling ADC sampling from the control loop would improve noise performance. The human has written the program in English and asked the AI to translate it — discarding the entire value of an AI-native system that can explore the solution space.

Both failures share a root cause: **humans lack tools for expressing intent at the right level of abstraction.** Natural language is too ambiguous. Programming languages force implementation details. Requirements documents are simultaneously too vague and too specific.

## Design Principles

### Specification Is a Process, Not a Document

Traditional engineering treats specification as a phase that produces a document. The document gets reviewed, approved, and handed to implementation. This model assumes the human knows what they want before development begins. They almost never do.

The Torc specification interface treats specification as a **living process with persistent state**. The specification evolves as the human's understanding deepens, as the AI explores the solution space, and as decisions interact to create emergent constraints. The interface manages this evolution explicitly rather than pretending it doesn't happen.

### Model the Human's Epistemic State

No current development tool models what the engineer *knows, doesn't know, is uncertain about, and has decided not to think about yet*. IDEs model code. Requirements tools model requirements. Project tools model timelines. Nothing models the engineer's state of understanding.

The Torc specification interface maintains an explicit model of the human's epistemic state for every decision in the design. This is the fundamental innovation.

### Iterative Layering Over Upfront Completeness

Humans don't solve problems by enumerating every constraint up front. They work in layers — establishing the broad shape first, then focusing on whatever layer becomes the most important blocker, then diving deeper where needed. The specification interface supports this natural workflow rather than fighting it.

## The Decision State Model

Every design decision in a Torc project occupies one of seven states:

### Unexplored

The system knows this decision exists (derived from domain knowledge, dependency analysis, or the structure of the problem) but has not yet surfaced it to the human. Decisions remain unexplored when higher-priority decisions must be resolved first, or when the decision isn't relevant at the current level of design detail.

The AI manages exploration priority based on: dependency ordering (decisions that block other decisions surface first), risk ranking (safety-critical decisions surface before optimization decisions), and the human's current focus area.

### Deferred

**Human response: "I don't know, and I don't want to focus on this now."**

The human acknowledges a decision exists but explicitly parks it. This tells the AI two things:

1. Use a reasonable default and proceed. The design should not stall on this decision.
2. This decision may change later. Do not let the rest of the design become brittle around it.

The AI treats deferred decisions as soft constraints — it makes a provisional choice but maintains the ability to revisit cheaply. In the Torc computation graph, deferred decisions produce regions with provisional bindings flagged for future revisitation. The materialization engine avoids optimizations that would make reversal expensive.

Deferred decisions have an optional **revisit trigger**: conditions under which the system should re-surface the decision. For example: "Defer PWM frequency selection, but revisit when I commit the motor selection, because the motor's electrical time constant constrains the viable PWM range."

### Exploring

**Human response: "I don't know, and I think you should recommend options and a preferred solution."**

The human is delegating analysis to the AI while retaining decision authority. This is the mode where the AI's ability to explore the solution space adds the most value.

The AI should respond with structured options, not just a recommendation:

```
Decision: Control topology for brushless DC motor

Option A: Field-Oriented Control (FOC)
  Pros: Best torque ripple (<2%), efficient across speed range, smooth low-speed operation
  Cons: Requires position sensor or observer, higher computational cost (~800 cycles/iteration)
  Fits your constraints: Yes — within WCET budget, meets ripple requirement
  Excludes: Nothing significant

Option B: Trapezoidal (Six-Step) Commutation
  Pros: Simple, low computational cost (~200 cycles), no position sensor needed for sensorless
  Cons: Higher torque ripple (10-15%), poor low-speed performance, audible noise
  Fits your constraints: NO — violates your <5% torque ripple requirement
  Excludes: Low-speed precision operation

Option C: Sinusoidal Commutation
  Pros: Good torque ripple (~5%), moderate complexity
  Cons: Less efficient than FOC at partial load, still requires position feedback
  Fits your constraints: Marginal — meets ripple requirement at boundary
  Excludes: Nothing significant

Recommendation: Option A (FOC)
Reasoning: Your torque ripple and low-speed requirements effectively eliminate Option B.
Option C meets requirements but offers no advantage over FOC given your target hardware
has sufficient computational headroom. FOC also gives you the most room for future
performance improvement.
```

The human can accept, reject, ask for more options, or ask the AI to dig deeper into a specific option. Accepting moves the decision to Committed. Asking for more analysis keeps it in Exploring.

### Tentative

**Human response: "I might know, and it is Y — but I'm uncertain."**

The human has an intuition, a partial answer, or a preference, but isn't confident it's correct or optimal. This is the state that traditional specification approaches handle worst — they force binary committed/uncommitted with no middle ground.

The AI treats tentative decisions as weighted soft constraints: prefer solutions consistent with Y, but don't eliminate solutions that aren't. If the AI discovers a dramatically better solution that contradicts Y, it surfaces the finding:

```
You indicated the PWM frequency should be around 20kHz (tentative).

I found that 16kHz gives significantly better efficiency on your target hardware
because it aligns with the ADC's native sampling architecture, eliminating one
conversion delay per control cycle. This recovers 4μs of your WCET budget.

Trade-off: 16kHz is at the upper edge of human hearing for young adults. In a
pump application with mechanical noise, this is typically not audible. In a quiet
direct-drive application, it might be.

Options:
  (a) Change to 16kHz (my recommendation for your application)
  (b) Keep 20kHz (your original preference)
  (c) Make this a configurable parameter (adds ~200 bytes flash, 0 WCET impact)
```

Tentative decisions can be strengthened to Committed or weakened to Exploring as the human's understanding evolves.

### Committed

**Human response: "I do know, and it shall be X."**

This is a hard constraint. The human is making a binding decision. This is the state that demands the most feedback from the system, because a committed decision reshapes the entire solution space.

When a decision is committed, the system immediately produces a **Decision Impact Report**:

```
COMMITTED: Control loop frequency = 20kHz

Now Determined (consequences of this decision):
  • PWM switching frequency is 20kHz or a sub-harmonic
  • ADC sampling must complete within 50μs per channel
  • Minimum motor electrical time constant: ~200μs
  • Control loop WCET budget: 50μs (at your target clock rate)
  • Interrupt rate: 20,000/sec — verify this doesn't starve lower-priority tasks

Now Excluded (options removed by this decision):
  • Variable-frequency PWM strategies (e.g., spread-spectrum for EMC)
  • Motors with electrical time constants below 200μs
  • Control algorithms requiring more than 50μs per iteration on your target

⚠ Flagged Concern:
  • 20kHz is at the upper edge of human hearing. Depending on motor construction
    and mounting, this may produce audible tonal noise. Your application context
    (pump) likely masks this, but confirm with your mechanical engineering team.

Still Open (unaffected by this decision):
  • Control algorithm selection
  • Current sensing method
  • Communication protocol
  • Safety monitoring architecture
```

The Decision Impact Report serves multiple purposes:

1. **Error catching.** If the human typed 2kHz instead of 20kHz, the consequences ("minimum motor time constant: 2ms — this excludes most brushless motors") immediately reveal the mistake.
2. **Informed consent.** The human sees exactly what they're giving up. If they didn't realize variable-frequency PWM was an option, now they know it was — and that they've just eliminated it.
3. **Downstream planning.** The "Still Open" section tells the human what decisions remain unaffected, helping them plan their next focus area.

### Derived

This decision was not made directly by the human. It was determined by the combination of other committed decisions, and the system can show the full chain of reasoning.

```
DERIVED: Maximum motor pole count = 8 poles

Derived from:
  • Control loop frequency = 20kHz (committed)
  • Maximum motor speed = 6000 RPM (committed)
  • Electrical frequency = (RPM × poles) / 120
  • At 6000 RPM with 10 poles: electrical frequency = 500 Hz
  • Nyquist: control loop must be >2× electrical frequency
  • At 20kHz control loop: maximum electrical frequency = 10kHz
  • Maximum poles at 6000 RPM: floor(10kHz × 120 / 6000) = 200 → effectively unconstrained

Actually, this is NOT constraining for your application. Updating...

DERIVED: No motor pole count constraint from current committed decisions.
```

Derived decisions update automatically when their source decisions change. The human doesn't need to manage them but can inspect the derivation chain at any time.

### Conflicted

Two or more committed or tentative decisions are incompatible. The system detects this through constraint propagation and immediately surfaces the conflict:

```
⛔ CONFLICT DETECTED

Your committed decisions are mutually unsatisfiable:

  Decision 1: WCET budget for safety monitor = 10μs (committed)
  Decision 2: Safety monitor must check all 3 phase currents, bus voltage,
              motor temperature, AND compute RMS current (committed)
  Target: Cortex-M4 @ 168MHz

The minimum execution time for the specified safety checks is approximately
14μs on your target hardware. This exceeds your 10μs budget by 40%.

Resolution options:
  (a) Increase WCET budget to 15μs (relaxes Decision 1)
  (b) Remove RMS current computation from safety monitor; compute it in
      the lower-priority speed loop instead (relaxes Decision 2)
  (c) Split safety monitor into two priority levels: critical checks (overcurrent,
      overvoltage) at 6μs in high-priority ISR; secondary checks (temperature,
      RMS) at 8μs in medium-priority task (restructures both decisions)
  (d) Change target to Cortex-M7 @ 480MHz (changes target constraints)

Recommendation: Option (c) — this maintains the fastest possible response
for the most dangerous fault conditions while keeping all checks within
a single control cycle.
```

Conflicts must be resolved before materialization can proceed for the affected regions. However, unrelated regions of the design can continue to be materialized and verified — the conflict doesn't block the entire project.

## The Structured Exploration Process

### Initial Engagement

When a human initiates a new project or describes a new feature, the AI performs an initial domain analysis and presents a structured exploration:

```
You've described: "Brushless DC motor controller for a pump application"

I've identified 47 decisions needed for this system.

I have reasonable defaults for 31 of them based on the domain (pump drive,
industrial application, safety-relevant but not safety-critical).

I need your input on 9 decisions that significantly shape the design.
I've organized them by priority — we should start with the first group.

There are 7 decisions where I want to flag that your choice will
substantially constrain the solution space.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
GROUP 1: Safety & Fault Behavior (decide first)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1. What happens when motor current exceeds the rated limit?
   [ ] I don't know, park it
   [ ] Recommend options
   [ ] Hard shutdown within 50μs ← (or your value)
   [ ] Other: ___

2. What happens when communication with the supervisory controller is lost?
   [ ] I don't know, park it
   [ ] Recommend options
   [ ] Continue at last commanded speed for ___ seconds, then ramp to stop
   [ ] Other: ___

3. Maximum motor winding temperature before protective action?
   [ ] I don't know, park it
   [ ] Recommend options
   [ ] ___ °C
   [ ] Not applicable (no temperature sensing)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
GROUP 2: Performance Requirements (needed to select topology)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[Presented after Group 1 is addressed]
```

The human responds to each decision with one of the four modes. They don't have to address all decisions in a group before moving on — deferral is always an option. But the system tracks what's been addressed and what hasn't, and periodically reminds the human of deferred decisions that are blocking downstream progress.

### Progressive Deepening

As high-level decisions are committed, the system reveals lower-level decisions that are now relevant:

```
You've committed: Control topology = FOC, Target = STM32F407

New decisions are now relevant:

GROUP 4: FOC Implementation Details
  (12 decisions, 8 have reasonable defaults, 4 need your input)

  1. Position feedback method?
     Context: FOC requires rotor position. Your motor has hall sensors
     AND an encoder. Both are usable.
     [ ] Recommend options
     [ ] Hall sensors (simpler, lower resolution)
     [ ] Encoder (higher resolution, better low-speed performance)
     [ ] Sensorless observer (no sensor cost, but limited at very low speed)

  2. Current sensing method?
     [ ] Recommend options ← I'd suggest this one; there are meaningful trade-offs
     [ ] Inline shunt resistors
     [ ] Hall-effect sensors

  ...
```

This progressive deepening mirrors how engineers naturally work: establish the architecture, then fill in the details. The system manages the layering so the human encounters decisions when they have enough context to make them meaningfully.

### The Assumption Layer

Every time the AI makes a choice — whether filling a default, making a provisional decision for a deferred item, or deriving a consequence — it logs the underlying assumption:

```
Assumption A-012: Motor back-EMF constant is temperature-stable within
the operating range.
  Confidence: Medium
  Source: Typical for ferrite-magnet BLDC motors
  Would not hold for: NdFeB magnets at >120°C, or motors with significant
    magnetic saturation effects
  Impact if wrong: Speed regulation error increases by up to 15% at thermal
    extremes
  Depends on: Motor selection (currently deferred)
  Revisit when: Motor selection is committed
```

Assumptions are surfaced to the human in two ways:

1. **On demand.** The human can view all assumptions at any time, filtered by confidence level, impact, or affected subsystem.
2. **Proactively.** The system surfaces high-impact, low-confidence assumptions when the human is working in a related area. "While you're deciding on the temperature protection threshold, you should know I'm assuming the motor's back-EMF constant doesn't change significantly with temperature. If you're using a rare-earth motor at high temperatures, this assumption may not hold."

## Integration with Torc Verification

The decision state model integrates directly with Torc's formal verification framework:

### Committed Decisions → Full Proof Obligations

For committed decisions, the verification engine generates and discharges full proof obligations. These are hard requirements that the materialized system must satisfy.

### Tentative Decisions → Conditional Verification

For tentative decisions, the engine performs conditional verification: "If Y holds, then these properties are guaranteed. If Y changes, these 14 proofs must be re-evaluated." This gives the human meaningful verification feedback even on an incomplete specification.

### Deferred Decisions → Universal Properties

For deferred decisions, the engine identifies which properties hold regardless of the deferred choice (universal properties) and which depend on it (contingent properties). This tells the human exactly where to focus:

```
Verification Status (with 5 deferred decisions):

  87 of 120 safety properties: VERIFIED (hold regardless of any outstanding decisions)
  28 safety properties: CONTINGENT on control topology selection (deferred)
   5 safety properties: CONTINGENT on PWM frequency decision (deferred)
   0 safety properties: CONFLICTED

You can materialize and deploy a verified partial system now. The 33 contingent
properties will be verified when their controlling decisions are committed.
```

### Conflicted Decisions → Verification Halt

Conflicted decisions produce verification failures in the affected region. The conflict report identifies exactly which constraints are incompatible and proposes resolution paths.

## The Specification Graph

The specification process produces its own graph structure — a **Decision Graph** — that sits above the Torc computation graph:

```
Decision Graph                          Torc Computation Graph
┌─────────────────────┐                ┌──────────────────────┐
│ Safety Behavior      │───committed───▶│ Safety Monitor Region │
│ [COMMITTED]          │                │ [VERIFIED]            │
├─────────────────────┤                ├──────────────────────┤
│ Control Topology     │───committed───▶│ FOC Algorithm Region  │
│ [COMMITTED: FOC]     │                │ [VERIFIED]            │
├─────────────────────┤                ├──────────────────────┤
│ PWM Frequency        │──tentative───▶│ PWM Region            │
│ [TENTATIVE: ~20kHz]  │                │ [CONDITIONALLY VERIFIED]│
├─────────────────────┤                ├──────────────────────┤
│ Current Sensing      │───deferred───▶│ Sensing Region        │
│ [DEFERRED]           │                │ [PROVISIONAL]         │
├─────────────────────┤                ├──────────────────────┤
│ CAN Protocol         │               │ (not yet materialized)│
│ [UNEXPLORED]         │                │                      │
└─────────────────────┘                └──────────────────────┘
```

As decisions progress through their lifecycle, corresponding regions of the computation graph are materialized, verified, and locked. The human can see — at any time — exactly how much of the system is firm, how much is provisional, and where the open questions are.

## Specification Persistence and Versioning

The Decision Graph is versioned alongside the Torc computation graph. Every state transition is recorded with timestamp, rationale, and the human's stated confidence level:

```
Decision: PWM Frequency
  v1 [2026-02-15 10:00] UNEXPLORED
  v2 [2026-02-15 10:30] EXPLORING (human requested options)
  v3 [2026-02-15 10:45] TENTATIVE: ~20kHz (human: "probably around 20k")
  v4 [2026-02-15 14:00] COMMITTED: 16kHz (human accepted AI recommendation
                         after analysis showed ADC alignment benefit)
     Impact report generated and acknowledged.
```

This history serves as design rationale documentation — invaluable for safety certification, design review, and future maintenance. It answers the question "why is the PWM frequency 16kHz?" with a complete trail: the human initially thought 20kHz, the AI recommended 16kHz for a specific technical reason, and the human accepted after reviewing the evidence.

## Example: Complete Specification Session

To illustrate the full workflow, here is a condensed example of a human specifying a motor controller through the Torc specification interface:

```
HUMAN: I need a BLDC motor controller for a coolant pump in an automotive
       application. ASIL-B.

SYSTEM: I've identified 63 decisions for an ASIL-B automotive BLDC pump controller.
        38 have standard defaults for this domain. 15 need your input across
        4 priority groups. Starting with Group 1: Safety Architecture.

        1. Overcurrent response?

HUMAN: Hard shutdown, hardware comparator backup. [COMMIT]

SYSTEM: ✓ Committed. Impact: This requires a dedicated comparator peripheral on
        your MCU. I've added "hardware comparator available" to target constraints.
        3 MCU options in our target library meet this requirement.

        2. Loss of communication behavior?

HUMAN: I don't know, park it. We need to discuss this with the systems team.
       Revisit when I commit the communication protocol. [DEFER]

SYSTEM: ✓ Deferred. Provisional default: maintain last command for 500ms,
        then controlled ramp to zero speed. I'll resurface this when the
        communication protocol is decided.

        3. Safe state definition?

HUMAN: Motor stopped, all FETs off, fault pin asserted. [COMMIT]

SYSTEM: ✓ Committed. Impact: "All FETs off" means no active braking in safe
        state. The pump will coast to stop based on mechanical friction.
        Coast-down time depends on pump inertia (currently unknown).
        Is coast-down acceptable, or do you need active braking before
        entering safe state?

HUMAN: Good catch. Active braking for 200ms, then all FETs off. [AMEND COMMIT]

SYSTEM: ✓ Amended. Updated impact analysis: WCET for safe-state entry
        is now 200ms + transition time. This is within typical automotive
        fault response requirements.

        Moving to Group 2: Performance Requirements.

        4. Speed range and accuracy?

HUMAN: Recommend options — I know the pump spec but I'm not sure how that
       translates to motor requirements. [EXPLORE]

SYSTEM: Based on typical automotive coolant pumps...
        [presents 3 options with trade-offs]

        ...

[Session continues through all priority groups, with the human
using different response modes for each decision based on their
knowledge and confidence level]

        ...

SYSTEM: Specification status after this session:
        Committed: 28 decisions (including 7 safety-critical)
        Tentative: 4 decisions
        Deferred: 6 decisions (3 awaiting external input)
        Exploring: 2 decisions (AI analysis in progress)
        Unexplored: 23 decisions (lower-priority, will surface as design progresses)

        Verification: 87/120 safety properties verified.
        28 contingent on deferred decisions. 5 contingent on tentative decisions.

        Materialization: 60% of computation graph materialized.
        Flash estimate: 34KB. RAM estimate: 2.8KB. All within budget.

        Next recommended focus: The 2 decisions currently in Exploring state.
        I'll have recommendations ready for your next session.
```

## The Deeper Principle

What this specification interface really does is model the collaboration between a human domain expert and an AI implementation expert — the same kind of structured dialogue that happens between a senior systems engineer and a senior software engineer on a well-run project.

The senior systems engineer doesn't hand over a complete specification. They have a conversation. They iterate. They say "I'm not sure about this part, what do you think?" and "make sure you consider this edge case" and "I don't care how you solve that, just make sure it meets this requirement." They sometimes defer decisions explicitly: "we'll figure out the communication protocol later, just make sure the interface is clean."

This specification interface formalizes that conversation into something an AI can participate in as a genuine collaborator — maintaining shared state, tracking uncertainty, propagating constraints, surfacing conflicts, and providing decision impact analysis that no human could compute in their head across a system with dozens of interacting decisions.

The result is specifications that are more complete (because the AI surfaces decisions the human would have missed), less over-constrained (because the AI nudges toward constraint envelopes rather than specific values), and more trustworthy (because every decision has a recorded rationale, a known confidence level, and a verified impact analysis).
