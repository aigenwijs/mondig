import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';
import { vindAfgekeurd } from './afgekeurd';

// Bewaakt de woordenlijst van het ontwerpsysteem (docs/ontwerp/ontwerpsysteem.md, §3): in `nl.ts`
// mag geen afgekeurd woord staan. De test leest het bronbestand en loopt de stringliteralen af
// (ook de stukken van sjabloonteksten, dus ook de functieberichten), zodat commentaar en
// sleutelnamen niet meetellen en er geen voorbeeldwaarden voor de functies nodig zijn.

/**
 * Bewuste uitzonderingen: sleutel in `nl.ts` naar de namen (uit AFGEKEURD) die daar mogen,
 * met de reden erbij. Houd deze lijst kort; een nieuwe uitzondering hoort eerst in §3 van het
 * ontwerpsysteem.
 */
const UITZONDERINGEN: Record<string, { toegestaan: string[]; reden: string }> = {
  stopwoordenKop: {
    toegestaan: ['weghalen'],
    reden: '§3: Opvulklanken weghalen (tekst opschonen)'
  },
  persoonWeghalen: { toegestaan: ['weghalen'], reden: '§3: een markering weghalen, geen bestand' },
  persoonWeghalenTitel: { toegestaan: ['weghalen'], reden: '§3: Markering weghalen' },
  notulenWeghalen: { toegestaan: ['weghalen'], reden: 'Een notulenpunt weghalen, geen bestand' },
  notulenPuntWeghalenAria: { toegestaan: ['weghalen'], reden: 'Idem, voor de schermlezer' }
};

interface Tekst {
  sleutel: string;
  tekst: string;
}

/** Alle tekst uit `nl.ts`, met de sleutel waaronder hij staat. */
function teksten(bron: string): Tekst[] {
  const bestand = ts.createSourceFile('nl.ts', bron, ts.ScriptTarget.Latest, true);
  const uit: Tekst[] = [];

  function sleutelVan(node: ts.Node): string {
    for (let n = node; !ts.isSourceFile(n); n = n.parent) {
      if (ts.isPropertyAssignment(n) && ts.isIdentifier(n.name)) return n.name.text;
    }
    return '(buiten een sleutel)';
  }

  function loop(node: ts.Node): void {
    if (ts.isSourceFile(node)) {
      ts.forEachChild(node, loop);
      return;
    }
    const isTekst =
      ts.isStringLiteral(node) ||
      ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) ||
      ts.isTemplateMiddle(node) ||
      ts.isTemplateTail(node);
    // Een sleutel tussen aanhalingstekens is geen tekst voor de gebruiker.
    const isSleutel = ts.isPropertyAssignment(node.parent) && node.parent.name === node;
    // Typen als 'vandaag' | 'morgen' zijn ook geen tekst voor de gebruiker.
    const isType = ts.isLiteralTypeNode(node.parent);
    if (isTekst && !isSleutel && !isType) uit.push({ sleutel: sleutelVan(node), tekst: node.text });
    ts.forEachChild(node, loop);
  }
  loop(bestand);
  return uit;
}

const bron = readFileSync(join(__dirname, 'nl.ts'), 'utf-8');
const alle = teksten(bron);

describe('de woordenlijst in nl.ts', () => {
  it('leest een flinke hoeveelheid tekst (anders is de controle stuk)', () => {
    expect(alle.length).toBeGreaterThan(800);
  });

  it('bevat geen afgekeurd woord', () => {
    const fouten: string[] = [];
    for (const { sleutel, tekst } of alle) {
      const toegestaan = UITZONDERINGEN[sleutel]?.toegestaan ?? [];
      for (const regel of vindAfgekeurd(tekst)) {
        if (toegestaan.includes(regel.naam)) continue;
        fouten.push(`${sleutel}: "${regel.naam}" in "${tekst}", gebruik: ${regel.beter}`);
      }
    }
    expect(
      fouten,
      `Afgekeurde woorden (zie docs/ontwerp/ontwerpsysteem.md §3):\n${fouten.join('\n')}`
    ).toEqual([]);
  });

  it('heeft alleen uitzonderingen die nog nodig zijn', () => {
    expect(Object.keys(UITZONDERINGEN).length).toBeGreaterThanOrEqual(0);
    for (const [sleutel, { toegestaan, reden }] of Object.entries(UITZONDERINGEN)) {
      expect(reden, `${sleutel} heeft een reden`).not.toBe('');
      const tekst = alle.filter((t) => t.sleutel === sleutel);
      expect(tekst.length, `${sleutel} bestaat nog in nl.ts`).toBeGreaterThan(0);
      for (const naam of toegestaan) {
        const nodig = tekst.some((t) => vindAfgekeurd(t.tekst).some((r) => r.naam === naam));
        expect(nodig, `${sleutel} heeft "${naam}" niet meer nodig`).toBe(true);
      }
    }
  });
});

describe('de controle zelf', () => {
  it('keurt de afgekeurde woorden af', () => {
    const voorbeelden: Record<string, string> = {
      wis: 'Alles wis je hier',
      wist: 'Mondig wist ze na 7 dagen',
      wissen: 'Alles wissen',
      gewist: 'De sleutel is gewist.',
      weggooi: 'Opname weggooien?',
      transcript: 'Het transcript staat klaar',
      'deze pc': 'Alles blijft op deze pc.',
      'Fysieke vergadering': 'Fysieke vergadering',
      'Map tonen': 'Map tonen',
      'Lokaal verwerkt': 'Lokaal verwerkt',
      Stopwoorden: 'Stopwoorden weghalen',
      'opnemen tot': 'Ingedrukt houden: opnemen tot je loslaat.',
      weghalen: 'Opname weghalen',
      afbreken: 'Het uitschrijven afbreken',
      exporteren: 'Exporteren als Word',
      'Uitschriften openen': 'Uitschriften openen',
      VAD: 'VAD downloaden',
      diarisatie: 'Diarisatie aan'
    };
    for (const [naam, tekst] of Object.entries(voorbeelden)) {
      expect(
        vindAfgekeurd(tekst).map((r) => r.naam),
        tekst
      ).toContain(naam);
    }
  });

  it('laat de goede woorden met rust', () => {
    for (const tekst of [
      'Verwijderen…',
      'Mondig verwijdert ze na 7 dagen',
      'Map om in de gaten te houden',
      'Tonen in Verkenner',
      'dicteren tot je loslaat',
      'Je wisselde van venster'
    ]) {
      expect(vindAfgekeurd(tekst), tekst).toEqual([]);
    }
  });
});
