import assert from 'node:assert/strict';
import { test } from 'node:test';
import { reusableRun } from './ci-reuse.mjs';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const head = 'a'.repeat(40);
const merge = 'b'.repeat(40);
const tested = 'c'.repeat(40);
const tree = 'd'.repeat(40);
const context = { event: 'push', ref: 'refs/heads/main', repository: 'owner/repo', sha: merge, workflow: 'ci.yml', tree };

function fixture() {
  const pr = { number: 4, merged_at: '2026-10-03', merge_commit_sha: merge, head: { sha: head }, base: { ref: 'main', repo: { full_name: 'owner/repo' } } };
  const run = { id: 7, run_attempt: 2, status: 'completed', event: 'pull_request', conclusion: 'success', head_sha: head, repository: { full_name: 'owner/repo' }, pull_requests: [{ number: 4 }] };
  const artifact = { id: 9, expired: false, name: 'ci-tree-7-2' };
  const receipt = { run: 7, attempt: 2, sha: tested, tree, workflow: 'ci.yml' };
  const commit = { sha: tested, tree: { sha: tree }, parents: [{ sha: head }] };
  const calls = [];
  const request = async path => {
    calls.push(path);
    if (path.endsWith('/pulls')) return [pr];
    if (path.includes('/actions/workflows/')) return { workflow_runs: [run] };
    if (path.includes('/artifacts?')) return { artifacts: [artifact] };
    if (path.includes('/git/commits/')) return commit;
    throw new Error(`Unexpected request: ${path}`);
  };
  return { pr, run, artifact, receipt, commit, calls, request, readReceipt: async () => receipt };
}

test('reuses only the successful workflow attempt that tested this merge tree', async () => {
  const f = fixture();
  assert.equal(await reusableRun(context, f.request, f.readReceipt), 7);
  assert.ok(f.calls.some(path => path.includes('ci.yml/runs?event=pull_request&status=success&head_sha=')));
});

test('PRs, manual runs, schedules and tags always execute verification', async () => {
  for (const changed of [{ event: 'pull_request' }, { event: 'workflow_dispatch' }, { event: 'schedule' }, { ref: 'refs/tags/v1' }]) {
    assert.equal(await reusableRun({ ...context, ...changed }, () => { throw new Error('Must not request evidence'); }), null);
  }
});

test('direct pushes and cross-repository PRs do not inherit PR verification', async () => {
  for (const changed of [{ merged_at: null }, { merge_commit_sha: head }, { base: { ref: 'main', repo: { full_name: 'other/repo' } } }]) {
    const f = fixture();
    Object.assign(f.pr, changed);
    assert.equal(await reusableRun(context, f.request, f.readReceipt), null);
  }
});

test('new merged content reruns checks even when the PR head is unchanged', async () => {
  const f = fixture();
  assert.equal(await reusableRun({ ...context, tree: 'e'.repeat(40) }, f.request, f.readReceipt), null);
});

test('failed, incomplete, unrelated and non-PR runs cannot be reused', async () => {
  for (const changed of [{ conclusion: 'failure' }, { status: 'in_progress' }, { event: 'push' }, { head_sha: merge }, { pull_requests: [] }, { repository: { full_name: 'other/repo' } }]) {
    const f = fixture();
    Object.assign(f.run, changed);
    assert.equal(await reusableRun(context, f.request, f.readReceipt), null);
  }
});

test('expired artifacts and artifacts from an earlier attempt rerun checks', async () => {
  for (const changed of [{ expired: true }, { name: 'ci-tree-7-1' }]) {
    const f = fixture();
    Object.assign(f.artifact, changed);
    assert.equal(await reusableRun(context, f.request, f.readReceipt), null);
  }
});

test('receipts must match the workflow, attempt, run and checked tree', async () => {
  for (const changed of [{ workflow: 'other.yml' }, { attempt: 1 }, { run: 8 }, { sha: 'invalid' }]) {
    const f = fixture();
    Object.assign(f.receipt, changed);
    assert.equal(await reusableRun(context, f.request, f.readReceipt), null);
  }
});

test('the checked commit must prove the receipt tree and contain the PR head', async () => {
  for (const changed of [{ tree: { sha: 'e'.repeat(40) } }, { parents: [] }]) {
    const f = fixture();
    Object.assign(f.commit, changed);
    assert.equal(await reusableRun(context, f.request, f.readReceipt), null);
  }
});

test('unavailable evidence is reported to the caller for a full verification fallback', async () => {
  await assert.rejects(reusableRun(context, async () => { throw new Error('HTTP 403'); }), /HTTP 403/);
});

test('CLI records the real checkout and defaults to full verification when GitHub is unavailable', () => {
  const directory = mkdtempSync(join(tmpdir(), 'ci-reuse-test-'));
  try {
    const git = (...args) => execFileSync('git', args, { cwd: directory, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
    git('init');
    git('-c', 'user.name=CI', '-c', 'user.email=ci@example.invalid', 'commit', '--allow-empty', '-m', 'fixture');
    const sha = git('rev-parse', 'HEAD');
    const env = { ...process.env, GITHUB_REPOSITORY: 'owner/repo', GITHUB_SHA: sha,
      GITHUB_REF: 'refs/heads/main', GITHUB_WORKFLOW_REF: 'owner/repo/.github/workflows/ci.yml@refs/heads/main',
      GITHUB_EVENT_NAME: 'pull_request', GITHUB_RUN_ID: '7', GITHUB_RUN_ATTEMPT: '2', RUNNER_TEMP: directory,
      GITHUB_OUTPUT: join(directory, 'outputs'), GITHUB_STEP_SUMMARY: join(directory, 'summary'),
      GITHUB_API_URL: 'http://127.0.0.1:1', GH_TOKEN: 'fixture' };
    const run = (mode, changed = {}) => spawnSync(process.execPath, [fileURLToPath(new URL('./ci-reuse.mjs', import.meta.url)), mode], { cwd: directory, env: { ...env, ...changed }, encoding: 'utf8' });
    assert.equal(run('record').status, 0);
    const receipt = JSON.parse(readFileSync(join(directory, 'ci-tested-tree/receipt.json'), 'utf8'));
    assert.equal(receipt.tree, git('rev-parse', 'HEAD^{tree}'));
    assert.equal(receipt.sha, sha);
    assert.equal(run('record', { GITHUB_SHA: merge }).status, 1);
    const decision = run('decide', { GITHUB_EVENT_NAME: 'push' });
    assert.equal(decision.status, 0);
    assert.match(readFileSync(env.GITHUB_OUTPUT, 'utf8'), /needed=true/);
    assert.match(readFileSync(env.GITHUB_STEP_SUMMARY, 'utf8'), /no successful PR run/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
