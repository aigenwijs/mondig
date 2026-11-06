// Gedeeld door npm-licenties.mjs en vermeldingen.mjs: zoekt het NOTICE-bestand in de map
// van een pakket of crate (Apache-2.0, artikel 4(d)).

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** De inhoud van het NOTICE-bestand in `map` (met LF), of `null` als er geen is. */
export function zoekNotice(map) {
  if (!existsSync(map)) return null;
  try {
    const naam = readdirSync(map).find((n) => /^notice([-.].*)?$/i.test(n));
    if (!naam) return null;
    return readFileSync(join(map, naam), "utf8")
      .replace(/\r\n/g, "\n")
      .trimEnd();
  } catch {
    return null;
  }
}
