import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';
import { vindAfgekeurd } from './afgekeurd';

// Tweede woordtest (UI-D): de teksten die Rust zelf aan de gebruiker laat zien, zoals
// `Foutmelding::nieuw("…")`, een `format!("…")` daarin, en `#[error("…")]`. Die komen niet door
// `nl.ts`, dus `woorden.test.ts` ziet ze niet. Deze test leest de Rust-bronnen met een kleine
// tokenizer (commentaar, tekst, tekens en `#[cfg(test)]`-modules worden herkend) en keurt
// dezelfde woorden af (`AFGEKEURD`, ontwerpsysteem §3).
//
// Grens: alleen tekst die direct in de aanroep staat wordt gelezen. Een melding die eerst in een
// variabele wordt gebouwd en dan aan `Foutmelding::nieuw` wordt gegeven, valt erbuiten.
// Ook buiten zicht: `match`-armen die een `&str` teruggeven (`DownloadFout::melding`), de tooltip
// van het systeemvak en bestandsnamen (`TERUGVAL` in `naam.rs`). Die blijven handwerk.

const APP_BRON = join(__dirname, '../../../src-tauri/src');
const CRATES = join(__dirname, '../../../../crates');

/**
 * Bewuste uitzonderingen: pad (vanaf de repo-root) naar een fragment van de tekst, met de reden.
 * Houd de lijst kort; alleen tekst die de gebruiker nooit ziet.
 */
const UITZONDERINGEN: { bestand: string; fragment: string; reden: string }[] = [];

interface Tekst {
  /** Waar de aanroep begint: "bestand:regel". */
  plek: string;
  tekst: string;
}

interface Delen {
  soort: 'tekst' | 'code';
  van: number;
  tot: number;
  inhoud: string;
}

/** Splitst Rust-bron in codestukken en tekstliteralen; commentaar valt weg. */
export function tokens(bron: string): Delen[] {
  const uit: Delen[] = [];
  let i = 0;
  let codeVan = 0;
  const sluitCode = (tot: number): void => {
    if (tot > codeVan)
      uit.push({ soort: 'code', van: codeVan, tot, inhoud: bron.slice(codeVan, tot) });
  };
  while (i < bron.length) {
    const c = bron[i];
    const volgende = bron[i + 1];
    const ruw = c === 'r' ? /^r(#*)"/.exec(bron.slice(i, i + 12)) : null;
    if (c === '/' && volgende === '/') {
      sluitCode(i);
      while (i < bron.length && bron[i] !== '\n') i++;
      codeVan = i;
    } else if (c === '/' && volgende === '*') {
      sluitCode(i);
      let diepte = 1;
      i += 2;
      while (i < bron.length && diepte > 0) {
        if (bron[i] === '/' && bron[i + 1] === '*') {
          diepte++;
          i += 2;
        } else if (bron[i] === '*' && bron[i + 1] === '/') {
          diepte--;
          i += 2;
        } else i++;
      }
      codeVan = i;
    } else if (ruw !== null && !/\w/.test(bron[i - 1] ?? ' ')) {
      // Ruwe tekst: r"…" of r#"…"#
      const hekjes = ruw[1]?.length ?? 0;
      const eind = '"' + '#'.repeat(hekjes);
      const begin = i + 2 + hekjes;
      const sluit = bron.indexOf(eind, begin);
      const tot = sluit < 0 ? bron.length : sluit;
      sluitCode(i);
      uit.push({ soort: 'tekst', van: i, tot: tot + eind.length, inhoud: bron.slice(begin, tot) });
      i = tot + eind.length;
      codeVan = i;
    } else if (c === '"') {
      sluitCode(i);
      const begin = i + 1;
      i++;
      while (i < bron.length && bron[i] !== '"') i += bron[i] === '\\' ? 2 : 1;
      uit.push({ soort: 'tekst', van: begin - 1, tot: i + 1, inhoud: bron.slice(begin, i) });
      i++;
      codeVan = i;
    } else if (c === "'") {
      // Teken ('x', '\n', '"') of levensduur ('a). Een teken sluit binnen vier tekens.
      const teken = /^'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'])'/.exec(
        bron.slice(i, i + 12)
      );
      if (teken) {
        sluitCode(i);
        i += teken[0].length;
        codeVan = i;
      } else i++;
    } else i++;
  }
  sluitCode(bron.length);
  return uit;
}

/** De tekst van een Rust-string zonder de ontsnappingen `\"`, `\\` en `\n` (genoeg voor woorden). */
function ontsnapt(inhoud: string): string {
  return inhoud
    .replace(/\\\r?\n\s*/g, '')
    .replace(/\\(.)/g, (_, c: string) => (c === 'n' ? '\n' : c));
}

/** Haalt de `#[cfg(test)]`-modules uit de delen: alles van het attribuut tot de sluitende `}`. */
function zonderTests(delen: Delen[]): Delen[] {
  const uit: Delen[] = [];
  const begin = /#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/g;
  // Hoeveel accolades diep we in een testmodule zitten; 0 is erbuiten.
  let diepte = 0;
  for (const deel of delen) {
    if (deel.soort === 'tekst') {
      if (diepte === 0) uit.push(deel);
      continue;
    }
    let i = 0;
    while (i < deel.inhoud.length) {
      if (diepte === 0) {
        begin.lastIndex = i;
        const m = begin.exec(deel.inhoud);
        const eind = m ? m.index : deel.inhoud.length;
        if (eind > i) {
          uit.push({ ...deel, van: deel.van + i, inhoud: deel.inhoud.slice(i, eind) });
        }
        if (!m) break;
        diepte = 1;
        i = m.index + m[0].length;
      }
      while (i < deel.inhoud.length && diepte > 0) {
        const ch = deel.inhoud[i++];
        if (ch === '{') diepte++;
        else if (ch === '}') diepte--;
      }
    }
  }
  return uit;
}

/** Het regelnummer van een plek in de bron. */
function regelVan(bron: string, plek: number): number {
  let regel = 1;
  for (let i = 0; i < plek; i++) if (bron[i] === '\n') regel++;
  return regel;
}

/** Het begin van een aanroep waarvan het eerste argument een tekst voor de gebruiker is. */
const AANROEP = /Foutmelding::nieuw\s*\(|#\[error\s*\(/g;

/**
 * De teksten die de gebruiker kan zien: de tekstliteralen in het eerste argument van
 * `Foutmelding::nieuw(…)` en in `#[error(…)]`. Een `format!(…)` of `&format!(…)` in dat argument
 * valt er vanzelf onder; het tweede argument (het vervolg) telt niet.
 */
export function gebruikersteksten(bron: string, bestand: string): Tekst[] {
  const delen = zonderTests(tokens(bron));
  const uit: Tekst[] = [];
  for (const [d, deel] of delen.entries()) {
    if (deel.soort !== 'code') continue;
    for (const m of deel.inhoud.matchAll(AANROEP)) {
      const plek = `${bestand}:${String(regelVan(bron, deel.van + m.index))}`;
      // De aanroep zelf is haakjesdiepte 1; het eerste argument loopt tot de komma op diepte 1.
      let diepte = 1;
      const loop = (code: string): boolean => {
        for (const ch of code) {
          if (ch === '(' || ch === '[' || ch === '{') diepte++;
          else if (ch === ')' || ch === ']' || ch === '}') diepte--;
          else if (ch === ',' && diepte === 1) return false;
          if (diepte === 0) return false;
        }
        return true;
      };
      let doorgaan = loop(deel.inhoud.slice(m.index + m[0].length));
      for (const volgend of delen.slice(d + 1)) {
        if (!doorgaan) break;
        if (volgend.soort === 'tekst') uit.push({ plek, tekst: ontsnapt(volgend.inhoud) });
        else doorgaan = loop(volgend.inhoud);
      }
    }
  }
  return uit;
}

function rustBestanden(map: string): string[] {
  const uit: string[] = [];
  for (const naam of readdirSync(map)) {
    const pad = join(map, naam);
    if (statSync(pad).isDirectory()) {
      if (naam !== 'target' && naam !== 'node_modules') uit.push(...rustBestanden(pad));
    } else if (naam.endsWith('.rs')) uit.push(pad);
  }
  return uit;
}

const REPO = join(__dirname, '../../../..');

function alleTeksten(): Tekst[] {
  const bronmappen = [
    APP_BRON,
    ...readdirSync(CRATES)
      .map((c) => join(CRATES, c, 'src'))
      .filter((m) => {
        try {
          return statSync(m).isDirectory();
        } catch {
          return false;
        }
      })
  ];
  const uit: Tekst[] = [];
  for (const map of bronmappen) {
    for (const bestand of rustBestanden(map)) {
      const naam = relative(REPO, bestand).replace(/\\/g, '/');
      uit.push(...gebruikersteksten(readFileSync(bestand, 'utf-8'), naam));
    }
  }
  return uit;
}

const alle = alleTeksten();

describe('de woordenlijst in de teksten uit Rust', () => {
  it('leest een flinke hoeveelheid tekst (anders is de controle stuk)', () => {
    expect(alle.length).toBeGreaterThan(150);
  });

  it('bevat geen afgekeurd woord', () => {
    const fouten: string[] = [];
    for (const { plek, tekst } of alle) {
      const bestand = plek.slice(0, plek.lastIndexOf(':'));
      for (const regel of vindAfgekeurd(tekst)) {
        const bewust = UITZONDERINGEN.some(
          (u) => u.bestand === bestand && tekst.includes(u.fragment)
        );
        if (bewust) continue;
        fouten.push(`${plek}: "${regel.naam}" in "${tekst}", gebruik: ${regel.beter}`);
      }
    }
    expect(
      fouten,
      `Afgekeurde woorden in Rust-teksten (zie docs/ontwerp/ontwerpsysteem.md §3):\n${fouten.join('\n')}`
    ).toEqual([]);
  });

  it('heeft alleen uitzonderingen die nog nodig zijn, elk met een reden', () => {
    const overbodig = UITZONDERINGEN.filter(
      (u) =>
        u.reden === '' ||
        !alle.some(
          (t) =>
            t.plek.startsWith(`${u.bestand}:`) &&
            t.tekst.includes(u.fragment) &&
            vindAfgekeurd(t.tekst).length > 0
        )
    ).map((u) => `${u.bestand}: "${u.fragment}"`);
    expect(overbodig).toEqual([]);
  });
});

describe('de controle op Rust-teksten zelf', () => {
  const lees = (bron: string): string[] => gebruikersteksten(bron, 'test.rs').map((t) => t.tekst);

  it('vindt de tekst van Foutmelding::nieuw, ook in een format! en over meer regels', () => {
    const bron = `
      fn a() -> Foutmelding {
        Foutmelding::nieuw("Dit transcript is er niet meer.", Vervolg::Geen)
      }
      fn b(n: u32) -> Foutmelding {
        Foutmelding::nieuw(
          format!("Het {n} lukte \\"niet\\", zie (hier)"),
          Vervolg::Geen,
        )
      }`;
    expect(lees(bron)).toEqual([
      'Dit transcript is er niet meer.',
      'Het {n} lukte "niet", zie (hier)'
    ]);
  });

  it('vindt de tekst van #[error] en negeert het tweede argument', () => {
    const bron = `
      #[derive(Debug, thiserror::Error)]
      enum Fout {
        #[error("Wissen mislukt: {0}")]
        Wis(String),
        #[error("geen tekst")]
        Geen,
      }
      fn x() { Foutmelding::nieuw(melding, "Vervolg wissen") }`;
    expect(lees(bron)).toEqual(['Wissen mislukt: {0}', 'geen tekst']);
  });

  it('slaat commentaar, gewone tekst en #[cfg(test)]-modules over', () => {
    const bron = `
      // Foutmelding::nieuw("in commentaar wissen", x)
      /* #[error("ook commentaar wissen")] */
      fn doc() { let _ = "Foutmelding::nieuw(\\"in een tekst\\")"; let c = '"'; }
      #[cfg(test)]
      mod tests {
        fn t() { Foutmelding::nieuw("alleen in tests wissen", Vervolg::Geen) }
        fn u() { let s = "} accolade in tekst"; }
      }
      fn echt() { Foutmelding::nieuw("Dit is echt", Vervolg::Geen) }`;
    expect(lees(bron)).toEqual(['Dit is echt']);
  });

  it('faalt aantoonbaar op een afgekeurd woord in zo een tekst', () => {
    const bron = 'fn x() { Foutmelding::nieuw("Het transcript is gewist.", Vervolg::Geen) }';
    const namen = lees(bron).flatMap((t) => vindAfgekeurd(t).map((r) => r.naam));
    expect(namen).toEqual(expect.arrayContaining(['transcript', 'gewist']));
  });
});
