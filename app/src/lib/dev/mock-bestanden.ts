/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een gewone waarde (een foutmelding), geen Error */
// De dev-mock voor het scherm Bestanden (sprint 4-B), los van mock.ts zodat die niet botst met
// ander werk. Opties via `?mock=` (komma's ertussen):
//   wachtrij:   de rij uit het ontwerp: een klaar bestand, een dat loopt (43 procent), een dat
//               wacht en een avi die Mondig niet kan lezen
//   slepen:     doet alsof er bestanden boven het venster hangen
//   geenmodel:  kiezen geeft "het spraakmodel is er nog niet"
//   afgebroken: het transcript krijgt de waarschuwing voor een afgebroken bestand
//   bestaat:    in de standaardmap staat al "2026-10-30 Interview wethouder.docx", zodat
//               Opslaan de vraag "Er staat al een bestand met deze naam" geeft (4-C)
//   automatisch: automatisch opslaan staat aan; een bestand dat klaar is, wordt meteen
//               opgeslagen (4-C)
//   sprekers:   het transcript heeft herkende sprekers (Spreker 1 en 2), zoals na de schakelaar
//               "Sprekers herkennen" (4b-D)
//   bewaakt:    de bewaakte map staat aan op een OneDrive-map (B-1); in de wachtrij komen twee
//               bestanden uit die map
//   bewaarfout: opslaan mislukt met "Mondig mag niet schrijven in deze map." (4-C)
//   geenaudio:  het bronbestand is weg: de regels hebben geen afspeelknop (V-1)
// "Bestanden kiezen" voegt twee bestanden toe die in een paar seconden klaar zijn.
// Bewaren (4-C): de schijf is een lijst namen in het geheugen; "Andere map kiezen" en "Opslaan
// op een andere plek" kiezen altijd D:\Archief\Zaken.

import type { InvokeArgs } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { BewaakInstellingen } from '../bewaakte-map';
import type { BestandenStand, BestandInfo, BestandTranscript } from '../bestanden';
import type {
  BewaarInstellingen,
  BewaarUitslag,
  Bouwsteen,
  Formaat,
  MapInfo,
  VerwijderUitslag
} from '../bewaren';
import type { Foutmelding, Gebeurtenis, Toestand } from '../taak';
import { maakTestWav } from './mock-audio';
import {
  mockMarkeringen,
  mockPersoonStand,
  mockUitvoer,
  nieuwMockPersoon,
  pasMockPersoonToe
} from './mock-persoon';

/** Het antwoord als de mock het commando niet kent. */
export const ONBEKEND = Symbol('onbekend');

const AVI_MELDING =
  'Mondig kan dit formaat niet lezen. Sla het bestand op als mp4 of wav, bijvoorbeeld met Clipchamp, en voeg het opnieuw toe.';

const REGELS = [
  {
    index: 0,
    startMs: 0,
    eindMs: 7000,
    tekst:
      'Goedemiddag, fijn dat u tijd heeft. We hebben het vandaag over de plannen voor het centrum.'
  },
  {
    index: 1,
    startMs: 7000,
    eindMs: 15000,
    tekst: "Ja, dank u. Het belangrijkste is dat we de markt behouden, maar met minder auto's."
  },
  {
    index: 2,
    startMs: 15000,
    eindMs: 18000,
    tekst:
      'Hoe reageren de ondernemers daarop? U kunt mij bereiken op 06 12345678 of jan@example.com.'
  },
  {
    index: 3,
    startMs: 18000,
    eindMs: 26000,
    tekst: 'Wisselend. Een deel ziet kansen, een deel maakt zich zorgen over de bereikbaarheid.'
  }
];

/** De regels met de sprekers die het model herkende: de vraagsteller en de wethouder. */
const SPREKER_REGELS = REGELS.map((regel, i) => ({
  ...regel,
  spreker: `Spreker ${i === 0 || i === 2 ? '1' : '2'}`
}));

export function maakBestandenMock(
  opties: Set<string>,
  volgendId: () => number
): (cmd: string, args: InvokeArgs | undefined) => unknown {
  const lijst: BestandInfo[] = [];
  // De regels van het transcript; een verbetering past ze hier aan, zoals Rust dat doet.
  const tekstregels = structuredClone(opties.has('sprekers') ? SPREKER_REGELS : REGELS);
  // De markeringen van persoonsgegevens (V-2) van het transcript dat open is.
  const persoon = nieuwMockPersoon();
  const regelsMet = (): BestandTranscript['regels'] =>
    tekstregels.map((r) => ({
      ...r,
      markeringen: mockMarkeringen(r.tekst, persoon),
      uitvoer: mockUitvoer(r.tekst, persoon)
    }));
  const timers = new Map<number, number>();
  let versie = 0;
  let fout: Foutmelding | null = null;

  /** De stand met een nieuw volgnummer, zoals Rust die na elke wijziging stuurt. */
  const stand = (): BestandenStand => {
    versie += 1;
    return { versie, lijst: structuredClone(lijst), fout };
  };
  const meldStand = (): void => {
    void emit('bestanden-gewijzigd', stand());
  };

  const meld = (taak: number, toestand: Toestand): void => {
    const item = lijst.find((b) => b.taak === taak);
    if (item) item.toestand = toestand;
    meldStand();
    void emit('taak', { taak, soort: 'bestand', toestand } satisfies Gebeurtenis);
  };

  const nieuw = (naam: string, duurMs: number | null, toestand: Toestand): BestandInfo => {
    const info: BestandInfo = {
      taak: volgendId(),
      naam,
      video: /\.(mp4|mov|mkv|webm|avi)$/i.test(naam),
      duurMs,
      toestand,
      verwerkingMs: toestand.type === 'klaar' ? 108_000 : null,
      heeftTranscript: toestand.type === 'klaar',
      opgeslagen: null,
      opgeslagenVerouderd: false,
      bewaarFout: null,
      uitMap: false,
      overgeslagen: false
    };
    lijst.push(info);
    return info;
  };

  /** Laat een bestand in stappen van 5 procent "uitschrijven". */
  const draai = (taak: number, vanaf: number, stapMs: number): void => {
    let procent = vanaf;
    const start = performance.now();
    meld(taak, { type: 'verwerkt', voortgang: procent });
    const timer = window.setInterval(() => {
      procent = Math.min(100, procent + 5);
      meld(taak, { type: 'verwerkt', voortgang: procent });
      if (procent >= 100) {
        window.clearInterval(timer);
        timers.delete(taak);
        const item = lijst.find((b) => b.taak === taak);
        if (item) {
          item.verwerkingMs = Math.round(performance.now() - start);
          item.heeftTranscript = true;
        }
        if (instellingen.automatisch) bewaar(taak, instellingen.formaat, standaardMap);
        meld(taak, { type: 'klaar' });
        volgende();
      }
    }, stapMs);
    timers.set(taak, timer);
  };

  /** Start het eerste wachtende bestand als er niets loopt. */
  const volgende = (): void => {
    if (lijst.some((b) => b.toestand.type === 'verwerkt')) return;
    const wacht = lijst.find((b) => b.toestand.type === 'wacht');
    if (wacht) draai(wacht.taak, 0, 300);
  };

  if (opties.has('wachtrij')) {
    nieuw('Interview wethouder.m4a', 1_394_000, { type: 'klaar' });
    const teams = nieuw('Teams-overleg 30 okt.mp4', 3_760_000, {
      type: 'verwerkt',
      voortgang: 43
    });
    const ahmed = nieuw('Spraakbericht Ahmed.opus', 125_000, { type: 'wacht' });
    const dictafoon = nieuw('Dictafoon 2 nov.m4a', 2_400_000, { type: 'wacht' });
    if (opties.has('bewaakt')) {
      ahmed.uitMap = true;
      dictafoon.uitMap = true;
      const overgeslagen = nieuw('Teams-overleg 29 okt.mp4', null, { type: 'geannuleerd' });
      overgeslagen.uitMap = true;
      overgeslagen.overgeslagen = true;
    }
    nieuw('Oude opname.avi', null, { type: 'fout', melding: AVI_MELDING, vervolg: 'geen' });
    // Langzaam verder, zodat de schatting van de resterende tijd verschijnt.
    window.setTimeout(() => {
      draai(teams.taak, 43, 4000);
    }, 500);
  }
  if (opties.has('slepen')) {
    window.setTimeout(() => void emit('bestanden-slepen', true), 600);
  }

  const argUit = (args: InvokeArgs | undefined, sleutel: string): unknown =>
    args && !Array.isArray(args) && !(args instanceof ArrayBuffer) && !(args instanceof Uint8Array)
      ? args[sleutel]
      : undefined;
  const taakUit = (args: InvokeArgs | undefined): number => Number(argUit(args, 'taak'));

  // Bewaren (4-C): de instellingen, de "schijf" en per transcript het label en de bestanden.
  const standaardMap: MapInfo = {
    pad: 'C:\\Users\\Jan\\Documents\\Mondig',
    inDocumenten: 'Mondig'
  };
  const andereMap: MapInfo = { pad: 'D:\\Archief\\Zaken', inDocumenten: null };
  const instellingen = {
    map: standaardMap,
    bouwstenen: ['datum', 'naam'] as Bouwsteen[],
    formaat: 'docx' as Formaat,
    tijden: true,
    automatisch: opties.has('automatisch'),
    audio: opties.has('audionaast')
  };
  const schijf = new Set<string>();
  const labels = new Map<number, string>();
  const opgeslagen = new Map<number, { naam: string; map: MapInfo }[]>();
  let vraag: { id: number; taak: number; formaat: Formaat; map: MapInfo; naam: string } | null =
    null;
  const KLAAR_OP = Date.parse('2026-10-30T14:00:00');

  /** Een eenvoudige versie van de naam uit Rust (zonder alle regels voor Windows). */
  const naamVoor = (
    bouwstenen: Bouwsteen[],
    naam: string,
    label: string,
    moment: Date,
    formaat: Formaat
  ): string => {
    const twee = (n: number): string => String(n).padStart(2, '0');
    const delen = bouwstenen
      .map((steen) => {
        switch (steen) {
          case 'datum':
            return `${String(moment.getFullYear())}-${twee(moment.getMonth() + 1)}-${twee(moment.getDate())}`;
          case 'naam':
            return naam;
          case 'label':
            return label;
          case 'tijd':
            return `${twee(moment.getHours())}.${twee(moment.getMinutes())}`;
          case 'model':
            return 'Parakeet Ultra';
        }
      })
      .map((deel) => deel.replace(/[<>:"/\\|?*]/g, '-').trim())
      .filter((deel) => deel !== '');
    return `${delen.join(' ') || naam || 'Transcript'}.${formaat}`;
  };
  const metNummer = (naam: string, n: number): string =>
    naam.replace(/(\.[^.]+)$/, ` (${String(n)})$1`);
  const vrij = (map: MapInfo, naam: string): string => {
    let n = 2;
    while (schijf.has(`${map.pad}\\${metNummer(naam, n)}`)) n += 1;
    return metNummer(naam, n);
  };
  const stand_instellingen = (): BewaarInstellingen => ({
    ...instellingen,
    bouwstenen: [...instellingen.bouwstenen],
    voorbeeld: naamVoor(
      instellingen.bouwstenen,
      'Interview wethouder',
      'Z-2026-1043',
      new Date(),
      instellingen.formaat
    )
  });
  const titelVan = (taak: number): string =>
    (lijst.find((b) => b.taak === taak)?.naam ?? 'Transcript').replace(/\.[^.]+$/, '');
  if (opties.has('bestaat')) {
    schijf.add(`${standaardMap.pad}\\2026-10-30 Interview wethouder.docx`);
  }

  /** Schrijft op de "schijf" en onthoudt het bij het transcript. */
  const schrijf = (taak: number, map: MapInfo, naam: string): BewaarUitslag => {
    schijf.add(`${map.pad}\\${naam}`);
    const lijstNamen = (opgeslagen.get(taak) ?? []).filter(
      (b) => b.naam !== naam || b.map.pad !== map.pad
    );
    lijstNamen.push({ naam, map });
    opgeslagen.set(taak, lijstNamen);
    const item = lijst.find((b) => b.taak === taak);
    if (item) item.opgeslagen = naam;
    meldStand();
    return { type: 'opgeslagen', naam, map };
  };

  /** Opslaan in een map; bestaat de naam al, dan de vraag. */
  const bewaar = (taak: number, formaat: Formaat, map: MapInfo): BewaarUitslag => {
    if (opties.has('bewaarfout')) {
      throw {
        melding: 'Mondig mag niet schrijven in deze map. Kies een andere map.',
        vervolg: 'opnieuw_proberen'
      };
    }
    const naam = naamVoor(
      instellingen.bouwstenen,
      titelVan(taak),
      labels.get(taak) ?? '',
      new Date(KLAAR_OP),
      formaat
    );
    if (schijf.has(`${map.pad}\\${naam}`)) {
      if (
        instellingen.automatisch &&
        lijst.find((b) => b.taak === taak)?.toestand.type !== 'klaar'
      ) {
        return schrijf(taak, map, vrij(map, naam));
      }
      vraag = { id: volgendId(), taak, formaat, map, naam };
      return { type: 'bestaatAl', vraag: vraag.id, naam, voorstel: vrij(map, naam) };
    }
    return schrijf(taak, map, naam);
  };

  // De bewaakte map (B-1): de instellingen in het geheugen; de mapkiezer kiest altijd dezelfde map.
  const bewaakMap: MapInfo = { pad: 'C:\\Users\\Jan\\OneDrive\\Opnames', inDocumenten: null };
  const bewaak: BewaakInstellingen = {
    aan: opties.has('bewaakt'),
    map: opties.has('bewaakt') ? bewaakMap : null,
    submappen: false,
    overslaan: true,
    naast: false,
    probleem: null,
    melding: null
  };
  const bewaakStand = (): BewaakInstellingen => ({ ...bewaak });

  /** Het transcript van een bestand, zoals Rust het geeft, of `null`. */
  const transcriptVan = (taak: number): BestandTranscript | null => {
    const item = lijst.find((b) => b.taak === taak);
    if (!item?.heeftTranscript) return null;
    return {
      taak: item.taak,
      naam: item.naam,
      duurMs: item.duurMs ?? 0,
      klaarOpMs: KLAAR_OP,
      model: 'Parakeet Ultra',
      soevereiniteit: 'lokaal',
      melding: null,
      waarschuwing: opties.has('afgebroken')
        ? 'Het bestand lijkt afgebroken; de tekst stopt bij 00:26.'
        : null,
      regels: regelsMet(),
      label: labels.get(item.taak) ?? '',
      audioAanwezig: !opties.has('geenaudio'),
      persoon: mockPersoonStand(persoon)
    } satisfies BestandTranscript;
  };

  return (cmd, args) => {
    switch (cmd) {
      case 'bewaak_instellingen':
        return bewaakStand();
      case 'bewaak_kies_map':
        bewaak.map = bewaakMap;
        return bewaakStand();
      case 'bewaak_zet_aan':
        if (argUit(args, 'aan') === true && bewaak.map === null) {
          throw { melding: 'Kies eerst een map om in de gaten te houden.', vervolg: 'geen' };
        }
        bewaak.aan = Boolean(argUit(args, 'aan'));
        return bewaakStand();
      case 'bewaak_zet_submappen':
        bewaak.submappen = Boolean(argUit(args, 'aan'));
        return bewaakStand();
      case 'bewaak_zet_overslaan':
        bewaak.overslaan = Boolean(argUit(args, 'aan'));
        return bewaakStand();
      case 'bewaak_zet_naast':
        bewaak.naast = Boolean(argUit(args, 'aan'));
        return bewaakStand();
      case 'bestanden_stand':
        return stand();
      case 'bestanden_kies':
        if (opties.has('geenmodel')) {
          fout = {
            melding: 'Het spraakmodel is er nog niet. Download het eerst.',
            vervolg: 'opnieuw_downloaden'
          };
          return stand();
        }
        fout = null;
        nieuw('Gesprek huurcommissie.mp3', 754_000, { type: 'wacht' });
        nieuw('Oude opname.avi', null, { type: 'fout', melding: AVI_MELDING, vervolg: 'geen' });
        window.setTimeout(volgende, 200);
        return stand();
      case 'bestand_annuleer': {
        const taak = taakUit(args);
        const timer = timers.get(taak);
        if (timer !== undefined) window.clearInterval(timer);
        timers.delete(taak);
        meld(taak, { type: 'geannuleerd' });
        window.setTimeout(volgende, 100);
        return null;
      }
      case 'bestand_weghalen': {
        const taak = taakUit(args);
        const timer = timers.get(taak);
        if (timer !== undefined) window.clearInterval(timer);
        timers.delete(taak);
        const plek = lijst.findIndex((b) => b.taak === taak);
        if (plek >= 0) lijst.splice(plek, 1);
        fout = null;
        window.setTimeout(volgende, 100);
        return stand();
      }
      case 'bewaren_instellingen':
        return stand_instellingen();
      case 'bewaren_kies_map':
        instellingen.map = andereMap;
        return stand_instellingen();
      case 'bewaren_open_map':
        return null;
      case 'bewaren_zet_bouwstenen':
        instellingen.bouwstenen = [...new Set(argUit(args, 'bouwstenen') as Bouwsteen[])];
        return stand_instellingen();
      case 'bewaren_zet_formaat':
        instellingen.formaat = argUit(args, 'formaat') as Formaat;
        return stand_instellingen();
      case 'bewaren_zet_tijden':
        instellingen.tijden = Boolean(argUit(args, 'aan'));
        return stand_instellingen();
      case 'bewaren_zet_automatisch':
        instellingen.automatisch = Boolean(argUit(args, 'aan'));
        return stand_instellingen();
      case 'bewaren_zet_audio':
        instellingen.audio = Boolean(argUit(args, 'aan'));
        return stand_instellingen();
      case 'bestand_zet_label':
        {
          const label = argUit(args, 'label');
          labels.set(taakUit(args), typeof label === 'string' ? label : '');
        }
        return null;
      case 'bestand_bewaar': {
        const formaat = (argUit(args, 'formaat') as Formaat | null) ?? instellingen.formaat;
        const map = argUit(args, 'anderePlek') === true ? andereMap : instellingen.map;
        return bewaar(taakUit(args), formaat, map);
      }
      case 'bestand_bewaar_beslis': {
        const open = vraag;
        vraag = null;
        if (open?.id !== Number(argUit(args, 'vraag'))) {
          throw { melding: 'Deze vraag is verlopen. Klik nog eens op Opslaan.', vervolg: 'geen' };
        }
        const keuze = argUit(args, 'keuze');
        if (keuze === 'annuleren') return { type: 'geannuleerd' } satisfies BewaarUitslag;
        return schrijf(
          open.taak,
          open.map,
          keuze === 'vervangen' ? open.naam : vrij(open.map, open.naam)
        );
      }
      case 'bestand_opgeslagen_lijst':
        return opgeslagen.get(taakUit(args)) ?? [];
      case 'bestand_open_opgeslagen':
      case 'bestand_toon_opgeslagen':
        return null;
      case 'bestand_verwijder': {
        const taak = taakUit(args);
        const verwijderd = (opgeslagen.get(taak) ?? []).map((b) => b.naam);
        opgeslagen.delete(taak);
        const plek = lijst.findIndex((b) => b.taak === taak);
        if (plek >= 0) lijst.splice(plek, 1);
        return {
          stand: stand(),
          weg: true,
          verwijderd,
          gebleven: [],
          inGebruik: [],
          mislukt: []
        } satisfies VerwijderUitslag;
      }
      case 'bestand_transcript':
        return transcriptVan(taakUit(args));
      case 'bestand_persoon_vervangen':
      case 'bestand_persoon_weghalen':
      case 'bestand_persoon_markeer': {
        const wat = cmd.replace('bestand_persoon_', '') as 'vervangen' | 'weghalen' | 'markeer';
        pasMockPersoonToe(persoon, wat, argUit(args, wat === 'vervangen' ? 'aan' : 'waarde'));
        return transcriptVan(taakUit(args));
      }
      case 'bestand_zet_tekst': {
        const regel = tekstregels.find((r) => r.index === Number(argUit(args, 'index')));
        if (!regel) {
          throw {
            melding: 'Deze regel staat niet (meer) in het transcript.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        const ruw = argUit(args, 'tekst');
        const nieuweTekst = typeof ruw === 'string' ? ruw.trim() : '';
        if (nieuweTekst !== '') regel.tekst = nieuweTekst;
        const rij = lijst.find((b) => b.taak === taakUit(args));
        if (rij?.opgeslagen) {
          rij.opgeslagenVerouderd = true;
          meldStand();
        }
        return structuredClone(tekstregels);
      }
      case 'bestand_audio_stuk':
        return maakTestWav(Number(argUit(args, 'beginMs')), Number(argUit(args, 'eindMs')));
      default:
        return ONBEKEND;
    }
  };
}
