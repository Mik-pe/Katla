# Debate and Implement

Run a debate to reach consensus, then implement the result.

## Workflow

1. **Debate Phase** - Use the debate-moderator skill to orchestrate a debate between Vulkan and App perspectives
2. **Planning Phase** - Spawn prometheus agent to create an implementation plan based on consensus
3. **Implementation Phase** - Spawn hephaestus agent to implement the plan in small chunks

## Usage

When you need to make an architectural decision:
1. First run `/debate-moderator` to get consensus on the approach
2. Then spawn the prometheus agent using Task tool for planning
3. Feed the plan to hephaestus agent for implementation

## Example

```
/debate-moderator
[debate happens, consensus reached]

Task tool -> prometheus: "Create implementation plan for [consensus decision]"
Task tool -> hephaestus: "Implement the plan from prometheus"
```
