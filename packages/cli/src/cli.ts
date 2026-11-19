import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { CaptureError, captureChromium, verifyInstrumentation } from '@spokenpath/capture-chromium';
import { InstrumentHtmlError } from '@spokenpath/instrument-html';
import { BUILD_COMMAND, binaryPath, repositoryRoot } from './binary.ts';
import { escapeForTerminal, escapeLines } from './escape.ts';

const USAGE =
  'usage: spokenpath <page.html> [--format text|json] [--no-speech] [--wait-for <selector>] [--verify-instrumentation]';

const HELP = `${USAGE}

Prints what the browser exposes for an HTML page, item by item, traced to the tags in the file.

  --format text|json         the text transcript (the default), or JSON for tools
  --no-speech                leave out the illustrative speech (~ lines, approx in JSON)
  --wait-for <selector>      after load, wait until a CSS selector matches before capturing
  --verify-instrumentation   check that instrumentation keeps the page's DOM; print one line
  -h, --help                 print this help
  --version                  print the version

Each option may be given once. See docs/usage.md for the output and the exit codes.
`;

/**
 * Exit codes: 0 on success, 1 when capture fails for a reason other than the page or the binary
 * is missing, 2 for a usage error or a page that cannot be read or instrumented. Otherwise the
 * binary's own code passes through: 2 for a snapshot it rejects, 3 for an unsupported version.
 */
const EXIT = { ok: 0, failed: 1, badInput: 2 } as const;

type Format = 'text' | 'json';

type Invocation =
  | { kind: 'help' }
  | { kind: 'version' }
  | { kind: 'verify'; page: string; waitFor?: string }
  | { kind: 'transcript'; page: string; waitFor?: string; format: Format; noSpeech: boolean };

const OPTIONS = {
  format: { type: 'string' },
  'no-speech': { type: 'boolean' },
  'wait-for': { type: 'string' },
  'verify-instrumentation': { type: 'boolean' },
  help: { type: 'boolean', short: 'h' },
  version: { type: 'boolean' },
} as const;

/**
 * The usage error a `parseArgs` failure stands for, in the command's own words. Node's messages
 * suggest `--` syntax and quote unevenly, so only the option they name is kept, found by the
 * error's code.
 */
function argumentError(error: TypeError): string {
  const code = 'code' in error ? error.code : undefined;
  const named = /'(-{1,2}[^' <]+)/.exec(error.message)?.[1];
  if (code === 'ERR_PARSE_ARGS_UNKNOWN_OPTION' && named !== undefined) {
    return `unknown option ${JSON.stringify(named)}`;
  }
  if (code === 'ERR_PARSE_ARGS_INVALID_OPTION_VALUE' && named !== undefined) {
    const option = Object.entries(OPTIONS).find(([name]) => `--${name}` === named)?.[1];
    return option?.type === 'boolean'
      ? `${JSON.stringify(named)} takes no value`
      : `${JSON.stringify(named)} needs a value`;
  }
  return 'the arguments cannot be read';
}

/** The invocation `args` describe, or the usage error they make. */
function parseInvocation(args: string[]): Invocation | { error: string } {
  let parsed: ReturnType<typeof parse>;
  try {
    parsed = parse(args);
  } catch (error) {
    if (error instanceof TypeError) {
      return { error: argumentError(error) };
    }
    throw error;
  }
  const { values, positionals, tokens } = parsed;
  const seen = new Set<string>();
  for (const token of tokens) {
    if (token.kind !== 'option') {
      continue;
    }
    if (seen.has(token.name)) {
      return { error: `${JSON.stringify(token.rawName)} is given more than once` };
    }
    seen.add(token.name);
  }
  if (values.help === true || values.version === true) {
    if (args.length !== 1) {
      return { error: `${values.help === true ? '--help' : '--version'} takes no other arguments` };
    }
    return { kind: values.help === true ? 'help' : 'version' };
  }
  if (positionals.length !== 1) {
    return { error: positionals.length === 0 ? 'no page given' : 'more than one page given' };
  }
  const [page] = positionals as [string];
  const waitFor = values['wait-for'] === undefined ? {} : { waitFor: values['wait-for'] };
  if (values['verify-instrumentation'] === true) {
    if (values.format !== undefined || values['no-speech'] === true) {
      return {
        error:
          '--format and --no-speech cannot be used with --verify-instrumentation, which prints no transcript',
      };
    }
    return { kind: 'verify', page, ...waitFor };
  }
  const format = values.format ?? 'text';
  if (format !== 'text' && format !== 'json') {
    return { error: `unknown format ${JSON.stringify(format)}; the formats are text and json` };
  }
  return { kind: 'transcript', page, ...waitFor, format, noSpeech: values['no-speech'] === true };
}

function parse(args: string[]) {
  return parseArgs({ args, allowPositionals: true, strict: true, tokens: true, options: OPTIONS });
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

/** A page that cannot be read or that the instrumenter refuses is bad input; any other failure is not. */
function exitCodeOf(error: unknown): number {
  if (error instanceof InstrumentHtmlError) {
    return EXIT.badInput;
  }
  if (error instanceof CaptureError && error.code === 'page-unreadable') {
    return EXIT.badInput;
  }
  return EXIT.failed;
}

/**
 * Writes a failure to stderr, every line escaped. A multi-line message, such as Playwright's
 * with its call log, keeps its line breaks; every other control character is escaped.
 */
function fail(message: string): void {
  process.stderr.write(`spokenpath: ${escapeLines(message)}\n`);
}

/** The workspace version, which the release commit sets in the root `package.json`. */
function version(): string {
  const manifest: unknown = JSON.parse(readFileSync(join(repositoryRoot, 'package.json'), 'utf8'));
  const value =
    typeof manifest === 'object' && manifest !== null && 'version' in manifest ? manifest.version : undefined;
  if (typeof value !== 'string') {
    throw new Error('the root package.json has no version');
  }
  return value;
}

/**
 * Runs the binary on the snapshot, written to its stdin. Its stdout and stderr are the terminal's,
 * so its output and escaping arrive unchanged; the result is its exit code, or 1 when a signal
 * ends it.
 */
function transcribe(binary: string, args: string[], snapshotJson: string): Promise<number> {
  return new Promise((resolveCode, reject) => {
    const child = spawn(binary, args, { stdio: ['pipe', 'inherit', 'inherit'] });
    child.on('error', reject);
    // The binary stops reading when it rejects the input, which closes the pipe early; its exit
    // code and message report that, so the broken pipe itself is not a second failure.
    child.stdin.on('error', (error: NodeJS.ErrnoException) => {
      if (error.code !== 'EPIPE') {
        reject(error);
      }
    });
    child.on('close', (code) => resolveCode(code ?? EXIT.failed));
    child.stdin.end(snapshotJson);
  });
}

async function run(args: string[]): Promise<number> {
  const invocation = parseInvocation(args);
  if ('error' in invocation) {
    fail(invocation.error);
    process.stderr.write(`${USAGE}\n`);
    return EXIT.badInput;
  }
  if (invocation.kind === 'help') {
    process.stdout.write(HELP);
    return EXIT.ok;
  }
  if (invocation.kind === 'version') {
    process.stdout.write(`spokenpath ${version()}\n`);
    return EXIT.ok;
  }
  const page = resolve(process.cwd(), invocation.page);
  const options = invocation.waitFor === undefined ? {} : { waitFor: invocation.waitFor };
  try {
    if (invocation.kind === 'verify') {
      const comparison = await verifyInstrumentation(page, options);
      const shown = escapeForTerminal(invocation.page);
      if (comparison.equal) {
        process.stdout.write(`${shown}: the instrumented page builds the same DOM as the original\n`);
        return EXIT.ok;
      }
      process.stdout.write(
        `${shown}: the instrumented page builds a different DOM, first at ${escapeForTerminal(comparison.path)}: ${escapeForTerminal(comparison.reason)}\n`,
      );
      return EXIT.failed;
    }

    const binary = binaryPath();
    if (!existsSync(binary)) {
      fail(`the spokenpath-core binary is not built at ${binary}; build it with \`${BUILD_COMMAND}\``);
      return EXIT.failed;
    }
    const snapshot = await captureChromium(page, options);
    const binaryArgs = ['transcript', '-', '--source', page, '--format', invocation.format];
    if (invocation.noSpeech) {
      binaryArgs.push('--no-speech');
    }
    return await transcribe(binary, binaryArgs, JSON.stringify(snapshot));
  } catch (error) {
    fail(describeFailure(error));
    return exitCodeOf(error);
  }
}

// The exit code is set rather than exiting, so output still being written is flushed first.
process.exitCode = await run(process.argv.slice(2));
