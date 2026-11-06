import { describe, expect, it } from 'vitest';
import {
  heeftSpraakmodel,
  downloadStand,
  teHalenBytes,
  totaalVoortgang,
  stapNummer,
  takenEersteModel,
  teDownloaden,
  wizardZichtbaar,
  type WizardModel
} from './eerstestart';
import type { ModelRegel, ModellenOverzicht } from './modellen';

function regel(id: string, status: ModelRegel['status'], soort: ModelRegel['soort'] = 'spraak') {
  return { id, status, soort } as ModelRegel;
}

function overzicht(...modellen: ModelRegel[]): ModellenOverzicht {
  return {
    modellen,
    dicteren: 'parakeet-ultra-q8',
    bestanden: 'parakeet-ultra-q8',
    downloadt: null,
    totaalBytes: 0,
    vrijBytes: null
  };
}

describe('wanneer de wizard verschijnt', () => {
  it('niet zolang het overzicht nog niet binnen is', () => {
    expect(wizardZichtbaar(null, false, false)).toBe(false);
  });

  it('niet voor wie al een spraakmodel heeft', () => {
    const bestaand = overzicht(regel('parakeet-ultra-q8', 'compleet'));
    expect(heeftSpraakmodel(bestaand)).toBe(true);
    expect(wizardZichtbaar(bestaand, false, false)).toBe(false);
  });

  it('een hulpmodel of sprekermodel telt niet als spraakmodel', () => {
    const alleenHulp = overzicht(
      regel('silero-vad-v6.2', 'compleet', 'hulp'),
      regel('sortformer-4spk-v2.1-q8', 'compleet', 'sprekers'),
      regel('parakeet-ultra-q8', 'ontbreekt')
    );
    expect(wizardZichtbaar(alleenHulp, false, false)).toBe(true);
  });

  it('wel zonder model, ook als een download half af is', () => {
    expect(wizardZichtbaar(overzicht(regel('parakeet-ultra-q8', 'ontbreekt')), false, false)).toBe(
      true
    );
    expect(
      wizardZichtbaar(overzicht(regel('parakeet-ultra-q8', 'gedeeltelijk')), false, false)
    ).toBe(true);
  });

  it('blijft staan als de download het model compleet maakt, tot de gebruiker klaar is', () => {
    const klaar = overzicht(regel('parakeet-ultra-q8', 'compleet'));
    expect(wizardZichtbaar(klaar, true, false)).toBe(true);
    expect(wizardZichtbaar(klaar, true, true)).toBe(false);
  });

  it('komt na "Nu niet" niet terug in dezelfde sessie', () => {
    const leeg = overzicht(regel('parakeet-ultra-q8', 'ontbreekt'));
    expect(wizardZichtbaar(leeg, true, true)).toBe(false);
  });
});

describe('de stappen en de downloads', () => {
  it('telt de stappen vanaf 1', () => {
    expect(stapNummer('waarom')).toBe(1);
    expect(stapNummer('klaar')).toBe(4);
  });

  it('het eerste model doet beide taken als er geen tweede model komt', () => {
    expect(takenEersteModel(false)).toEqual(['dicteren', 'bestanden']);
    expect(takenEersteModel(true)).toEqual(['dicteren']);
  });

  it('downloadt het tweede model alleen als het advies het geeft en de gebruiker het wil', () => {
    const apart = { dicteren: 'ultra', bestanden: 'canary', bestandenApart: true };
    expect(teDownloaden(apart, true)).toEqual(['ultra', 'canary']);
    expect(teDownloaden(apart, false)).toEqual(['ultra']);
    const een = { dicteren: 'ultra', bestanden: 'ultra', bestandenApart: false };
    expect(teDownloaden(een, true)).toEqual(['ultra']);
  });
});

describe('de downloads van de wizard in de rij', () => {
  const model = (id: string, anders: Partial<WizardModel> = {}): WizardModel => ({
    id,
    compleet: false,
    fase: 'start',
    toestand: null,
    fout: null,
    ...anders
  });

  it('loopt zolang een model wacht, binnenkomt of net gestart is', () => {
    // Het tweede model wacht achter het eerste: de wizard toont wat binnenkomt.
    expect(
      downloadStand([
        model('ultra', { fase: 'bezig', toestand: 'bezig' }),
        model('canary', { fase: 'wacht', toestand: 'wacht' })
      ])
    ).toEqual({ soort: 'bezig', model: 'ultra' });
    // Het eerste staat er, het tweede wacht nog achter een andere download: niet gestopt.
    expect(
      downloadStand([
        model('ultra', { compleet: true, toestand: 'klaar' }),
        model('canary', { fase: 'wacht', toestand: 'wacht' })
      ])
    ).toEqual({ soort: 'bezig', model: 'canary' });
    expect(downloadStand([model('ultra', { fase: 'wordt_gestart' })]).soort).toBe('bezig');
    expect(
      downloadStand([model('ultra', { fase: 'opnieuw', toestand: 'nieuwe_poging' })]).soort
    ).toBe('bezig');
  });

  it('is alleen klaar als alles op schijf compleet is', () => {
    expect(
      downloadStand([model('ultra', { compleet: true }), model('canary', { compleet: true })])
    ).toEqual({ soort: 'klaar' });
    // De rij zegt klaar, maar het overzicht van schijf nog niet: eerst controleren.
    expect(downloadStand([model('ultra', { toestand: 'klaar' })])).toEqual({
      soort: 'controleren',
      model: 'ultra'
    });
  });

  it('een oude fout van een ander model of van vroeger telt niet zolang er iets loopt', () => {
    expect(
      downloadStand([
        model('ultra', { fout: 'De verbinding viel weg.', toestand: 'fout' }),
        model('canary', { fase: 'bezig', toestand: 'bezig' })
      ]).soort
    ).toBe('bezig');
  });

  it('een fout of een gestopte download telt nooit als klaar', () => {
    expect(
      downloadStand([
        model('ultra', { compleet: true }),
        model('canary', { fout: 'De schijf is vol.', toestand: 'fout' })
      ])
    ).toEqual({ soort: 'fout', model: 'canary', melding: 'De schijf is vol.' });
    expect(downloadStand([model('ultra', { toestand: 'gestopt' })])).toEqual({
      soort: 'gestopt'
    });
    expect(downloadStand([model('ultra', { toestand: 'onderbroken' })]).soort).toBe('gestopt');
    // Nooit gestart (of door Rust vergeten): ook gestopt, niet klaar.
    expect(downloadStand([model('ultra')]).soort).toBe('gestopt');
  });
});

describe('totaalVoortgang', () => {
  const MB = 1_000_000;
  const ultra = {
    grootteBytes: 742 * MB,
    aanwezigBytes: 0,
    startAanwezigBytes: 0,
    compleet: false,
    binnenBytes: null
  };
  const canary = {
    grootteBytes: 1058 * MB,
    aanwezigBytes: 0,
    startAanwezigBytes: 0,
    compleet: false,
    binnenBytes: null
  };

  it('telt de groottes van alle modellen op, zodat het totaal bij de knop past', () => {
    // "Downloaden (1,8 GB)": de voortgang zegt dan "van 1,8 GB" en niet "van 742 MB".
    expect(totaalVoortgang([ultra, canary]).totaal).toBe(1800 * MB);
    expect(teHalenBytes([ultra, canary])).toBe(1800 * MB);
  });

  it('telt wat het binnenkomende model heeft en wat al klaar is', () => {
    const bezig = { ...ultra, binnenBytes: 10 * MB };
    expect(totaalVoortgang([bezig, canary])).toEqual({ binnen: 10 * MB, totaal: 1800 * MB });
    const eerstKlaar = { ...ultra, compleet: true };
    const tweedeBezig = { ...canary, binnenBytes: 300 * MB };
    expect(totaalVoortgang([eerstKlaar, tweedeBezig])).toEqual({
      binnen: 1042 * MB,
      totaal: 1800 * MB
    });
  });

  it('gaat nooit terug en komt nooit boven het totaal', () => {
    // Er stond 400 MB, en de rij meldt (nog) minder: er is nog niets bij gekomen.
    const hervat = {
      ...ultra,
      aanwezigBytes: 400 * MB,
      startAanwezigBytes: 400 * MB,
      binnenBytes: 100 * MB
    };
    expect(totaalVoortgang([hervat]).binnen).toBe(0);
    const teveel = { ...ultra, binnenBytes: 900 * MB };
    expect(totaalVoortgang([teveel]).binnen).toBe(742 * MB);
  });

  it('klopt bij een hervatte download met de knop', () => {
    // Ultra stond voor 400 van 742 MB op schijf: de knop zei 342 MB + 1058 MB = 1,4 GB.
    const hervat = { ...ultra, aanwezigBytes: 400 * MB, startAanwezigBytes: 400 * MB };
    expect(teHalenBytes([hervat, canary])).toBe(1400 * MB);
    expect(totaalVoortgang([hervat, canary]).totaal).toBe(1400 * MB);
    // Tijdens het downloaden groeit wat op schijf staat; het totaal blijft wat de knop zei.
    const onderweg = { ...hervat, aanwezigBytes: 500 * MB, binnenBytes: 500 * MB };
    expect(totaalVoortgang([onderweg, canary])).toEqual({ binnen: 100 * MB, totaal: 1400 * MB });
    // Alles klaar: binnen is precies het totaal.
    expect(
      totaalVoortgang([
        { ...hervat, compleet: true },
        { ...canary, compleet: true }
      ])
    ).toEqual({ binnen: 1400 * MB, totaal: 1400 * MB });
  });

  it('is leeg zonder modellen', () => {
    expect(totaalVoortgang([])).toEqual({ binnen: 0, totaal: 0 });
  });
});
