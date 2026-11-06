import { describe, expect, it } from 'vitest';
import {
  aangevinkt,
  heeftIetsTeMelden,
  ruimteMelding,
  adviesRegels,
  controleDatum,
  downloadZin,
  heeftVerbeteringen,
  overgenomenTekst,
  type AdviesOmgeving
} from './afstellen';
import { standaardAdvies, type ComputerInfo } from './computer';
import { talen } from './i18n/index.svelte';
import type { ModelRegel } from './modellen';

const nl = talen.nl;
const ULTRA = 'parakeet-ultra-q8';
const CANARY = 'canary-1b-v2-q8';

function model(id: string, grootteBytes: number, aanwezigBytes: number): ModelRegel {
  return {
    id,
    grootteBytes,
    aanwezigBytes,
    status:
      aanwezigBytes >= grootteBytes ? 'compleet' : aanwezigBytes > 0 ? 'gedeeltelijk' : 'ontbreekt'
  } as ModelRegel;
}

/** Een computer van 16 GB met een losse kaart; Ultra staat er, Canary nog niet. */
function info(wijzig: (i: ComputerInfo) => void = () => undefined): ComputerInfo {
  const basis = standaardAdvies();
  basis.advies = {
    ...basis.advies,
    geheugen: 'ruim',
    dicteren: ULTRA,
    bestanden: CANARY,
    bestandenApart: true,
    videokaart: { stand: 'aanzetten', keerSneller: 9.2 }
  };
  basis.dicterenInGebruik = ULTRA;
  basis.bestandenInGebruik = ULTRA;
  wijzig(basis);
  return basis;
}

function omgeving(wijzig: Partial<AdviesOmgeving> = {}): AdviesOmgeving {
  return {
    kaartAan: false,
    modellen: [model(ULTRA, 740_000_000, 740_000_000), model(CANARY, 1_144_000_000, 0)],
    loopt: () => false,
    taal: 'nl',
    ...wijzig
  };
}

describe('de regels van het advies', () => {
  it('zet elke verbetering aan en laat wat al goed staat zonder vinkje', () => {
    const regels = adviesRegels(info(), omgeving(), nl);
    expect(regels.map((r) => [r.sleutel, r.invoer !== null])).toEqual([
      ['kaart', true],
      ['bestanden', true],
      ['dicteren', false]
    ]);
    expect(regels[0]?.titel).toBe('Videokaart gebruiken');
    expect(regels[0]?.uitleg).toBe('Uitschrijven gaat ongeveer 9,2 keer zo snel.');
    expect(regels[1]?.titel).toBe('Nauwkeurig model voor opnames en bestanden');
    // De grootte staat erbij, een tijd niet (Mondig meet de verbinding niet).
    expect(regels[1]?.uitleg).toBe('Minder fouten in verslagen. Downloaden: 1,1 GB.');
    expect(regels[2]?.titel).toBe('Dicteren blijft zoals het is');
    // Geen modelnamen: die staan alleen op Modellen.
    expect(regels[2]?.uitleg).toBe('Het snelle model past al het beste.');
    for (const r of regels) expect(r.titel + r.uitleg).not.toMatch(/Parakeet|Canary|Whisper/);
    expect(heeftVerbeteringen(regels)).toBe(true);
  });

  it('stuurt per aangevinkt punt het juiste naar Rust', () => {
    const regels = adviesRegels(info(), omgeving(), nl);
    expect(aangevinkt(regels, new Set())).toEqual([
      { soort: 'videokaart' },
      { soort: 'model', gebruik: 'bestanden', model: CANARY }
    ]);
    // Een weggehaald vinkje valt uit de opdracht; zonder vinkjes is er niets te doen.
    expect(aangevinkt(regels, new Set(['kaart']))).toEqual([
      { soort: 'model', gebruik: 'bestanden', model: CANARY }
    ]);
    expect(aangevinkt(regels, new Set(['kaart', 'bestanden']))).toEqual([]);
  });

  it('zegt "Alles staat al goed" als er niets te verbeteren valt', () => {
    const goed = info((i) => {
      i.advies.videokaart = { stand: 'geen', keerSneller: null };
      i.bestandenInGebruik = CANARY;
    });
    const regels = adviesRegels(goed, omgeving(), nl);
    expect(heeftVerbeteringen(regels)).toBe(false);
    expect(regels.map((r) => r.sleutel)).toEqual(['dicteren', 'bestanden']);
    expect(aangevinkt(regels, new Set())).toEqual([]);
  });

  it('raadt de videokaart niet aan als hij al aan staat of niet sneller is', () => {
    const aan = adviesRegels(info(), omgeving({ kaartAan: true }), nl);
    expect(aan.find((r) => r.sleutel === 'kaart')?.invoer).toBeNull();
    const langzaam = info((i) => {
      i.advies.videokaart = { stand: 'laten', keerSneller: 0.8 };
    });
    const regels = adviesRegels(langzaam, omgeving(), nl);
    expect(regels.find((r) => r.sleutel === 'kaart')?.titel).toBe('De videokaart blijft uit');
    expect(regels.find((r) => r.sleutel === 'kaart')?.invoer).toBeNull();
  });

  it('stelt een ander dicteermodel voor als Mondig nu iets anders gebruikt', () => {
    const anders = info((i) => {
      i.dicterenInGebruik = CANARY;
    });
    const regels = adviesRegels(anders, omgeving(), nl);
    const dicteren = regels.find((r) => r.sleutel === 'dicteren');
    expect(dicteren?.invoer).toEqual({ soort: 'model', gebruik: 'dicteren', model: ULTRA });
    expect(dicteren?.uitleg).toContain('Staat al op je computer.');
  });

  it('raadt geen zwaarder model aan op een computer met weinig geheugen', () => {
    const weinig = info((i) => {
      i.advies.geheugen = 'weinig';
      i.advies.bestanden = ULTRA;
      i.advies.bestandenApart = false;
      i.bestandenInGebruik = CANARY;
      i.advies.videokaart = { stand: 'geen', keerSneller: null };
    });
    const regels = adviesRegels(weinig, omgeving(), nl);
    // Wie bewust het nauwkeurige model koos, krijgt geen "downgrade" voorgesteld.
    expect(regels.find((r) => r.sleutel === 'bestanden')).toBeUndefined();
    expect(heeftVerbeteringen(regels)).toBe(false);
  });
});

describe('wat een model kost', () => {
  it('noemt de grootte van wat nog binnen moet komen, niet van het hele model', () => {
    const gedeeltelijk = omgeving({
      modellen: [model(CANARY, 1_144_000_000, 144_000_000)]
    });
    expect(downloadZin(CANARY, gedeeltelijk, nl)).toBe('Downloaden: 1,0 GB.');
  });

  it('zegt dat het model er al staat of al binnenkomt', () => {
    expect(downloadZin(ULTRA, omgeving(), nl)).toBe('Staat al op je computer.');
    expect(downloadZin(CANARY, omgeving({ loopt: (id) => id === CANARY }), nl)).toBe(
      'Wordt al gedownload.'
    );
  });

  it('zwijgt zolang het overzicht van de modellen er niet is', () => {
    expect(downloadZin(CANARY, omgeving({ modellen: null }), nl)).toBe('');
  });
});

describe('wat er overgenomen is', () => {
  it('noemt de videokaart, een model dat meteen gebruikt wordt en een download', () => {
    const tekst = overgenomenTekst(
      {
        videokaartAan: true,
        gewisseld: [{ gebruik: 'dicteren', model: ULTRA }],
        downloadt: [{ model: CANARY, taken: ['bestanden'] }]
      },
      nl
    );
    expect(tekst).toBe(
      'De videokaart staat aan. Mondig dicteert nu met het snelle model. Het nauwkeurige model wordt gedownload; tot het klaar is, gebruikt Mondig het huidige model.'
    );
  });

  it('zegt eerlijk dat er niets veranderd is', () => {
    expect(overgenomenTekst({ videokaartAan: false, gewisseld: [], downloadt: [] }, nl)).toBe(
      'Er is niets veranderd.'
    );
  });
});

describe('de datum van de controle', () => {
  it('schrijft dag en maand uit', () => {
    expect(controleDatum(new Date(2026, 10, 4), 'nl')).toBe('4 november');
    expect(controleDatum(new Date(2026, 10, 4), 'en')).toBe('4 November');
  });
});

describe('de videokaart als open punt', () => {
  it('maakt van een mislukte snelheidstest nooit "Alles staat al goed"', () => {
    const mislukt = info((i) => {
      i.advies.videokaart = { stand: 'testen', keerSneller: null };
      i.bestandenInGebruik = CANARY;
    });
    const regels = adviesRegels(
      mislukt,
      omgeving({ kaartFout: 'Mondig schrijft nu iets uit.' }),
      nl
    );
    expect(heeftVerbeteringen(regels)).toBe(false);
    expect(heeftIetsTeMelden(regels)).toBe(true);
    expect(regels[0]?.titel).toBe('De videokaart is niet getest');
    expect(regels[0]?.uitleg).toBe(
      'Mondig kon de videokaart niet testen: Mondig schrijft nu iets uit. Probeer het later opnieuw.'
    );
  });

  it('meldt ook een videokaart die in de proef een fout gaf', () => {
    const faalde = info((i) => {
      i.advies.videokaart = { stand: 'faalde', keerSneller: null };
      i.bestandenInGebruik = CANARY;
    });
    const regels = adviesRegels(faalde, omgeving(), nl);
    expect(heeftIetsTeMelden(regels)).toBe(true);
    expect(regels[0]?.uitleg).toContain('gaf een fout');
  });
});

describe('ruimte voor de aangevinkte downloads', () => {
  const GB = 1_000_000_000;
  const tweeModellen = (): AdviesOmgeving =>
    omgeving({
      modellen: [model(ULTRA, 740_000_000, 0), model(CANARY, 1_144_000_000, 0)]
    });
  const beide = (): ReturnType<typeof adviesRegels> =>
    adviesRegels(
      info((i) => {
        i.dicterenInGebruik = CANARY;
      }),
      tweeModellen(),
      nl
    );

  it('zegt vooraf dat beide modellen samen niet passen', () => {
    const regels = beide();
    expect(ruimteMelding(regels, new Set(), 2 * GB, tweeModellen(), nl)).toBe(
      'Er is niet genoeg ruimte voor beide modellen; maak ruimte vrij of kies er één.'
    );
  });

  it('laat één model toe als alleen dat past, en alles bij genoeg ruimte', () => {
    const regels = beide();
    expect(ruimteMelding(regels, new Set(['dicteren']), 2 * GB, tweeModellen(), nl)).toBeNull();
    expect(ruimteMelding(regels, new Set(), 10 * GB, tweeModellen(), nl)).toBeNull();
    // Niet te meten: Rust controleert het bij het overnemen.
    expect(ruimteMelding(regels, new Set(), null, tweeModellen(), nl)).toBeNull();
  });

  it('telt een download die al loopt mee', () => {
    const regels = adviesRegels(info(), tweeModellen(), nl);
    const loopt = { ...tweeModellen(), loopt: (id: string) => id === ULTRA };
    expect(ruimteMelding(regels, new Set(['kaart']), 2 * GB, loopt, nl)).toBe(
      'Er is niet genoeg ruimte voor beide modellen; maak ruimte vrij of kies er één.'
    );
  });
});
