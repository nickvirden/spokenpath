import { spawn, type StdioOptions } from 'node:child_process';
import type { Writable } from 'node:stream';

/** Where the binary's stdout and stderr go: the terminal's by default. */
export type Output = 'inherit' | number;

/** The exit code `transcribe` returns when a signal ends the binary. */
const SIGNALLED = 1;

/**
 * The path the binary reads the second snapshot from, the cross-check or aligned Safari snapshot:
 * the extra pipe on fd 3. Node gives the child a socket there, which macOS reopens through
 * `/dev/fd/3`; Linux refuses to open a socket by that path, so the route serves the real command,
 * which runs only on macOS.
 */
export const SECOND_SNAPSHOT_FD_PATH = '/dev/fd/3';

/** The write errors of a pipe whose reader has gone: `EPIPE`, or `ECONNRESET` on a socket. */
const READER_GONE: ReadonlySet<string> = new Set(['EPIPE', 'ECONNRESET']);

/**
 * Writes `text` to a pipe into the binary. The binary stops reading when it rejects its input,
 * which closes the pipe early, possibly before it reads the pipe at all; its exit code and message
 * report that, so the broken pipe itself is not a second failure.
 */
function feed(pipe: Writable | null, text: string, reject: (error: Error) => void): void {
  if (pipe === null) {
    reject(new Error('the binary has no pipe to write the snapshot to'));
    return;
  }
  pipe.on('error', (error: NodeJS.ErrnoException) => {
    if (error.code === undefined || !READER_GONE.has(error.code)) {
      reject(error);
    }
  });
  pipe.end(text);
}

/**
 * Runs the binary on the snapshot, written to its stdin, and, when `secondSnapshotJson` is given,
 * on that Safari snapshot written to an extra pipe on fd 3, which the binary reads as
 * `/dev/fd/3`. No temporary file is written. The binary's stdout and stderr are the terminal's
 * unless `output` names other file descriptors, so its output and escaping arrive unchanged; the
 * result is its exit code, or 1 when a signal ends it.
 */
export function transcribe(
  binary: string,
  args: string[],
  snapshotJson: string,
  secondSnapshotJson?: string,
  output: { stdout: Output; stderr: Output } = { stdout: 'inherit', stderr: 'inherit' },
): Promise<number> {
  return new Promise((resolveCode, reject) => {
    const stdio: StdioOptions =
      secondSnapshotJson === undefined
        ? ['pipe', output.stdout, output.stderr]
        : ['pipe', output.stdout, output.stderr, 'pipe'];
    const child = spawn(binary, args, { stdio });
    child.on('error', reject);
    child.on('close', (code) => resolveCode(code ?? SIGNALLED));
    feed(child.stdin, snapshotJson, reject);
    if (secondSnapshotJson !== undefined) {
      feed(child.stdio[3] as Writable | null, secondSnapshotJson, reject);
    }
  });
}
