/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een gewone waarde (een foutmelding), geen Error */
// De dev-mock voor het scherm Vergadering (sprint 4b-C), los van mock.ts. Opties via `?mock=`
// (komma's ertussen), bijvoorbeeld /vergadering?mock=compleet,opname:
//   ruimte:       de laatst gekozen soort is Fysieke vergadering
//   alleen:       de laatst gekozen soort is "Alleen ik" (fase UX-D)
//   opname:       de opname loopt al (online, 12 minuten en 34 seconden)
//   opnameruimte: idem, Fysieke vergadering (alleen de microfoon)
//   opnamealleen: idem, Alleen ik (alleen de microfoon, zonder Jij en Anderen)
//   stil:         idem, met de waarschuwing dat er geen geluid van de computer binnenkomt
//   uitschrijven: het uitschrijven loopt al (34 procent, langzaam verder)
//   verslag:      het verslag van een online vergadering, met labels
//   verslagruimte: het verslag van een fysieke vergadering
//   verslagalleen: het verslag van Alleen ik: "Notitie van", alinea's zonder sprekers
//   verslagsprekers: het verslag van een online vergadering met herkende sprekers: Anderen is
//                 Spreker 1 tot en met 3 (de derde is eigenlijk dezelfde persoon als de
//                 eerste, om samenvoegen te proberen), plus een beurt zonder spreker
//   verslagruimtesprekers: het verslag van een fysieke vergadering met drie sprekers
//   sprekers:     bij elk verslag van deze mock zijn de sprekers herkend, en na het uitschrijven
//                 volgt de stap "Sprekers herkennen" (drie seconden)
//   klaarmoment:  het uitschrijven staat op 90 procent en rondt in een paar seconden af; daarna staat het
//                 verslag er met de strook "Je verslag staat klaar" (zes seconden)
//   sprekerstap:  het uitschrijven is klaar en de stap "Sprekers herkennen" loopt (zonder percentage)
//   echo:         bij het verslag: de melding dat dubbele tekst is weggelaten, plus een
//                 melding uit de opname
//   onafgemaakt:  vier opnames die wachten: een niet goed afgesloten, een voor later bewaard,
//                 en twee die bijna verlopen (over 20 uur en over 2 uur, met de melding vooraf)
//   veelwachtend: zes opnames die wachten, waarvan één onderbroken, één van Fysieke
//                 vergadering en één van Alleen ik (de lijst toont alleen de gekozen soort)
//   driewachtend: precies drie opnames die wachten, één onderbroken (de eis van UX-D)
//   twaalfwachtend: twaalf online opnames (tien in beeld, de rest achter "Alle 12 tonen")
//   eerder:       negen bewaarde verslagen onder "Eerder opgenomen" (N-C3): de lijst toont er vijf,
//                 de rest staat achter "Alle 9 tonen"; het laatste wordt vandaag of morgen gewist
//   eerderweinig: drie bewaarde verslagen
//   bewaarfout:   bij het verslag: de laatste wijzigingen zijn niet bewaard (schijf vol); Sluiten
//                 weigert, "Toch sluiten" sluit wel
//   allesoorten:  opnames van alle drie de soorten: online twee (één wordt morgen gewist),
//                 Fysieke vergadering drie, Alleen ik één (vandaag gewist); voor de regels
//                 "Ook nog ..." onder de lijst
//   audionaast:   Opslaan zet ook de opname als WAV ernaast (de instelling staat aan)
//   audiofout:    idem, maar de WAV lukt niet: het verslag staat er wel, met een melding
//   startfout:    starten mislukt: de microfoon is bezet
//   geenapparaten: de lijst met luidsprekers is niet op te halen
//   geenaudio:    bij het verslag is de opname weg: geen afspeelknoppen (V-1)
//   bestaat:      Opslaan geeft de vraag "Er staat al een bestand met deze naam" (4-C)
//   snelmodel:    het verslag is gemaakt met het snelle model (Parakeet), zoals in het ontwerp van N-C4
//   verbeterd:    bij het verslag zijn twee regels zelf verbeterd (de dialoog noemt ze)
//   whisper:      het extra nauwkeurige model staat al op de computer (anders: eerst downloaden)
//   geenmeting:   Mondig heeft de snelheid van de modellen nog niet gemeten
//   gewist:       bij het verslag is de opname gewist: Opnieuw uitschrijven staat grijs
//   eerdergewist: in Eerder opgenomen is de opname van het derde verslag gewist
//   opnieuwbezig: het verslag wordt opnieuw uitgeschreven (34 procent, blijft staan)
//   opnieuwklaar: het verslag is net opnieuw uitgeschreven: de melding en gemarkeerde regels
// Zonder opties werkt alles door te klikken: starten, stoppen (het uitschrijven duurt dan
// ongeveer een halve minuut), later uitschrijven, weggooien, namen geven en opslaan. Sluiten wist
// niets (ADR 0022): het verslag staat daarna onder "Eerder opgenomen" en is weer te openen,
// te hernoemen en te wissen, zoals in Rust.

import type { InvokeArgs } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { BewaarUitslag } from '../bewaren';
import { maakTestWav } from './mock-audio';
import {
  mockMarkeringen,
  mockPersoonStand,
  nieuwMockPersoon,
  pasMockPersoonToe,
  type MockPersoon
} from './mock-persoon';
import { ONBEKEND } from './mock-bestanden';
import { NOTULEN_BEZIG, maakNotulenMock } from './mock-notulen';
import { aantalSprekers } from '../verslagkop';
import type { Foutmelding, Gebeurtenis } from '../taak';
import type {
  AfspeelInfo,
  BeurtInfo,
  Kant,
  Modelrol,
  NaOpnieuw,
  Naambereik,
  OpnameInfo,
  OpnieuwKeuzes,
  OpnieuwStand,
  StandToestand,
  VergaderingStand,
  VerslagInfo,
  Vergadersoort
} from '../vergadering';

const MAP = { pad: 'C:\\Users\\Jan\\Documents\\Mondig', inDocumenten: 'Mondig' };
const VERSLAG_NAAM = '2026-11-02 Vergadering van 2 november.docx';
const WAV_NAAM = 'Vergadering 2026-11-02 14.00.wav';
// Bij Alleen ik heet het een notitie, zoals in Rust (`Vergadersoort::titelwoord`).
const NOTITIE_NAAM = '2026-11-02 Notitie van 2 november.docx';
const NOTITIE_WAV = 'Notitie 2026-11-02 14.00.wav';
const BEWAARFOUT: Foutmelding = {
  melding:
    'De schijf is vol, dus Mondig kon het verslag niet bewaren. Maak ruimte vrij, of kopieer of sla het op als je het wilt houden.',
  vervolg: 'geen'
};
const DAG = 86_400_000;
const UUR = 3_600_000;
// De modellen van opnieuw uitschrijven (N-C4), met hun rol zoals Rust (`model::modelrol`).
const MODELLEN: Record<string, { naam: string; rol: Modelrol; grootte: number; factor: number }> = {
  'parakeet-ultra-q8': { naam: 'Parakeet Ultra', rol: 'snel', grootte: 740_363_168, factor: 0.046 },
  'canary-1b-v2-q8': { naam: 'Canary', rol: 'nauwkeurig', grootte: 1_144_290_016, factor: 0.088 },
  'whisper-large-v3-turbo-q8': {
    naam: 'Whisper large-v3 turbo',
    rol: 'extra_nauwkeurig',
    grootte: 886_381_728,
    factor: 0.436
  }
};
/** De volgorde van de rollen: snel, nauwkeurig, extra nauwkeurig; een onbekend model telt als snel. */
const rang = (rol: Modelrol | undefined): number =>
  rol === 'extra_nauwkeurig' ? 2 : rol === 'nauwkeurig' ? 1 : 0;
const fout2 = (melding: string): Foutmelding => ({ melding, vervolg: 'geen' });

const OPNIEUW_BEZIG: Foutmelding = {
  melding: 'Mondig schrijft dit verslag opnieuw uit. Verbeteren kan weer als het klaar is.',
  vervolg: 'geen'
};
const OPNIEUW_SLUITEN: Foutmelding = {
  melding: 'Mondig schrijft dit verslag opnieuw uit. Stop dat eerst, of wacht tot het klaar is.',
  vervolg: 'geen'
};

function beurt(
  index: number,
  kant: Kant,
  startMs: number,
  eindMs: number,
  tekst: string
): BeurtInfo {
  return {
    index,
    kant,
    sprekernummer: null,
    spreker: kant === 'jij' ? 'Jij' : 'Anderen',
    eigenNaam: false,
    startMs,
    eindMs,
    tekst,
    markeringen: [],
    veranderd: false
  };
}

function onlineBeurten(): BeurtInfo[] {
  return [
    beurt(
      0,
      'jij',
      0,
      8000,
      'Fijn dat jullie er zijn. We bespreken vandaag de planning van de buurtbijeenkomst over het nieuwe speelveld.'
    ),
    beurt(
      1,
      'anderen',
      9000,
      20000,
      'Wij stellen voor om de bijeenkomst op een donderdagavond te houden, dan kunnen de meeste bewoners komen. Mijn BSN is 111222333 voor de aanvraag.'
    ),
    beurt(
      2,
      'jij',
      21000,
      27000,
      'Prima. Past de wijkschool als locatie, of is de zaal daar te klein? Mijn nummer is 06 12345678 en mijn mail is jan@example.com.'
    ),
    beurt(
      3,
      'anderen',
      28000,
      40000,
      'De aula heeft plek voor zo\u2019n tachtig mensen. Dat lijkt me genoeg voor de eerste ronde.'
    ),
    beurt(
      4,
      'jij',
      41000,
      51000,
      'Dan nodigen we de omwonenden uit met een brief, en we hangen een bericht op in het buurthuis.'
    ),
    beurt(
      5,
      'anderen',
      52000,
      62000,
      'Vergeet de speeltuinvereniging niet. Zij kennen de kinderen en de ouders het best.'
    ),
    beurt(
      6,
      'jij',
      63000,
      72000,
      'Goed punt. Ik stuur hen deze week een uitnodiging en laat het weten als ze kunnen.'
    )
  ];
}

function ruimteBeurten(): BeurtInfo[] {
  const zonder = (b: BeurtInfo): BeurtInfo => ({ ...b, spreker: null });
  return [
    zonder(
      beurt(
        0,
        'jij',
        0,
        10000,
        'Goedemiddag allemaal. We bespreken vandaag de planning van de buurtbijeenkomst over het nieuwe speelveld.'
      )
    ),
    zonder(
      beurt(
        1,
        'jij',
        11000,
        33000,
        'Het voorstel is om de bijeenkomst op een donderdagavond te houden, dan kunnen de meeste bewoners komen. De wijkschool heeft een aula met plek voor zo\u2019n tachtig mensen.'
      )
    ),
    zonder(
      beurt(
        2,
        'jij',
        34000,
        57000,
        'Dan nodigen we de omwonenden uit met een brief, en we hangen een bericht op in het buurthuis. De speeltuinvereniging vragen we apart, want zij kennen de kinderen en de ouders het best.'
      )
    ),
    zonder(
      beurt(
        3,
        'jij',
        58000,
        80000,
        'Goed. Ik stuur hen deze week een uitnodiging en laat het weten als ze kunnen. Dan kijken we volgende week naar het programma van de avond.'
      )
    )
  ];
}

function notitieBeurten(): BeurtInfo[] {
  const zonder = (b: BeurtInfo): BeurtInfo => ({ ...b, spreker: null });
  return [
    zonder(
      beurt(
        0,
        'jij',
        0,
        14000,
        'Even een paar gedachten over de buurtbijeenkomst. Ik denk dat een donderdagavond het beste werkt, dan kunnen de meeste bewoners.'
      )
    ),
    zonder(
      beurt(
        1,
        'jij',
        16000,
        38000,
        'Nog uitzoeken: past iedereen in de aula van de wijkschool, en wie maakt de uitnodiging? Misschien vraag ik de speeltuinvereniging om mee te denken.'
      )
    )
  ];
}

/** Een beurt van een herkende spreker; zonder eigen naam heet hij "Spreker N". */
function vanSpreker(b: BeurtInfo, nummer: number | null): BeurtInfo {
  return {
    ...b,
    sprekernummer: nummer,
    spreker: nummer === null ? (b.kant === 'jij' ? 'Jij' : 'Anderen') : `Spreker ${String(nummer)}`
  };
}

/**
 * Online met herkende sprekers (4b-D): "Anderen" is drie sprekers, waarvan de derde eigenlijk
 * dezelfde persoon is als de eerste (het model splitste hem); de laatste beurt heeft geen spreker.
 */
function onlineSprekerBeurten(): BeurtInfo[] {
  const nummers = new Map<number, number | null>([
    [1, 1],
    [3, 2],
    [5, 3]
  ]);
  const beurten = onlineBeurten().map((b) =>
    b.kant === 'anderen' ? vanSpreker(b, nummers.get(b.index) ?? null) : b
  );
  return [
    ...beurten,
    vanSpreker(
      beurt(7, 'anderen', 73000, 79000, 'Dank je wel, dat is helder. Tot donderdag dan.'),
      null
    )
  ];
}

/** In de ruimte met herkende sprekers: drie mensen aan tafel. */
function ruimteSprekerBeurten(): BeurtInfo[] {
  const nummers = [1, 2, 1, 3];
  return ruimteBeurten().map((b, i) => vanSpreker(b, nummers[i] ?? 1));
}

function beurtenVoor(soort: Vergadersoort, metSprekers = false): BeurtInfo[] {
  if (soort === 'online') return metSprekers ? onlineSprekerBeurten() : onlineBeurten();
  if (soort === 'ruimte') return metSprekers ? ruimteSprekerBeurten() : ruimteBeurten();
  return notitieBeurten();
}

/** Voegt opeenvolgende beurten van dezelfde kant, spreker en naam samen en telt opnieuw. */
function voegBurenSamen(beurten: BeurtInfo[]): BeurtInfo[] {
  const uit: BeurtInfo[] = [];
  for (const b of beurten) {
    const vorige = uit[uit.length - 1];
    if (
      vorige?.kant === b.kant &&
      vorige.sprekernummer === b.sprekernummer &&
      vorige.spreker === b.spreker &&
      vorige.eigenNaam === b.eigenNaam
    ) {
      uit[uit.length - 1] = {
        ...vorige,
        eindMs: Math.max(vorige.eindMs, b.eindMs),
        tekst: `${vorige.tekst} ${b.tekst}`
      };
    } else {
      uit.push({ ...b });
    }
  }
  return uit.map((b, index) => ({ ...b, index }));
}

/** Een bewaard verslag in de mock: wat Rust in verslag.json zet. */
interface BewaardVerslag {
  info: VerslagInfo;
  beurten: BeurtInfo[];
  persoon: MockPersoon;
}

/** De naam van een verslag zoals Rust hem schoonmaakt: witruimte wordt één spatie, hooguit 100 tekens. */
function schoneVerslagnaam(naam: string): string | null {
  const schoon = naam
    .replace(/\p{Cc}/gu, '')
    .split(/\s+/)
    .filter(Boolean)
    .join(' ')
    .slice(0, 100);
  return schoon.trimEnd() === '' ? null : schoon.trimEnd();
}

export function maakVergaderMock(
  opties: Set<string>,
  volgendId: () => number
): (cmd: string, args: InvokeArgs | undefined) => unknown {
  let versie = 0;
  let toestand: StandToestand = { type: 'klaar' };
  let fout: Foutmelding | null = null;
  let laatsteSoort: Vergadersoort = opties.has('ruimte')
    ? 'ruimte'
    : opties.has('alleen')
      ? 'alleen'
      : 'online';
  let opnames: OpnameInfo[] = [];
  // De bewaarde verslagen ("Eerder opgenomen"); het verslag dat open is, staat er niet in.
  let bewaard: BewaardVerslag[] = [];
  // Wat het verslag dat nu open is, nog nodig heeft: zijn id, naam, duur en de bewaarde markeringen.
  let huidigId = '';
  let verslagNaam: string | null = null;
  let verslagDuurMs: number | null = null;
  // De bewaartermijn uit Instellingen (1, 7 of 30 dagen) bepaalt `wistMs` van elke opname.
  let bewaarDagen = 7;
  const wistOp = (beginMs: number): number => beginMs + bewaarDagen * DAG;
  let beurten: BeurtInfo[] = [];
  // Opnieuw uitschrijven (N-C4, ADR 0023): het model van het open verslag, de zelf verbeterde
  // regels, wat er nu loopt, de vorige versie en de veranderde regels.
  let modelId = opties.has('snelmodel') ? 'parakeet-ultra-q8' : 'canary-1b-v2-q8';
  const verbeterd = new Set<number>(opties.has('verbeterd') ? [1, 3] : []);
  let opnieuw: OpnieuwStand | null = null;
  let opnieuwModel = '';
  let opnieuwTimer: number | null = null;
  let naOpnieuw: NaOpnieuw | null = null;
  let vorige: {
    beurten: BeurtInfo[];
    modelId: string;
    persoon: MockPersoon;
    verbeterd: number[];
  } | null = null;
  let veranderd = new Set<number>();
  // Zijn de sprekers herkend in de verslagen die deze mock maakt?
  const metSprekers =
    opties.has('sprekers') ||
    opties.has('sprekerstap') ||
    opties.has('verslagsprekers') ||
    opties.has('verslagruimtesprekers');
  let vraag: { id: number } | null = null;
  // Het pad van het laatst opgeslagen verslag: Openen en Map tonen werken er pas na opslaan op.
  let opgeslagenPad: string | null = null;
  const timers: number[] = [];
  let niveauTimer: number | null = null;

  const argUit = (args: InvokeArgs | undefined, sleutel: string): unknown =>
    args && !Array.isArray(args) && !(args instanceof ArrayBuffer) && !(args instanceof Uint8Array)
      ? args[sleutel]
      : undefined;

  // De markeringen van persoonsgegevens (V-2) van het verslag dat open is.
  const persoon = nieuwMockPersoon();
  const metMarkeringen = (t: StandToestand): StandToestand =>
    t.type === 'verslag'
      ? {
          ...t,
          beurten: t.beurten.map((b) => ({
            ...b,
            markeringen: mockMarkeringen(b.tekst, persoon)
          })),
          persoon: mockPersoonStand(persoon)
        }
      : t;
  // De velden van opnieuw uitschrijven, zoals ze nu zijn, over het verslag heen.
  const metOpnieuw = (t: StandToestand): StandToestand =>
    t.type === 'verslag'
      ? {
          ...t,
          model: MODELLEN[modelId]?.naam ?? t.model,
          modelId,
          modelRol: MODELLEN[modelId]?.rol ?? null,
          verbeterd: verbeterd.size,
          opnieuw: opnieuw === null ? null : { ...opnieuw },
          naOpnieuw: naOpnieuw === null ? null : { ...naOpnieuw },
          vorigeVersie: vorige !== null,
          notulen: notulen.stand(),
          beurten: t.beurten.map((b) => ({ ...b, veranderd: veranderd.has(b.index) }))
        }
      : t;
  const stand = (): VergaderingStand => {
    versie += 1;
    return {
      versie,
      toestand: metMarkeringen(metOpnieuw(structuredClone(toestand))),
      opnames: structuredClone(opnames),
      verslagen: bewaard.map((v) => structuredClone(v.info)).sort((a, b) => b.beginMs - a.beginMs),
      fout,
      laatsteSoort
    };
  };
  const nieuweMarkeringen = (): void => {
    const leeg = nieuwMockPersoon();
    persoon.vervangen = leeg.vervangen;
    persoon.genegeerd = leeg.genegeerd;
    persoon.eigen = leeg.eigen;
  };
  const meldStand = (): void => {
    void emit('vergadering-gewijzigd', stand());
  };
  // Notulen (T3): de instellingen en het maken, in mock-notulen.ts.
  const notulen = maakNotulenMock(
    opties,
    () => {
      meldStand();
    },
    () => beurten,
    stand,
    () => persoon.vervangen
  );
  const zet = (nieuw: StandToestand): void => {
    toestand = nieuw;
    meldStand();
  };
  const na = (ms: number, werk: () => void): void => {
    timers.push(window.setTimeout(werk, ms));
  };
  const wacht = (ms: number): Promise<void> =>
    new Promise((klaar) => {
      window.setTimeout(klaar, ms);
    });
  const stopTimers = (): void => {
    timers.splice(0).forEach((t) => {
      window.clearTimeout(t);
      window.clearInterval(t);
    });
  };

  const stopNiveaus = (): void => {
    if (niveauTimer !== null) window.clearInterval(niveauTimer);
    niveauTimer = null;
  };
  const startNiveaus = (soort: Vergadersoort, stil: boolean): void => {
    stopNiveaus();
    let tik = 0;
    niveauTimer = window.setInterval(() => {
      tik += 1;
      const golf = (Math.sin(tik / 3) + 1) / 2;
      void emit('vergadering-niveau', {
        microfoon: Math.min(1, 0.15 + golf * 0.55 + Math.random() * 0.15),
        systeem: soort === 'online' ? (stil ? 0.02 : Math.random() * 0.5) : null
      });
    }, 100);
  };

  const neemtOp = (soort: Vergadersoort, begin: number, stil: boolean): StandToestand => ({
    type: 'neemtOp',
    soort,
    beginMs: begin,
    duurMs: Date.now() - begin,
    niveaus: { microfoon: 0.4, systeem: soort === 'online' ? 0.2 : null },
    systeemStil: stil ? 'Er komt al een minuut geen geluid van de computer binnen.' : null,
    meldingen: []
  });

  const verslag = (soort: Vergadersoort, begin: number, taak: number): StandToestand => ({
    type: 'verslag',
    taak,
    soort,
    beginMs: begin,
    duurMs: verslagDuurMs ?? (soort === 'online' ? 52 * 60_000 : 38 * 60_000),
    model: 'Canary',
    soevereiniteit: 'lokaal',
    meldingen: opties.has('echo')
      ? ['De opname van het geluid van de computer viel even uit.']
      : [],
    weggelaten: opties.has('echo') && soort === 'online' ? 4 : 0,
    beurten: structuredClone(beurten),
    audioAanwezig: !opties.has('geenaudio') && !opties.has('gewist'),
    persoon: { vervangen: false, eigen: [] },
    naam: verslagNaam,
    wistMs: wistOp(begin),
    bewarenMislukt: opties.has('bewaarfout'),
    modelId,
    modelRol: MODELLEN[modelId]?.rol ?? null,
    opnameAanwezig: !opties.has('geenaudio') && !opties.has('gewist'),
    verbeterd: verbeterd.size,
    geluidMs:
      (verslagDuurMs ?? (soort === 'online' ? 52 * 60_000 : 38 * 60_000)) *
      (soort === 'online' ? 2 : 1),
    opnieuw: null,
    naOpnieuw: null,
    vorigeVersie: false,
    notulen: { info: null, onleesbaar: false, maken: null }
  });

  /** Vergeet alles van opnieuw uitschrijven, bij sluiten of een ander verslag. */
  const vergeetOpnieuw = (): void => {
    notulen.vergeet();
    if (opnieuwTimer !== null) window.clearInterval(opnieuwTimer);
    opnieuwTimer = null;
    opnieuw = null;
    naOpnieuw = null;
    vorige = null;
    veranderd = new Set();
    verbeterd.clear();
  };

  /**
   * Het opnieuw uitschrijven is klaar: de oude versie wordt de vorige, de veranderde regels krijgen
   * hun vlag. In de mock blijft de tekst gelijk; de zelf verbeterde regels en een paar andere
   * tellen als veranderd, en een eigen markering die nergens meer staat telt als verdwenen.
   */
  const rondOpnieuwAf = (): void => {
    if (toestand.type !== 'verslag') return;
    vorige = {
      beurten: structuredClone(beurten),
      modelId,
      persoon: {
        vervangen: persoon.vervangen,
        genegeerd: new Set(persoon.genegeerd),
        eigen: new Set(persoon.eigen)
      },
      verbeterd: [...verbeterd]
    };
    veranderd = new Set(
      beurten.filter((b) => verbeterd.has(b.index) || b.index % 3 === 2).map((b) => b.index)
    );
    const tekst = beurten.map((b) => b.tekst.toLowerCase()).join(' ');
    const verdwenen = [...persoon.eigen].filter((e) => !tekst.includes(e)).length;
    naOpnieuw = {
      rol: MODELLEN[opnieuwModel]?.rol ?? null,
      veranderd: veranderd.size,
      verdwenen
    };
    modelId = opnieuwModel;
    verbeterd.clear();
    opnieuw = null;
  };

  /** Weigert een wijziging zolang het verslag opnieuw wordt uitgeschreven, zoals Rust. */
  const weigerTijdensOpnieuw = (): void => {
    if (opnieuw !== null) throw OPNIEUW_BEZIG;
    if (notulen.bezig()) throw NOTULEN_BEZIG;
    veranderd = new Set();
  };

  const startUitschrijven = (
    soort: Vergadersoort,
    begin: number,
    vanaf: number,
    stapMs: number
  ): void => {
    const taak = volgendId();
    let procent = vanaf;
    const duur = 52 * 60_000;
    zet({
      type: 'uitschrijven',
      taak,
      soort,
      beginMs: begin,
      duurMs: duur,
      voortgang: procent,
      model: 'Canary'
    });
    void emit('taak', {
      taak,
      soort: 'vergadering',
      toestand: { type: 'verwerkt', voortgang: procent }
    } satisfies Gebeurtenis);
    beurten = beurtenVoor(soort, metSprekers && soort !== 'alleen');
    const klaar = (): void => {
      void emit('taak', {
        taak,
        soort: 'vergadering',
        toestand: { type: 'klaar' }
      } satisfies Gebeurtenis);
      // Een nieuw verslag staat meteen naast zijn opname, zonder naam en zonder markeringen.
      huidigId = `vergadering-${String(begin)}`;
      verslagNaam = null;
      verslagDuurMs = null;
      nieuweMarkeringen();
      notulen.vergeet();
      zet(verslag(soort, begin, taak));
    };
    // De stap Sprekers herkennen: eerst het lezen (met een percentage dat het scherm negeert),
    // dan het rekenen zonder tussenbericht.
    const sprekersStap = (): void => {
      void emit('taak', {
        taak,
        soort: 'vergadering',
        toestand: { type: 'verwerkt', voortgang: 100, stap: 'sprekers' }
      } satisfies Gebeurtenis);
      zet({
        type: 'uitschrijven',
        taak,
        soort,
        beginMs: begin,
        duurMs: duur,
        voortgang: 100,
        stap: 'sprekers',
        model: 'Canary'
      });
      na(opties.has('sprekerstap') ? 600_000 : 3000, klaar);
    };
    const timer = window.setInterval(() => {
      procent = Math.min(100, procent + (opties.has('uitschrijven') ? 1 : 4));
      void emit('taak', {
        taak,
        soort: 'vergadering',
        toestand: { type: 'verwerkt', voortgang: procent }
      } satisfies Gebeurtenis);
      if (procent >= 100) {
        window.clearInterval(timer);
        if (metSprekers && soort !== 'alleen') sprekersStap();
        else klaar();
      } else {
        zet({
          type: 'uitschrijven',
          taak,
          soort,
          beginMs: begin,
          duurMs: duur,
          voortgang: procent,
          model: 'Canary'
        });
      }
    }, stapMs);
    timers.push(timer);
  };

  // Beginstanden voor het nalopen van elke toestand.
  const begin = Date.now() - 754_000;
  if (opties.has('onafgemaakt')) {
    opnames = [
      {
        id: '2026-11-01-1402',
        soort: 'online',
        beginMs: Date.now() - DAG - 3_600_000,
        duurMs: 47 * 60_000,
        afgesloten: false,
        onzeker: false,
        wistMs: wistOp(Date.now() - DAG - 3_600_000)
      },
      {
        id: '2026-11-02-0915',
        soort: 'ruimte',
        beginMs: Date.now() - 5 * 3_600_000,
        duurMs: 31 * 60_000,
        afgesloten: true,
        onzeker: false,
        wistMs: wistOp(Date.now() - 5 * 3_600_000)
      },
      // Bijna verlopen: alleen bij de standaard van 7 dagen. Over 20 uur en over 2 uur.
      {
        id: '2026-10-26-1630',
        soort: 'online',
        beginMs: Date.now() - 7 * DAG + 20 * UUR,
        duurMs: 58 * 60_000,
        afgesloten: true,
        onzeker: false,
        wistMs: wistOp(Date.now() - 7 * DAG + 20 * UUR)
      },
      {
        id: '2026-10-26-1100',
        soort: 'ruimte',
        beginMs: Date.now() - 7 * DAG + 2 * UUR,
        duurMs: 22 * 60_000,
        afgesloten: false,
        onzeker: false,
        wistMs: wistOp(Date.now() - 7 * DAG + 2 * UUR)
      }
    ];
  }
  // Veel opnames: zes, en precies drie, voor de eis "Opname starten" in beeld (UX-D).
  const wachtende = (
    id: string,
    soort: Vergadersoort,
    beginMs: number,
    minuten: number,
    afgesloten: boolean
  ): OpnameInfo => ({
    id,
    soort,
    beginMs,
    duurMs: minuten * 60_000,
    afgesloten,
    onzeker: false,
    wistMs: wistOp(beginMs)
  });
  if (opties.has('veelwachtend')) {
    opnames = [
      wachtende('veel-1', 'online', Date.now() - 4 * DAG, 52, true),
      wachtende('veel-2', 'ruimte', Date.now() - 3 * DAG, 38, true),
      wachtende('veel-3', 'online', Date.now() - DAG - 2 * UUR, 47, false),
      wachtende('veel-4', 'alleen', Date.now() - 6 * UUR, 4, true),
      wachtende('veel-5', 'online', Date.now() - 3 * UUR, 1, true),
      wachtende('veel-6', 'online', Date.now() - UUR, 3, true)
    ];
  } else if (opties.has('twaalfwachtend')) {
    opnames = Array.from({ length: 12 }, (_, i) =>
      wachtende(`twaalf-${String(i + 1)}`, 'online', Date.now() - (12 - i) * 5 * UUR, 20 + i, true)
    );
  } else if (opties.has('allesoorten')) {
    opnames = [
      wachtende('soort-1', 'online', Date.now() - 7 * DAG + 20 * UUR, 52, true),
      wachtende('soort-2', 'online', Date.now() - 2 * DAG, 47, true),
      wachtende('soort-3', 'ruimte', Date.now() - 3 * DAG, 38, true),
      wachtende('soort-4', 'ruimte', Date.now() - DAG, 61, false),
      wachtende('soort-5', 'ruimte', Date.now() - 2 * UUR, 25, true),
      wachtende('soort-6', 'alleen', Date.now() - 7 * DAG + 2 * UUR, 4, true)
    ];
  } else if (opties.has('driewachtend')) {
    opnames = [
      wachtende('drie-1', 'online', Date.now() - DAG - 2 * UUR, 47, false),
      wachtende('drie-2', 'online', Date.now() - 3 * UUR, 1, true),
      wachtende('drie-3', 'online', Date.now() - UUR, 3, true)
    ];
  }
  // Bewaarde verslagen, zoals in het ontwerp (sectie 2): de nieuwste vijf zijn in beeld.
  const bewaardVoorbeeld = (
    id: string,
    naam: string | null,
    soort: Vergadersoort,
    beginMs: number,
    minuten: number,
    // Zijn de sprekers herkend (groter dan nul)? Het aantal in de lijst telt de beurten, zoals Rust.
    herkend: number
  ): BewaardVerslag => {
    const beurtenVoorbeeld = beurtenVoor(soort, herkend > 0 && soort !== 'alleen');
    return {
      info: {
        id,
        naam,
        soort,
        beginMs,
        duurMs: minuten * 60_000,
        sprekers: aantalSprekers(beurtenVoorbeeld),
        wistMs: wistOp(beginMs),
        opnameAanwezig: !(opties.has('eerdergewist') && id === 'eerder-3')
      },
      beurten: beurtenVoorbeeld,
      persoon: nieuwMockPersoon()
    };
  };
  const vandaagOm = (uur: number, minuut: number, dagenTerug = 0): number => {
    const d = new Date();
    d.setDate(d.getDate() - dagenTerug);
    d.setHours(uur, minuut, 0, 0);
    return d.getTime();
  };
  if (opties.has('eerder')) {
    bewaard = [
      bewaardVoorbeeld(
        'eerder-1',
        'Overleg buurtbijeenkomst speelveld',
        'online',
        vandaagOm(14, 0),
        52,
        3
      ),
      bewaardVoorbeeld('eerder-2', null, 'alleen', vandaagOm(17, 12, 1), 4, 0),
      bewaardVoorbeeld(
        'eerder-3',
        'Teamoverleg Ruimte en Wonen',
        'ruimte',
        vandaagOm(10, 0, 2),
        64,
        5
      ),
      bewaardVoorbeeld(
        'eerder-4',
        'Interview wethouder De Vries',
        'online',
        vandaagOm(15, 30, 5),
        38,
        2
      ),
      bewaardVoorbeeld('eerder-5', null, 'online', vandaagOm(9, 15, 5), 27, 0),
      bewaardVoorbeeld('eerder-6', 'Overleg verkeersplan', 'ruimte', vandaagOm(13, 0, 6), 45, 3),
      bewaardVoorbeeld('eerder-7', null, 'alleen', vandaagOm(8, 45, 6), 2, 0),
      bewaardVoorbeeld(
        'eerder-8',
        'Kennismaking nieuwe collega',
        'online',
        vandaagOm(11, 0, 6),
        31,
        0
      ),
      // Bijna verlopen: wordt over 20 uur gewist (alleen bij de standaard van 7 dagen).
      bewaardVoorbeeld(
        'eerder-9',
        'Terugkoppeling subsidie',
        'online',
        Date.now() - 7 * DAG + 20 * UUR,
        58,
        2
      )
    ];
  } else if (opties.has('eerderweinig')) {
    bewaard = [
      bewaardVoorbeeld(
        'eerder-1',
        'Overleg buurtbijeenkomst speelveld',
        'online',
        vandaagOm(14, 0),
        52,
        3
      ),
      bewaardVoorbeeld('eerder-2', null, 'alleen', vandaagOm(17, 12, 1), 4, 0),
      bewaardVoorbeeld(
        'eerder-3',
        'Teamoverleg Ruimte en Wonen',
        'ruimte',
        vandaagOm(10, 0, 2),
        64,
        5
      )
    ];
  }
  if (opties.has('opname') || opties.has('stil')) {
    toestand = neemtOp('online', begin, opties.has('stil'));
    startNiveaus('online', opties.has('stil'));
  } else if (opties.has('opnameruimte')) {
    laatsteSoort = 'ruimte';
    toestand = neemtOp('ruimte', begin, false);
    startNiveaus('ruimte', false);
  } else if (opties.has('uitschrijven')) {
    window.setTimeout(() => {
      startUitschrijven('online', begin, 34, 1500);
    }, 0);
  } else if (opties.has('klaarmoment')) {
    // Het uitschrijven rondt zichtbaar af, zodat het klaar-moment van het verslag te zien is.
    window.setTimeout(() => {
      startUitschrijven('online', begin, 90, 700);
    }, 0);
  } else if (opties.has('sprekerstap')) {
    window.setTimeout(() => {
      startUitschrijven('online', begin, 99, 100);
    }, 0);
  } else if (opties.has('opnamealleen')) {
    laatsteSoort = 'alleen';
    toestand = neemtOp('alleen', begin, false);
    startNiveaus('alleen', false);
  } else if (
    opties.has('verslag') ||
    opties.has('verslagruimte') ||
    opties.has('verslagalleen') ||
    opties.has('verslagsprekers') ||
    opties.has('verslagruimtesprekers')
  ) {
    const soort: Vergadersoort =
      opties.has('verslagruimte') || opties.has('verslagruimtesprekers')
        ? 'ruimte'
        : opties.has('verslagalleen')
          ? 'alleen'
          : 'online';
    beurten = beurtenVoor(soort, metSprekers);
    huidigId = 'vergadering-voorbeeld';
    toestand = verslag(soort, new Date(2026, 10, 2, 14, 0).getTime(), volgendId());
    if (opties.has('bewaarfout')) fout = BEWAARFOUT;
  }
  if (toestand.type === 'verslag' && opties.has('opnieuwbezig')) {
    opnieuwModel = 'canary-1b-v2-q8';
    opnieuw = { rol: 'nauwkeurig', voortgang: 34, restMs: 6 * 60_000 };
  } else if (toestand.type === 'verslag' && opties.has('opnieuwklaar')) {
    opnieuwModel = 'canary-1b-v2-q8';
    rondOpnieuwAf();
  }

  const sluit = (): void => {
    opgeslagenPad = null;
    stopTimers();
    stopNiveaus();
    fout = null;
    toestand = { type: 'klaar' };
    verslagNaam = null;
    verslagDuurMs = null;
    vergeetOpnieuw();
  };

  /** Het open verslag gaat naar de lijst "Eerder opgenomen", zoals `sluit_verslag` in Rust. */
  const bewaarOpenVerslag = (): void => {
    if (toestand.type !== 'verslag') return;
    bewaard = [
      {
        info: {
          id: huidigId,
          naam: verslagNaam,
          soort: toestand.soort,
          beginMs: toestand.beginMs,
          duurMs: toestand.duurMs,
          sprekers: aantalSprekers(beurten),
          wistMs: wistOp(toestand.beginMs),
          opnameAanwezig: toestand.opnameAanwezig
        },
        beurten: structuredClone(beurten),
        persoon: {
          vervangen: persoon.vervangen,
          genegeerd: new Set(persoon.genegeerd),
          eigen: new Set(persoon.eigen)
        }
      },
      ...bewaard.filter((v) => v.info.id !== huidigId)
    ];
  };

  const nietNu = (): Foutmelding => {
    const melding =
      toestand.type === 'neemtOp' || toestand.type === 'starten'
        ? 'Er loopt een opname. Open een verslag als je haar hebt gestopt.'
        : toestand.type === 'voorbereiden'
          ? 'Mondig is nog bezig met de vorige opname. Even geduld.'
          : 'Mondig schrijft nog een vergadering uit. Open een verslag als dat klaar is.';
    return { melding, vervolg: 'geen' };
  };

  const voegBewaardToe = (soort: Vergadersoort, beginMs: number, duurMs: number): void => {
    opnames = [
      ...opnames,
      {
        id: `opname-${String(beginMs)}`,
        soort,
        beginMs,
        duurMs,
        afgesloten: true,
        onzeker: false,
        wistMs: wistOp(beginMs)
      }
    ];
  };

  return (cmd, args) => {
    const notulenAntwoord = notulen.handel(cmd, args);
    if (notulenAntwoord !== ONBEKEND) return notulenAntwoord;
    switch (cmd) {
      case 'bewaartermijn_instelling':
        return bewaarDagen;
      case 'bewaartermijn_zet': {
        const dagen = argUit(args, 'dagen');
        if (dagen !== 1 && dagen !== 7 && dagen !== 30) {
          throw {
            melding: 'Deze bewaartijd bestaat niet. Kies 1, 7 of 30 dagen.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        bewaarDagen = dagen;
        opnames = opnames.map((o) => ({ ...o, wistMs: wistOp(o.beginMs) }));
        bewaard = bewaard.map((v) => ({
          ...v,
          info: { ...v.info, wistMs: wistOp(v.info.beginMs) }
        }));
        if (toestand.type === 'verslag')
          toestand = { ...toestand, wistMs: wistOp(toestand.beginMs) };
        meldStand();
        return null;
      }
      case 'vergadering_stand':
        return stand();
      case 'vergadering_start_gevraagd':
        // Het systeemvak bestaat in de browser niet: nooit een verzoek.
        return null;
      case 'vergadering_afspeelapparaten': {
        if (opties.has('geenapparaten')) {
          throw {
            melding: 'Het geluid van de computer opnemen kan hier nog niet.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        return [
          { id: 'luidspreker', naam: 'Luidsprekers (Realtek Audio)', standaard: true },
          { id: 'jabra', naam: 'Headset (Jabra Evolve2 65)', standaard: false },
          { id: 'monitor', naam: 'Monitor (HDMI)', standaard: false }
        ] satisfies AfspeelInfo[];
      }
      case 'vergadering_start': {
        const soort = argUit(args, 'soort') as Vergadersoort;
        return (async () => {
          if (opties.has('startfout')) {
            throw {
              melding:
                'Een ander programma gebruikt de microfoon. Sluit dat programma en probeer het opnieuw.',
              // Zoals mondig-core (FoutSoort::Bezet): opnieuw proberen.
              vervolg: 'opnieuw_proberen'
            } satisfies Foutmelding;
          }
          fout = null;
          laatsteSoort = soort;
          zet({ type: 'starten', soort });
          await wacht(1200);
          zet(neemtOp(soort, Date.now(), false));
          startNiveaus(soort, false);
          if (soort === 'online') {
            na(20_000, () => {
              if (toestand.type !== 'neemtOp') return;
              toestand = {
                ...toestand,
                systeemStil: 'Er komt al een minuut geen geluid van de computer binnen.'
              };
              meldStand();
            });
          }
          return stand();
        })();
      }
      case 'vergadering_stop': {
        if (toestand.type !== 'neemtOp') {
          throw { melding: 'Er loopt geen opname.', vervolg: 'geen' } satisfies Foutmelding;
        }
        const { soort, beginMs } = toestand;
        const duurMs = Date.now() - beginMs;
        const uitschrijven = argUit(args, 'uitschrijven') === true;
        stopNiveaus();
        stopTimers();
        if (!uitschrijven) {
          voegBewaardToe(soort, beginMs, duurMs);
          toestand = { type: 'klaar' };
          return stand();
        }
        zet({ type: 'voorbereiden', soort, beginMs, duurMs, herstellen: false });
        na(600, () => {
          startUitschrijven(soort, beginMs, 0, 800);
        });
        return stand();
      }
      case 'vergadering_later': {
        if (toestand.type === 'uitschrijven') {
          voegBewaardToe(toestand.soort, toestand.beginMs, toestand.duurMs ?? 0);
        }
        stopTimers();
        toestand = { type: 'klaar' };
        return stand();
      }
      case 'vergadering_weggooien': {
        const id = argUit(args, 'id');
        if (typeof id === 'string') {
          opnames = opnames.filter((o) => o.id !== id);
        } else {
          sluit();
        }
        return stand();
      }
      case 'vergadering_uitschrijven':
      case 'vergadering_herstel': {
        const id = argUit(args, 'id');
        const gekozen = opnames.find((o) => o.id === id);
        if (!gekozen) {
          throw {
            melding: 'Deze opname staat er niet meer.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        opnames = opnames.filter((o) => o.id !== id);
        zet({
          type: 'voorbereiden',
          soort: gekozen.soort,
          beginMs: gekozen.beginMs,
          duurMs: gekozen.duurMs,
          herstellen: cmd === 'vergadering_herstel'
        });
        na(800, () => {
          startUitschrijven(gekozen.soort, gekozen.beginMs, 0, 800);
        });
        return stand();
      }
      case 'vergadering_naam': {
        const index = Number(argUit(args, 'index'));
        const naamArg = argUit(args, 'naam');
        const naam = typeof naamArg === 'string' ? naamArg.trim() : '';
        const bereik = argUit(args, 'bereik') as Naambereik;
        const doel = beurten.find((b) => b.index === index);
        if (!doel || toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        weigerTijdensOpnieuw();
        const huidig = doel.spreker;
        const standaard =
          doel.sprekernummer === null
            ? doel.kant === 'jij'
              ? 'Jij'
              : 'Anderen'
            : `Spreker ${String(doel.sprekernummer)}`;
        beurten = beurten.map((b) =>
          (
            bereik === 'beurt'
              ? b.index === index
              : bereik === 'spreker'
                ? b.kant === doel.kant &&
                  b.sprekernummer === doel.sprekernummer &&
                  b.spreker === huidig
                : b.kant === doel.kant && b.spreker === huidig
          )
            ? { ...b, spreker: naam === '' ? standaard : naam, eigenNaam: naam !== '' }
            : b
        );
        toestand = { ...toestand, beurten: structuredClone(beurten) };
        return stand();
      }
      case 'vergadering_persoon_vervangen':
        weigerTijdensOpnieuw();
        pasMockPersoonToe(persoon, 'vervangen', argUit(args, 'aan'));
        return stand();
      case 'vergadering_persoon_weghalen':
        weigerTijdensOpnieuw();
        pasMockPersoonToe(persoon, 'weghalen', argUit(args, 'waarde'));
        return stand();
      case 'vergadering_persoon_markeer':
        weigerTijdensOpnieuw();
        pasMockPersoonToe(persoon, 'markeer', argUit(args, 'waarde'));
        return stand();
      case 'vergadering_zet_tekst': {
        const index = Number(argUit(args, 'index'));
        const doel = beurten.find((b) => b.index === index);
        if (!doel || toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        weigerTijdensOpnieuw();
        const ruw = argUit(args, 'tekst');
        const nieuweTekst = typeof ruw === 'string' ? ruw.trim() : '';
        if (nieuweTekst !== '' && nieuweTekst !== doel.tekst) {
          doel.tekst = nieuweTekst;
          verbeterd.add(index);
        }
        toestand = { ...toestand, beurten: structuredClone(beurten) };
        return stand();
      }
      case 'vergadering_audio_stuk':
        return maakTestWav(Number(argUit(args, 'beginMs')), Number(argUit(args, 'eindMs')));
      case 'vergadering_geef_aan_spreker': {
        const index = Number(argUit(args, 'index'));
        const nummerArg = argUit(args, 'spreker');
        const doel = beurten.find((b) => b.index === index);
        if (!doel || toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        // Null is "Nieuwe spreker": het eerstvolgende vrije nummer, tot en met 8.
        const gebruikt = new Set(beurten.map((b) => b.sprekernummer));
        let nummer = typeof nummerArg === 'number' ? nummerArg : 1;
        if (typeof nummerArg !== 'number') {
          while (gebruikt.has(nummer)) nummer += 1;
        }
        if (nummer > 8) {
          throw {
            melding: 'Kies een spreker van 1 tot en met 8.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        weigerTijdensOpnieuw();
        // De beurt krijgt de naam van die spreker (van zijn eerste beurt), of geen naam.
        const bestaand = beurten.find((b) => b.sprekernummer === nummer);
        beurten = voegBurenSamen(
          beurten.map((b) =>
            b.index === index
              ? {
                  ...vanSpreker(b, nummer),
                  spreker: bestaand?.spreker ?? `Spreker ${String(nummer)}`,
                  eigenNaam: bestaand?.eigenNaam ?? false
                }
              : b
          )
        );
        toestand = { ...toestand, beurten: structuredClone(beurten) };
        return stand();
      }
      case 'vergadering_voeg_sprekers_samen': {
        const van = Number(argUit(args, 'van'));
        const naar = Number(argUit(args, 'naar'));
        if (toestand.type !== 'verslag' || van === naar) {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        weigerTijdensOpnieuw();
        const bestaand = beurten.find((b) => b.sprekernummer === naar);
        beurten = voegBurenSamen(
          beurten.map((b) =>
            b.sprekernummer === van
              ? {
                  ...vanSpreker(b, naar),
                  spreker: bestaand?.spreker ?? `Spreker ${String(naar)}`,
                  eigenNaam: bestaand?.eigenNaam ?? false
                }
              : b
          )
        );
        toestand = { ...toestand, beurten: structuredClone(beurten) };
        return stand();
      }
      case 'vergadering_opslaan': {
        if (opties.has('bestaat') && vraag === null) {
          vraag = { id: volgendId() };
          return {
            type: 'bestaatAl',
            vraag: vraag.id,
            naam: VERSLAG_NAAM,
            voorstel: VERSLAG_NAAM.replace('.docx', ' (2).docx')
          } satisfies BewaarUitslag;
        }
        const naam =
          toestand.type === 'verslag' && toestand.soort === 'alleen' ? NOTITIE_NAAM : VERSLAG_NAAM;
        opgeslagenPad = MAP.pad + '\\' + naam;
        if (opties.has('audionaast') || opties.has('audiofout')) {
          // Het mengen duurt even; zolang staat "Bezig met opslaan" er.
          const uitslag: BewaarUitslag = {
            type: 'opgeslagen',
            naam,
            map: MAP,
            ...(opties.has('audiofout')
              ? { audioFout: 'De schijf is vol. Maak ruimte vrij of kies een andere plek.' }
              : { audio: naam.replace(/\.docx$/, '.wav') })
          };
          return wacht(900).then(() => uitslag);
        }
        return { type: 'opgeslagen', naam, map: MAP } satisfies BewaarUitslag;
      }
      case 'bestand_open_opgeslagen':
      case 'bestand_toon_opgeslagen': {
        // Alleen voor het verslag van deze mock; transcripten van Bestanden blijven bij die mock.
        if (toestand.type !== 'verslag' || Number(argUit(args, 'taak')) !== toestand.taak) {
          return ONBEKEND;
        }
        if (opgeslagenPad === null) {
          throw {
            melding: 'Het opgeslagen bestand staat niet meer op die plek.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        console.info(
          `[mock] ${cmd === 'bestand_open_opgeslagen' ? 'geopend' : 'in de Verkenner getoond'}: ${opgeslagenPad}`
        );
        return null;
      }
      case 'bestand_bewaar_beslis': {
        // Alleen onze eigen vraag; de rest is van Bestanden.
        if (vraag?.id !== Number(argUit(args, 'vraag'))) return ONBEKEND;
        vraag = null;
        const keuze = argUit(args, 'keuze');
        if (keuze === 'annuleren') return { type: 'geannuleerd' } satisfies BewaarUitslag;
        opgeslagenPad =
          MAP.pad +
          '\\' +
          (keuze === 'vervangen' ? VERSLAG_NAAM : VERSLAG_NAAM.replace('.docx', ' (2).docx'));
        return {
          type: 'opgeslagen',
          naam: keuze === 'vervangen' ? VERSLAG_NAAM : VERSLAG_NAAM.replace('.docx', ' (2).docx'),
          map: MAP
        } satisfies BewaarUitslag;
      }
      case 'vergadering_kopieer':
        return null;
      case 'vergadering_audio_bewaren':
        return wacht(900).then(() =>
          toestand.type === 'verslag' && toestand.soort === 'alleen' ? NOTITIE_WAV : WAV_NAAM
        );
      case 'vergadering_sluiten':
        if (toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        if (opnieuw !== null) throw OPNIEUW_SLUITEN;
        if (notulen.bezig()) {
          throw fout2(
            'Mondig maakt notulen van dit verslag. Stop dat eerst, of wacht tot het klaar is.'
          );
        }
        // Is het verslag niet bewaard, dan weigert Sluiten, tenzij de gebruiker toch sluit.
        if (opties.has('bewaarfout') && argUit(args, 'toch') !== true) {
          throw {
            melding:
              'Mondig kon je laatste wijzigingen niet bewaren. Kopieer of sla het verslag op, of sluit toch.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        // Sluiten wist niets: het verslag staat daarna onder Eerder opgenomen.
        bewaarOpenVerslag();
        sluit();
        // De melding over het niet bewaarde verslag verdwijnt niet stil.
        if (opties.has('bewaarfout')) fout = BEWAARFOUT;
        return stand();
      case 'vergadering_verslag_openen': {
        const id = argUit(args, 'id');
        if (toestand.type !== 'klaar' && toestand.type !== 'verslag') throw nietNu();
        if (opnieuw !== null) throw OPNIEUW_SLUITEN;
        const gekozen = bewaard.find((v) => v.info.id === id);
        if (!gekozen) {
          throw {
            melding: 'Dit verslag staat er niet meer.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        if (toestand.type === 'verslag') {
          // Het open verslag gaat eerst dicht, zonder wissen.
          bewaarOpenVerslag();
          opgeslagenPad = null;
        }
        vergeetOpnieuw();
        modelId = opties.has('snelmodel') ? 'parakeet-ultra-q8' : 'canary-1b-v2-q8';
        bewaard = bewaard.filter((v) => v.info.id !== id);
        beurten = structuredClone(gekozen.beurten);
        huidigId = gekozen.info.id;
        verslagNaam = gekozen.info.naam;
        verslagDuurMs = gekozen.info.duurMs;
        persoon.vervangen = gekozen.persoon.vervangen;
        persoon.genegeerd = new Set(gekozen.persoon.genegeerd);
        persoon.eigen = new Set(gekozen.persoon.eigen);
        fout = null;
        toestand = verslag(gekozen.info.soort, gekozen.info.beginMs, volgendId());
        return stand();
      }
      case 'vergadering_verslag_wissen': {
        const id = argUit(args, 'id');
        if (typeof id === 'string') {
          if (!bewaard.some((v) => v.info.id === id)) {
            throw {
              melding: 'Dit verslag staat er niet meer.',
              vervolg: 'geen'
            } satisfies Foutmelding;
          }
          bewaard = bewaard.filter((v) => v.info.id !== id);
        } else {
          if (toestand.type !== 'verslag') {
            throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
          }
          if (opnieuw !== null) throw OPNIEUW_SLUITEN;
          sluit();
        }
        return stand();
      }
      case 'vergadering_verslag_naam': {
        if (toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        weigerTijdensOpnieuw();
        const ruw = argUit(args, 'naam');
        verslagNaam = typeof ruw === 'string' ? schoneVerslagnaam(ruw) : null;
        toestand = { ...toestand, naam: verslagNaam };
        return stand();
      }
      case 'vergadering_opnieuw_keuzes': {
        if (toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        const geluidMs = toestand.duurMs * (toestand.soort === 'online' ? 2 : 1);
        const keuzes: OpnieuwKeuzes = {
          rol: MODELLEN[modelId]?.rol ?? null,
          keuzes: Object.entries(MODELLEN)
            // Alleen nauwkeuriger dan het model van nu, zoals Rust.
            .filter(([id, m]) => id !== modelId && rang(m.rol) > rang(MODELLEN[modelId]?.rol))
            .map(([id, m]) => ({
              modelId: id,
              rol: m.rol,
              aanwezig: m.rol === 'nauwkeurig' || opties.has('whisper'),
              grootteBytes: m.grootte,
              // Alleen het model dat al eens draaide, is gemeten.
              duurMs:
                opties.has('geenmeting') || m.rol !== 'nauwkeurig'
                  ? null
                  : Math.round(geluidMs * m.factor)
            }))
        };
        return wacht(150).then(() => keuzes);
      }
      case 'vergadering_opnieuw_start': {
        if (toestand.type !== 'verslag') {
          throw { melding: 'Er is geen verslag open.', vervolg: 'geen' } satisfies Foutmelding;
        }
        if (opnieuw !== null) {
          throw {
            melding: 'Mondig schrijft dit verslag al opnieuw uit.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        const model = String(argUit(args, 'model'));
        const gekozen = MODELLEN[model];
        if (!gekozen || (gekozen.rol === 'extra_nauwkeurig' && !opties.has('whisper'))) {
          throw {
            melding: 'Dit model staat nog niet op deze computer. Download het eerst bij Modellen.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        opnieuwModel = model;
        naOpnieuw = null;
        const duurMs = toestand.duurMs * (toestand.soort === 'online' ? 2 : 1) * gekozen.factor;
        opnieuw = { rol: gekozen.rol, voortgang: 0, restMs: Math.round(duurMs) };
        // In de mock duurt het ongeveer tien seconden.
        opnieuwTimer = window.setInterval(() => {
          if (opnieuw === null) return;
          const voortgang = Math.min(100, (opnieuw.voortgang ?? 0) + 5);
          opnieuw = {
            ...opnieuw,
            voortgang,
            restMs: Math.round((duurMs * (100 - voortgang)) / 100)
          };
          if (voortgang >= 100) {
            if (opnieuwTimer !== null) window.clearInterval(opnieuwTimer);
            opnieuwTimer = null;
            rondOpnieuwAf();
          }
          meldStand();
        }, 500);
        return stand();
      }
      case 'vergadering_opnieuw_stoppen':
        if (opnieuwTimer !== null) window.clearInterval(opnieuwTimer);
        opnieuwTimer = null;
        opnieuw = null;
        return stand();
      case 'vergadering_vorige_terugzetten': {
        if (opnieuw !== null) throw OPNIEUW_BEZIG;
        if (vorige === null || toestand.type !== 'verslag') {
          throw {
            melding: 'Er is geen vorige versie van dit verslag.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        beurten = vorige.beurten;
        modelId = vorige.modelId;
        persoon.vervangen = vorige.persoon.vervangen;
        persoon.genegeerd = vorige.persoon.genegeerd;
        persoon.eigen = vorige.persoon.eigen;
        verbeterd.clear();
        vorige.verbeterd.forEach((i) => verbeterd.add(i));
        vorige = null;
        naOpnieuw = null;
        veranderd = new Set();
        toestand = { ...toestand, beurten: structuredClone(beurten) };
        return stand();
      }
      default:
        return ONBEKEND;
    }
  };
}
