import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function reusableRun(context, request, readReceipt) {
  if (context.event !== 'push' || context.ref !== 'refs/heads/main') return null;
  const root = `/repos/${context.repository}`;
  const prs = await request(`${root}/commits/${context.sha}/pulls`);
  const pr = prs.find(pr => pr.merged_at && pr.base.ref === 'main'
    && pr.base.repo.full_name === context.repository && pr.merge_commit_sha === context.sha);
  if (!pr) return null;
  const { workflow_runs: runs } = await request(
    `${root}/actions/workflows/${encodeURIComponent(context.workflow)}/runs?event=pull_request&status=success&head_sha=${pr.head.sha}&per_page=20`,
  );
  for (const run of runs) {
    if (run.event !== 'pull_request' || run.status !== 'completed' || run.conclusion !== 'success'
      || run.head_sha !== pr.head.sha || run.repository.full_name !== context.repository
      || !run.pull_requests.some(candidate => candidate.number === pr.number)) continue;
    const { artifacts } = await request(`${root}/actions/runs/${run.id}/artifacts?per_page=100`);
    const matches = artifacts.filter(artifact => !artifact.expired && artifact.name === `ci-tree-${run.id}-${run.run_attempt}`);
    if (matches.length !== 1) continue;
    const receipt = await readReceipt(`${root}/actions/artifacts/${matches[0].id}/zip`);
    if (receipt.tree !== context.tree || receipt.run !== run.id || receipt.attempt !== run.run_attempt
      || receipt.workflow !== context.workflow || !/^[0-9a-f]{40}$/.test(receipt.sha)) continue;
    const commit = await request(`${root}/git/commits/${receipt.sha}`);
    // The receipt names the checked-out PR merge tree, not merely the PR head.
    if (commit.tree.sha === context.tree && (commit.sha === pr.head.sha
      || commit.parents.some(parent => parent.sha === pr.head.sha))) return run.id;
  }
  return null;
}

async function main() {
  const env = process.env;
  const context = {
    event: env.GITHUB_EVENT_NAME,
    ref: env.GITHUB_REF,
    repository: env.GITHUB_REPOSITORY,
    sha: env.GITHUB_SHA,
    workflow: env.GITHUB_WORKFLOW_REF.slice(env.GITHUB_REPOSITORY.length + 1).split('@')[0].split('/').pop(),
    tree: execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { encoding: 'utf8' }).trim(),
  };
  if (process.argv[2] === 'record') {
    if (context.event !== 'pull_request') throw new Error('Receipts are only recorded after PR verification');
    const checkedSha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
    if (checkedSha !== context.sha) throw new Error('Checkout does not match the workflow commit');
    const directory = join(env.RUNNER_TEMP, 'ci-tested-tree');
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, 'receipt.json'), JSON.stringify({
      tree: context.tree, sha: checkedSha, workflow: context.workflow,
      run: Number(env.GITHUB_RUN_ID), attempt: Number(env.GITHUB_RUN_ATTEMPT),
    }));
    return;
  }
  if (process.argv[2] !== 'decide') throw new Error('Expected decide or record');
  const request = async (path, binary = false) => {
    const response = await fetch(`${env.GITHUB_API_URL}${path}`, {
      headers: { Authorization: `Bearer ${env.GH_TOKEN}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' },
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw new Error(`GitHub evidence unavailable (HTTP ${response.status})`);
    return binary ? Buffer.from(await response.arrayBuffer()) : response.json();
  };
  const readReceipt = async path => {
    const directory = mkdtempSync(join(env.RUNNER_TEMP, 'ci-receipt-'));
    try {
      const archive = join(directory, 'receipt.zip');
      writeFileSync(archive, await request(path, true));
      return JSON.parse(execFileSync('unzip', ['-p', archive, 'receipt.json'], { encoding: 'utf8', maxBuffer: 4096 }));
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  };
  let run = null;
  try {
    run = await reusableRun(context, request, readReceipt);
  } catch (error) {
    console.log(`Running verification: ${error.message}`);
  }
  appendFileSync(env.GITHUB_OUTPUT, `needed=${run === null}\n`);
  appendFileSync(env.GITHUB_STEP_SUMMARY, run === null
    ? 'Run verification: no successful PR run proves this exact tree.\n'
    : `Reuse verification of the identical tree from [PR run ${run}](${env.GITHUB_SERVER_URL}/${context.repository}/actions/runs/${run}).\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
