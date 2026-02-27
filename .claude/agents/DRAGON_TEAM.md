# Dragon Team 🐉

The Dragon Team is a specialized multi-agent system for the Katla project. Each agent has a specific role, clear directives, and defined outputs.

## Team Roster

### Leadership & Strategy

| Agent | Role | Model | When to Invoke |
|-------|------|-------|----------------|
| **coordinator** | Routes tasks to specialists | sonnet | Complex multi-step tasks |
| **oracle** | Architectural decisions | opus | Design guidance, big picture |
| **planner** | Implementation planning | opus | "How do I build X?" |

### Execution

| Agent | Role | Model | When to Invoke |
|-------|------|-------|----------------|
| **code-monkey** | Implements features | sonnet | "Write code for X" |
| **firefighter** | Fixes broken things | sonnet | Build fails, bugs, crashes |

### Investigation

| Agent | Role | Model | When to Invoke |
|-------|------|-------|----------------|
| **explorer** | Finds code fast | haiku | "Where is X?" |
| **researcher** | External knowledge | sonnet | "How does library X work?" |

### Quality & Verification

| Agent | Role | Model | When to Invoke |
|-------|------|-------|----------------|
| **sentinel** | Quality gatekeeper | sonnet | "Is this code good?" |
| **critic** | Finds flaws | opus | "Review this thoroughly" |
| **test-runner** | Runs tests | haiku | "Run the tests" |

### Knowledge Management

| Agent | Role | Model | When to Invoke |
|-------|------|-------|----------------|
| **librarian** | Documentation | sonnet | "Update docs" |

## Typical Workflows

### Feature Implementation
```
planner → code-monkey → test-runner → sentinel
```

### Bug Fix
```
explorer → firefighter → test-runner → sentinel
```

### Architecture Decision
```
oracle → planner → (review with critic) → code-monkey
```

### Code Review
```
critic → sentinel → (fixes: code-monkey)
```

### Research & Implement
```
researcher → planner → code-monkey → test-runner
```

## Design Principles

### 1. Clear Role Boundaries
Each agent has ONE primary responsibility. They don't overlap.

### 2. Defined Inputs and Outputs
Every agent delivers structured outputs. No guessing what you'll get.

### 3. Appropriate Model Selection
- **opus**: Deep reasoning (oracle, planner, critic)
- **sonnet**: Balanced performance (code-monkey, firefighter, sentinel)
- **haiku**: Speed over depth (explorer, test-runner)

### 4. Minimal Tool Access
Agents only get tools they need. This prevents scope creep.

### 5. Project Memory
Agents share project context via memory system.

## Invoking Agents

Use the Task tool with the agent name:

```
Task:
  subagent_type: code-monkey
  prompt: "Implement feature X following pattern Y"
```

Or invoke via coordinator for complex tasks:

```
Task:
  subagent_type: coordinator
  prompt: "I need to implement feature X, coordinate the team"
```

## Katla-Specific Knowledge

All agents know:
- Dependency boundaries (katla_vulkan → nothing)
- ash::vk types must stay internal
- ECS patterns and conventions
- Render graph architecture
- Vulkan best practices
