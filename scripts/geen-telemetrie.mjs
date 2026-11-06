#!/usr/bin/env node
// Weigert bekende analytics-, telemetrie- en crashrapportagepakketten in de
// npm-lockfile van de app (ADR 0003). Voor Rust doet deny.toml hetzelfde.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERBODEN = [
  /^posthog-(js|node)$/,
  /^@sentry\//,
  /^@aptabase\//,
  /^mixpanel(-browser)?$/,
  /^@amplitude\//,
  /^@segment\//,
  /^plausible-tracker$/,
  /^@vercel\/analytics$/,
  /^@datadog\//,
  /^@bugsnag\//,
  /^logrocket$/,
];

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(
  readFileSync(join(root, "app", "package-lock.json"), "utf8"),
);

const gevonden = Object.keys(lock.packages ?? {})
  .filter((pad) => pad !== "")
  .map((pad) => pad.split("node_modules/").at(-1) ?? pad)
  .filter((naam) => VERBODEN.some((re) => re.test(naam)));

if (gevonden.length > 0) {
  console.error("Telemetrie- of analyticspakket gevonden (ADR 0003):");
  for (const naam of [...new Set(gevonden)]) console.error(`  - ${naam}`);
  process.exit(1);
}
console.log("geen-telemetrie: geen verboden pakketten gevonden.");
