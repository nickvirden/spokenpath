import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** The repository root, found from this file's location in `packages/capture-safari/src/`. */
export const repositoryRoot: string = fileURLToPath(new URL('../../../', import.meta.url));

/**
 * Where the cargo debug binary `name` is: `target/debug/` under the repository root, or `debug/`
 * under `CARGO_TARGET_DIR` when it is set, resolved against the repository root when relative, as
 * cargo resolves it for a command run there. On Windows the file name ends in `.exe`.
 */
export function cargoBinaryPath(name: string, env: NodeJS.ProcessEnv = process.env): string {
  const targetDirectory = env.CARGO_TARGET_DIR;
  const target =
    targetDirectory === undefined || targetDirectory === ''
      ? join(repositoryRoot, 'target')
      : isAbsolute(targetDirectory)
        ? targetDirectory
        : resolve(repositoryRoot, targetDirectory);
  const fileName = process.platform === 'win32' ? `${name}.exe` : name;
  return join(target, 'debug', fileName);
}
