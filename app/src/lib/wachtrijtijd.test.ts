import { describe, expect, it } from 'vitest';
import type { BestandInfo } from './bestanden';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import { resterendeTijdTekst } from './resterendetijd';
import type { Toestand } from './taak';
import { schatWachtrij } from './wachtrijtijd';

function bestand(
  taak: number,
  toestand: Toestand,
  duurMs: number | null,
  verwerkingMs: number | null = null
): BestandInfo {
  return {
    taak,
    naam: `opname-${String(taak)}.mp3`,
    video: false,
    duurMs,
    toestand,
    verwerkingMs,
    heeftTranscript: toestand.type === 'klaar',
    opgeslagen: null,
    opgeslagenVerouderd: false,
    bewaarFout: null,
    uitMap: false,
    overgeslagen: false
  };
}

const geenLopend = (): undefined => undefined;

describe('schatWachtrij', () => {
  it('geeft niets bij een enkel bestand: dat heeft zijn eigen schatting', () => {
    const lijst = [bestand(1, { type: 'verwerkt', voortgang: 50 }, 600_000)];
    expect(schatWachtrij(lijst, () => ({ ms: 60_000, vanafProcent: 0 }))).toBeNull();
  });

  it('geeft niets zolang er te kort gemeten is', () => {
    const lijst = [
      bestand(1, { type: 'klaar' }, 600_000, 3_000),
      bestand(2, { type: 'wacht' }, 600_000)
    ];
    expect(schatWachtrij(lijst, geenLopend)).toBeNull();
  });

  it('geeft niets als er nog niets is uitgeschreven waar een snelheid uit volgt', () => {
    const lijst = [
      bestand(1, { type: 'verwerkt', voortgang: 0 }, 600_000),
      bestand(2, { type: 'wacht' }, 600_000)
    ];
    expect(schatWachtrij(lijst, () => ({ ms: 30_000, vanafProcent: 0 }))).toBeNull();
  });

  it('rekent met de snelheid van klaar bestanden voor wat nog wacht', () => {
    // Tien minuten opname in 60 s uitgeschreven: 10 keer sneller dan de opname zelf.
    const lijst = [
      bestand(1, { type: 'klaar' }, 600_000, 60_000),
      bestand(2, { type: 'wacht' }, 1_200_000),
      bestand(3, { type: 'wacht' }, 600_000)
    ];
    const schatting = schatWachtrij(lijst, geenLopend);
    // 1800 s opname, 10 keer sneller: 180 s.
    expect(schatting?.seconden).toBeCloseTo(180, 5);
    expect(schatting?.aantal).toBe(2);
  });

  it('telt het lopende bestand mee: wat al gedaan is, en wat nog moet', () => {
    // Klaar: 10 min in 60 s. Loopt: 20 min opname, 50 procent na 60 s. Wacht: 10 min.
    const lijst = [
      bestand(1, { type: 'klaar' }, 600_000, 60_000),
      bestand(2, { type: 'verwerkt', voortgang: 50 }, 1_200_000),
      bestand(3, { type: 'wacht' }, 600_000)
    ];
    const schatting = schatWachtrij(lijst, (taak) =>
      taak === 2 ? { ms: 60_000, vanafProcent: 0 } : undefined
    );
    // Gedaan: 600 + 600 s opname in 120 s: 10x. Nog: 600 + 600 s opname: 120 s.
    expect(schatting?.seconden).toBeCloseTo(120, 5);
    expect(schatting?.aantal).toBe(2);
  });

  it('telt bij een pagina die halverwege opende alleen de voortgang sinds die tijd', () => {
    // Het bestand stond al op 40 procent toen de pagina opende; sindsdien 10 procent in 60 s:
    // 120 s opname in 60 s, dus 2x. Nog 50 procent van 20 min = 600 s opname: 300 s.
    const lijst = [
      bestand(1, { type: 'verwerkt', voortgang: 50 }, 1_200_000),
      bestand(2, { type: 'wacht' }, 0)
    ];
    const schatting = schatWachtrij(lijst, () => ({ ms: 60_000, vanafProcent: 40 }));
    expect(schatting?.seconden).toBeCloseTo(300, 5);
  });

  it('laat fouten, geannuleerde en klaar bestanden zonder tijd buiten het aantal', () => {
    const lijst = [
      bestand(1, { type: 'klaar' }, 600_000, 60_000),
      bestand(2, { type: 'fout', melding: 'kapot', vervolg: 'geen' }, null),
      bestand(3, { type: 'geannuleerd' }, 300_000),
      bestand(4, { type: 'wacht' }, 600_000),
      bestand(5, { type: 'wacht' }, null)
    ];
    const schatting = schatWachtrij(lijst, geenLopend);
    expect(schatting?.aantal).toBe(2);
    // Een wachtend bestand zonder bekende duur telt niet mee in de tijd: 600 s opname, 10x.
    expect(schatting?.seconden).toBeCloseTo(60, 5);
  });

  it('beschouwt het herkennen van sprekers als klaar met uitschrijven', () => {
    const lijst = [
      bestand(1, { type: 'verwerkt', voortgang: null, stap: 'sprekers' }, 600_000),
      bestand(2, { type: 'wacht' }, 600_000)
    ];
    // 600 s opname in 60 s: 10x; nog 600 s opname.
    const schatting = schatWachtrij(lijst, () => ({ ms: 60_000, vanafProcent: 0 }));
    expect(schatting?.seconden).toBeCloseTo(60, 5);
  });
});

describe('de tekst van de wachtrij', () => {
  it('zegt het in gewone taal, in beide talen', () => {
    const tijd = resterendeTijdTekst(180, nl);
    expect(nl.wachtrijRest(tijd, 4)).toBe('Nog ongeveer 3 minuten voor 4 bestanden');
    expect(nl.wachtrijRest(tijd, 1)).toBe('Nog ongeveer 3 minuten voor 1 bestand');
    expect(en.wachtrijRest(resterendeTijdTekst(180, en), 4)).toBe(
      'About 3 minutes left for 4 files'
    );
  });
});
