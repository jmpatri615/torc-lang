# 15. The Specification Workspace

## Beyond Documents

The specification interface described in documents 13 and 14 — decision states, probabilistic inference, sensitivity analysis, information value ranking — cannot be expressed as a text document. It is not a form, not a conversation, and not a requirements database. It is a **shared cognitive workspace**: a persistent, interactive, stateful environment where a human and an AI maintain a joint understanding of a system that doesn't exist yet.

This document describes what that workspace looks like, how it behaves, and how humans interact with it.

## The Decision Map

The central element of the workspace is the Decision Map — a visual, spatial, interactive representation of the decision graph.

### Visual Encoding

Each decision is a node. Its visual properties communicate its state without requiring the user to click or read:

| Property | Encodes |
|----------|---------|
| **Color** | Decision state — solid blue for committed, pulsing amber for tentative, dimmed gray for deferred, flashing red for conflicted, spinning green for exploring, outlined for constrained set, hatched for blocked, bright white for volatile |
| **Size** | Impact — decisions affecting many downstream choices are visually larger |
| **Position** | Domain clustering — safety decisions group together, performance decisions group together, hardware decisions group together. Spatial proximity implies conceptual relatedness |
| **Edges** | Dependency — visible connections show which decisions affect which. Edge thickness indicates coupling strength |
| **Glow/halo** | AI attention — a subtle halo appears when the AI has generated new analysis, a recommendation, or a concern for this decision |
| **Border** | Confidence — thick solid border for high-confidence states, thin dashed border for low-confidence. Visually distinguishes a committed decision from a tentative one at a glance |

### Spatial Behavior

The Decision Map is not a static diagram. It is a force-directed layout that continuously rebalances as decisions are added, resolved, and connected:

- When a decision is committed, its node solidifies and its downstream connections strengthen — dependent nodes drift slightly toward it, visually reinforcing the causal chain.
- When a conflict emerges, the conflicting nodes repel slightly while a red connection highlights between them, creating visual tension that draws the eye.
- When a cluster of decisions is fully committed, the cluster contracts and dims slightly — it's resolved, it no longer demands attention, and it makes room for the active frontier.
- New decisions that emerge from progressive deepening animate into existence at the edge of the relevant cluster, drawing attention through motion.

The user can zoom, pan, and rearrange clusters manually. The layout engine respects manual positioning — if the user moves the safety cluster to the upper left, it stays there as new safety decisions are added to it. The workspace learns the user's preferred organization.

### Interaction

Clicking a decision node expands it into a **Decision Card** — a rich, interactive panel. The card contains:

**Header:** Decision name, current state, owner, last modified timestamp.

**State controls:** Direct manipulation of the decision state. The ten response modes from Document 14 are available as actions: Commit, Tentative, Defer, Explore, Constrained Set, Bounded Range, Volatile, Blocked, Challenge, Exclude. Each action prompts for the appropriate information (a value for Commit, a range for Bounded Range, an owner for Blocked, etc.).

**Probability display:** For uncertain decisions, a visual representation of the current probability distribution. For continuous parameters, a density curve. For discrete choices, a bar chart. The user can manipulate the distribution directly — drag to narrow a range, click to exclude an option, adjust a confidence slider to widen or tighten a tentative estimate.

**Dependency panel:** What this decision affects (downstream) and what affects it (upstream). Each dependency link shows the coupling strength and the nature of the relationship.

**AI analysis:** If the AI has performed analysis on this decision (exploring, sensitivity, adversarial), the results appear here — structured options, trade-off comparisons, recommendations. Not as chat text, but as interactive, sortable, filterable analysis panels.

**History:** The full decision history — every state transition with timestamp, rationale, and the human's stated confidence level. This is the design rationale documentation that safety certification requires.

**Impact preview:** Before committing, the user can preview the Decision Impact Report — seeing what would be determined, excluded, and flagged *before* actually making the commitment. This "what if" capability prevents accidental over-constraint.

### The Entropy Gauge

A persistent indicator — positioned like a fuel gauge or progress bar — displays the total specification entropy. It provides at-a-glance awareness of how "decided" the overall design is:

```
Specification Completeness
[████████████████░░░░░░░░░░░░░░░░] 47%
Entropy: 412 / 847 bits remaining
```

The gauge is color-coded: green when entropy is decreasing at a healthy rate, yellow when progress has stalled (the remaining decisions may need external input), red if entropy has increased (decisions were un-committed or new requirements were added).

Clicking the gauge opens an entropy breakdown by domain, showing where the remaining uncertainty concentrates. This helps the user decide what to focus on next — or whether to wait for external dependencies before continuing.

### The Information Value Indicator

Adjacent to the entropy gauge, a small ranked list shows the top 3-5 decisions by Expected Value of Information — the decisions that, if resolved, would most reduce overall uncertainty:

```
Highest-Value Decisions:
  1. Motor selection       (EVI: 34 bits)
  2. Safety integrity level (EVI: 23 bits)
  3. Target MCU            (EVI: 18 bits)
```

This dynamically recomputes as decisions are made. It's the workspace's way of saying "here's where your attention is most valuable right now."

## The AI as a Spatial Presence

The AI is not a chat window bolted to the side of the workspace. It is a presence *within* the workspace, manifesting at the decision nodes where it has something relevant to contribute.

### Decision-Attached Intelligence

When the AI has information relevant to a specific decision — a completed analysis, a concern, a recommendation, a detected conflict — it attaches that information directly to the decision node as an indicator badge. The user sees at a glance which decisions have AI input waiting.

This is a fundamental departure from conversation-based AI interaction. In a chat, the AI's analysis about the PWM frequency and its analysis about the safety architecture end up in the same linear scroll, separated by time and context. In the workspace, each analysis lives at the decision it pertains to, accessible when the user is thinking about that decision.

### Consequence Visualization

When the user is exploring a decision, the AI renders the consequences of each option as branching paths in the Decision Map. Selecting "Option A" temporarily highlights the downstream nodes that would be affected, showing how each option reshapes the decision landscape. The user sees the topology of consequences — not as a text table, but as a spatial propagation through the graph.

For binary or ternary choices, the workspace can show a split view: "Here's what the map looks like if you choose FOC. Here's what it looks like if you choose trapezoidal commutation. Notice that FOC resolves 6 more downstream decisions immediately, while trapezoidal leaves them open."

### Trade-off Space Visualization

For complex multi-objective trade-offs, the AI can project a **scenario space** into the workspace — a two- or three-dimensional visualization where axes represent key trade-off dimensions (cost vs. performance vs. reliability, size vs. speed vs. power consumption).

Each point in the space represents a possible design configuration. The space is populated through Monte Carlo sampling of the uncertain decision space. Committed decisions constrain the point cloud. Tentative decisions define a region of interest. The AI highlights the Pareto frontier — the set of configurations where no dimension can be improved without degrading another.

The user can interact with the scenario space:
- Click a point to see which decision values produce that configuration
- Drag selection boundaries to explore "what if we accepted slightly worse efficiency to get better ripple?"
- Watch the point cloud narrow in real time as decisions are committed

### Proactive Alerts

The AI monitors the decision graph continuously and surfaces alerts when it detects:

- **Emerging conflicts:** Two tentative decisions are trending toward incompatibility
- **Assumption invalidation:** A committed decision has made a previous assumption incorrect
- **Opportunity discovery:** A combination of committed decisions has opened up an optimization that wasn't available before
- **External change detection:** If connected to real-time data sources (component databases, regulatory feeds), the AI can alert when an external change affects the design — "The STM32F407 has been marked as not recommended for new designs by ST. Your volatile MCU decision should be revisited."

Alerts appear as unobtrusive indicators on affected decision nodes, escalating in visual urgency based on impact.

## The Conversation Channel

Natural language conversation exists as a secondary input channel. It is not the primary interface — the Decision Map is. But conversation handles what structured interaction cannot:

### Unstructured Intent

When the human has a thought that doesn't map neatly to a decision node, they speak or type it:

"This pump is going into a submarine, and I forgot to mention that until now."

The AI processes this natural language input and updates the Decision Map structurally: environmental constraints change, new decision nodes materialize (corrosion protection, pressure rating, acoustic signature), and existing committed decisions get flagged for re-evaluation if affected. The conversation becomes a trigger for structural change in the workspace, not a log entry to be read later.

### Design Rationale

Sometimes the human wants to explain *why*, not just *what*. "I'm choosing the higher-rated motor because our field service team has had reliability problems with the smaller one in high-humidity environments." This rationale gets attached to the decision's provenance record — it's not just what was decided, but why, captured at the moment of decision in the human's own words.

### Exploratory Discussion

For genuinely open-ended thinking — "I'm not sure whether we should even use a brushless motor for this application, what are the alternatives?" — conversation is the right medium. The AI engages in a dialogue, and as the discussion produces concrete decisions or constraints, those are reflected back into the Decision Map. The conversation generates structure rather than replacing it.

### Context-Aware Conversation

The conversation channel is always aware of the current workspace context. If the user has a decision card open, the AI knows they're thinking about that decision. If they're zoomed into the safety cluster, the AI knows the current focus area. Conversational responses are tailored accordingly — the AI doesn't need to be told what the question is about because it can see where the human's attention is.

## Multiple Projections

The same underlying decision state can be viewed through multiple projections, each optimized for a different task or audience:

### Decision Map (default)

The spatial graph view. Best for understanding structure, dependencies, and consequence propagation. This is the primary working view.

### Priority Queue

A sorted, filterable list of decisions ranked by the AI's Expected Value of Information calculation. "These are the decisions that, if resolved, would most reduce overall uncertainty." Best for focused work sessions when the user wants to make maximum progress in limited time.

The list updates dynamically. As the user commits decisions, the rankings shift. The queue always shows the current most-valuable next action.

### Domain View

Decisions organized by engineering domain — electrical, mechanical, software, safety, communication, thermal, manufacturing. Each domain appears as a panel with its own completeness indicator.

Best when a domain specialist is reviewing their area: the electrical engineer sees only electrical decisions, with their safety implications highlighted but not dominant. Each specialist works in their domain while the system maintains global coherence.

### Timeline View

Decisions organized by when they need to be resolved, based on project schedule and dependency ordering. A Gantt-like view that shows:

- Decision deadlines derived from downstream dependencies ("Motor selection must be committed by March 1 for the hardware team to start PCB layout")
- Blocked decisions with their expected resolution dates
- The critical path through the decision graph (the chain of unresolved decisions that determines the earliest possible design freeze date)

Best for project management and schedule planning.

### Risk View

Decisions colored and sized by risk — the product of impact and uncertainty. A large, brightly colored node is a high-impact decision with high uncertainty: the most dangerous combination. A large, dim node is high-impact but resolved. A small, bright node is uncertain but low-impact.

The risk view makes it immediately obvious where the "known unknowns" concentrate. Best for safety reviews, management reporting, and milestone gate reviews.

### Verification View

Decisions colored by their verification status. Committed and verified decisions are green. Committed but unverified are yellow. Decisions blocking verification of safety properties are red. Contingent properties show which decision they're waiting on.

This view answers: "How much of the design is formally verified right now, and what's blocking the rest?"

### Diff View

A comparison between two points in time — the current state versus the last session, the last milestone, or any historical snapshot. Shows:

- Decisions that changed state (committed, revised, un-committed)
- New decisions that emerged
- Assumptions that were added or invalidated
- Verification status changes
- Entropy change

Best for team handoffs, design reviews, and "what happened while I was away" catch-up.

## Multi-User Collaboration

Engineering organizations have multiple people contributing to a specification. The workspace supports this natively.

### Decision Ownership

Each decision can have an assigned owner — the person responsible for resolving it. The Decision Map can be filtered by owner, showing each person only their decisions while maintaining the full dependency context.

Ownership can be delegated: "I'm assigning motor selection to the mechanical engineering team. When they commit, the consequences propagate and I'll review the impact on my decisions."

### Simultaneous Editing

Multiple users can work in the workspace simultaneously, seeing each other's changes in real time. Cursor awareness (similar to collaborative document editors) shows where each person's attention is focused.

When one person's committed decision creates a conflict with another person's committed decision, both receive an immediate notification. The conflict resolution options are presented to both, and the workspace supports negotiation — each person can see the other's constraints and preferences.

### Role-Based AI Adaptation

The AI adapts its communication style and analysis depth to each user's role and expertise:

- For the motor specialist, the AI asks detailed questions about magnetic saturation, thermal derating, and bearing life — and provides analysis in those terms.
- For the safety engineer, the AI frames every decision in terms of safety integrity, fault tolerance, and diagnostic coverage — and highlights safety implications that other specialists might miss.
- For the systems architect, the AI presents high-level trade-offs and integration concerns — and flags when domain-specific decisions conflict with system-level requirements.
- For the project manager, the AI summarizes progress, highlights schedule risks, and identifies blocked decisions that need organizational action.

All users work on the same underlying decision graph. The AI's role adaptation is a presentation layer, not a data separation. Every user can see every decision if they need to — but the AI helps them focus on what's most relevant to their role.

## Physical Interaction Models

### Large-Format Display

On a wall-mounted display or large touchscreen (common in engineering war rooms), the Decision Map becomes a team collaboration surface. Multiple people can interact simultaneously — committing decisions, exploring options, resolving conflicts — while seeing the full system context. The physical size of the display matches the cognitive scale of the design problem.

### Desktop

The standard interaction mode. The workspace occupies the full screen. Decision cards open as overlays. Multiple projections can be tiled side-by-side. Keyboard shortcuts support rapid state transitions for power users.

### Mobile / Tablet

A read-heavy projection optimized for review and lightweight updates. A field engineer commissioning hardware can update tentative decisions based on physical measurements: "I measured the actual motor back-EMF constant at 0.043 V/rad/s." The workspace updates, consequences propagate, and the field engineer sees immediately if any parameter adjustments are needed.

### Mixed Reality (Future)

For systems with significant spatial or physical layout constraints — PCB design, mechanical packaging, cable routing, thermal management — the workspace extends into three dimensions. The user can see the decision map overlaid on a 3D model of the physical system, making the connection between abstract decisions and physical reality immediate and intuitive.

Trade-off spaces that are naturally three-dimensional (cost vs. performance vs. reliability) can be explored as volumetric point clouds that the user can walk around, reach into, and manipulate.

## What This Is and What It Is Not

### What It Is

A shared cognitive workspace where a human and an AI collaborate to resolve the uncertainty inherent in designing a complex system. It is:

- **Stateful:** It remembers everything — every decision, every state transition, every rationale, every assumption.
- **Probabilistic:** It models uncertainty explicitly using Bayesian inference, information theory, and Monte Carlo analysis.
- **Active:** It doesn't wait passively for input. It identifies high-value decisions, detects conflicts, discovers opportunities, and surfaces concerns.
- **Spatial:** It represents the structure of the design problem as a navigable space, not a linear document.
- **Multi-perspectival:** It presents the same underlying reality through different views optimized for different tasks and audiences.
- **Collaborative:** It supports multiple humans and one AI working together on a shared understanding.

### What It Is Not

**Not a chat interface with visualization.** The spatial, stateful workspace *is* the specification. Conversation is a secondary input channel for unstructured intent. The AI's intelligence is distributed across the workspace, not confined to a text stream.

**Not a requirements management tool with AI.** Requirements tools (DOORS, Jama, Polarion) are fundamentally document databases with traceability links. They store static text that humans maintain. This workspace is a live inference engine that actively reasons about implications, maintains probabilistic uncertainty, and co-evolves with the design.

**Not an AI that takes orders.** The AI is not a command executor waiting for instructions. It is a collaborator that maintains shared state, tracks uncertainty, propagates constraints, computes information value, and provides decision impact analysis. It actively participates in the specification process by identifying gaps, challenging assumptions, and recommending where to focus.

**Not a replacement for human judgment.** Every binding decision is made by a human. The AI analyzes, recommends, challenges, and informs — but it does not commit. The human's epistemic state is modeled, respected, and supported, not overridden. The workspace is designed to make human judgment *better informed*, not to make it unnecessary.

## The Connection to Torc

The specification workspace produces the Decision Graph that drives Torc's computation graph materialization:

```
Specification Workspace        Torc Ecosystem
┌─────────────────────┐       ┌──────────────────────┐
│ Decision Map         │       │                      │
│ (human + AI)         │──────▶│ Decision Graph (.tdg) │
│                      │       │ (versioned artifact)  │
└─────────────────────┘       └──────────┬───────────┘
                                         │
                                         ▼
                              ┌──────────────────────┐
                              │ Torc Computation      │
                              │ Graph (.trc)          │
                              │ (AI-generated)        │
                              └──────────┬───────────┘
                                         │
                                         ▼
                              ┌──────────────────────┐
                              │ Materialization       │
                              │ Engine                │
                              │ (constraint solving)  │
                              └──────────┬───────────┘
                                         │
                                         ▼
                              ┌──────────────────────┐
                              │ Executable Artifact   │
                              │ (ELF, PE, bitstream)  │
                              └──────────────────────┘
```

The Decision Graph is a first-class versioned artifact in the Torc ecosystem. It's stored, diffed, branched, and merged just like the computation graph. Design reviews examine the Decision Graph. Safety certification audits trace from the Decision Graph through the computation graph to the materialized artifact. Every property of the final system can be traced back to a human decision with a recorded rationale, a known confidence level, and a verified impact analysis.

This is the complete specification pipeline: from human intent, through collaborative uncertainty resolution, through AI-generated computation graphs, through formal verification, to deployed executable code — with full traceability at every step.
