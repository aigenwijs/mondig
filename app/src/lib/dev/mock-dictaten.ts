/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een foutmelding, geen Error */
// De dictaatgeschiedenis (ADR 0021) in de dev-mock, zodat Dicteren en Instellingen in de gewone
// browser na te lopen zijn. Dit volgt het gedrag van Rust (`dictaten.rs` in mondig-core en de
// app), niet alleen de vorm: een dictaat dat klaar is gaat in het geheugen en, behalve bij
// Niet bewaren, in de lijst; de lijst laat verlopen dictaten al bij het lezen weg; korter
// zetten wist meteen; Niet bewaren wist alles maar houdt het laatste dictaat in het geheugen;
// een id wordt streng gecontroleerd. Elke wijziging stuurt `dictaten-gewijzigd`, en de
// interface haalt de lijst dan opnieuw op, precies als bij Rust.
//
// Scenario's (`?mock=`, naast de andere opties):
//   (niets)        24 uur, met voorbeelddictaten van vandaag en gisteren
//   dictatenweek   7 dagen, met ook een dictaat van drie dagen terug
//   dictatenleeg   24 uur, nog niets bewaard (de eerste keer)
//   dictatenniet   Niet bewaren: niets op schijf en na een herstart ook niets in het geheugen
//   dictatenwisfout  Verwijderen en Alles wissen mislukken
//   dictatenschrijffout  Een nieuw dictaat komt wel in het geheugen maar niet op schijf
//   dictaatopgeschoond   Het nieuwste voorbeelddictaat is opgeschoond (ADR 0027), met de
//                        letterlijke tekst en Mistral Small (Europees)
// De voorbeelden staan op vaste afstand van nu (en van middernacht voor "gisteren"), zodat
// "Vandaag" en "Gisteren" kloppen; "gisteren" is 23:00 de vorige dag en verloopt bij 24 uur dus
// om 23:00 vanavond.

import type { InvokeArgs } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type {
  BewaardDictaat,
  DictatenAntwoord,
  DictatenTermijn,
  LaatsteDictaat
} from '../dictaten';
import { termijnUit } from '../dictaten';
import type { OpgeschoondDoor } from '../opschonen';
import { ONBEKEND } from './mock-bestanden';

const UUR = 3_600_000;
const MINUUT = 60_000;
const DUUR_MS: Record<DictatenTermijn, number> = {
  niet: 0,
  '24u': 24 * UUR,
  '7d': 7 * 24 * UUR
};

/** Dezelfde regel als `is_dictaatnaam` in Rust: alleen onze eigen bestandsnamen. */
const GELDIG_ID = /^dictaat-[a-z0-9.-]+\.json$/;

function tekstArg(args: InvokeArgs | undefined, naam: string): string | null {
  if (
    args &&
    !Array.isArray(args) &&
    !(args instanceof ArrayBuffer) &&
    !(args instanceof Uint8Array)
  ) {
    const waarde = args[naam];
    if (typeof waarde === 'string') return waarde;
  }
  return null;
}

/** De voorbeelddictaten, nieuwste eerst. */
function voorbeelden(nu: number, week: boolean): BewaardDictaat[] {
  const middernacht = new Date(nu);
  middernacht.setHours(0, 0, 0, 0);
  const gisterenAvond = middernacht.getTime() - UUR;
  const lijst: [number, string][] = [
    [
      nu - 3 * MINUUT,
      'Ik stuur de stukken voor donderdag morgen rond, inclusief de begroting van het speelveld en de brief aan de omwonenden.'
    ],
    [nu - 40 * MINUUT, 'Beste Sanne, dank voor je reactie. Ik bel je morgen over de planning.'],
    [nu - 75 * MINUUT, 'Actiepunt: de wijkagent uitnodigen voor de tweede ronde.'],
    [
      gisterenAvond,
      'Notitie voor het verslag van de raad: het voorstel gaat in maart naar de commissie.'
    ]
  ];
  if (week) {
    lijst.push([
      nu - 3 * 24 * UUR,
      'Graag de offerte voor de training van november nog deze week naar de gemeente sturen.'
    ]);
  }
  return lijst.map(([tijdUnixMs, tekst], i) => ({
    id: `dictaat-${String(tijdUnixMs)}-${String(i)}.json`,
    tijdUnixMs,
    tekst
  }));
}

export interface DictatenMock {
  /** Antwoordt op de commando's van de geschiedenis; `ONBEKEND` voor alle andere. */
  handel(cmd: string, args: InvokeArgs | undefined): unknown;
  /**
   * Een dictaat is klaar (sneltoets of proef): zoals `na_dictaat` in Rust. Bij een opgeschoond
   * dictaat (`na_opgeschoond`) gaan de letterlijke tekst en wie het opschoonde mee.
   */
  nieuw(tekst: string, opgeschoond?: { letterlijk: string; door: OpgeschoondDoor }): void;
  /** Het laatste dictaat in het geheugen (het commando `laatste_dictaat`). */
  laatste(): string | null;
}

export function maakDictatenMock(opties: Set<string>): DictatenMock {
  let termijn: DictatenTermijn = opties.has('dictatenniet')
    ? 'niet'
    : opties.has('dictatenweek')
      ? '7d'
      : '24u';
  let bewaard: BewaardDictaat[] =
    opties.has('dictatenleeg') || termijn === 'niet'
      ? []
      : voorbeelden(Date.now(), termijn === '7d');
  const nieuwste = bewaard[0];
  if (opties.has('dictaatopgeschoond') && nieuwste) {
    bewaard[0] = {
      ...nieuwste,
      letterlijk:
        'ik stuur de stukken voor ehm donderdag morgen rond inclusief de eh de begroting van het speelveld en de brief aan de omwonenden',
      opgeschoondDoor: { model: 'mistral-small-latest', aanbieder: 'mistral', label: 'europees' }
    };
  }
  // Bij het starten met 24 uur of 7 dagen vult Rust het geheugen met het nieuwste dictaat.
  const eerste = bewaard[0];
  let geheugen: LaatsteDictaat | null = eerste ? { tekst: eerste.tekst, id: eerste.id } : null;
  let teller = 1000;

  const melding = (): void => {
    void emit('dictaten-gewijzigd');
  };

  const verlopen = (d: BewaardDictaat, nu: number): boolean =>
    d.tijdUnixMs + DUUR_MS[termijn] <= nu || d.tijdUnixMs > nu + UUR;

  const lijst = (): BewaardDictaat[] => {
    const nu = Date.now();
    return bewaard
      .filter((d) => !verlopen(d, nu))
      .sort((a, b) => b.tijdUnixMs - a.tijdUnixMs || (a.id < b.id ? 1 : -1));
  };

  /** Zoals `vergeet_gewiste` in Rust: weg als precies dat bestand verdween (op id), met het
   *  nieuwste dat nog staat in de plaats. Bij Niet bewaren blijft het geheugen zoals het is. */
  const vergeetGewiste = (ids: string[]): void => {
    if (termijn === 'niet') return;
    if (geheugen?.id != null && ids.includes(geheugen.id)) {
      const nieuwste = lijst()[0];
      geheugen = nieuwste ? { tekst: nieuwste.tekst, id: nieuwste.id } : null;
    }
  };

  /** Een wisronde zoals `ruim_op_in`: weg wat verlopen is. */
  const ruimOp = (): void => {
    const nu = Date.now();
    const weg = bewaard.filter((d) => verlopen(d, nu));
    bewaard = bewaard.filter((d) => !verlopen(d, nu));
    vergeetGewiste(weg.map((d) => d.id));
  };

  const fout = (tekst: string): never => {
    throw { melding: tekst, vervolg: 'geen' };
  };

  return {
    handel(cmd, args) {
      switch (cmd) {
        case 'dictaten_stand':
          return {
            termijn,
            dictaten: termijn === 'niet' ? [] : lijst(),
            bewaard: bewaard.length,
            laatste: geheugen
          } satisfies DictatenAntwoord;
        case 'dictaten_bewaren_zet': {
          const gekozen = tekstArg(args, 'termijn');
          if (gekozen === null || termijnUit(gekozen) !== gekozen) {
            return fout('Deze keuze bestaat niet. Kies Niet bewaren, 24 uur of 7 dagen.');
          }
          termijn = termijnUit(gekozen);
          ruimOp();
          melding();
          return null;
        }
        case 'dictaat_verwijder': {
          const id = tekstArg(args, 'id');
          if (id === null || !GELDIG_ID.test(id) || id.includes('..')) {
            return fout('Dit dictaat bestaat niet.');
          }
          if (opties.has('dictatenwisfout')) {
            return fout('Mondig kon het dictaat niet wissen.');
          }
          bewaard = bewaard.filter((d) => d.id !== id);
          vergeetGewiste([id]);
          melding();
          return null;
        }
        case 'dictaten_wis_alles':
          if (opties.has('dictatenwisfout')) {
            return fout('Mondig kon niet alles wissen. Probeer het opnieuw.');
          }
          bewaard = [];
          // Zoals `wis_alles_in`: met 24 uur of 7 dagen ook het geheugen; bij Niet bewaren niet.
          if (termijn !== 'niet') geheugen = null;
          melding();
          return null;
        case 'laatste_dictaat':
          return geheugen?.tekst ?? null;
        default:
          return ONBEKEND;
      }
    },
    nieuw(tekst, opgeschoond) {
      geheugen = { tekst, id: null };
      if (termijn !== 'niet' && !opties.has('dictatenschrijffout')) {
        const nu = Date.now();
        teller += 1;
        const id = `dictaat-${String(nu)}-${String(teller)}.json`;
        bewaard.push(
          opgeschoond
            ? {
                id,
                tijdUnixMs: nu,
                tekst,
                letterlijk: opgeschoond.letterlijk,
                opgeschoondDoor: opgeschoond.door
              }
            : { id, tijdUnixMs: nu, tekst }
        );
        // Zoals in Rust: het id komt in het geheugen zodra het bestand geschreven is.
        geheugen = { tekst, id };
      }
      melding();
    },
    laatste: () => geheugen?.tekst ?? null
  };
}
