import { describe, expect, it } from 'vitest';
import {
  BOUWSTENEN,
  FORMATEN,
  bouwsteenNaam,
  formaatExtensie,
  formaatNaam,
  mapTekst,
  plekVanBouwsteen,
  schuifBouwsteen,
  steenVolgorde,
  verplaatsBouwsteen,
  volgnummer,
  wisselBouwsteen
} from './bewaren';
import { en } from './i18n/en';
import { nl } from './i18n/nl';

describe('formaten', () => {
  it('namen die een leek kent, met de extensie erachter', () => {
    expect(FORMATEN.map((f) => formaatNaam(f, nl))).toEqual([
      'Word',
      'PDF voor archief',
      'LibreOffice',
      'Platte tekst',
      'Markdown',
      'Webpagina',
      'Ondertitels',
      'Ondertitels voor het web'
    ]);
    expect(formaatExtensie('docx')).toBe('.docx');
    expect(formaatExtensie('md')).toBe('.md');
    expect(formaatExtensie('html')).toBe('.html');
    expect(formaatExtensie('pdf')).toBe('.pdf (PDF/A)');
    expect(formaatNaam('pdf', en)).toBe('PDF for archiving');
  });
});

describe('bouwstenen van de bestandsnaam', () => {
  it('een klik zet een blokje achteraan of haalt het eruit', () => {
    expect(wisselBouwsteen(['datum', 'naam'], 'label')).toEqual(['datum', 'naam', 'label']);
    expect(wisselBouwsteen(['datum', 'naam', 'label'], 'naam')).toEqual(['datum', 'label']);
    expect(wisselBouwsteen([], 'model')).toEqual(['model']);
  });

  it('eerst de gekozen blokjes in hun volgorde, dan de rest', () => {
    expect(steenVolgorde(['label', 'datum'])).toEqual(['label', 'datum', 'naam', 'tijd', 'model']);
    expect(steenVolgorde([])).toEqual([...BOUWSTENEN]);
  });

  it('heeft een naam voor elk blokje', () => {
    expect(BOUWSTENEN.map((s) => bouwsteenNaam(s, nl))).toEqual([
      'Datum',
      'Naam van de opname',
      'Label',
      'Tijd',
      'Model'
    ]);
  });
});

describe('de map zoals de gebruiker hem kent', () => {
  it('in Documenten met de naam van Windows', () => {
    const map = { pad: 'C:\\Users\\Jan\\Documents\\Mondig', inDocumenten: 'Mondig' };
    expect(mapTekst(map, nl)).toBe('Documenten\\Mondig');
    expect(mapTekst(map, en)).toBe('Documents\\Mondig');
  });

  it('Documenten zelf, een andere map of nog geen map', () => {
    expect(mapTekst({ pad: '/home/jan/Documents', inDocumenten: '' }, nl)).toBe('Documenten');
    expect(mapTekst({ pad: '/home/jan/Documents/Mondig', inDocumenten: 'Mondig' }, nl)).toBe(
      'Documenten/Mondig'
    );
    expect(mapTekst({ pad: 'D:\\Archief\\Zaken', inDocumenten: null }, nl)).toBe(
      'D:\\Archief\\Zaken'
    );
    expect(mapTekst(null, nl)).toBe('Nog geen map gekozen');
  });
});

describe('het volgnummer voor "Opslaan als (2)"', () => {
  it('haalt het nummer uit de voorgestelde naam', () => {
    expect(volgnummer('2026-11-01 Interview wethouder (2).docx')).toBe('(2)');
    expect(volgnummer('Verslag v1.2 (13).pdf')).toBe('(13)');
    expect(volgnummer('zonder nummer.docx')).toBe('');
  });
});

describe('volgorde van de bestandsnaam', () => {
  it('schuift een blokje naar voren of naar achteren', () => {
    expect(schuifBouwsteen(['datum', 'naam'], 'datum', 1)).toEqual(['naam', 'datum']);
    expect(schuifBouwsteen(['datum', 'naam'], 'naam', -1)).toEqual(['naam', 'datum']);
    expect(schuifBouwsteen(['datum', 'naam', 'tijd'], 'naam', 1)).toEqual([
      'datum',
      'tijd',
      'naam'
    ]);
  });

  it('blijft binnen de rij en laat een uitgezet blokje met rust', () => {
    expect(schuifBouwsteen(['datum', 'naam'], 'datum', -1)).toEqual(['datum', 'naam']);
    expect(schuifBouwsteen(['datum', 'naam'], 'naam', 1)).toEqual(['datum', 'naam']);
    expect(schuifBouwsteen(['datum', 'naam'], 'model', 1)).toEqual(['datum', 'naam']);
  });

  it('verplaatst een blokje naar een gekozen plek', () => {
    expect(verplaatsBouwsteen(['datum', 'naam', 'tijd'], 'tijd', 0)).toEqual([
      'tijd',
      'datum',
      'naam'
    ]);
    expect(verplaatsBouwsteen(['datum', 'naam', 'tijd'], 'datum', 99)).toEqual([
      'naam',
      'tijd',
      'datum'
    ]);
    expect(verplaatsBouwsteen(['datum', 'naam'], 'datum', -3)).toEqual(['datum', 'naam']);
  });

  it('wijzigt de lijst die erin gaat niet', () => {
    const lijst = Object.freeze(['datum', 'naam'] as const);
    expect(schuifBouwsteen(lijst, 'datum', 1)).toEqual(['naam', 'datum']);
    expect(lijst).toEqual(['datum', 'naam']);
  });

  it('geeft de plek vanaf 1 voor de aankondiging', () => {
    expect(plekVanBouwsteen(['naam', 'datum'], 'datum')).toBe(2);
    expect(plekVanBouwsteen(['naam', 'datum'], 'model')).toBeNull();
  });

  it('de aankondiging noemt blokje, plek en totaal, in beide talen', () => {
    expect(nl.bestandsnaamPlek('Datum', 3, 4)).toBe('Datum staat nu op plek 3 van 4');
    expect(en.bestandsnaamPlek('Date', 3, 4)).toBe('Date is now at position 3 of 4');
  });

  it('een gewisseld blokje komt achteraan, daarna is het te verplaatsen', () => {
    const met = wisselBouwsteen(['datum', 'naam'], 'tijd');
    expect(verplaatsBouwsteen(met, 'tijd', 0)).toEqual(['tijd', 'datum', 'naam']);
  });
});
