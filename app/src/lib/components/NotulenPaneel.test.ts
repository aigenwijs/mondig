import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import {
  MISTRAL_NAUWKEURIG,
  type NotulenInfo,
  type NotulenInstellingen,
  type NotulenStand
} from '#lib/notulen.js';
import NotulenPaneel from './NotulenPaneel.svelte';

// Het tabblad Notulen (T3, ADR 0026). Servermodus: de HTML is wat oog en schermlezer krijgen; wat
// een klik doet, staat in notulen.test.ts (springNaarBron).

const ingesteld: NotulenInstellingen = {
  aanbieder: 'mistral',
  mistralModel: MISTRAL_NAUWKEURIG,
  ollamaModel: null,
  trainingUit: true,
  sleutelStaat: true,
  sleutelFout: null
};

const bron = (beurt: number | null, ms: number | null, gevonden = true) => ({
  gevonden,
  beurt,
  ms,
  citaat: 'een letterlijk citaat uit het verslag'
});

function notulen(extra: Partial<NotulenInfo> = {}): NotulenInfo {
  return {
    samenvatting: 'Het gaat over de buurtbijeenkomst.',
    samenvattingGetoond: 'Het gaat over de buurtbijeenkomst.',
    besluiten: [
      {
        id: 1,
        tekst: 'De bijeenkomst is op donderdag.',
        tekstGetoond: 'De bijeenkomst is op donderdag.',
        bron: bron(1, 28_000)
      },
      {
        id: 2,
        tekst: 'De gemeente betaalt de zaal.',
        tekstGetoond: 'De gemeente betaalt de zaal.',
        bron: bron(null, null, false)
      }
    ],
    actiepunten: [
      {
        id: 1,
        wat: 'Een uitnodiging sturen',
        wie: 'Jan',
        wanneer: '',
        watGetoond: 'Een uitnodiging sturen',
        wieGetoond: 'Jan',
        wanneerGetoond: '',
        wieNietTeruggevonden: true,
        wanneerNietTeruggevonden: false,
        bron: bron(6, 63_000)
      }
    ],
    aanbieder: 'mistral',
    model: MISTRAL_NAUWKEURIG,
    momentMs: new Date(2026, 10, 5, 14, 30).getTime(),
    taal: 'nl',
    vorigeVersie: false,
    gevonden: 2,
    nietGevonden: 1,
    ...extra
  };
}

const leeg: NotulenStand = { info: null, onleesbaar: false, maken: null };

function html(
  stand: NotulenStand,
  props: Partial<{ instellingen: NotulenInstellingen | null; bevestigd: boolean }> = {}
): string {
  return render(NotulenPaneel, {
    props: {
      stand,
      instellingen: ingesteld,
      vervangen: false,
      bevestigd: false,
      maak: vi.fn(),
      stop: vi.fn(),
      spring: vi.fn(),
      kopieer: vi.fn(),
      pasAan: vi.fn(() => Promise.resolve(null)),
      haalWeg: vi.fn(() => Promise.resolve(null)),
      ...props
    }
  }).body;
}

describe('het tabblad Notulen, leeg', () => {
  it('heeft één knop "Notulen maken" als alles ingesteld is', () => {
    const tekst = html(leeg);
    expect(tekst).toContain('Nog geen notulen');
    expect(tekst.match(/>\s*Notulen maken\s*</g)).toHaveLength(1);
    expect(tekst).not.toContain('Notulen instellen');
  });

  it('verwijst naar Instellingen als er niets is ingesteld', () => {
    const tekst = html(leeg, { instellingen: { ...ingesteld, aanbieder: null } });
    expect(tekst).toContain('Notulen maken is nog niet ingesteld.');
    expect(tekst).toContain('Notulen instellen');
    expect(tekst).toContain('/instellingen#notulen');
    expect(tekst).not.toMatch(/>\s*Notulen maken\s*</);
  });

  it('verwijst ook naar Instellingen als het vinkje over training of de sleutel ontbreekt', () => {
    for (const extra of [{ trainingUit: false }, { sleutelStaat: false }]) {
      const tekst = html(leeg, { instellingen: { ...ingesteld, ...extra } });
      expect(tekst).toContain('nog niet klaar voor gebruik');
      expect(tekst).toContain('Notulen instellen');
    }
  });

  it('zegt dat onleesbare notulen bewaard blijven', () => {
    expect(html({ ...leeg, onleesbaar: true })).toContain('Ze blijven bewaard.');
  });
});

describe('het tabblad Notulen, bezig', () => {
  it('toont het deel en Stoppen, en geen notulen', () => {
    const tekst = html({ ...leeg, maken: { klaar: 1, totaal: 3 } });
    expect(tekst).toContain('Mondig maakt de notulen');
    expect(tekst).toContain('Deel 2 van 3');
    expect(tekst).toContain('Stoppen');
    expect(tekst).toContain('role="progressbar"');
    expect(tekst).toContain('aria-valuenow="33"');
    expect(tekst).not.toContain('Nog geen notulen');
  });

  it('heeft bij één deel geen voortgangsbalk', () => {
    const tekst = html({ ...leeg, maken: { klaar: 0, totaal: 1 } });
    expect(tekst).toContain('Mondig leest het verslag.');
    expect(tekst).not.toContain('role="progressbar"');
  });
});

describe('het tabblad Notulen, klaar', () => {
  const tekst = html({ info: notulen(), onleesbaar: false, maken: null });

  it('toont samenvatting, besluiten en actiepunten', () => {
    expect(tekst).toContain('Samenvatting');
    expect(tekst).toContain('Gemaakt door een AI-dienst, lees na.');
    expect(tekst).toContain('Het gaat over de buurtbijeenkomst.');
    expect(tekst).toContain('Besluiten');
    expect(tekst).toContain('De bijeenkomst is op donderdag.');
    expect(tekst).toContain('Actiepunten');
    expect(tekst).toContain('Een uitnodiging sturen');
  });

  it('heeft per gevonden punt een bronknop met de tijd', () => {
    expect(tekst).toContain('▸');
    expect(tekst).toContain('0:28');
    expect(tekst).toContain('aria-label="Ga naar 0:28 in het verslag"');
    expect(tekst).toContain('aria-label="Ga naar 1:03 in het verslag"');
    // Het citaat staat in de tooltip, zodat de gebruiker ziet wat het model aanhaalde.
    expect(tekst).toContain('Het model citeerde: een letterlijk citaat uit het verslag');
  });

  it('toont de vervangen tekst, niet het echte gegeven, als Rust die als getoond geeft', () => {
    const info = notulen({
      samenvatting: 'Mijn BSN is 111222333.',
      samenvattingGetoond: 'Mijn BSN is [BSN].',
      besluiten: [
        {
          id: 1,
          tekst: 'Bel 06 12345678.',
          tekstGetoond: 'Bel [TELEFOON].',
          bron: { ...bron(1, 28_000), citaat: 'Bel [TELEFOON].' }
        }
      ],
      actiepunten: [
        {
          id: 1,
          wanneer: '',
          wanneerGetoond: '',
          wieNietTeruggevonden: false,
          wanneerNietTeruggevonden: false,
          bron: bron(6, 63_000),
          wat: 'Mail jan@example.com',
          watGetoond: 'Mail [E-MAIL]',
          wie: 'Anna',
          wieGetoond: '[GEGEVEN]'
        }
      ]
    });
    const uit = html({ info, onleesbaar: false, maken: null });
    for (const echt of ['111222333', '06 12345678', 'jan@example.com', 'Anna']) {
      expect(uit).not.toContain(echt);
    }
    for (const getoond of ['[BSN]', '[TELEFOON]', '[E-MAIL]', '[GEGEVEN]']) {
      expect(uit).toContain(getoond);
    }
  });

  it('markeert een niet teruggevonden punt en zet er geen bronknop bij', () => {
    expect(tekst).toContain('Niet teruggevonden in het verslag');
    expect(tekst).toContain('1 punt is niet teruggevonden in het verslag. Controleer dat punt.');
    expect(tekst.match(/aria-label="Ga naar/g)).toHaveLength(2);
  });

  it('markeert een wie die niet in het verslag staat en zegt "niet genoemd" bij leeg', () => {
    expect(tekst).toContain('niet in het verslag gevonden');
    expect(tekst).toContain('niet genoemd');
    expect(tekst).toContain('Jan');
  });

  it('heeft per punt Aanpassen en Weghalen', () => {
    expect(tekst).toContain('aria-label="Besluit aanpassen"');
    expect(tekst).toContain('aria-label="Besluit weghalen"');
    expect(tekst).toContain('aria-label="Actiepunt aanpassen"');
    expect(tekst).toContain('aria-label="Actiepunt weghalen"');
  });

  it('eindigt met wie het maakte en wanneer, Kopiëren en Opnieuw maken', () => {
    expect(tekst).toMatch(/Gemaakt met Mistral Large \(Europees\) · 5 november/);
    expect(tekst).toContain('Kopiëren');
    expect(tekst).toContain('Opnieuw maken');
  });

  it('houdt de bronknop minstens 32 pixel groot', () => {
    expect(tekst).toContain('min-h-8 min-w-8');
  });
});

describe('waar de notulen gemaakt zijn', () => {
  it('zegt bij Ollama dat het op deze computer was, met het label Lokaal', () => {
    const tekst = html({
      info: notulen({ aanbieder: 'ollama', model: 'qwen3:8b' }),
      onleesbaar: false,
      maken: null
    });
    expect(tekst).toMatch(/Gemaakt met qwen3:8b op deze computer \(Lokaal\) · 5 november/);
  });

  it('vraagt bij onleesbare notulen eerst of ze overschreven mogen worden', () => {
    // De vraag staat pas na een klik in beeld; de knop erachter is nu al "Notulen maken".
    const tekst = html({ info: null, onleesbaar: true, maken: null });
    expect(tekst).toContain('Ze blijven bewaard.');
  });
});

describe('het tabblad Notulen, gemaakt bij de vorige versie van het verslag', () => {
  const vorige = notulen({
    vorigeVersie: true,
    besluiten: [
      {
        id: 1,
        tekst: 'De bijeenkomst is op donderdag.',
        tekstGetoond: 'De bijeenkomst is op donderdag.',
        bron: bron(null, null, true)
      }
    ],
    actiepunten: [],
    nietGevonden: 0
  });
  const tekst = html({ info: vorige, onleesbaar: false, maken: null });

  it('toont de balk met Opnieuw maken', () => {
    expect(tekst).toContain('Gemaakt bij de vorige versie van het verslag.');
    expect(tekst).toContain('Opnieuw maken');
  });

  it('heeft geen klikbare bronknoppen meer en geen oranje markering voor gevonden punten', () => {
    expect(tekst).not.toContain('aria-label="Ga naar');
    expect(tekst).not.toContain('Niet teruggevonden in het verslag');
  });
});
