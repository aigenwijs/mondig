// De naam van het bestand in de dev-mock (UI-E): de bouwstenen, het standaardformaat en de
// klantnamen, gedeeld door de mock van Bestanden en die van Opnemen, zodat het voorbeeld en de
// suggesties op beide schermen hetzelfde doen. Een eenvoudige versie van wat Rust uitrekent
// (`mondig-core/src/bewaren/naam.rs`), zonder de regels voor gereserveerde namen van Windows.
// Opties via `?mock=`:
//   geenklanten: Mondig onthoudt nog geen klantnamen (Instellingen toont de uitleg, geen knop)
//   klantwisfout: "Verwijderen" van de klantnamen mislukt

import type { InvokeArgs } from '@tauri-apps/api/core';
import type { Bouwsteen, Formaat } from '../bewaren';
import type { Vergadersoort } from '../vergadering';

/** Twaalf klantnamen, zoals in het ontwerp (scherm 6: "12 klantnamen"). */
const KLANTEN_START = [
  'Gemeente Vlaardingen',
  'Gemeente Meerwijk',
  'Gemeente Schiedam',
  'Provincie Zuid-Holland',
  'Waterschap Hollandse Delta',
  'Stichting Buurtkracht',
  'Woonstichting Maasdelta',
  'Veiligheidsregio Rotterdam-Rijnmond',
  'Havenbedrijf Rotterdam',
  'Omgevingsdienst Rijnmond',
  'Rijkswaterstaat West-Nederland',
  'Dienst Gezondheid en Jeugd'
];

export interface MockNaamstand {
  bouwstenen: Bouwsteen[];
  formaat: Formaat;
  klanten: string[];
  klantWisFout: boolean;
}

let stand: MockNaamstand | null = null;

/** Begint opnieuw, volgens de opties van deze start van de mock. */
export function resetNaamstand(opties: Set<string>): MockNaamstand {
  stand = {
    bouwstenen: ['datum', 'klant', 'onderwerp'],
    formaat: 'docx',
    klanten: opties.has('geenklanten') ? [] : [...KLANTEN_START],
    klantWisFout: opties.has('klantwisfout')
  };
  return stand;
}

/** De stand die de mocks delen. */
export function naamstand(): MockNaamstand {
  return stand ?? resetNaamstand(new Set());
}

/** Onthoudt een klantnaam, zoals Rust na een geslaagd opslaan: nieuwste eerst, hooguit 50. */
export function onthoudKlant(naam: string): void {
  const schoon = naam.trim().replace(/\s+/g, ' ').slice(0, 80);
  if (schoon === '') return;
  const s = naamstand();
  s.klanten = [schoon, ...s.klanten.filter((k) => k.toLowerCase() !== schoon.toLowerCase())].slice(
    0,
    50
  );
}

/** De woorden "Vergadering" en "Notitie" in de taal van de app, zoals de interface ze aan Rust geeft. */
const soortwoorden = { vergadering: 'Vergadering', notitie: 'Notitie' };

/** Onthoudt de soortwoorden (`vergadering_zet_soortwoorden`). */
export function zetMockSoortwoorden(vergadering: string, notitie: string): void {
  soortwoorden.vergadering = vergadering;
  soortwoorden.notitie = notitie;
}

/** Het soortwoord bij een soort, het standaardonderwerp van een verslag. */
export function mockSoortwoord(soort: Vergadersoort): string {
  return soort === 'alleen' ? soortwoorden.notitie : soortwoorden.vergadering;
}

/** Een tekstargument van een commando; leeg als het er niet is. */
export function tekstUit(args: InvokeArgs | undefined, sleutel: string): string {
  const waarde =
    args && !Array.isArray(args) && !(args instanceof ArrayBuffer) && !(args instanceof Uint8Array)
      ? args[sleutel]
      : undefined;
  return typeof waarde === 'string' ? waarde : '';
}

const twee = (n: number): string => String(n).padStart(2, '0');

/** De datum zoals de naam hem standaard heeft: 2026-10-30. */
export function mockStandaardDatum(moment: Date): string {
  return `${String(moment.getFullYear())}-${twee(moment.getMonth() + 1)}-${twee(moment.getDate())}`;
}

/** Wat de gebruiker typte tegenover de standaard: gelijk aan de standaard is "nooit aangeraakt". */
export function mockEigenWaarde(getypt: string, standaard: string): string | null {
  const schoon = getypt.trim().replace(/\s+/g, ' ');
  return schoon === standaard.trim() ? null : schoon;
}

export interface MockNaamDelen {
  /** De datum zoals getypt, of `null`: de datum van `moment`. */
  datum: string | null;
  klant: string;
  /** Het onderwerp zoals het geldt (de standaard als de gebruiker niets typte). */
  onderwerp: string;
  /** Wat de naam wordt als er niets overblijft. */
  terugval: string;
  moment: Date;
}

/** De bestandsnaam uit de bouwstenen, zoals Rust: lege delen vallen weg. */
export function mockNaamVoor(
  bouwstenen: Bouwsteen[],
  delen: MockNaamDelen,
  formaat: Formaat
): string {
  const schoon = (tekst: string): string => tekst.replace(/[<>:"/\\|?*]/g, '-').trim();
  const stukken = bouwstenen
    .map((steen) => {
      switch (steen) {
        case 'datum':
          return delen.datum ?? mockStandaardDatum(delen.moment);
        case 'klant':
          return delen.klant;
        case 'onderwerp':
          return delen.onderwerp;
        case 'tijd':
          return `${twee(delen.moment.getHours())}.${twee(delen.moment.getMinutes())}`;
        case 'model':
          return 'Parakeet Ultra';
      }
    })
    .map(schoon)
    .filter((deel) => deel !== '');
  return `${stukken.join(' ') || schoon(delen.terugval) || 'Uitschrift'}.${formaat}`;
}
