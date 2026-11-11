import { describe, expect, it } from 'vitest';
import {
  duurTekst,
  foutVanStand,
  isNieuwer,
  naamZonderExtensie,
  rijKnop,
  vraagBijWeghalen,
  stand,
  tijdcode,
  transcriptTekst,
  verwerkingTekst,
  type BestandenStand,
  type BestandInfo
} from './bestanden';
import { en } from './i18n/en';
import { nl } from './i18n/nl';

function info(over: Partial<BestandInfo>): BestandInfo {
  return {
    taak: 1,
    naam: 'Interview.m4a',
    video: false,
    duurMs: 1_394_000,
    toestand: { type: 'wacht' },
    verwerkingMs: null,
    heeftTranscript: false,
    opgeslagen: null,
    opgeslagenVerouderd: false,
    bewaarFout: null,
    uitMap: false,
    overgeslagen: false,
    ...over
  };
}

describe('een overgeslagen bestand uit de bewaakte map', () => {
  it('zegt dat er al een uitschrift is, in beide talen', () => {
    const bestand = info({ toestand: { type: 'geannuleerd' }, overgeslagen: true, uitMap: true });
    expect(stand(bestand, null, nl)).toBe('overgeslagen, er is al een uitschrift');
    expect(stand(bestand, null, en)).toBe('skipped, a transcript already exists');
  });
});

describe('tijden in het scherm Bestanden', () => {
  it('tijdcodes als m:ss, boven het uur met uren', () => {
    expect(tijdcode(0)).toBe('0:00');
    expect(tijdcode(7_400)).toBe('0:07');
    expect(tijdcode(760_000)).toBe('12:40');
    expect(tijdcode(3_760_000)).toBe('1:02:40');
    expect(tijdcode(-5)).toBe('0:00');
  });

  it('de duur van een bestand zoals een speler hem toont', () => {
    expect(duurTekst(125_000)).toBe('2:05');
    expect(duurTekst(1_394_000)).toBe('23:14');
    expect(duurTekst(3_760_000)).toBe('1:02:40');
  });

  it('hoe lang het uitschrijven duurde', () => {
    expect(verwerkingTekst(300, nl)).toBe('1 s');
    expect(verwerkingTekst(12_400, nl)).toBe('12 s');
    expect(verwerkingTekst(108_000, nl)).toBe('1 min 48 s');
    expect(verwerkingTekst(108_000, en)).toBe('1 min 48 s');
  });

  it('de naam zonder extensie', () => {
    expect(naamZonderExtensie('Interview wethouder.m4a')).toBe('Interview wethouder');
    expect(naamZonderExtensie('geen-extensie')).toBe('geen-extensie');
    expect(naamZonderExtensie('.verborgen')).toBe('.verborgen');
  });
});

describe('de regel onder een bestand', () => {
  it('wacht, bezig en voortgang in gewone taal', () => {
    expect(stand(info({}), null, nl)).toBe('wacht');
    expect(stand(info({ toestand: { type: 'verwerkt', voortgang: null } }), null, nl)).toBe(
      'bezig'
    );
    const bij43 = info({ toestand: { type: 'verwerkt', voortgang: 43 } });
    expect(stand(bij43, null, nl)).toBe('43%');
    expect(stand(bij43, nl.nogMinuten(3), nl)).toBe('43%, nog ongeveer 3 minuten');
    expect(stand(bij43, en.nogMinuten(3), en)).toBe('43%, about 3 minutes left');
  });

  it('klaar, geannuleerd en een fout met de melding', () => {
    expect(stand(info({ toestand: { type: 'klaar' }, verwerkingMs: 108_000 }), null, nl)).toBe(
      'uitgeschreven in 1 min 48 s'
    );
    expect(stand(info({ toestand: { type: 'klaar' } }), null, nl)).toBe('klaar');
    expect(stand(info({ toestand: { type: 'geannuleerd' } }), null, nl)).toBe('geannuleerd');
    const melding =
      'Mondig kan dit formaat niet lezen. Sla het bestand op als mp4 of wav, bijvoorbeeld met Clipchamp, en voeg het opnieuw toe.';
    expect(stand(info({ toestand: { type: 'fout', melding, vervolg: 'geen' } }), null, nl)).toBe(
      melding
    );
  });
});

describe('de volgorde van standen', () => {
  it('een oudere stand draait een nieuwere niet terug', () => {
    const stand = (versie: number) => ({
      versie,
      lijst: [info({ toestand: { type: 'klaar' } })],
      fout: null,
      nietGepakt: false
    });
    expect(isNieuwer(stand(5), 4)).toBe(true);
    expect(isNieuwer(stand(4), 4)).toBe(false);
    expect(isNieuwer(stand(3), 4)).toBe(false);
    // De eerste stand (volgnummer 0) wordt altijd overgenomen.
    expect(isNieuwer(stand(0), -1)).toBe(true);
  });
});

describe('de stap Sprekers herkennen in de rij', () => {
  it('toont de stap in plaats van een percentage', () => {
    const bezig = info({ toestand: { type: 'verwerkt', voortgang: 100, stap: 'sprekers' } });
    expect(stand(bezig, null, nl)).toBe('Sprekers herkennen…');
    expect(stand(bezig, null, en)).toBe('Recognising speakers…');
    // Zonder stap blijft het een percentage.
    const gewoon = info({ toestand: { type: 'verwerkt', voortgang: 43 } });
    expect(stand(gewoon, null, nl)).toBe('43%');
  });
});

describe('opgeslagen als', () => {
  it('zegt dat het bestand de laatste wijzigingen mist', () => {
    expect(nl.bestandOpgeslagenVerouderd('Interview.docx')).toBe(
      'opgeslagen als Interview.docx, zonder je laatste wijzigingen'
    );
  });
});

describe('kopiëren', () => {
  it('een regel per stuk tekst', () => {
    expect(
      transcriptTekst({
        taak: 1,
        naam: 'x',
        duurMs: 10,
        klaarOpMs: 0,
        model: 'Parakeet Ultra',
        soevereiniteit: 'lokaal',
        melding: null,
        waarschuwing: null,
        regels: [
          {
            index: 0,
            startMs: 0,
            eindMs: 5,
            tekst: 'Eerste zin.',
            markeringen: [],
            uitvoer: 'Eerste zin.'
          },
          {
            index: 1,
            startMs: 5,
            eindMs: 10,
            tekst: 'Tweede zin.',
            markeringen: [],
            uitvoer: 'Tweede zin.'
          }
        ],
        klant: '',
        onderwerp: 'interview',
        datum: '2026-10-30',
        audioAanwezig: true,
        persoon: { vervangen: false, eigen: [] }
      })
    ).toBe('Eerste zin.\nTweede zin.');
  });

  it('kopieert de tekst met plaatsvervangers als Rust die als uitvoer geeft', () => {
    expect(
      transcriptTekst({
        taak: 1,
        naam: 'x',
        duurMs: 10,
        klaarOpMs: 0,
        model: 'm',
        soevereiniteit: 'lokaal',
        melding: null,
        waarschuwing: null,
        regels: [
          {
            index: 0,
            startMs: 0,
            eindMs: 5,
            tekst: 'Bel 06 12345678.',
            markeringen: [{ soort: 'telefoon', van: 4, tot: 15 }],
            uitvoer: 'Bel [TELEFOON].'
          }
        ],
        klant: '',
        onderwerp: 'interview',
        datum: '2026-10-30',
        audioAanwezig: true,
        persoon: { vervangen: true, eigen: [] }
      })
    ).toBe('Bel [TELEFOON].');
  });

  it('zet met herkende sprekers het label voor de tekst', () => {
    expect(
      transcriptTekst({
        taak: 1,
        naam: 'x',
        duurMs: 10,
        klaarOpMs: 0,
        model: 'Parakeet Ultra',
        soevereiniteit: 'lokaal',
        melding: null,
        waarschuwing: null,
        regels: [
          {
            index: 0,
            startMs: 0,
            eindMs: 5,
            tekst: 'Eerste zin.',
            markeringen: [],
            uitvoer: 'Eerste zin.',
            spreker: 'Spreker 1'
          },
          {
            index: 1,
            startMs: 5,
            eindMs: 10,
            tekst: 'Tweede zin.',
            markeringen: [],
            uitvoer: 'Tweede zin.',
            spreker: 'Spreker 2'
          },
          {
            index: 2,
            startMs: 10,
            eindMs: 15,
            tekst: 'Zonder.',
            markeringen: [],
            uitvoer: 'Zonder.',
            spreker: null
          }
        ],
        klant: '',
        onderwerp: 'interview',
        datum: '2026-10-30',
        audioAanwezig: true,
        persoon: { vervangen: false, eigen: [] }
      })
    ).toBe('Spreker 1: Eerste zin.\nSpreker 2: Tweede zin.\nZonder.');
  });
});

describe('een drop die niets oplevert (ADR 0030)', () => {
  const leeg: BestandenStand = { versie: 1, lijst: [], fout: null, nietGepakt: false };

  it('zegt in gewone taal wat wel kan, in beide talen', () => {
    const stand = { ...leeg, nietGepakt: true };
    expect(foutVanStand(stand, nl)).toEqual({
      melding:
        'Mondig kon dit bestand niet pakken. Sleep het eerst naar het bureaublad en dan hierheen.',
      vervolg: 'geen'
    });
    expect(foutVanStand(stand, en)?.melding).toBe(
      "Mondig couldn't pick up this file. Drag it to the desktop first, then drag it here."
    );
  });

  it('toont niets zonder lege drop, en een fout van het toevoegen gaat voor', () => {
    expect(foutVanStand(leeg, nl)).toBeNull();
    const fout = { melding: 'Download eerst een spraakmodel.', vervolg: 'geen' as const };
    expect(foutVanStand({ ...leeg, fout, nietGepakt: true }, nl)).toBe(fout);
  });
});

describe('de knop aan het eind van een rij', () => {
  it('geeft een kruisje voor annuleren en uit de rij halen, de prullenbak alleen bij klaar', () => {
    expect(rijKnop({ type: 'verwerkt', voortgang: 40 })).toEqual({
      soort: 'annuleren',
      icoon: 'kruis'
    });
    expect(rijKnop({ type: 'wacht' })).toEqual({ soort: 'uitRij', icoon: 'kruis' });
    expect(rijKnop({ type: 'luistert' })).toEqual({ soort: 'uitRij', icoon: 'kruis' });
    expect(rijKnop({ type: 'geannuleerd' })).toEqual({ soort: 'uitRij', icoon: 'kruis' });
    expect(rijKnop({ type: 'fout', melding: 'Mislukt', vervolg: 'geen' })).toEqual({
      soort: 'uitRij',
      icoon: 'kruis'
    });
    expect(rijKnop({ type: 'klaar' })).toEqual({ soort: 'verwijderen', icoon: 'prullenbak' });
  });

  it('heeft teksten die zeggen wat er gebeurt', () => {
    expect(nl.bestandAnnulerenKort).toBe('Annuleren');
    expect(nl.bestandUitRijKort).toBe('Uit de rij halen');
    expect(nl.bestandWeghalen('Interview.m4a')).toContain('uit de lijst verwijderen');
    expect(en.bestandWeghalen('Interview.m4a')).toContain('from the list');
  });
});

describe('de vraag bij de prullenbak', () => {
  it('komt alleen bij een klaar uitschrift dat nog niet is opgeslagen', () => {
    expect(vraagBijWeghalen(info({ toestand: { type: 'klaar' }, opgeslagen: null }))).toBe(true);
    expect(
      vraagBijWeghalen(info({ toestand: { type: 'klaar' }, opgeslagen: 'C:/Mondig/a.docx' }))
    ).toBe(false);
    expect(vraagBijWeghalen(info({ toestand: { type: 'wacht' } }))).toBe(false);
    expect(
      vraagBijWeghalen(info({ toestand: { type: 'fout', melding: 'x', vervolg: 'geen' } }))
    ).toBe(false);
  });

  it('heeft teksten die zeggen wat er gebeurt', () => {
    expect(nl.bestandWeghalenOnopgeslagen).toContain('nog niet opgeslagen');
    expect(nl.bestandWeghalenKortOpgeslagen).toContain('opgeslagen bestand blijft staan');
    expect(en.bestandOpgeslagenBlijft).toContain('saved file stays');
  });
});
