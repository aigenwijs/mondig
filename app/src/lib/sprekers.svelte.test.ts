import { describe, expect, it } from 'vitest';
import type { ModelRegel, ModelStand } from './modellen';
import { SPREKERMODEL } from './sprekers';
import { Sprekers } from './sprekers.svelte';

// De poort met een nep-modellenstand. De effecten van Svelte draaien in deze testomgeving niet
// (servermodus), dus de test roept volg() aan zoals het effect in de root-layout dat doet. Dat
// volg() zijn reactieve waarden leest voordat het vroegtijdig stopt, staat in de code zelf.

function opzet() {
  const wereld: {
    status: ModelStand;
    straks: ModelStand;
    /** Het model dat in de downloadrij staat (wacht of binnenkomt). */
    lopend: string | null;
    fout: string | null;
    opgeslagen: boolean[];
    /** Hoe vaak er een download gestart is. */
    starts: number;
  } = $state({
    status: 'ontbreekt',
    /** Wat het overzicht na het verversen zegt. */
    straks: 'ontbreekt',
    lopend: null,
    fout: null,
    opgeslagen: [],
    starts: 0
  });
  const poort = new Sprekers({
    lees: () => Promise.resolve(true),
    schrijf: (aan) => {
      wereld.opgeslagen.push(aan);
      return Promise.resolve();
    },
    model: () => ({ id: SPREKERMODEL, status: wereld.status }) as ModelRegel,
    actief: () => wereld.lopend === SPREKERMODEL,
    start: () => {
      wereld.starts += 1;
      wereld.lopend = SPREKERMODEL;
      return Promise.resolve(null);
    },
    stop: () => {
      wereld.lopend = null;
    },
    herlaad: () => {
      wereld.status = wereld.straks;
      return Promise.resolve();
    },
    downloadFout: () => wereld.fout,
    opslaanFout: () => 'niet bewaard'
  });
  return { wereld, poort };
}

describe('de poort voor Sprekers herkennen', () => {
  it('zet aan na een geslaagde download, ook de tweede keer', async () => {
    const { wereld, poort } = opzet();
    for (const keer of [1, 2]) {
      wereld.status = 'ontbreekt';
      wereld.straks = 'compleet';
      await poort.zet(true);
      expect(poort.fase).toBe('vraag');
      await poort.download();
      expect(poort.fase).toBe('wacht');
      // Het klaar-event zet lopend op null terwijl het overzicht nog oud is, daarna komt het model.
      wereld.lopend = null;
      // Twee keer tegelijk (effect en overzicht): de tweede mag niets stuk maken.
      await Promise.all([poort.volg(), poort.volg()]);
      await poort.volg();
      expect(poort.fase, `keer ${String(keer)}`).toBe('rust');
      expect(wereld.opgeslagen.at(-1)).toBe(true);
      expect(wereld.opgeslagen.filter(Boolean).length).toBe(keer);
      await poort.zet(false);
    }
  });

  it('toont de fout als de download mislukt', async () => {
    const { wereld, poort } = opzet();
    await poort.zet(true);
    await poort.download();
    wereld.fout = 'De verbinding viel weg.';
    wereld.lopend = null;
    await poort.volg();
    expect(poort.fase).toBe('rust');
    expect(poort.fout).toBe('De verbinding viel weg.');
    expect(poort.schakelaar).toBe(false);
  });

  it('laat direct na de klik zien dat er iets gebeurt', async () => {
    const { poort } = opzet();
    await poort.zet(true);
    expect(poort.weergave).toBe('vraag');
    const bezig = poort.download();
    // Nog voordat Rust antwoordt: de vraag is weg en de stand van de download staat er.
    expect(poort.fase).toBe('wacht');
    expect(poort.weergave).toBe('voortgang');
    await bezig;
  });

  it('start niets dubbel bij een dubbele klik', async () => {
    const { wereld, poort } = opzet();
    await poort.zet(true);
    await Promise.all([poort.download(), poort.download()]);
    expect(wereld.starts).toBe(1);
    expect(wereld.lopend).toBe(SPREKERMODEL);
  });

  it('mag achter een andere download in de rij', async () => {
    const { wereld, poort } = opzet();
    wereld.lopend = 'canary-1b-v2-q8';
    await poort.zet(true);
    await poort.download();
    expect(wereld.starts).toBe(1);
    expect(poort.fase).toBe('wacht');
  });

  it('kijkt mee als het sprekermodel al in de rij staat', async () => {
    const { wereld, poort } = opzet();
    // Via Modellen al gestart: aanzetten vraagt niets meer en start niets.
    wereld.lopend = SPREKERMODEL;
    await poort.zet(true);
    expect(poort.fase).toBe('wacht');
    expect(wereld.starts).toBe(0);
    wereld.status = 'compleet';
    await poort.volg();
    expect(poort.schakelaar).toBe(true);
  });

  it('laat na een geslaagde download zien dat de schakelaar aan ging', async () => {
    const { wereld, poort } = opzet();
    await poort.zet(true);
    await poort.download();
    wereld.status = 'compleet';
    await poort.volg();
    expect(poort.schakelaar).toBe(true);
    expect(poort.weergave).toBe('aangezet');
    // Zelf weer omzetten haalt de bevestiging weg.
    await poort.zet(false);
    expect(poort.weergave).toBe('niets');
  });
});
