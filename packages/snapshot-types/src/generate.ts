import { readFile, writeFile } from 'node:fs/promises';
import { compile, type JSONSchema } from 'json-schema-to-typescript';

export const generateCommand = 'pnpm --filter @spokenpath/snapshot-types generate';

/** The JSON Schema that `cargo xtask schema` writes from the Rust snapshot types. */
export const schemaUrl: URL = new URL('../../../schema/snapshot.v0.json', import.meta.url);

/** The committed TypeScript types generated from the schema. */
export const generatedTypesUrl: URL = new URL('./snapshot.generated.ts', import.meta.url);

/**
 * Generates the TypeScript types for `schema`. `additionalProperties: false` keeps the types closed
 * for writers, except where the schema itself allows extra properties (the accessibility nodes),
 * whose types keep an index signature.
 */
export async function generateTypes(schema: JSONSchema): Promise<string> {
  return compile(structuredClone(schema), 'Snapshot', {
    additionalProperties: false,
    bannerComment: `// Generated from schema/snapshot.v0.json; do not edit.\n// Regenerate with \`${generateCommand}\`.`,
  });
}

export async function readSchema(): Promise<JSONSchema> {
  return JSON.parse(await readFile(schemaUrl, 'utf8')) as JSONSchema;
}

if (import.meta.main) {
  await writeFile(generatedTypesUrl, await generateTypes(await readSchema()));
}
