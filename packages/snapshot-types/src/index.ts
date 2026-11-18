import { Ajv2020, type ErrorObject } from 'ajv/dist/2020.js';
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
  uint32: (value: number) => Number.isInteger(value) && value >= 0 && value <= 0xffff_ffff,
  // JSON numbers above 2^53 lose precision when parsed, so the check is the range, not exactness.
  uint64: (value: number) => Number.isInteger(value) && value >= 0 && value < 2 ** 64,
  double: (value: number) => Number.isFinite(value),
} satisfies Record<string, (value: number) => boolean>;

const ajv = new Ajv2020({ strict: true, allErrors: true });
for (const [name, validate] of Object.entries(formats)) {
  ajv.addFormat(name, { type: 'number', validate });
}
const validate = ajv.compile<Snapshot>(schema);

/** Escapes one JSON Pointer reference token (RFC 6901). */
function escapePointerToken(token: string): string {
  return token.replaceAll('~', '~0').replaceAll('/', '~1');
}

function toIssue(error: ErrorObject): ValidationIssue {
  const missing: unknown = error.params.missingProperty;
  const path =
    error.keyword === 'required' && typeof missing === 'string'
      ? `${error.instancePath}/${escapePointerToken(missing)}`
      : error.instancePath;
  return { path, message: error.message ?? `fails the ${error.keyword} check` };
}

/**
 * Checks `value` against the snapshot schema. Like the Rust reader, it accepts unknown fields and
 * any minor version, and rejects a wrong `schema` identifier and any major version other than 0.
 */
export function validateSnapshot(value: unknown): ValidationResult {
  if (validate(value)) {
    return { ok: true, snapshot: value };
  }
  return { ok: false, errors: (validate.errors ?? []).map(toIssue) };
}
