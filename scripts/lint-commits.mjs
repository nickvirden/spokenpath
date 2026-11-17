// Lints every commit between origin/main and HEAD with commitlint.
//
// commitlint refuses an empty range ("--from and --to point to the same
// commit"), which is the state of main itself and of any branch with no commits
// of its own, so an empty range passes here instead. Anything else that stops
// the range from being counted (origin/main missing, git failing) fails the
// check: the commit-message gate is never silently skipped.
import { spawnSync } from 'node:child_process';

const BASE = 'origin/main';

const count = spawnSync('git', ['rev-list', '--count', `${BASE}..HEAD`], { encoding: 'utf8' });
if (count.error) {
  console.error(`lint:commits: could not run git: ${count.error.message}`);
  process.exit(1);
}
if (count.status !== 0) {
  console.error(`lint:commits: could not count commits in ${BASE}..HEAD (run 'git fetch origin' first).`);
  console.error(count.stderr.trim());
  process.exit(1);
}

if (count.stdout.trim() === '0') {
  console.log(`lint:commits: no commits in ${BASE}..HEAD, nothing to lint.`);
  process.exit(0);
}

// pnpm puts node_modules/.bin on PATH for package scripts.
const lint = spawnSync('commitlint', ['--from', BASE, '--to', 'HEAD', '--verbose'], { stdio: 'inherit' });
if (lint.error) {
  console.error(`lint:commits: could not run commitlint: ${lint.error.message}`);
  process.exit(1);
}
process.exit(lint.status ?? 1);
