import { writeFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';
import { captureChromium, verifyInstrumentation } from './capture.ts';

const USAGE =
  'usage: node packages/capture-chromium/src/cli.ts <page.html> [--out <file>] [--wait-for <selector>] [--verify-instrumentation]';

/** Exit codes: 0 on success, 1 when capture or verification fails, 2 on a usage error. */
const EXIT = { ok: 0, failed: 1, usage: 2 } as const;

interface Invocation {
  page: string;
  out?: string;
  waitFor?: string;
  verify: boolean;
}

/** The invocation `args` describe, or the usage error they make. */
function parseInvocation(args: string[]): Invocation | { error: string } {
  let parsed: ReturnType<typeof parse>;
  try {
    parsed = parse(args);
  } catch (error) {
    if (error instanceof TypeError) {
      return { error: error.message };
    }
    throw error;
  }
  const { values, positionals } = parsed;
  if (positionals.length !== 1) {
    return { error: positionals.length === 0 ? 'no page given' : 'more than one page given' };
  }
  const [page] = positionals as [string];
  if (values['verify-instrumentation'] === true && values.out !== undefined) {
    return { error: '--out cannot be used with --verify-instrumentation, which writes no snapshot' };
  }
  return {
    page,
    ...(values.out === undefined ? {} : { out: values.out }),
    ...(values['wait-for'] === undefined ? {} : { waitFor: values['wait-for'] }),
    verify: values['verify-instrumentation'] === true,
  };
}

function parse(args: string[]) {
  return parseArgs({
    args,
    allowPositionals: true,
    strict: true,
    options: {
      out: { type: 'string' },
      'wait-for': { type: 'string' },
      'verify-instrumentation': { type: 'boolean' },
    },
  });
}

/** The error's message, prefixed with its code when it carries one. */
function describeFailure(error: unknown): string {
  if (!(error instanceof Error)) {
    return String(error);
  }
  const code = 'code' in error && typeof error.code === 'string' ? error.code : undefined;
  // Node's file system errors already start their message with the code.
  return code === undefined || error.message.startsWith(`${code}:`)
    ? error.message
    : `${code}: ${error.message}`;
}

async function run(args: string[]): Promise<number> {
  const invocation = parseInvocation(args);
  if ('error' in invocation) {
    process.stderr.write(`${invocation.error}\n${USAGE}\n`);
    return EXIT.usage;
  }
  const options = invocation.waitFor === undefined ? {} : { waitFor: invocation.waitFor };
  try {
    if (invocation.verify) {
      const comparison = await verifyInstrumentation(invocation.page, options);
      if (comparison.equal) {
        process.stdout.write(
          `${invocation.page}: the instrumented page builds the same DOM as the original\n`,
        );
        return EXIT.ok;
      }
      process.stdout.write(
        `${invocation.page}: the instrumented page builds a different DOM, first at ${comparison.path}: ${comparison.reason}\n`,
      );
      return EXIT.failed;
    }
    const snapshot = await captureChromium(invocation.page, options);
    const json = `${JSON.stringify(snapshot, null, 2)}\n`;
    if (invocation.out === undefined) {
      process.stdout.write(json);
    } else {
      await writeFile(invocation.out, json);
    }
    return EXIT.ok;
  } catch (error) {
    process.stderr.write(`capture failed: ${describeFailure(error)}\n`);
    return EXIT.failed;
  }
}

// The exit code is set rather than exiting, so a large snapshot on stdout is flushed first.
process.exitCode = await run(process.argv.slice(2));
