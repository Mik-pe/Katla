---
name: debate-and-implement
description: >-
  Full debate-to-delivery pipeline. Orchestrates debate between perspectives, 
  creates implementation plans, and executes the changes. Use for 
  architectural decisions requiring consensus.
model: inherit
---

# Debate and Implement

Run a debate to reach consensus, then implement the result.

## Workflow

1. **Debate Phase** - Use the debate-moderator skill to orchestrate a debate between GFX (cross-backend) and App perspectives
2. **Planning Phase** - Spawn prometheus agent to create an implementation plan based on consensus
3. **Implementation Phase** - Spawn hephaestus agent to implement the plan in small chunks

## Usage

When you need to make an architectural decision:
1. First run `/debate-moderator` to get consensus on the approach
2. Then spawn the prometheus agent for planning
3. Feed the plan to hephaestus agent for implementation

## Spawning Agents

**CRITICAL**: Use `subagent_type: "general-purpose"` and point to project agent files:

```
Agent(
  subagent_type: "general-purpose",
  team_name: "debate-team",
  name: "prometheus",
  description: "Strategic planning",
  model: "opus",
  prompt: "Read .Codex/agents/prometheus.md and adopt that persona. Create an implementation plan for: [consensus decision]"
)

Agent(
  subagent_type: "general-purpose",
  team_name: "debate-team",
  name: "hephaestus",
  description: "Implementation executor",
  model: "opus",
  prompt: "Read .Codex/agents/hephaestus.md and adopt that persona. Implement the plan: [plan details]"
)
```

## Example

```
/debate-moderator
[debate happens, consensus reached]

Spawn prometheus: "Read .Codex/agents/prometheus.md and create an implementation plan for [consensus decision]"
Spawn hephaestus: "Read .Codex/agents/hephaestus.md and implement the plan from prometheus"
```
