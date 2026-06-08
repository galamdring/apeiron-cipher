/**
 * Smoke test: validate every example payload against the JSON Schema.
 *
 * Run with: `npm run validate` (via ts-node) or after build via `node dist/validate-examples.js`
 */

import { validateEnvelope, assertEnvelope } from "./schema.js";
import { ALL_EXAMPLES } from "./examples.js";

let passed = 0;
let failed = 0;

for (const example of ALL_EXAMPLES) {
  try {
    assertEnvelope(example);
    console.log(`  PASS  ${example.type}`);
    passed++;
  } catch (err) {
    console.error(`  FAIL  ${example.type}:`, err);
    failed++;
  }
}

console.log(`\n${passed} passed, ${failed} failed`);

if (failed > 0) {
  process.exit(1);
}
