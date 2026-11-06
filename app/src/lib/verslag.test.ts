import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  alsSnelheid,
  bewerkToets,
  deelTekst,
  gemarkeerdeStukken,
  naarTreffer,
  neemtSpatieOver,
  PLAATSVERVANGER,
  snelheidFactor,
  vindTreffers,
  vervangenRegel,
  volgendeSleutel,
  type VerslagRegel
} from './verslag';

describe('zoeken', () => {
  const teksten = ['Goedemiddag allemaal.', 'Dank u wel, goedemiddag!', 'Tot ziens.'];

  it('vindt treffers zonder op hoofdletters te letten, in de volgorde van het verslag', () => {
    expect(vindTreffers(teksten, 'GOEDEMIDDAG')).toEqual([
      { regel: 0, van: 0, tot: 11 },
      { regel: 1, van: 12, tot: 23 }
    ]);
  });

  it('vindt meerdere treffers in één regel, zonder overlap', () => {
    expect(vindTreffers(['aaaa'], 'aa')).toEqual([
      { regel: 0, van: 0, tot: 2 },
      { regel: 0, van: 2, tot: 4 }
    ]);
  });

  it('behandelt tekens van een patroon als gewone tekst', () => {
    expect(vindTreffers(['Kosten (ex. btw)? Ja.', 'a.b'], '(ex. btw)?')).toEqual([
      { regel: 0, van: 7, tot: 17 }
    ]);
    expect(vindTreffers(['axb', 'a.b'], 'a.b')).toEqual([{ regel: 1, van: 0, tot: 3 }]);
  });

  it('geeft geen treffers bij leeg zoeken of als niets past', () => {
    expect(vindTreffers(teksten, '')).toEqual([]);
    expect(vindTreffers(teksten, 'wethouder')).toEqual([]);
    expect(vindTreffers([], 'x')).toEqual([]);
  });

  it('houdt de plekken kloppend bij letters met accent', () => {
    expect(vindTreffers(['Café Éclair'], 'éclair')).toEqual([{ regel: 0, van: 5, tot: 11 }]);
  });

  it('knipt een tekst in stukken met de actieve treffer gemarkeerd', () => {
    const treffers = vindTreffers(['Dank u wel, dank je.'], 'dank');
    expect(deelTekst('Dank u wel, dank je.', treffers, 1)).toEqual([
      { tekst: 'Dank', treffer: true, actief: false, soort: null },
      { tekst: ' u wel, ', treffer: false, actief: false, soort: null },
      { tekst: 'dank', treffer: true, actief: true, soort: null },
      { tekst: ' je.', treffer: false, actief: false, soort: null }
    ]);
    expect(deelTekst('Zonder.', [], -1)).toEqual([
      { tekst: 'Zonder.', treffer: false, actief: false, soort: null }
    ]);
  });

  it('knipt ook op markeringen van persoonsgegevens, met een treffer erbinnen', () => {
    const tekst = 'Bel 06 12345678 nu.';
    const treffers = vindTreffers([tekst], '1234');
    expect(deelTekst(tekst, treffers, 0, [{ soort: 'telefoon', van: 4, tot: 15 }])).toEqual([
      { tekst: 'Bel ', treffer: false, actief: false, soort: null },
      { tekst: '06 ', treffer: false, actief: false, soort: 'telefoon' },
      { tekst: '1234', treffer: true, actief: true, soort: 'telefoon' },
      { tekst: '5678', treffer: false, actief: false, soort: 'telefoon' },
      { tekst: ' nu.', treffer: false, actief: false, soort: null }
    ]);
  });

  it('maakt een lijst van de gemarkeerde stukken zonder dubbelen', () => {
    const regels: VerslagRegel[] = [
      {
        sleutel: 0,
        startMs: 0,
        eindMs: 1,
        tekst: 'Mail a@b.nl of A@B.nl.',
        markeringen: [
          { soort: 'email', van: 5, tot: 11 },
          { soort: 'email', van: 15, tot: 21 }
        ]
      },
      { sleutel: 1, startMs: 1, eindMs: 2, tekst: 'Geen.' }
    ];
    expect(gemarkeerdeStukken(regels)).toEqual([{ tekst: 'a@b.nl', soort: 'email', aantal: 2 }]);
  });

  it('loopt rond bij volgende en vorige', () => {
    expect(naarTreffer(0, 3, 1)).toBe(1);
    expect(naarTreffer(2, 3, 1)).toBe(0);
    expect(naarTreffer(0, 3, -1)).toBe(2);
    expect(naarTreffer(1, 3, -1)).toBe(0);
    expect(naarTreffer(0, 0, 1)).toBe(0);
  });
});

describe('bewerken', () => {
  it('Enter bewaart, Shift+Enter is een nieuwe regel en Esc annuleert', () => {
    expect(bewerkToets('Enter', false, false)).toBe('bewaar-volgende');
    expect(bewerkToets('Enter', true, false)).toBe('niets');
    expect(bewerkToets('Escape', false, false)).toBe('annuleer');
    expect(bewerkToets('a', false, false)).toBe('niets');
  });

  it('doet niets met Enter of Esc tijdens het samenstellen van een teken', () => {
    expect(bewerkToets('Enter', false, true)).toBe('niets');
    expect(bewerkToets('Escape', false, true)).toBe('niets');
  });

  it('kent de volgende regel, en niets na de laatste', () => {
    const regels: VerslagRegel[] = [
      { sleutel: 0, startMs: 0, eindMs: 1, tekst: 'a' },
      { sleutel: 2, startMs: 1, eindMs: 2, tekst: 'b' },
      { sleutel: 5, startMs: 2, eindMs: 3, tekst: 'c' }
    ];
    expect(volgendeSleutel(regels, 0)).toBe(2);
    expect(volgendeSleutel(regels, 2)).toBe(5);
    expect(volgendeSleutel(regels, 5)).toBeNull();
    expect(volgendeSleutel(regels, 9)).toBeNull();
  });
});

describe('afspelen', () => {
  it('kent drie snelheden en valt voor de rest terug op gewoon', () => {
    expect(alsSnelheid(10)).toBe(10);
    expect(alsSnelheid(15)).toBe(15);
    expect(alsSnelheid(20)).toBe(20);
    for (const onbekend of [0, 12, 30, '15', null, undefined, Number.NaN]) {
      expect(alsSnelheid(onbekend)).toBe(10);
    }
  });

  it('rekent de snelheid om naar een afspeelfactor', () => {
    expect(snelheidFactor(10)).toBe(1);
    expect(snelheidFactor(15)).toBe(1.5);
    expect(snelheidFactor(20)).toBe(2);
  });

  it('laat spatie met rust buiten de tekst, maar niet in een veld of op een knop', () => {
    // De testomgeving heeft geen document; zonder element speelt spatie af.
    expect(neemtSpatieOver(null)).toBe(false);
    expect(neemtSpatieOver({})).toBe(false);
  });
});

describe('vervangen in beeld', () => {
  const regel: VerslagRegel = {
    sleutel: 1,
    startMs: 0,
    eindMs: 1000,
    tekst: 'Mijn BSN is 111222333 en mail jan@x.nl.',
    markeringen: [
      { soort: 'bsn', van: 12, tot: 21 },
      { soort: 'email', van: 30, tot: 38 }
    ]
  };

  it('zet de plaatsvervanger op de plek van elk gemarkeerd stuk', () => {
    expect(vervangenRegel(regel).tekst).toBe('Mijn BSN is [BSN] en mail [E-MAIL].');
  });

  it('laat de markeringen naar de plaatsvervangers wijzen, zodat ze opvallen', () => {
    const nieuw = vervangenRegel(regel);
    const stukken = (nieuw.markeringen ?? []).map((m) => nieuw.tekst.slice(m.van, m.tot));
    expect(stukken).toEqual(['[BSN]', '[E-MAIL]']);
  });

  it('laat een regel zonder markeringen zoals hij is', () => {
    const kaal = { ...regel, markeringen: [] };
    expect(vervangenRegel(kaal)).toBe(kaal);
  });

  it('slaat een overlappende markering over in plaats van tekst dubbel te zetten', () => {
    const dubbel: VerslagRegel = {
      ...regel,
      markeringen: [
        { soort: 'bsn', van: 12, tot: 21 },
        { soort: 'handmatig', van: 15, tot: 25 }
      ]
    };
    expect(vervangenRegel(dubbel).tekst).toBe('Mijn BSN is [BSN] en mail jan@x.nl.');
  });

  it('verandert de echte regel niet', () => {
    vervangenRegel(regel);
    expect(regel.tekst).toBe('Mijn BSN is 111222333 en mail jan@x.nl.');
  });
});

describe('plaatsvervangers gelijk aan Rust', () => {
  // De logica blijft in TypeScript; deze test houdt de tabel gelijk aan `Soort::plaatsvervanger`
  // in Rust, zodat een nieuwe Soort of een gewijzigde tekst niet stil verschilt.
  const rust = readFileSync(
    fileURLToPath(new URL('../../../crates/mondig-core/src/persoonsgegevens.rs', import.meta.url)),
    'utf8'
  );
  const vanaf = rust.slice(rust.indexOf('fn plaatsvervanger'));
  const functie = vanaf.slice(0, vanaf.search(/\r?\n {4}\}/));
  const uitRust = Object.fromEntries(
    [...functie.matchAll(/Self::(\w+)\s*=>\s*"([^"]*)"/g)].map(([, naam, tekst]) => [
      (naam ?? '').toLowerCase(),
      tekst ?? ''
    ])
  );

  it('vindt de tabel in het Rust-bronbestand', () => {
    expect(Object.keys(uitRust).length).toBeGreaterThan(0);
  });

  it('heeft elke plaatsvervanger uit Rust, met dezelfde tekst', () => {
    for (const [soort, tekst] of Object.entries(uitRust)) {
      expect(PLAATSVERVANGER, `soort ${soort}`).toHaveProperty(soort, tekst);
    }
  });

  it('heeft geen soort die in Rust niet bestaat', () => {
    expect(Object.keys(PLAATSVERVANGER).sort()).toEqual(Object.keys(uitRust).sort());
  });
});
