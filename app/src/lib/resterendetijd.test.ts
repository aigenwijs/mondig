import { describe, expect, it } from 'vitest';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import { resterendeTijdTekst, schatResterendeSeconden, type Meting } from './resterendetijd';

/** Metingen met een vaste snelheid: elke 250 ms komt `bytesPerKwartSeconde` binnen. */
function constant(aantal: number, bytesPerKwartSeconde: number, begin = 0): Meting[] {
  return Array.from({ length: aantal }, (_, i) => ({
    tijd: i * 250,
    binnen: begin + i * bytesPerKwartSeconde
  }));
}

describe('schatResterendeSeconden', () => {
  it('geeft niets zolang er nog geen paar seconden gemeten is', () => {
    expect(schatResterendeSeconden([], 1000)).toBeNull();
    expect(schatResterendeSeconden(constant(1, 100), 1_000_000)).toBeNull();
    // 2,75 seconden aan metingen is nog te weinig.
    expect(schatResterendeSeconden(constant(12, 100), 1_000_000)).toBeNull();
  });

  it('rekent met de snelheid van de metingen', () => {
    // 4 MB per seconde; 3,0 seconden gemeten, 12 MB binnen; nog 28 MB te gaan van 40 MB.
    const metingen = constant(13, 1_000_000);
    expect(schatResterendeSeconden(metingen, 40_000_000)).toBeCloseTo(7, 5);
  });

  it('telt een hervatte download niet vanaf nul: alleen het verschil telt', () => {
    // De download begon bij 500 MB binnen; de snelheid blijft 4 MB per seconde.
    const metingen = constant(13, 1_000_000, 500_000_000);
    expect(schatResterendeSeconden(metingen, 540_000_000)).toBeCloseTo(7, 5);
  });

  it('kijkt alleen naar de laatste tien seconden', () => {
    // Eerst twintig seconden langzaam (1 MB/s), daarna tien seconden snel (10 MB/s).
    const langzaam = Array.from({ length: 80 }, (_, i) => ({
      tijd: i * 250,
      binnen: i * 250_000
    }));
    const laatsteBinnen = 79 * 250_000;
    const snel = Array.from({ length: 41 }, (_, i) => ({
      tijd: 20_000 + i * 250,
      binnen: laatsteBinnen + i * 2_500_000
    }));
    const eind = snel.at(-1);
    const totaal = (eind?.binnen ?? 0) + 100_000_000;
    // Honderd megabyte met 10 MB/s: tien seconden.
    expect(schatResterendeSeconden([...langzaam, ...snel], totaal)).toBeCloseTo(10, 5);
  });

  it('geeft niets als de download stilstaat', () => {
    const stil = Array.from({ length: 20 }, (_, i) => ({ tijd: i * 250, binnen: 5_000 }));
    expect(schatResterendeSeconden(stil, 1_000_000)).toBeNull();
  });

  it('geeft nul als alles binnen is', () => {
    expect(schatResterendeSeconden([{ tijd: 0, binnen: 1000 }], 1000)).toBe(0);
  });
});

describe('resterendeTijdTekst', () => {
  it('zet seconden om in gewone taal', () => {
    expect(resterendeTijdTekst(40, nl)).toBe('Nog ongeveer 40 seconden');
    expect(resterendeTijdTekst(42, nl)).toBe('Nog ongeveer 40 seconden');
    expect(resterendeTijdTekst(1, nl)).toBe('Nog ongeveer 5 seconden');
  });

  it('gaat over op minuten vlak onder de minuut', () => {
    expect(resterendeTijdTekst(57, nl)).toBe('Nog ongeveer 55 seconden');
    expect(resterendeTijdTekst(58, nl)).toBe('Nog ongeveer 1 minuut');
    expect(resterendeTijdTekst(89, nl)).toBe('Nog ongeveer 1 minuut');
    expect(resterendeTijdTekst(170, nl)).toBe('Nog ongeveer 3 minuten');
  });

  it('gaat over op uren bij anderhalf uur of meer', () => {
    expect(resterendeTijdTekst(89 * 60, nl)).toBe('Nog ongeveer 89 minuten');
    expect(resterendeTijdTekst(2 * 3600, nl)).toBe('Nog ongeveer 2 uur');
  });

  it('bestaat ook in het Engels', () => {
    expect(resterendeTijdTekst(40, en)).toBe('About 40 seconds left');
    expect(resterendeTijdTekst(60, en)).toBe('About 1 minute left');
    expect(resterendeTijdTekst(180, en)).toBe('About 3 minutes left');
  });
});
