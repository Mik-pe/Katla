export const meta = {
  name: 'batch-fix-todo',
  description: 'Fix many TODO items in parallel across different crates',
  phases: [
    { title: 'Fix production readiness', model: 'haiku' },
    { title: 'Verify', model: 'haiku' },
  ],
}

phase('Fix production readiness')

const items = [
  {
    label: 'agent-test-compile',
    prompt: `Fix katla_agent test compilation errors. The tests in katla_agent/src/co_creator/tools.rs and katla_agent/src/co_creator/local_handler.rs reference types that are behind the "llm-assistant" feature flag. Gate these test modules with #[cfg(all(test, feature = "llm-assistant"))] instead of #[cfg(test)].

Steps:
1. Read katla_agent/src/co_creator/tools.rs and find the #[cfg(test)] mod tests block
2. Change #[cfg(test)] to #[cfg(all(test, feature = "llm-assistant"))]
3. Read katla_agent/src/co_creator/local_handler.rs and do the same
4. Run cargo test -p katla_agent --no-run 2>&1 | tail -5 to verify tests compile

Project rules: Do NOT add comments. Do NOT change any production code.`
  },
  {
    label: 'ecs-allow-removal',
    prompt: `Remove remaining #[allow] attributes in katla_ecs. There are #[allow] attributes in:
- katla_ecs/src/spawn.rs (1 occurrence)
- katla_ecs/src/query/macros.rs (6 occurrences)

For each one:
1. Read the file and find the #[allow] attribute
2. If the attribute suppresses a dead_code warning: remove the dead code AND the attribute
3. If it suppresses a different warning: check if the code is actually needed. If yes, fix the underlying issue. If no, remove the code.

After fixing, run: cargo clippy -p katla_ecs --tests -- -D warnings 2>&1 | tail -5

Project rules: NEVER add #[allow(dead_code)] - remove unused code instead. Do NOT add comments.`
  },
  {
    label: 'script-panic-unwrap',
    prompt: `Reduce panic!/unwrap/expect calls in katla_script. Count current calls:
grep -c "panic!" katla_script/src/ --include="*.rs" -r
grep -c "unwrap()" katla_script/src/ --include="*.rs" -r  
grep -c "expect(" katla_script/src/ --include="*.rs" -r

Find 5-10 easy ones to convert to proper Result propagation (preferably in non-test code). Focus on:
- Converting .unwrap() to .ok_or(Error)? or proper ? propagation
- Converting panic!() to return Err(...)
- Do NOT change test code

After fixing, run: cargo clippy -p katla_script -- -D warnings 2>&1 | tail -5

Project rules: Do NOT add #[allow(...)]. Do NOT add comments. Use existing error types.`
  },
  {
    label: 'ecs-panic-unwrap',
    prompt: `Reduce panic!/unwrap/expect calls in katla_ecs. Count current calls first:
grep -c "panic!" katla_ecs/src/ --include="*.rs" -r
grep -c "unwrap()" katla_ecs/src/ --include="*.rs" -r
grep -c "expect(" katla_ecs/src/ --include="*.rs" -r

Find 5-10 easy ones to convert in non-test code. Focus on:
- Converting .unwrap() to proper Option/Result handling
- Converting panic!() to return Err(...)
- Do NOT change test code (files with #[cfg(test)] modules)

After fixing, run: cargo clippy -p katla_ecs -- -D warnings 2>&1 | tail -5

Project rules: Do NOT add #[allow(...)]. Do NOT add comments. Use SceneToolError or other existing error types.`
  },
  {
    label: 'agent-panic-unwrap',
    prompt: `Reduce panic!/unwrap/expect calls in katla_agent. Count current calls first:
grep -c "panic!" katla_agent/src/ --include="*.rs" -r
grep -c "unwrap()" katla_agent/src/ --include="*.rs" -r
grep -c "expect(" katla_agent/src/ --include="*.rs" -r

Find 5-10 easy ones to convert in non-test code. Focus on:
- Converting .unwrap() to .ok_or(Error)? or proper ? propagation
- Converting panic!() to return Err(...)
- Do NOT change test code

After fixing, run: cargo clippy -p katla_agent -- -D warnings 2>&1 | tail -5

Project rules: Do NOT add #[allow(...)]. Do NOT add comments. Use LlmError or other existing error types.`
  },
  {
    label: 'audio-unwrap-reduce',
    prompt: `Reduce unwrap/expect calls in katla_audio. Count current calls first:
grep -c "unwrap()" katla_audio/src/ --include="*.rs" -r
grep -c "expect(" katla_audio/src/ --include="*.rs" -r

Find 5-10 easy ones to convert in non-test code (mixer.rs has most). Focus on:
- Converting .unwrap() on mutex locks to proper handling
- Converting .expect() to proper error types
- Do NOT change test code

After fixing, run: cargo clippy -p katla_audio -- -D warnings 2>&1 | tail -5

Project rules: Do NOT add #[allow(...)]. Do NOT add comments.`
  },
]

const results = await parallel(items.map(item => () =>
  agent(item.prompt, { label: item.label, phase: 'Fix production readiness' })
))

log(`Completed ${results.filter(Boolean).length}/${items.length} fixes`)

// Phase 2: Verify
phase('Verify')

const verifyResult = await agent(
  `Run verification:
1. cargo fmt
2. cargo check 2>&1 | tail -3
3. cargo test --workspace 2>&1 | tail -10
4. echo "=== CLIPPY ===" && cargo clippy -p katla_audio -p katla_ecs -p katla_script -p katla_agent -- -D warnings 2>&1 | tail -5

Report: build status, test pass/fail count, clippy warning count.`,
  { label: 'verify', phase: 'Verify' }
)

return { results, verify: verifyResult }
