import { describe, expect, it } from 'vitest';
import type { LaatsteControle } from './computer';
import {
  beheerIndeling,
  pastBijComputer,
  passendVoor,
  kiesbaarVoor,
  rijStand,
  talenSamenvatting,
  type ModelRegel
} from './modellen';

function regel(extra: Partial<ModelRegel> & { id: string }): ModelRegel {
  return {
    naam: extra.id,
    soort: 'spraak',
    rol: null,
    nauwkeurigheid: 4,
    snelheid: 4,
    talen: ['nl', 'en'],
    status: 'ontbreekt',
    grootteBytes: 740_000_000,
    aanwezigBytes: 0,
    tekortBytes: 0,
    nietVoor: [],
    inGebruikVoor: [],
    kanWeg: false,
    nieuw: false,
    bron: 'huggingface.co',
    daarnaBijHervatten: [],
    ...extra
  };
}

const ultra = (extra: Partial<ModelRegel> = {}): ModelRegel =>
  regel({
    id: 'parakeet-ultra-q8',
    ...extra
  });
const canary = (extra: Partial<ModelRegel> = {}): ModelRegel =>
  regel({
    id: 'canary-1b-v2-q8',
    grootteBytes: 1_145_000_000,
    ...extra
  });
const whisper = (extra: Partial<ModelRegel> = {}): ModelRegel =>
  regel({
    id: 'whisper-large-v3-turbo-q8',
    nietVoor: ['dicteren'],
    ...extra
  });

describe('talen', () => {
  it('telt de talen en herkent Turks en Arabisch', () => {
    expect(talenSamenvatting(ultra())).toEqual({ aantal: 2, turksArabisch: false });
    expect(talenSamenvatting(whisper({ talen: ['nl', 'tr', 'ar'] }))).toEqual({
      aantal: 3,
      turksArabisch: true
    });
    expect(talenSamenvatting(whisper({ talen: ['nl', 'tr'] })).turksArabisch).toBe(false);
  });
});

describe('welke taken een model mag', () => {
  it('Whisper turbo is niet te kiezen om mee te dicteren', () => {
    expect(kiesbaarVoor(whisper(), 'dicteren')).toBe(false);
    expect(kiesbaarVoor(whisper(), 'bestanden')).toBe(true);
    expect(kiesbaarVoor(ultra(), 'dicteren')).toBe(true);
  });
});

describe('wat een rij toont', () => {
  it('biedt een model dat er niet staat aan om gewoon te downloaden', () => {
    const stand = rijStand(canary(), false);
    expect(stand.download).toBe('gewoon');
    expect(stand.verwijderen).toBeNull();
  });

  it('hervat een afgebroken download en laat het deel opruimen', () => {
    const stand = rijStand(canary({ status: 'gedeeltelijk', aanwezigBytes: 400_000_000 }), false);
    expect(stand.download).toBe('verder');
    expect(stand.verwijderen).toBe('uit');
    expect(rijStand(canary({ status: 'gedeeltelijk', kanWeg: true }), false).verwijderen).toBe(
      'aan'
    );
  });

  it('toont de stand bij een model in de rij, en laat een ander model gewoon starten', () => {
    const binnen = rijStand(canary(), true);
    expect(binnen.bezig).toBe(true);
    expect(binnen.download).toBeNull();
    expect(binnen.verwijderen).toBeNull();
    // Een ander model mag erachter in de rij: de knop blijft er.
    const ander = rijStand(whisper(), false);
    expect(ander.bezig).toBe(false);
    expect(ander.download).toBe('gewoon');
  });

  it('laat de knop staan maar meldt dat er te weinig ruimte is', () => {
    const stand = rijStand(canary({ tekortBytes: 500_000_000 }), false);
    expect(stand.teWeinigRuimte).toBe(true);
    expect(stand.download).toBe('gewoon');
    expect(rijStand(canary({ tekortBytes: 500_000_000 }), true).teWeinigRuimte).toBe(false);
  });

  it('laat verder downloaden afmaken wat de eerste start begon', () => {
    expect(rijStand(ultra({ status: 'gedeeltelijk', aanwezigBytes: 100 }), false).download).toBe(
      'verder'
    );
  });

  it('laat een model dat niet in gebruik is weghalen', () => {
    expect(rijStand(whisper({ status: 'compleet', kanWeg: true }), false).verwijderen).toBe('aan');
  });

  it('zet "Verwijderen" uit bij een model in gebruik', () => {
    const stand = rijStand(
      ultra({ status: 'compleet', inGebruikVoor: ['dicteren'], kanWeg: false }),
      false
    );
    expect(stand.verwijderen).toBe('uit');
  });
});

describe('Wijzigen per functie (na de gebruikerstest van N-C)', () => {
  const lijst = [
    regel({ id: 'ultra', rol: 'snel' }),
    regel({ id: 'canary', rol: 'nauwkeurig' }),
    regel({ id: 'whisper', rol: 'extra_nauwkeurig', nietVoor: ['dicteren'] }),
    regel({ id: 'sortformer', soort: 'sprekers', nietVoor: ['dicteren', 'bestanden'] })
  ];

  it('toont bij Dicteren alleen modellen die snel genoeg zijn om te dicteren', () => {
    expect(passendVoor(lijst, 'dicteren').map((m) => m.id)).toEqual(['ultra', 'canary']);
  });

  it('toont bij Opnames en bestanden alle spraakmodellen voor bestanden, niet het sprekermodel', () => {
    expect(passendVoor(lijst, 'bestanden').map((m) => m.id)).toEqual([
      'ultra',
      'canary',
      'whisper'
    ]);
  });

  it('zet het sprekermodel in de beheerlijst achter de spraakmodellen', () => {
    const beheer = beheerIndeling([
      regel({ id: 'sortformer', soort: 'sprekers', status: 'compleet' }),
      regel({ id: 'ultra', status: 'compleet' }),
      regel({ id: 'whisper' })
    ]);
    expect(beheer.opDezeComputer.map((m) => m.id)).toEqual(['ultra', 'sortformer']);
    expect(beheer.teDownloaden.map((m) => m.id)).toEqual(['whisper']);
  });
});

describe('Past bij deze computer', () => {
  const controle: LaatsteControle = {
    dicteren: 'ultra',
    bestanden: 'canary',
    videokaart: 'laten',
    datumMs: 0
  };

  it('staat bij het model dat de laatste controle voor die functie aanraadt', () => {
    expect(pastBijComputer(controle, 'dicteren', 'ultra')).toBe(true);
    expect(pastBijComputer(controle, 'bestanden', 'canary')).toBe(true);
    expect(pastBijComputer(controle, 'dicteren', 'canary')).toBe(false);
    expect(pastBijComputer(controle, 'bestanden', 'ultra')).toBe(false);
  });

  it('staat nergens zonder controle', () => {
    expect(pastBijComputer(null, 'dicteren', 'ultra')).toBe(false);
    expect(pastBijComputer(null, 'bestanden', 'canary')).toBe(false);
  });
});
