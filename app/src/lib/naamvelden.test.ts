import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  gelijkeNaamvelden,
  maakNaamveldenBeheer,
  VOORBEELD_WACHT_MS,
  type Naamvelden,
  type NaamveldenWerk
} from './naamvelden';

// De kaart "Naam van het bestand" (UI-E): het voorbeeld volgt de invoer met een korte
// vertraging, en bewaren gebeurt bij blur, Enter of een keuze, niet bij elke toets.

const start: Naamvelden = { klant: '', onderwerp: 'Interview', datum: '2026-10-30' };

function opzet(bewaarUitkomst: string | null = null) {
  const werk = {
    haalVoorbeeld: vi.fn((v: Naamvelden) =>
      Promise.resolve(`${v.datum} ${v.klant} ${v.onderwerp}.docx`.replace('  ', ' '))
    ),
    bewaar: vi.fn((): Promise<string | null> => Promise.resolve(bewaarUitkomst)),
    zetVoorbeeld: vi.fn(),
    zetFout: vi.fn()
  } satisfies NaamveldenWerk;
  return { werk, beheer: maakNaamveldenBeheer(werk, start) };
}

describe('het voorbeeld van de bestandsnaam', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('volgt de invoer, na een korte vertraging', async () => {
    const { werk, beheer } = opzet();
    beheer.invoer({ ...start, klant: 'Gem' });
    // Nog niet: Rust wordt niet bij elke toets gevraagd.
    expect(werk.haalVoorbeeld).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS);
    expect(werk.haalVoorbeeld).toHaveBeenCalledTimes(1);
    expect(werk.zetVoorbeeld).toHaveBeenLastCalledWith('2026-10-30 Gem Interview.docx');
  });

  it('wacht met vragen zolang er verder wordt getypt en vraagt dan één keer', async () => {
    const { werk, beheer } = opzet();
    for (const klant of ['G', 'Ge', 'Gem', 'Geme']) {
      beheer.invoer({ ...start, klant });
      await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS - 50);
    }
    expect(werk.haalVoorbeeld).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(50);
    expect(werk.haalVoorbeeld).toHaveBeenCalledTimes(1);
    expect(werk.haalVoorbeeld).toHaveBeenCalledWith({ ...start, klant: 'Geme' });
  });

  it('negeert een antwoord dat te laat komt', async () => {
    const antwoorden: ((naam: string) => void)[] = [];
    const werk: NaamveldenWerk = {
      haalVoorbeeld: () =>
        new Promise<string>((klaar) => {
          antwoorden.push(klaar);
        }),
      bewaar: () => Promise.resolve(null),
      zetVoorbeeld: vi.fn(),
      zetFout: vi.fn()
    };
    const beheer = maakNaamveldenBeheer(werk, start);
    beheer.invoer({ ...start, klant: 'A' });
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS);
    beheer.invoer({ ...start, klant: 'AB' });
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS);
    expect(antwoorden).toHaveLength(2);
    // Het tweede antwoord komt eerst, het eerste daarna: alleen het laatste telt.
    antwoorden[1]?.('voor AB');
    antwoorden[0]?.('voor A');
    await vi.advanceTimersByTimeAsync(0);
    expect(werk.zetVoorbeeld).toHaveBeenCalledTimes(1);
    expect(werk.zetVoorbeeld).toHaveBeenCalledWith('voor AB');
  });

  it('houdt het vorige voorbeeld als Rust het niet kan geven', async () => {
    const werk: NaamveldenWerk = {
      haalVoorbeeld: () => Promise.reject(new Error('weg')),
      bewaar: () => Promise.resolve(null),
      zetVoorbeeld: vi.fn(),
      zetFout: vi.fn()
    };
    const beheer = maakNaamveldenBeheer(werk, start);
    beheer.invoer({ ...start, klant: 'A' });
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS);
    expect(werk.zetVoorbeeld).not.toHaveBeenCalled();
    expect(werk.zetFout).not.toHaveBeenCalled();
  });

  it('vraagt het voorbeeld meteen bij nieuwe bewaarde waarden, zonder te wachten', async () => {
    const { werk, beheer } = opzet();
    beheer.bewaard({ ...start, klant: 'Gemeente Vlaardingen' });
    await vi.advanceTimersByTimeAsync(0);
    expect(werk.haalVoorbeeld).toHaveBeenCalledWith({ ...start, klant: 'Gemeente Vlaardingen' });
  });

  it('stop ruimt de wachtende vraag op', async () => {
    const { werk, beheer } = opzet();
    beheer.invoer({ ...start, klant: 'A' });
    beheer.stop();
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS * 2);
    expect(werk.haalVoorbeeld).not.toHaveBeenCalled();
  });
});

describe('bewaren van de velden', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('gebeurt niet bij typen, alleen als je bewaarNu vraagt (blur, Enter of een keuze)', async () => {
    const { werk, beheer } = opzet();
    beheer.invoer({ ...start, klant: 'G' });
    beheer.invoer({ ...start, klant: 'Ge' });
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS * 2);
    expect(werk.bewaar).not.toHaveBeenCalled();
    expect(await beheer.bewaarNu()).toBe(true);
    expect(werk.bewaar).toHaveBeenCalledTimes(1);
    expect(werk.bewaar).toHaveBeenCalledWith({ ...start, klant: 'Ge' });
    expect(werk.zetFout).toHaveBeenLastCalledWith(null);
  });

  it('bewaart niet nog eens als er niets veranderde', async () => {
    const { werk, beheer } = opzet();
    // Niets getypt: er valt niets te bewaren (ook niet bij elke blur door de velden heen).
    expect(await beheer.bewaarNu()).toBe(true);
    expect(werk.bewaar).not.toHaveBeenCalled();
    beheer.invoer({ ...start, onderwerp: 'Intake' });
    await beheer.bewaarNu();
    await beheer.bewaarNu();
    expect(werk.bewaar).toHaveBeenCalledTimes(1);
    // Terug naar wat er eerst was: dat is weer een wijziging.
    beheer.invoer(start);
    await beheer.bewaarNu();
    expect(werk.bewaar).toHaveBeenCalledTimes(2);
  });

  it('geeft bij een fout de melding door, geeft false en probeert het opnieuw', async () => {
    const { werk, beheer } = opzet('Dit uitschrift is er niet meer.');
    beheer.invoer({ ...start, klant: 'Gem' });
    expect(await beheer.bewaarNu()).toBe(false);
    expect(werk.zetFout).toHaveBeenLastCalledWith('Dit uitschrift is er niet meer.');
    // Het is niet bewaard, dus een volgende poging probeert het weer.
    expect(await beheer.bewaarNu()).toBe(false);
    expect(werk.bewaar).toHaveBeenCalledTimes(2);
  });

  it('telt wat Rust terugstuurt als bewaard', async () => {
    const { werk, beheer } = opzet();
    beheer.invoer({ ...start, klant: 'Gem' });
    await beheer.bewaarNu();
    // Rust geeft de waarden terug zoals het ze bewaarde; staat er niets nieuws getypt, dan is dat
    // ook wat er nu staat en is er niets meer te bewaren.
    beheer.bewaard({ ...start, klant: 'Gem' });
    await beheer.bewaarNu();
    expect(werk.bewaar).toHaveBeenCalledTimes(1);
  });

  it('houdt getypte tekst die nog niet bewaard is als er een nieuwe stand binnenkomt', async () => {
    const { werk, beheer } = opzet();
    // Klant bewaard bij blur, daarna typt de gebruiker een onderwerp en pauzeert langer dan de
    // vertraging; dan komt het antwoord van Rust op het bewaren van Klant binnen.
    beheer.invoer({ ...start, klant: 'G' });
    await beheer.bewaarNu();
    beheer.invoer({ ...start, klant: 'G', onderwerp: 'Mijn onderwerp' });
    await vi.advanceTimersByTimeAsync(VOORBEELD_WACHT_MS * 2);
    beheer.bewaard({ ...start, klant: 'G' });
    await beheer.bewaarNu();
    expect(werk.bewaar).toHaveBeenCalledTimes(2);
    expect(werk.bewaar).toHaveBeenLastCalledWith({
      ...start,
      klant: 'G',
      onderwerp: 'Mijn onderwerp'
    });
  });

  it('start geen tweede schrijfactie als er al een loopt (blur en dan Opslaan)', async () => {
    let klaar: (melding: string | null) => void = () => undefined;
    const werk: NaamveldenWerk = {
      haalVoorbeeld: () => Promise.resolve(''),
      bewaar: vi.fn(
        () =>
          new Promise<string | null>((r) => {
            klaar = r;
          })
      ),
      zetVoorbeeld: vi.fn(),
      zetFout: vi.fn()
    };
    const beheer = maakNaamveldenBeheer(werk, start);
    beheer.invoer({ ...start, klant: 'Gem' });
    const eerste = beheer.bewaarNu();
    const tweede = beheer.bewaarNu();
    await vi.advanceTimersByTimeAsync(0);
    expect(werk.bewaar).toHaveBeenCalledTimes(1);
    klaar(null);
    expect(await eerste).toBe(true);
    expect(await tweede).toBe(true);
    expect(werk.bewaar).toHaveBeenCalledTimes(1);
  });

  it('vergelijkt velden op inhoud', () => {
    expect(gelijkeNaamvelden(start, { ...start })).toBe(true);
    expect(gelijkeNaamvelden(start, { ...start, datum: '2026-10-31' })).toBe(false);
  });
});
