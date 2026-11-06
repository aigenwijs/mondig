import { readdirSync, readFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { describe, expect, it } from 'vitest';

// Bewaakt de leesbaarheid van de tokens in app.css (ADR 0014, aanvulling 4-11-2026) en dat er
// geen vaste kleuren in de schermen sluipen. Gelezen uit de bron, dus zonder browser.

const SRC = join(__dirname, '..');
const css = readFileSync(join(SRC, 'app.css'), 'utf-8');

type Tokens = Record<string, string>;

/** De --naam: #hex-paren uit één CSS-blok. */
function tokensUit(blok: string): Tokens {
  const uit: Tokens = {};
  for (const m of blok.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    uit[m[1] ?? ''] = m[2] ?? '';
  }
  return uit;
}

/** Het blok `{ ... }` dat na `begin` begint (geen geneste accolades in deze blokken). */
function blokNa(begin: string, selector: string): string {
  const vanaf = css.indexOf(begin);
  expect(vanaf, `${begin} staat in app.css`).toBeGreaterThanOrEqual(0);
  const open = css.indexOf('{', css.indexOf(selector, vanaf));
  return css.slice(open + 1, css.indexOf('}', open));
}

const donker = tokensUit(blokNa(':root {', ':root'));
const licht = tokensUit(blokNa('@media (prefers-color-scheme: light)', ':root'));
const pil = tokensUit(blokNa('.pil-donker {', '.pil-donker'));

/** Relatieve luminantie volgens WCAG 2.x. */
function luminantie(hex: string): number {
  const kanaal = (i: number): number => {
    const c = parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * kanaal(0) + 0.7152 * kanaal(1) + 0.0722 * kanaal(2);
}

function contrast(a: string, b: string): number {
  const [hoog, laag] = [luminantie(a), luminantie(b)].sort((x, y) => y - x) as [number, number];
  return (hoog + 0.05) / (laag + 0.05);
}

function token(tokens: Tokens, naam: string): string {
  const waarde = tokens[naam];
  expect(waarde, `--${naam} bestaat`).toBeDefined();
  return waarde ?? '';
}

describe('contrast van de tokens', () => {
  const sets: [string, Tokens][] = [
    ['licht', { ...donker, ...licht }],
    ['donker', donker],
    ['de pil', pil]
  ];

  for (const [naam, tokens] of sets) {
    for (const achter of ['vlak', 'verhoogd', 'achtergrond']) {
      it(`--rand-veld haalt 3:1 tegen --${achter} (${naam})`, () => {
        expect(contrast(token(tokens, 'rand-veld'), token(tokens, achter))).toBeGreaterThanOrEqual(
          3
        );
      });
    }

    for (const achter of ['vlak', 'verhoogd', 'achtergrond']) {
      it(`--meter-leeg haalt 2:1 tegen --${achter} (${naam})`, () => {
        expect(contrast(token(tokens, 'meter-leeg'), token(tokens, achter))).toBeGreaterThanOrEqual(
          2
        );
      });
    }

    // Een uitgeschakelde knop heeft de tekstkleur --dim (button.svelte) op wat erachter zit.
    for (const achter of ['vlak', 'verhoogd', 'achtergrond', 'hover']) {
      it(`--dim (uitgeschakelde knop) haalt 3:1 tegen --${achter} (${naam})`, () => {
        expect(contrast(token(tokens, 'dim'), token(tokens, achter))).toBeGreaterThanOrEqual(3);
      });
    }

    it(`--opname bestaat en haalt 3:1 tegen --achtergrond (${naam})`, () => {
      expect(
        contrast(token(tokens, 'opname'), token(tokens, 'achtergrond'))
      ).toBeGreaterThanOrEqual(3);
    });
  }

  // De stopknop tijdens het opnemen: witte tekst op rood, ook in donker (N-C1).
  for (const [naam, tokens] of sets.slice(0, 2)) {
    it(`--op-opname is leesbaar (4,5:1) op --opname (${naam})`, () => {
      expect(contrast(token(tokens, 'op-opname'), token(tokens, 'opname'))).toBeGreaterThanOrEqual(
        4.5
      );
    });
  }

  it('--waarsch is leesbaar (4,5:1) op de donkere pil', () => {
    for (const achter of ['achtergrond', 'vlak', 'verhoogd']) {
      expect(contrast(token(pil, 'waarsch'), token(pil, achter))).toBeGreaterThanOrEqual(4.5);
    }
  });
});

describe('uitgeschakelde knoppen', () => {
  const knop = readFileSync(
    join(SRC, 'lib', 'components', 'ui', 'button', 'button.svelte'),
    'utf-8'
  );

  it('maken niets doorzichtig: de tekst blijft leesbaar in de kleur --dim', () => {
    expect(knop).not.toMatch(/(?:aria-)?disabled:opacity-/);
    for (const variant of ['primair', 'tweede', 'spook', 'rood']) {
      const regel = new RegExp(variant + String.raw`:\s*'([^']*)'`).exec(knop)?.[1] ?? '';
      // Op woordgrens: 'disabled:text-dim' zit ook als stuk in 'aria-disabled:text-dim'.
      expect(regel, `variant ${variant}`).toMatch(/(?:^|\s)aria-disabled:text-dim(?:\s|$)/);
      expect(regel, `variant ${variant}`).toMatch(/(?:^|\s)disabled:text-dim(?:\s|$)/);
    }
  });
});

/** Alle .svelte-bestanden onder src, behalve de mockbestanden in lib/dev. */
function svelteBestanden(map: string): string[] {
  const uit: string[] = [];
  for (const item of readdirSync(map, { withFileTypes: true })) {
    const pad = join(map, item.name);
    if (item.isDirectory()) {
      if (relative(SRC, pad) === ['lib', 'dev'].join(sep)) continue;
      uit.push(...svelteBestanden(pad));
    } else if (item.name.endsWith('.svelte')) {
      uit.push(pad);
    }
  }
  return uit;
}

describe('geen vaste kleuren in de schermen', () => {
  it('bevat geen vaste kleur (hex, rgb() of hsl()) in een .svelte-bestand', () => {
    const gevonden: string[] = [];
    for (const bestand of svelteBestanden(SRC)) {
      const regels = readFileSync(bestand, 'utf-8').split(/\r?\n/);
      regels.forEach((regel, i) => {
        if (
          /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b|\b(?:rgba?|hsla?)\(/.test(regel)
        ) {
          gevonden.push(`${relative(SRC, bestand)}:${String(i + 1)}`);
        }
      });
    }
    expect(gevonden, 'gebruik een token uit app.css in plaats van een vaste kleur').toEqual([]);
  });
});
