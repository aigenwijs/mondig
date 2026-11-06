import { describe, expect, it, vi } from 'vitest';
import { t, zetTaal } from '#lib';
import {
  MISTRAL_NAUWKEURIG,
  bevestigingGeldt,
  bewaarSleutel,
  deelTekst,
  modelNaam,
  notulenKlaar,
  plekTekst,
  springNaarBron,
  voortgangProcent,
  type NotulenInstellingen
} from './notulen';

const mistral = (extra: Partial<NotulenInstellingen> = {}): NotulenInstellingen => ({
  aanbieder: 'mistral',
  mistralModel: MISTRAL_NAUWKEURIG,
  ollamaModel: null,
  trainingUit: true,
  sleutelStaat: true,
  sleutelFout: null,
  ...extra
});

describe('kan Mondig notulen maken met deze instellingen', () => {
  it('weet het niet zolang de instellingen laden', () => {
    expect(notulenKlaar(null)).toBeNull();
  });

  it('maakt niets zonder gekozen aanbieder', () => {
    expect(notulenKlaar(mistral({ aanbieder: null }))).toBe('niet-ingesteld');
  });

  it('eist bij Mistral het vinkje over training en een sleutel', () => {
    expect(notulenKlaar(mistral({ trainingUit: false }))).toBe('training');
    expect(notulenKlaar(mistral({ sleutelStaat: false }))).toBe('sleutel');
    expect(notulenKlaar(mistral({ trainingUit: false, sleutelStaat: false }))).toBe('training');
    expect(notulenKlaar(mistral())).toBe('ok');
  });

  it('eist bij Ollama een model, en geen sleutel of vinkje', () => {
    const ollama = mistral({ aanbieder: 'ollama', trainingUit: false, sleutelStaat: false });
    expect(notulenKlaar(ollama)).toBe('model');
    expect(notulenKlaar({ ...ollama, ollamaModel: 'qwen3:8b' })).toBe('ok');
  });
});

describe('de sleutel van Mistral', () => {
  it('is na bewaren uit het veld en komt niet terug in wat Rust geeft', async () => {
    const zet = vi.fn(() => Promise.resolve(mistral()));
    const uitslag = await bewaarSleutel('geheime-sleutel-123', zet);
    expect(zet).toHaveBeenCalledWith('geheime-sleutel-123');
    expect(uitslag.veld).toBe('');
    expect(uitslag.fout).toBeNull();
    // De instellingen hebben geen veld dat een sleutel kan bevatten, alleen of er een staat.
    expect(JSON.stringify(uitslag.instellingen)).not.toContain('geheime-sleutel-123');
    expect(Object.keys(uitslag.instellingen ?? {}).sort()).toEqual([
      'aanbieder',
      'mistralModel',
      'ollamaModel',
      'sleutelFout',
      'sleutelStaat',
      'trainingUit'
    ]);
  });

  it('blijft bij een fout in het veld staan, zodat de gebruiker hem kan verbeteren', async () => {
    const fout = { melding: 'Dit lijkt geen geldige sleutel.', vervolg: 'geen' };
    const uitslag = await bewaarSleutel('twee woorden', () =>
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- een Foutmelding, zoals Rust die geeft
      Promise.reject(fout)
    );
    expect(uitslag.veld).toBe('twee woorden');
    expect(uitslag.instellingen).toBeNull();
    expect(uitslag.fout).toBe(fout);
  });
});

describe('de bronknop', () => {
  it('springt naar de beurt van het punt', () => {
    const spring = vi.fn();
    expect(springNaarBron({ beurt: 3 }, spring)).toBe(true);
    expect(spring).toHaveBeenCalledWith(3);
  });

  it('springt nergens heen zonder beurt (niet teruggevonden of een vorige versie)', () => {
    const spring = vi.fn();
    expect(springNaarBron({ beurt: null }, spring)).toBe(false);
    expect(spring).not.toHaveBeenCalled();
  });

  it('springt ook naar de eerste beurt (index 0)', () => {
    const spring = vi.fn();
    expect(springNaarBron({ beurt: 0 }, spring)).toBe(true);
    expect(spring).toHaveBeenCalledWith(0);
  });
});

describe('de voortgang van het maken', () => {
  it('noemt het deel waar Mondig mee bezig is', () => {
    zetTaal('nl');
    expect(deelTekst({ klaar: 1, totaal: 4 }, t())).toBe('Deel 2 van 4');
    expect(deelTekst({ klaar: 4, totaal: 4 }, t())).toBe('Deel 4 van 4');
  });

  it('zegt bij één deel of een onbekend aantal alleen dat Mondig leest', () => {
    zetTaal('nl');
    expect(deelTekst({ klaar: 0, totaal: 0 }, t())).toBe('Mondig leest het verslag.');
    expect(deelTekst({ klaar: 0, totaal: 1 }, t())).toBe('Mondig leest het verslag.');
  });

  it('rekent een percentage pas uit als er meer delen zijn', () => {
    expect(voortgangProcent({ klaar: 0, totaal: 0 })).toBeNull();
    expect(voortgangProcent({ klaar: 0, totaal: 1 })).toBeNull();
    expect(voortgangProcent({ klaar: 1, totaal: 4 })).toBe(25);
    expect(voortgangProcent({ klaar: 9, totaal: 4 })).toBe(100);
  });

  it('noemt de bekende modellen bij hun naam', () => {
    expect(modelNaam('mistral-large-latest')).toBe('Mistral Large');
    expect(modelNaam('mistral-small-latest')).toBe('Mistral Small');
    expect(modelNaam('qwen3:8b')).toBe('qwen3:8b');
  });
});

describe('de bevestiging waar de tekst heen gaat', () => {
  const eerder = { taak: 7, aanbieder: 'mistral' as const, vervangen: false };

  it('geldt voor dezelfde aanbieder en dezelfde stand van Vervangen in hetzelfde verslag', () => {
    expect(bevestigingGeldt(eerder, 7, 'mistral', false)).toBe(true);
  });

  it('vervalt als de aanbieder, Vervangen of het verslag anders is, of er niets bevestigd was', () => {
    expect(bevestigingGeldt(eerder, 7, 'ollama', false)).toBe(false);
    expect(bevestigingGeldt(eerder, 7, 'mistral', true)).toBe(false);
    expect(bevestigingGeldt(eerder, 8, 'mistral', false)).toBe(false);
    expect(bevestigingGeldt(null, 7, 'mistral', false)).toBe(false);
    expect(bevestigingGeldt(eerder, 7, null, false)).toBe(false);
  });
});

describe('waar de notulen gemaakt zijn', () => {
  it('noemt het label bij Mistral en bij Ollama op deze computer', () => {
    zetTaal('nl');
    expect(plekTekst('mistral', MISTRAL_NAUWKEURIG, t())).toBe('Mistral Large (Europees)');
    expect(plekTekst('ollama', 'qwen3:8b', t())).toBe('qwen3:8b op deze computer (Lokaal)');
  });
});
