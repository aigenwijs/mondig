/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een gewone waarde (een foutmelding), geen Error */
// Alleen voor ontwikkeling in de gewone browser: een kleine nabootsing van de herkenning van
// persoonsgegevens (V-2), zodat de markeringen, het paneel en het vervangen te proberen zijn. De
// echte regels staan in Rust (`mondig-core`, persoonsgegevens.rs); dit herkent alleen e-mail,
// telefoon, datum, IBAN-achtige reeksen en een BSN met de elfproef.

import type { Markering, PersoonStand, Soort } from '../verslag';

const PLAATSVERVANGERS: Record<Soort, string> = {
  bsn: '[BSN]',
  iban: '[IBAN]',
  telefoon: '[TELEFOON]',
  email: '[E-MAIL]',
  adres: '[ADRES]',
  datum: '[DATUM]',
  kenteken: '[KENTEKEN]',
  handmatig: '[GEGEVEN]'
};

function elfproef(cijfers: string): boolean {
  if (!/^\d{9}$/.test(cijfers) || /^(\d)\1{8}$/.test(cijfers)) return false;
  const som = Array.from(cijfers).reduce((s, c, i) => s + Number(c) * (i === 8 ? -1 : 9 - i), 0);
  return som % 11 === 0;
}

/** De mock-staat van de markeringen van één verslag. */
export interface MockPersoon {
  vervangen: boolean;
  genegeerd: Set<string>;
  eigen: Set<string>;
}

export function nieuwMockPersoon(): MockPersoon {
  return { vervangen: false, genegeerd: new Set(), eigen: new Set() };
}

export function sleutel(tekst: string): string {
  return tekst.trim().replace(/\s+/g, ' ').toLowerCase();
}

/** De markeringen in een tekst, zonder overlap en op volgorde. */
export function mockMarkeringen(tekst: string, persoon: MockPersoon): Markering[] {
  const lijst: Markering[] = [];
  const voeg = (soort: Soort, van: number, tot: number): void => {
    if (persoon.genegeerd.has(sleutel(tekst.slice(van, tot)))) return;
    if (lijst.some((m) => van < m.tot && tot > m.van)) return;
    lijst.push({ soort, van, tot });
  };
  const patronen: [Soort, RegExp][] = [
    ['email', /[\w.+-]+@[\w-]+(?:\.[\w-]+)*\.[a-z]{2,}/gi],
    ['telefoon', /(?:\+31|0)[\s-]?[1-9](?:[\s-]?\d){8}/g],
    ['datum', /\b\d{1,2}[-/.]\d{1,2}[-/.]\d{4}\b/g],
    ['iban', /\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]{4}){3}(?:\s?[A-Z0-9]{1,4})?\b/g]
  ];
  for (const [soort, patroon] of patronen) {
    for (const m of tekst.matchAll(patroon)) voeg(soort, m.index, m.index + m[0].length);
  }
  for (const m of tekst.matchAll(/\b\d(?:[ -]?\d){8}\b/g)) {
    if (elfproef(m[0].replace(/\D/g, ''))) voeg('bsn', m.index, m.index + m[0].length);
  }
  for (const eigen of persoon.eigen) {
    const patroon = new RegExp(
      `(?<![\\p{L}\\p{N}])${eigen.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}\\p{N}])`,
      'giu'
    );
    for (const m of tekst.matchAll(patroon)) voeg('handmatig', m.index, m.index + m[0].length);
  }
  return lijst.sort((a, b) => a.van - b.van);
}

/** De tekst voor Kopiëren: vervangen als de schakelaar aan staat. */
export function mockUitvoer(tekst: string, persoon: MockPersoon): string {
  if (!persoon.vervangen) return tekst;
  let uit = '';
  let positie = 0;
  for (const m of mockMarkeringen(tekst, persoon)) {
    uit += tekst.slice(positie, m.van) + PLAATSVERVANGERS[m.soort];
    positie = m.tot;
  }
  return uit + tekst.slice(positie);
}

/**
 * Een naam of titel voor Kopiëren en opslaan, zoals `Markeringen::voor_naam` in Rust: vervangen als
 * de schakelaar aan staat, maar een datum blijft staan, want die is in een naam meestal gewoon een
 * datum.
 */
export function mockNaam(tekst: string, persoon: MockPersoon): string {
  if (!persoon.vervangen) return tekst;
  let uit = '';
  let positie = 0;
  for (const m of mockMarkeringen(tekst, persoon)) {
    if (m.soort === 'datum') continue;
    uit += tekst.slice(positie, m.van) + PLAATSVERVANGERS[m.soort];
    positie = m.tot;
  }
  return uit + tekst.slice(positie);
}

export function mockPersoonStand(persoon: MockPersoon): PersoonStand {
  return { vervangen: persoon.vervangen, eigen: [...persoon.eigen] };
}

/** Voert een van de drie commando's uit op de mock-staat; `true` als het commando bekend was. */
export function pasMockPersoonToe(
  persoon: MockPersoon,
  wat: 'vervangen' | 'weghalen' | 'markeer',
  waarde: unknown
): void {
  if (wat === 'vervangen') {
    persoon.vervangen = waarde === true;
    return;
  }
  const s = sleutel(typeof waarde === 'string' ? waarde : '');
  if (wat === 'weghalen') {
    if (!persoon.eigen.delete(s)) persoon.genegeerd.add(s);
    return;
  }
  if (s.length < 2) throw { melding: 'Markeer minstens twee tekens.', vervolg: 'geen' };
  persoon.genegeerd.delete(s);
  persoon.eigen.add(s);
}
