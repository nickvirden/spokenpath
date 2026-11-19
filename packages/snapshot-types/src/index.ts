import { Ajv2020, type ErrorObject, type ValidateFunction } from 'ajv/dist/2020.js';
import schema from '../../../schema/snapshot.v0.json' with { type: 'json' };
import type { Snapshot } from './snapshot.generated.js';

export type * from './snapshot.generated.js';

export const packageName: '@spokenpath/snapshot-types' = '@spokenpath/snapshot-types';

/** One reason a value is not a valid snapshot. */
export interface ValidationIssue {
  /** The JSON Pointer of the failing location; for a missing field, the field's own location. */
  path: string;
  message: string;
}

export type ValidationResult = { ok: true; snapshot: Snapshot } | { ok: false; errors: ValidationIssue[] };

/**
 * The `format`s the schema uses, each checked for what it means rather than ignored. Ajv's strict
 * mode rejects a schema that names a format it does not know, so a new format in the schema
 * fails at load until it is registered here.
 */
const formats = {
  int32: (value: number) => Number.isInteger(value) && value >= -(2 ** 31) && value < 2 ** 31,
  uint32: (value: number) => Number.isInteger(value) && value >= 0 && value <= 0xffff_ffff,
  // As for uint64, the check is the range, not exactness.
  int64: (value: number) => Number.isInteger(value) && value >= -(2 ** 63) && value < 2 ** 63,
  // JSON numbers above 2^53 lose precision when parsed, so the check is the range, not exactness.
  uint64: (value: number) => Number.isInteger(value) && value >= 0 && value < 2 ** 64,
  double: (value: number) => Number.isFinite(value),
} satisfies Record<string, (value: number) => boolean>;

const ajv = new Ajv2020({ strict: true, allErrors: true });
for (const [name, validate] of Object.entries(formats)) {
  ajv.addFormat(name, { type: 'number', validate });
}
ajv.addSchema(schema, 'snapshot');

function compiled<T>(ref: string): ValidateFunction<T> {
  const validate = ajv.getSchema<T>(ref);
  if (validate === undefined) {
    throw new Error(`the snapshot schema has no ${ref}`);
  }
  return validate;
}

const validate = compiled<Snapshot>('snapshot');

/**
 * One validator per payload variant, keyed by its `engine`, read from the schema's `Payload`
 * `oneOf`. When a payload fails, Ajv reports every `oneOf` branch; validating against the branch
 * that `payload.engine` names keeps only the failures that matter.
 */
const payloadValidators = new Map(
  schema.$defs.Payload.oneOf.flatMap((branch) =>
    branch.allOf.map((part) => [branch.properties.engine.const, compiled(`snapshot${part.$ref}`)] as const),
  ),
);

/** Escapes one JSON Pointer reference token (RFC 6901). */
function escapePointerToken(token: string): string {
  return token.replaceAll('~', '~0').replaceAll('/', '~1');
}

function toIssue(error: ErrorObject, prefix = ''): ValidationIssue {
  const missing: unknown = error.params.missingProperty;
  const location = `${prefix}${error.instancePath}`;
  const path =
    error.keyword === 'required' && typeof missing === 'string'
      ? `${location}/${escapePointerToken(missing)}`
      : location;
  return { path, message: error.message ?? `fails the ${error.keyword} check` };
}

function field(value: unknown, key: string): unknown {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>)[key] : undefined;
}

/** The schema errors, with the payload's reduced to those of the variant `payload.engine` names. */
function schemaIssues(value: unknown, errors: ErrorObject[]): ValidationIssue[] {
  const payload = field(value, 'payload');
  const engine = field(payload, 'engine');
  const validatePayload = typeof engine === 'string' ? payloadValidators.get(engine) : undefined;
  if (validatePayload === undefined) {
    return errors.map((error) => toIssue(error));
  }
  const envelopeIssues = errors
    .filter((error) => error.instancePath !== '/payload' && !error.instancePath.startsWith('/payload/'))
    .map((error) => toIssue(error));
  const payloadIssues = validatePayload(payload)
    ? []
    : (validatePayload.errors ?? []).map((error) => toIssue(error, '/payload'));
  return [...envelopeIssues, ...payloadIssues];
}

/**
 * The engine rule the schema cannot state: `capture.engine` must equal `payload.engine`. It is
 * checked here, once, rather than as a schema `if`/`then`, which would report it a second time
 * through every `oneOf` branch and add nothing to the generated types.
 */
function engineMismatch(value: unknown): ValidationIssue | undefined {
  const captureEngine = field(field(value, 'capture'), 'engine');
  const payloadEngine = field(field(value, 'payload'), 'engine');
  if (
    typeof captureEngine !== 'string' ||
    typeof payloadEngine !== 'string' ||
    !payloadValidators.has(captureEngine) ||
    !payloadValidators.has(payloadEngine) ||
    captureEngine === payloadEngine
  ) {
    return undefined;
  }
  return {
    path: '/payload/engine',
    message: `capture.engine is \`${captureEngine}\` but payload.engine is \`${payloadEngine}\`; they must name the same engine`,
  };
}

/**
 * Checks `value` against the snapshot schema and the engine rule. Like the Rust reader, it accepts
 * unknown fields and any minor version, and rejects a wrong `schema` identifier, any major version
 * other than 0, and a `capture.engine` that differs from `payload.engine`. It is the strict gate
 * for writers: it also enforces the patterns and ranges that the Rust reader leaves unchecked.
 */
export function validateSnapshot(value: unknown): ValidationResult {
  const valid = validate(value);
  const mismatch = engineMismatch(value);
  if (valid && mismatch === undefined) {
    return { ok: true, snapshot: value };
  }
  const issues = valid ? [] : schemaIssues(value, validate.errors ?? []);
  return { ok: false, errors: mismatch === undefined ? issues : [...issues, mismatch] };
}
