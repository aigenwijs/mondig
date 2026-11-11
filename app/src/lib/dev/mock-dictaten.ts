/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een foutmelding, geen Error */
// De dictaatgeschiedenis (ADR 0021) in de dev-mock, zodat Dicteren en Instellingen in de gewone
// browser na te lopen zijn. Dit volgt het gedrag van Rust (`dictaten.rs` in mondig-core en de
// app), niet alleen de vorm: een dictaat dat klaar is gaat in het geheugen en, behalve bij
// Niet bewaren, in de lijst; de lijst laat verlopen dictaten al bij het lezen weg; korter
// zetten wist meteen; Niet bewaren wist alles maar houdt het laatste dictaat in het geheugen;
// een id wordt streng gecontroleerd. Elke wijziging stuurt `dictaten-gewijzigd`, en de
// interface haalt de lijst dan opnieuw op, precies als bij Rust.
//
// De lijst komt in delen, zoals bij Rust: `dictaten_stand` krijgt een begin en een limiet en
// geeft ook het totaal. Laatste 10 houdt de tien nieuwste en wist de rest direct.
//
// Scenario's (`?mock=`, naast de andere opties):
//   (niets)        24 uur, met voorbeelddictaten van vandaag en gisteren
//   dictatenweek   7 dagen, met 38 dictaten verspreid over de week (genoeg voor "Meer tonen")
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
import { LIJST_EERSTE, termijnUit } from '../dictaten';
import type { OpgeschoondDoor } from '../opschonen';
import { ONBEKEND } from './mock-bestanden';

const UUR = 3_600_000;
const MINUUT = 60_000;
const DUUR_MS: Record<DictatenTermijn, number> = {
  niet: 0,
  // Laatste 10 heeft geen tijdgrens; het aantal regelt `teWissen`.
  laatste10: Number.POSITIVE_INFINITY,
  '24u': 24 * UUR,
  '7d': 7 * 24 * UUR,
  '30d': 30 * 24 * UUR
};

const ONBEKENDE_KEUZE =
  'Deze keuze bestaat niet. Kies Niet bewaren, Laatste 10, 24 uur, 7 dagen of 30 dagen.';

/** Dezelfde regel als `is_dictaatnaam` in Rust: alleen onze eigen bestandsnamen. */
const GELDIG_ID = /^dictaat-[a-z0-9.-]+\.json$/;

function getalArg(args: InvokeArgs | undefined, naam: string): number | null {
  if (
    args &&
    !Array.isArray(args) &&
    !(args instanceof ArrayBuffer) &&
    !(args instanceof Uint8Array)
  ) {
    const waarde = args[naam];
    if (typeof waarde === 'number') return waarde;
  }
  return null;
}

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
    // Een lange week: ruim meer dan tien dictaten, zodat "Meer tonen" te zien is. Alles ligt
    // binnen de 7 dagen, op vaste afstand van nu.
    const zinnen = [
      'Herinner me eraan de agenda voor het overleg van dinsdag door te sturen.',
      'De begroting moet nog langs de controller voor we hem naar de raad sturen.',
      'Bel de beheerder van het buurthuis over de sleutel voor zaterdag.',
      'Notitie: de subsidieaanvraag moet voor het einde van de maand binnen zijn.',
      'Vraag aan Jeroen of de cijfers van het derde kwartaal al klaar zijn.',
      'Het concept van de brief aan de omwonenden mag een stuk korter.'
    ];
    for (let i = 0; i < 33; i += 1) {
      lijst.push([
        nu - Math.round((1.6 + i * 0.14) * 24 * UUR),
        `${zinnen[i % zinnen.length] ?? ''} (${String(i + 1)})`
      ]);
    }
    lijst.sort((a, b) => b[0] - a[0]);
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

  const nieuwsteEerst = (a: BewaardDictaat, b: BewaardDictaat): number =>
    b.tijdUnixMs - a.tijdUnixMs || (a.id < b.id ? 1 : -1);

  /** Wat bij `doel` mag blijven: niet verlopen en, bij Laatste 10, hoogstens de tien nieuwste. */
  const geldend = (doel: DictatenTermijn, nu: number): BewaardDictaat[] => {
    const over = bewaard
      .filter((d) => d.tijdUnixMs + DUUR_MS[doel] > nu && d.tijdUnixMs <= nu + UUR)
      .sort(nieuwsteEerst);
    return doel === 'laatste10' ? over.slice(0, 10) : over;
  };

  const lijst = (): BewaardDictaat[] => geldend(termijn, Date.now());

  /** Wat zou wegvallen als `doel` gold: zoals `te_wissen` in Rust. */
  const teWissen = (doel: DictatenTermijn, nu: number): BewaardDictaat[] => {
    const blijft = new Set(geldend(doel, nu).map((d) => d.id));
    return bewaard.filter((d) => !blijft.has(d.id));
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

  /** Een wisronde zoals `ruim_op_in`: weg wat verlopen is, en bij Laatste 10 de rest. */
  const ruimOp = (): void => {
    const weg = teWissen(termijn, Date.now());
    const ids = new Set(weg.map((d) => d.id));
    bewaard = bewaard.filter((d) => !ids.has(d.id));
    vergeetGewiste(weg.map((d) => d.id));
  };

  const fout = (tekst: string): never => {
    throw { melding: tekst, vervolg: 'geen' };
  };

  return {
    handel(cmd, args) {
      switch (cmd) {
        case 'dictaten_stand': {
          const alle = termijn === 'niet' ? [] : lijst();
          const begin = getalArg(args, 'begin') ?? 0;
          const limiet = getalArg(args, 'limiet') ?? LIJST_EERSTE;
          return {
            termijn,
            dictaten: alle.slice(begin, begin + limiet),
            totaal: alle.length,
            bewaard: bewaard.length,
            laatste: geheugen
          } satisfies DictatenAntwoord;
        }
        case 'dictaten_wegval': {
          const doel = tekstArg(args, 'termijn');
          if (doel === null || termijnUit(doel) !== doel) return fout(ONBEKENDE_KEUZE);
          return teWissen(termijnUit(doel), Date.now()).length;
        }
        case 'dictaten_bewaren_zet': {
          const gekozen = tekstArg(args, 'termijn');
          if (gekozen === null || termijnUit(gekozen) !== gekozen) {
            return fout(ONBEKENDE_KEUZE);
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
            return fout('Mondig kon het dictaat niet verwijderen.');
          }
          bewaard = bewaard.filter((d) => d.id !== id);
          vergeetGewiste([id]);
          melding();
          return null;
        }
        case 'dictaten_wis_alles':
          if (opties.has('dictatenwisfout')) {
            return fout('Mondig kon niet alles verwijderen. Probeer het opnieuw.');
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
        // Zoals in Rust: bij Laatste 10 gaat na elk nieuw dictaat alles behalve de tien
        // nieuwste direct weg.
        if (termijn === 'laatste10') ruimOp();
      }
      melding();
    },
    laatste: () => geheugen?.tekst ?? null
  };
}
