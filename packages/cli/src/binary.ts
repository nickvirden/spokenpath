import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** The repository root, found from this file's location in `packages/cli/src/`. */
export const repositoryRoot: string = fileURLToPath(new URL('../../../', import.meta.url));

/** The command that builds the binary, which the missing-binary message names. */
export const BUILD_COMMAND = 'cargo build -p spokenpath_cli';

/**
 * Where the `spokenpath-core` debug binary is: `target/debug/` under the repository root, or
 * `debug/` under `CARGO_TARGET_DIR` when it is set, resolved against the repository root when
 * relative, as cargo resolves it for a command run there.
 */
export function binaryPath(env: NodeJS.ProcessEnv = process.env): string {
  const targetDirectory = env.CARGO_TARGET_DIR;
  const target =
    targetDirectory === undefined || targetDirectory === ''
      ? join(repositoryRoot, 'target')
      : isAbsolute(targetDirectory)
        ? targetDirectory
        : resolve(repositoryRoot, targetDirectory);
  const fileName = process.platform === 'win32' ? 'spokenpath-core.exe' : 'spokenpath-core';
  return join(target, 'debug', fileName);
}
