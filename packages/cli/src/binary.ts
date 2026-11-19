import { cargoBinaryPath, repositoryRoot } from '@spokenpath/capture-safari/binary';

export { repositoryRoot };

/** The command that builds the binary, which the missing-binary message names. */
export const BUILD_COMMAND = 'cargo build -p spokenpath_cli';

/** Where the `spokenpath-core` debug binary is, by the rule `cargoBinaryPath` states. */
export function binaryPath(env: NodeJS.ProcessEnv = process.env): string {
  return cargoBinaryPath('spokenpath-core', env);
}
