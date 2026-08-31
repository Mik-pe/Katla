---
name: discuss
description: >-
  Full debate-to-delivery pipeline with supervisor orchestration. Runs debates 
  between perspectives, creates implementation plans, and executes changes 
  for architectural decisions.
model: inherit
---

# Discuss - Debate to Delivery Pipeline

Run the full debate-to-delivery pipeline with supervisor orchestration using Agent Teams.

## Usage

```
/discuss [TOPIC or TASK DESCRIPTION]
```

## What This Does

1. **Debate Phase** - GFX (cross-backend) vs App perspectives reach consensus
2. **Planning Phase** - Prometheus creates implementation plan
3. **Build Phase** - Hephaestus implements in chunks

---

## Setup (Do This First)

**CRITICAL**: For Agent Teams to work, ensure `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is enabled in your settings.

1. **Check for existing team** - If TeamCreate fails with "already leading team", run TeamDelete first
2. **Create team** - Use TeamCreate with a descriptive team_name like "debate-team"
3. **Spawn teammates** - All teammates use `subagent_type: "general-purpose"` and read their persona file

**TASK**: $ARGUMENTS

---

## Phase 1: Debate (Unbiased)

**IMPORTANT**: As Supervisor, you are the moderator. Do NOT pre-explore the codebase. Let the debaters investigate and form their own conclusions.

### Spawn Debaters

Spawn both debaters as teammates using the Agent tool:

```python
# GFX Maintainer
Agent(
  subagent_type: "general-purpose",
  team_name: "debate-team",
  name: "gfx-maintainer",
  model: "Opus",
  prompt: """YOU MUST READ .Codex/agents/gfx-maintainer.md and adopt that persona completely.

Your task: DEBATE THIS TOPIC with app-maintainer: $ARGUMENTS

Use SendMessage to communicate with app-maintainer. When you reach consensus, report to the supervisor with the implementation criteria."""
)

# App Maintainer
Agent(
  subagent_type: "general-purpose",
  team_name: "debate-team",
  name: "app-maintainer",
  model: "Opus",
  prompt: """YOU MUST READ .Codex/agents/app-maintainer.md and adopt that persona completely.

Your task: DEBATE THIS TOPIC with gfx-maintainer: $ARGUMENTS

Use SendMessage to communicate with gfx-maintainer. When you reach consensus, report to the supervisor with the implementation criteria."""
)
```

### Moderation Loop

After receiving consensus or after 3 rounds:
1. Identify points of agreement
2. Identify core disagreement
3. Make decision and move forward if deadlocked

### ⚠️ CRITICAL: Wait for Implementation Criteria

Before moving to Phase 2 (Planning), **BOTH debaters must provide**:

1. **Clear decision statement** - What was agreed
2. **Detailed criteria** - Specific requirements the implementation must meet
3. **File locations** - Which files/modules are affected
4. **Constraints** - Architectural rules to follow
5. **Acceptance criteria** - How to verify correctness

**DO NOT proceed to Prometheus until you have this information.**

Example of what to wait for:
```
Supervisor, consensus reached. Implementation criteria:
- Decision: [what was agreed]
- Technical criteria: [specific requirements]
- Files: katla_gfx/src/material/mod.rs, katla_gfx/src/renderer.rs
- Constraints: katla_gfx must NOT depend on katla_math
- Acceptance: cargo test passes, no new public API items
```

---

## Phase 2: Consult Prometheus

Once debaters reach consensus, spawn Prometheus:

```python
Agent(
  subagent_type: "general-purpose",
  team_name: "debate-team",
  name: "prometheus",
  model: "sonnet",
  prompt: """YOU MUST READ .Codex/agents/prometheus.md and adopt that persona completely.

Your task: Create an implementation plan based on this consensus:

[Insert consensus details from debaters]

Use SendMessage to communicate with the supervisor when your plan is ready."""
)
```

---

## Phase 3: Invoke Hephaestus

Once Prometheus has a plan, spawn Hephaestus:

```python
Agent(
  subagent_type: "general-purpose",
  team_name: "debate-team",
  name: "hephaestus",
  model: "Opus",
  prompt: """YOU MUST READ .Codex/agents/hephaestus.md and adopt that persona completely.

Your task: Implement this plan:

[Insert plan from Prometheus]

Use SendMessage to report progress to the supervisor."""
)
```

---

## Decision Authority

As Supervisor, you have final say when:
- Debate exceeds 3 rounds without convergence
- Both sides have valid but incompatible positions
- Time pressure requires shipping

When you decide, document:
1. The decision
2. The rationale
3. Any dissenting views for future reconsideration

---

## Output Template

After full pipeline completion:

```markdown
## Pipeline Complete: [Task Name]

### Consensus Decision
[What was agreed]

### Implementation Summary
[What was built]

### Files Changed
- `path/to/file.rs` - [change description]

### Next Steps
[Any follow-up work identified]
```
