# Team - Debate to Delivery Pipeline

Run the full debate-to-delivery pipeline with supervisor orchestration.

---

## Usage

```
/team [TOPIC or TASK DESCRIPTION]
```

## What This Does

1. **Debate Phase** - Vulkan vs App perspectives reach consensus
2. **Planning Phase** - Prometheus creates implementation plan
3. **Build Phase** - Hephaestus implements in chunks
4. **Review Phase** - Code reviewer validates quality

---

## Invocation

You are the **Supervisor**. Read your full persona:

```
Read .claude/agents/supervisor.md
```

## Setup (Do This First)

1. **Check for existing team** - If TeamCreate fails with "already leading team", run TeamDelete first
2. **Create team** - Use TeamCreate with a descriptive team_name
3. **Spawn teammates** - Use Agent tool with ALL required parameters

**CRITICAL**: When spawning agents, you MUST include ALL of these:
- `subagent_type`: The agent type (e.g., "gfx-maintainer")
- `team_name`: The team name from TeamCreate (e.g., "debate-team")
- `name`: A human-readable name for messaging (e.g., "gfx-maintainer")
- `description`: A 3-5 word summary of what the agent does (e.g., "GFX layer material review")
- `model`: The model to use (e.g., "opus")
- `prompt`: The task for the agent

Example Agent call:
```
Agent(
  subagent_type: "gfx-maintainer",
  team_name: "debate-team",
  name: "gfx-maintainer",
  description: "GFX layer material review",
  model: "opus",
  prompt: "Your task here"
)
```

| Agent | subagent_type | model |
|-------|---------------|-------|
| GFX Maintainer | `gfx-maintainer` | `opus` |
| App Maintainer | `app-maintainer` | `opus` |
| Prometheus | `prometheus` | `opus` |
| Hephaestus | `hephaestus` | `opus` |
| Code Reviewer | `code-reviewer` | `opus` |

**TASK**: $ARGUMENTS

---

## Phase 1: Debate (Unbiased)

**IMPORTANT**: As Supervisor, do NOT pre-explore the codebase. Let the debaters investigate and form their own conclusions. This ensures unbiased moderation.

### Spawn Debaters

Spawn both debaters with the topic. Each should:
1. Explore the relevant code themselves
2. Form their own position
3. Debate with each other via SendMessage

**IMPORTANT**: Debaters must use the SendMessage tool to communicate with each other, not just output text.

Example message flow:
```
Supervisor → gfx-maintainer: "TOPIC: [topic], debate with app-maintainer"
Supervisor → app-maintainer: "TOPIC: [topic], debate with gfx-maintainer"

gfx-maintainer → app-maintainer: Opening position
app-maintainer → gfx-maintainer: Counter-position

gfx-maintainer → app-maintainer: Response to counter
app-maintainer → gfx-maintainer: Concession/agreement

gfx-maintainer → Supervisor: Consensus reached
app-maintainer → Supervisor: Consensus reached
```

### Moderation Loop

After receiving consensus or after 3 rounds:
1. Identify points of agreement
2. Identify core disagreement
3. Make decision and move forward if deadlocked

### Phase 2: Consult Prometheus

DECISION: [consensus from debate]
If no actionable plan emerges from debate, skip this and following phases.

Spawn Prometheus to gather context and output a step-by-step implementation plan with file paths.

### Phase 3: Invoke Hephaestus

Implement the following plan:
[plan from Prometheus]

Spawn Hephaestus to work systematically. Run cargo check and cargo fmt after each major change.
Commit atomic changes. Do not skip tests.

### Phase 4: Invoke Reviewer

TASK: [original task]
IMPLEMENTED: [what was built]

Spawn Code Reviewer to focus on:
- Correctness and edge cases
- Performance (allocation in hot paths, cache efficiency)
- Architecture constraint adherence
- Code quality and maintainability

Run: cargo clippy, cargo test, cargo fmt --check

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

### Review Outcome
[Any issues found and resolved]

### Files Changed
- `path/to/file.rs` - [change description]

### Next Steps
[Any follow-up work identified]
```
