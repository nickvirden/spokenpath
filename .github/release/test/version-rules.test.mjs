// Drives the pinned semantic-release through its JS API against throwaway git
// repositories, so the version each Conventional Commit type produces is proven
// before a real release on main depends on it. The analyzer and notes plugins
// come from the repository's .releaserc.json; the publishing plugins are left
// out, so the test needs no token and never reaches GitHub.
//
// semantic-release hooks process.stdout and process.stderr while it runs, which
// corrupts the binary channel between node:test's runner and an isolated test
// process. The test:release script therefore runs this file with
// --test-isolation=none.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Writable } from 'node:stream';
import { after, describe, test } from 'node:test';
import assert from 'node:assert/strict';
import semanticRelease from 'semantic-release';

const repoRoot = new URL('../../../', import.meta.url);
const releaseConfig = JSON.parse(readFileSync(new URL('.releaserc.json', repoRoot), 'utf8'));

const VERSION_PLUGINS = new Set([
  '@semantic-release/commit-analyzer',
  '@semantic-release/release-notes-generator',
]);
const pluginName = (entry) => (Array.isArray(entry) ? entry[0] : entry);
const versionPlugins = releaseConfig.plugins.filter((entry) => VERSION_PLUGINS.has(pluginName(entry)));

const BRANCH = 'main';

// Only these variables reach git and semantic-release. Leaving out the CI
// runner's GITHUB_* variables keeps env-ci from reporting the pull request's
// branch instead of the throwaway repository's, and pointing git at an empty
// global config keeps a developer's signing or hook settings out of the run.
const gitEnv = {
  PATH: process.env.PATH,
  HOME: process.env.HOME,
  TMPDIR: process.env.TMPDIR ?? tmpdir(),
  LANG: 'C',
  GIT_CONFIG_GLOBAL: '/dev/null',
  GIT_CONFIG_NOSYSTEM: '1',
  GIT_AUTHOR_NAME: 'Version Rules Test',
  GIT_AUTHOR_EMAIL: 'version-rules@example.invalid',
  GIT_COMMITTER_NAME: 'Version Rules Test',
  GIT_COMMITTER_EMAIL: 'version-rules@example.invalid',
};

const workDirs = [];
after(() => {
  for (const dir of workDirs) rmSync(dir, { recursive: true, force: true });
});

function git(cwd, ...args) {
  return execFileSync('git', args, { cwd, env: gitEnv, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
}

// A repository whose only release is the seed tag v0.0.0, with a local bare
// repository as origin, then one commit carrying `message`.
function repoWithCommit(message) {
  const root = mkdtempSync(join(gitEnv.TMPDIR, 'version-rules-'));
  workDirs.push(root);
  const origin = join(root, 'origin.git');
  const work = join(root, 'work');

  execFileSync('git', ['init', '--quiet', '--bare', `--initial-branch=${BRANCH}`, origin], { env: gitEnv });
  execFileSync('git', ['init', '--quiet', `--initial-branch=${BRANCH}`, work], { env: gitEnv });
  git(work, 'commit', '--quiet', '--allow-empty', '-m', 'Initial commit');
  git(work, 'tag', '-a', 'v0.0.0', '-m', 'v0.0.0');
  git(work, 'commit', '--quiet', '--allow-empty', '-m', message);
  git(work, 'remote', 'add', 'origin', origin);
  git(work, 'push', '--quiet', '--follow-tags', 'origin', BRANCH);

  return { work, origin };
}

function collector() {
  let text = '';
  const stream = new Writable({
    write(chunk, _encoding, callback) {
      text += chunk;
      callback();
    },
  });
  return { stream, text: () => text };
}

async function nextVersion(message) {
  const { work, origin } = repoWithCommit(message);
  const out = collector();
  const result = await semanticRelease(
    {
      branches: [BRANCH],
      // release-notes-generator parses the repository URL, so a bare path is not enough.
      repositoryUrl: pathToFileURL(origin).href,
      dryRun: true,
      ci: false,
      plugins: versionPlugins,
    },
    { cwd: work, env: { ...gitEnv }, stdout: out.stream, stderr: out.stream },
  );
  return { version: result ? result.nextRelease.version : null, log: out.text() };
}

describe('version rules from .releaserc.json', () => {
  test('reads both version plugins from the repository config', () => {
    assert.deepEqual(versionPlugins.map(pluginName), [...VERSION_PLUGINS]);
  });

  const releasing = [
    ['feat: add a feature', '0.1.0'],
    ['fix: correct a bug', '0.0.1'],
    ['feat!: change an interface', '0.1.0'],
    ['fix: correct a bug\n\nBREAKING CHANGE: the old behaviour is gone', '0.1.0'],
  ];
  for (const [message, expected] of releasing) {
    test(`"${message.split('\n')[0]}"${message.includes('BREAKING CHANGE') ? ' with a BREAKING CHANGE footer' : ''} releases ${expected}`, async () => {
      const { version, log } = await nextVersion(message);
      assert.equal(version, expected, log);
    });
  }

  for (const type of ['chore', 'docs', 'ci', 'build', 'test', 'refactor']) {
    test(`"${type}:" cuts no release`, async () => {
      const { version, log } = await nextVersion(`${type}: change something`);
      assert.equal(version, null, log);
      assert.match(log, /There are no relevant changes, so no new version is released/);
    });
  }
});
