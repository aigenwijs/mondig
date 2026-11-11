import { beforeEach, describe, expect, it, vi } from 'vitest';

// De commando's voor bewaarde verslagen (N-C3, ADR 0022) roepen de commando's van Rust aan met de
// juiste namen en argumenten. Een id uit de lijst is een mapnaam, nooit een pad.

const kern = vi.hoisted(() => ({
  invoke: vi.fn((): Promise<unknown> => Promise.resolve(null)),
  isTauri: vi.fn(() => false)
}));
vi.mock('@tauri-apps/api/core', () => kern);

import {
  vergaderingSluiten,
  vergaderingNaamvoorbeeld,
  vergaderingVerslagNaamvelden,
  vergaderingVerslagOpenen,
  vergaderingVerslagToonOpgeslagen,
  vergaderingVerslagWissen
} from './vergadering';

describe("de commando's voor bewaarde verslagen", () => {
  beforeEach(() => {
    kern.invoke.mockClear();
  });

  it('opent een verslag uit de lijst met zijn id', async () => {
    await vergaderingVerslagOpenen('vergadering-1762');
    expect(kern.invoke).toHaveBeenCalledWith('vergadering_verslag_openen', {
      id: 'vergadering-1762',
      toch: false
    });
    await vergaderingVerslagOpenen('vergadering-1762', true);
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_verslag_openen', {
      id: 'vergadering-1762',
      toch: true
    });
  });

  it('wist het verslag dat open is zonder id en een verslag uit de lijst met id', async () => {
    await vergaderingVerslagWissen(null);
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_verslag_wissen', { id: null });
    await vergaderingVerslagWissen('vergadering-1762');
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_verslag_wissen', {
      id: 'vergadering-1762'
    });
  });

  it('toont het opgeslagen bestand van een verslag uit de lijst met alleen zijn id', async () => {
    await vergaderingVerslagToonOpgeslagen('vergadering-1762');
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_verslag_toon_opgeslagen', {
      id: 'vergadering-1762'
    });
  });

  it('bewaart klant, onderwerp en datum van het verslag', async () => {
    await vergaderingVerslagNaamvelden('Gemeente Vlaardingen', 'Intake', '2026-11-02');
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_verslag_naamvelden', {
      klant: 'Gemeente Vlaardingen',
      onderwerp: 'Intake',
      datum: '2026-11-02'
    });
    await vergaderingVerslagNaamvelden('', '', '');
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_verslag_naamvelden', {
      klant: '',
      onderwerp: '',
      datum: ''
    });
  });

  it('vraagt Rust om de bestandsnaam bij de velden, zonder iets te bewaren', async () => {
    kern.invoke.mockResolvedValueOnce('2026-11-02 Gemeente Vlaardingen Intake.docx');
    const naam = await vergaderingNaamvoorbeeld('Gemeente Vlaardingen', 'Intake', '2026-11-02');
    expect(naam).toBe('2026-11-02 Gemeente Vlaardingen Intake.docx');
    expect(kern.invoke).toHaveBeenLastCalledWith('vergadering_naamvoorbeeld', {
      klant: 'Gemeente Vlaardingen',
      onderwerp: 'Intake',
      datum: '2026-11-02'
    });
  });

  it('sluit het verslag met één commando, zonder bevestiging of wisopdracht', async () => {
    await vergaderingSluiten();
    expect(kern.invoke).toHaveBeenCalledTimes(1);
    expect(kern.invoke).toHaveBeenCalledWith('vergadering_sluiten', { toch: false });
  });

  it('sluit toch als het verslag niet bewaard is en de gebruiker dat kiest', async () => {
    await vergaderingSluiten(true);
    expect(kern.invoke).toHaveBeenCalledWith('vergadering_sluiten', { toch: true });
  });
});
