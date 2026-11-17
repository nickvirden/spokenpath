// Proves that every manifest in the repository carries one version
// (ADR-0004), and that scripts/set-version.mjs, which the release step runs,
// moves all of them together in a form cargo still accepts with --locked.
//
// The script runs only against scratch copies of the tracked files under
// TMPDIR, never against the checkout itself. Versions are read independently
// of the script: workspace members come from `cargo metadata`, not from the
// script's own parsing of Cargo.toml.
import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, matchesGlob } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, describe, test } from 'node:test';
import assert from 'node:assert/strict';

const repoRoot = fileURLToPath(new URL('../../', import.meta.url));
const SAMPLE = '0.42.0';

const workDirs = [];
after(() => {
  for (const dir of workDirs) rmSync(dir, { recursive: true, force: true });
});

function cargoMetadata(root, ...args) {
  return JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', ...args], {
      cwd: root,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
}

// Every version the release must keep equal, as [file label, version] pairs.
function manifestVersions(root) {
  const versions = [];
  const readJson = (path) => JSON.parse(readFileSync(join(root, path), 'utf8'));
  versions.push(['package.json', readJson('package.json').version]);
  for (const entry of readdirSync(join(root, 'packages'), { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const path = `packages/${entry.name}/package.json`;
    versions.push([path, readJson(path).version]);
  }

  const cargoToml = readFileSync(join(root, 'Cargo.toml'), 'utf8');
  const workspacePackage = /^\[workspace\.package\]$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(cargoToml);
  assert.ok(workspacePackage, 'Cargo.toml has a [workspace.package] table');
  versions.push([
    'Cargo.toml [workspace.package]',
    /^version\s*=\s*"([^"]*)"/m.exec(workspacePackage[1])?.[1],
  ]);

  const metadata = cargoMetadata(root, '--no-deps');
  const members = new Set(
    metadata.workspace_members.map((id) => metadata.packages.find((p) => p.id === id).name),
  );
  const lockEntries = readFileSync(join(root, 'Cargo.lock'), 'utf8')
    .split(/^\[\[package\]\]$/m)
    .slice(1)
    .filter((block) => !/^source = /m.test(block))
    .map((block) => [/^name = "([^"]+)"$/m.exec(block)?.[1], /^version = "([^"]+)"$/m.exec(block)?.[1]])
    .filter(([name]) => members.has(name));
  assert.deepEqual(
    lockEntries.map(([name]) => name).sort(),
    [...members].sort(),
    'Cargo.lock has one entry per workspace member',
  );
  for (const [name, version] of lockEntries) versions.push([`Cargo.lock ${name}`, version]);
  return versions;
}

function assertAllAt(versions, expected) {
  const disagreeing = versions.filter(([, version]) => version !== expected);
  assert.deepEqual(
    disagreeing,
    [],
    `every manifest should be at ${expected}; these disagree: ${disagreeing.map(([file, version]) => `${file} (${version})`).join(', ')}`,
  );
}

function trackedFiles() {
  return execFileSync('git', ['ls-files', '-z'], { cwd: repoRoot, encoding: 'utf8' })
    .split('\0')
    .filter(Boolean);
}

// A scratch copy of the tracked files, so the script runs against exactly what
// the release job checks out.
function scratchCopy() {
  const copy = mkdtempSync(join(process.env.TMPDIR ?? tmpdir(), 'set-version-'));
  workDirs.push(copy);
  const files = trackedFiles();
  for (const file of files) {
    mkdirSync(dirname(join(copy, file)), { recursive: true });
    cpSync(join(repoRoot, file), join(copy, file));
  }
  return { copy, files };
}

function contents(root, files) {
  return new Map(files.map((file) => [file, readFileSync(join(root, file))]));
}

function setVersion(root, version) {
  return spawnSync(process.execPath, ['scripts/set-version.mjs', version], { cwd: root, encoding: 'utf8' });
}

// The files a run reports writing, one `set-version: <path>` line each.
function writtenFiles(stdout) {
  return stdout
    .split('\n')
    .map((line) => /^set-version: (\S+)$/.exec(line)?.[1])
    .filter(Boolean);
}

describe('lockstep versions', () => {
  test('all manifests agree on one version in the checkout', () => {
    const versions = manifestVersions(repoRoot);
    assertAllAt(versions, versions[0][1]);
  });

  test(`set-version ${SAMPLE} moves every manifest, and cargo accepts the lockfile`, () => {
    const { copy, files } = scratchCopy();
    const first = setVersion(copy, SAMPLE);
    assert.equal(first.status, 0, first.stderr);
    assertAllAt(manifestVersions(copy), SAMPLE);

    const metadata = cargoMetadata(copy);
    for (const id of metadata.workspace_members) {
      const member = metadata.packages.find((p) => p.id === id);
      assert.equal(member.version, SAMPLE, `cargo metadata reports ${member.name} at ${member.version}`);
    }

    const afterFirst = contents(copy, files);
    const second = setVersion(copy, SAMPLE);
    assert.equal(second.status, 0, second.stderr);
    assert.deepEqual(writtenFiles(second.stdout), [], 'a second run reports no written file');
    assert.deepEqual(contents(copy, files), afterFirst, 'a second run changes no file');
  });

  for (const invalid of ['banana', 'v1.2.3', '1.2', '01.2.3', '']) {
    test(`set-version rejects ${JSON.stringify(invalid)} and changes no file`, () => {
      const { copy, files } = scratchCopy();
      const before = contents(copy, files);
      const result = setVersion(copy, invalid);
      assert.notEqual(result.status, 0, result.stdout);
      assert.match(result.stderr, /not a valid semver version/);
      assert.deepEqual(contents(copy, files), before);
    });
  }

  test('every file set-version writes is a release commit asset', () => {
    const { copy } = scratchCopy();
    const result = setVersion(copy, SAMPLE);
    assert.equal(result.status, 0, result.stderr);
    const written = writtenFiles(result.stdout);
    assert.ok(written.length > 0, result.stdout);

    const releaseConfig = JSON.parse(readFileSync(join(repoRoot, '.releaserc.json'), 'utf8'));
    const gitPlugin = releaseConfig.plugins.find(
      (entry) => Array.isArray(entry) && entry[0] === '@semantic-release/git',
    );
    assert.ok(gitPlugin, '.releaserc.json configures @semantic-release/git');
    const { assets } = gitPlugin[1];
    for (const file of written) {
      assert.ok(
        assets.some((asset) => matchesGlob(file, asset)),
        `${file} is written but matches no git plugin asset (${assets.join(', ')})`,
      );
    }
  });
});
