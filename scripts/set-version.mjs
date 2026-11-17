// Sets one version across every manifest in the repository (ADR-0004): the
// root package.json, every packages/*/package.json, [workspace.package] in
// Cargo.toml, and the workspace members' entries in Cargo.lock. The release
// step runs it through @semantic-release/exec before the release commit.
//
// Usage: node scripts/set-version.mjs <version>
//
// The release job installs no Rust toolchain, so Cargo.lock is edited as text.
// Workspace members are the only lockfile entries without a `source` line, and
// they carry no checksum, so changing their `version` is exactly what
// `cargo update --workspace` would write.
//
// Every file is read and every edit is computed before anything is written, so
// an invalid version or an unexpected file layout fails without changing any
// file. Values are replaced in place, which keeps each file's formatting and
// trailing newline.
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { isDeepStrictEqual } from 'node:util';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

// The semver 2.0.0 grammar, from https://semver.org/#is-there-a-suggested-regular-expression-regex-to-check-a-semver-string
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;

const repoRoot = fileURLToPath(new URL('..', import.meta.url));

class SetVersionError extends Error {}

function fail(message) {
  throw new SetVersionError(message);
}

// Returns the [start, end) offsets of the string value of the top-level
// "version" key in a JSON document, skipping nested objects and the contents
// of strings, or null when the document has no top-level "version".
function topLevelVersionSpan(text) {
  let depth = 0;
  let i = 0;
  let expectVersionValue = false;
  while (i < text.length) {
    const char = text[i];
    if (char === '"') {
      const start = i;
      i += 1;
      while (text[i] !== '"') {
        if (text[i] === '\\') i += 1;
        i += 1;
      }
      i += 1;
      const token = text.slice(start, i);
      if (expectVersionValue) return [start + 1, i - 1];
      if (depth === 1 && token === '"version"' && /^\s*:\s*"/.test(text.slice(i))) {
        expectVersionValue = true;
      }
      continue;
    }
    if (char === '{' || char === '[') depth += 1;
    if (char === '}' || char === ']') depth -= 1;
    i += 1;
  }
  return null;
}

function planPackageJson(path, version) {
  const text = readFileSync(path, 'utf8');
  const original = JSON.parse(text);
  const span = topLevelVersionSpan(text);
  if (span === null) fail(`${relative(repoRoot, path)} has no top-level "version" string.`);
  const next = text.slice(0, span[0]) + version + text.slice(span[1]);
  // The edit must change exactly the version and nothing else.
  if (!isDeepStrictEqual(JSON.parse(next), { ...original, version })) {
    fail(`${relative(repoRoot, path)}: replacing the version would change more than the version.`);
  }
  return next;
}

// The lines of a TOML table, from its header to the next header.
function tableRange(lines, header) {
  const start = lines.findIndex((line) => line.trim() === header);
  if (start === -1) return null;
  let end = start + 1;
  while (end < lines.length && !lines[end].trimStart().startsWith('[')) end += 1;
  return [start + 1, end];
}

function workspaceMemberPaths(cargoToml) {
  const lines = cargoToml.split('\n');
  const range = tableRange(lines, '[workspace]');
  if (range === null) fail('Cargo.toml has no [workspace] table.');
  const body = lines.slice(...range).join('\n');
  const members = /^members\s*=\s*\[([^\]]*)\]/m.exec(body);
  if (members === null) fail('Cargo.toml [workspace] has no members list.');
  const paths = [...members[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  if (paths.length === 0) fail('Cargo.toml [workspace] members list is empty.');
  for (const path of paths) {
    if (/[*?[]/.test(path)) fail(`Cargo.toml member "${path}" is a glob; list each member path explicitly.`);
  }
  return paths;
}

// A member's package name, after checking that it takes its version from the
// workspace rather than declaring its own.
function memberName(memberPath) {
  const manifest = join(repoRoot, memberPath, 'Cargo.toml');
  if (!existsSync(manifest)) fail(`workspace member ${memberPath} has no Cargo.toml.`);
  const lines = readFileSync(manifest, 'utf8').split('\n');
  const range = tableRange(lines, '[package]');
  if (range === null) fail(`${memberPath}/Cargo.toml has no [package] table.`);
  const body = lines.slice(...range);
  const name = body.map((line) => /^name\s*=\s*"([^"]+)"/.exec(line)).find(Boolean);
  if (!name) fail(`${memberPath}/Cargo.toml has no package name.`);
  if (!body.some((line) => /^version\.workspace\s*=\s*true\b/.test(line))) {
    fail(`${memberPath}/Cargo.toml must take its version from the workspace (version.workspace = true).`);
  }
  return name[1];
}

function planCargoToml(text, version) {
  const lines = text.split('\n');
  const range = tableRange(lines, '[workspace.package]');
  if (range === null) fail('Cargo.toml has no [workspace.package] table.');
  const pattern = /^(version\s*=\s*")[^"]*(".*)$/;
  const index = lines.findIndex((line, i) => i >= range[0] && i < range[1] && pattern.test(line));
  if (index === -1) fail('Cargo.toml [workspace.package] has no version.');
  lines[index] = lines[index].replace(pattern, `$1${version}$2`);
  return lines.join('\n');
}

function planCargoLock(text, version, names) {
  const blocks = text.split(/(?=^\[\[package\]\]$)/m);
  const found = new Set();
  const next = blocks.map((block) => {
    if (!block.startsWith('[[package]]')) return block;
    const name = /^name = "([^"]+)"$/m.exec(block)?.[1];
    if (!names.has(name) || /^source = /m.test(block)) return block;
    if (found.has(name)) fail(`Cargo.lock has more than one workspace entry for ${name}.`);
    found.add(name);
    if (!/^version = "[^"]*"$/m.test(block)) fail(`Cargo.lock entry for ${name} has no version.`);
    return block.replace(/^version = "[^"]*"$/m, `version = "${version}"`);
  });
  const missing = [...names].filter((name) => !found.has(name));
  if (missing.length > 0) fail(`Cargo.lock has no workspace entry for ${missing.join(', ')}.`);
  return next.join('');
}

function packageManifests() {
  const dir = join(repoRoot, 'packages');
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(dir, entry.name, 'package.json'))
    .filter((path) => existsSync(path))
    .sort();
}

// Every file and its new content. Reads only; writes nothing.
function plan(version) {
  const edits = new Map();
  for (const path of [join(repoRoot, 'package.json'), ...packageManifests()]) {
    edits.set(path, planPackageJson(path, version));
  }

  const cargoTomlPath = join(repoRoot, 'Cargo.toml');
  const cargoToml = readFileSync(cargoTomlPath, 'utf8');
  const names = new Set(workspaceMemberPaths(cargoToml).map(memberName));
  edits.set(cargoTomlPath, planCargoToml(cargoToml, version));

  const cargoLockPath = join(repoRoot, 'Cargo.lock');
  edits.set(cargoLockPath, planCargoLock(readFileSync(cargoLockPath, 'utf8'), version, names));
  return edits;
}

function main(args) {
  if (args.length !== 1) fail('usage: node scripts/set-version.mjs <version>');
  const [version] = args;
  if (!SEMVER.test(version)) fail(`"${version}" is not a valid semver version (for example 1.2.3).`);

  const edits = plan(version);
  let changed = 0;
  for (const [path, content] of edits) {
    if (readFileSync(path, 'utf8') === content) continue;
    writeFileSync(path, content);
    changed += 1;
    console.log(`set-version: ${relative(repoRoot, path)}`);
  }
  if (changed === 0) console.log(`set-version: every manifest is already at ${version}.`);
}

try {
  main(process.argv.slice(2));
} catch (error) {
  if (!(error instanceof SetVersionError)) throw error;
  console.error(`set-version: ${error.message} No file was changed.`);
  process.exit(1);
}
