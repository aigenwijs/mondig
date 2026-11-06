import { describe, expect, it } from 'vitest';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import {
  isBedienbaar,
  kaartTekst,
  keerGetal,
  standTekst,
  testTekst,
  versieRegelApparaat,
  voorkeurKaart,
  type Videokaart
} from './videokaart';

const los: Videokaart = { naam: 'NVIDIA GeForce RTX 4090', soort: 'los' };
const ingebouwd: Videokaart = { naam: 'Intel(R) Iris(R) Xe Graphics', soort: 'ingebouwd' };

describe('voorkeurKaart', () => {
  it('kiest een losse kaart boven een ingebouwde chip', () => {
    expect(voorkeurKaart([ingebouwd, los])).toEqual(los);
    expect(voorkeurKaart([ingebouwd])).toEqual(ingebouwd);
  });

  it('geeft niets zonder kaart of als zoeken niet lukte', () => {
    expect(voorkeurKaart([])).toBeNull();
    expect(voorkeurKaart(null)).toBeNull();
  });
});

describe('isBedienbaar', () => {
  it('is alleen te bedienen met een gevonden kaart', () => {
    expect(isBedienbaar({ aan: false, kaarten: [los] })).toBe(true);
    expect(isBedienbaar({ aan: false, kaarten: [ingebouwd] })).toBe(true);
    expect(isBedienbaar({ aan: false, kaarten: [] })).toBe(false);
    expect(isBedienbaar({ aan: false, kaarten: null })).toBe(false);
  });

  it('mag altijd uit als hij nog aan staat', () => {
    expect(isBedienbaar({ aan: true, kaarten: [] })).toBe(true);
  });
});

describe('kaartTekst', () => {
  it('noemt de naam van de kaart', () => {
    expect(kaartTekst({ aan: false, kaarten: [los] }, nl)).toBe(
      'Gevonden: NVIDIA GeForce RTX 4090.'
    );
    expect(kaartTekst({ aan: false, kaarten: [ingebouwd] }, nl)).toContain('(ingebouwd)');
  });

  it('zegt waarom de schakelaar uitgegrijsd is', () => {
    expect(kaartTekst({ aan: false, kaarten: [] }, nl)).toBe('Geen geschikte videokaart gevonden.');
    expect(kaartTekst({ aan: false, kaarten: null }, nl)).toBe(nl.videokaartOnbekend);
  });
});

describe('standTekst', () => {
  it('geeft de drie standen en niets zonder geladen motor', () => {
    expect(standTekst('videokaart', nl)).toBe('In gebruik: videokaart');
    expect(standTekst('processor', nl)).toBe('In gebruik: processor');
    expect(standTekst('terugval', nl)).toBe(
      'De videokaart gaf een fout; Mondig gebruikt nu de processor'
    );
    expect(standTekst(null, nl)).toBeNull();
    expect(standTekst('videokaart', en)).toBe('In use: graphics card');
  });
});

describe('keerGetal', () => {
  it('onder de tien met één decimaal, daarboven afgerond', () => {
    expect(keerGetal(9.24, 'nl')).toBe('9,2');
    expect(keerGetal(9.24, 'en')).toBe('9.2');
    expect(keerGetal(2, 'nl')).toBe('2');
    expect(keerGetal(15.4, 'nl')).toBe('15');
  });
});

describe('testTekst', () => {
  it('zegt in gewone taal of aanzetten loont', () => {
    expect(
      testTekst({ oordeel: 'loont', keerSneller: 9, processor: 0.09, videokaart: 0.01 }, nl, 'nl')
    ).toBe('De videokaart is 9 keer sneller. Aanzetten loont.');
    expect(
      testTekst(
        { oordeel: 'loont_niet', keerSneller: 1, processor: 0.1, videokaart: 0.1 },
        nl,
        'nl'
      )
    ).toBe('Op deze computer is de processor even snel of sneller; laat dit uit.');
    expect(
      testTekst(
        { oordeel: 'videokaart_faalde', keerSneller: null, processor: 0.1, videokaart: null },
        nl,
        'nl'
      )
    ).toBe(nl.snelheidFaalde);
  });
});

describe('versieRegelApparaat', () => {
  it('zet het apparaat in de versiegegevens', () => {
    expect(versieRegelApparaat('videokaart')).toBe('Rekent op: videokaart');
    expect(versieRegelApparaat('terugval')).toContain('de videokaart gaf een fout');
    expect(versieRegelApparaat(null)).toBe('Rekent op: nog geen model geladen');
  });
});
