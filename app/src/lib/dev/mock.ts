/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een gewone waarde (tekst of foutmelding), geen Error */
// Alleen voor ontwikkeling in de gewone browser (`vite dev` zonder Tauri): doet alsof de app
// er is, zodat de schermen en hun toestanden na te lopen zijn. Zit niet in de productiebuild
// (de aanroep in +layout.ts staat achter `import.meta.env.DEV`).
//
// Kies een scenario met `?mock=` in het adres, losse opties gescheiden door komma's:
//   status:  gedeeltelijk | compleet | krap | statusfout   (standaard: ontbreekt)
//   modellen: drie | canaryloopt                  (pagina Modellen, open /modellen?mock=...)
//            compleet: Parakeet Ultra staat er, voor beide taken (ontwerp 5-A, scherm 1);
//            drie: Ultra, Canary en Whisper turbo staan er, Ultra voor dicteren en Canary
//            voor bestanden (scherm 5); canaryloopt: Ultra staat er en Canary wordt gedownload
//            (scherm 2 en 3); met krap is er te weinig ruimte voor de rest (randgeval A)
//   download: snel | fout | hash | traag | hapering | traagstart   (standaard: 20 MB/s)
//            fout: bij 42 procent valt de verbinding weg, twee nieuwe pogingen, dan een fout;
//            hapering: één nieuwe poging en daarna gaat hij door; traagstart: Rust antwoordt
//            pas na twee seconden op Downloaden (om "Wordt gestart…" te zien)
//   rij:     de downloadrij met drie items: Canary komt binnen, Whisper turbo wacht en het
//            sprekermodel is mislukt (zijbalk). onderbroken: Canary staat als onderbroken
//   proef:   stil | procesfout | geenmodel | traag         (standaard: lukt met tekst)
//   dictaten: dictatenweek | dictatenleeg | dictatenniet | dictatenwisfout, zie mock-dictaten.ts.
//            Standaard staan er voorbeelddictaten van vandaag en gisteren (24 uur bewaard).
//   sneltoets: terugval | geensneltoets                    (standaard: Ctrl+Spatie)
//   dictaat: dictaat | dictaatfout | dictaatstil | dictaatlang | dictaatblok
//            start 1,5 s na het laden een dictaat met de sneltoets, zoals de app dat doet
//            (open /instellingen?mock=compleet,dictaat voor een dictaat op een ander scherm);
//            dictaatblok: Windows blokkeert de microfoon
//   pil:     open /pil?mock=dictaat voor de zwevende pil; met `&klok=540000` staat de klok
//            negen minuten verder (de waarschuwing). `snel` maakt het uitschrijven korter
//            dan een seconde, zodat "verwerkt" niet verschijnt
//   sprekers: sprekermodel | sprekersuit, voor "Sprekers herkennen" (4b-D). Zonder opties staat het
//            sprekermodel er niet: de schakelaar vraagt dan om de download (Instellingen en
//            Bestanden). sprekermodel: het model staat er al; sprekersuit: de instelling staat uit.
//            De verslagen met sprekers staan bij vergadering: sprekers | verslagsprekers |
//            verslagruimtesprekers | sprekerstap, zie mock-vergadering.ts.
//   microfoon: micweg | geenmic
//            micweg: de bewaarde microfoon is niet meer aangesloten; geenmic: de lijst mislukt
//            (de meters hebben eigen scenario's, zie mock-meter.ts)
//   bezet:   elke andere sneltoets is "al in gebruik"; vangfout: opschorten lukt niet
//   plakken: beheer | plakfout | klembord
//            beheer: het venster draait met beheerrechten; plakfout: plakken mislukt;
//            klembord: de terugval "Alleen op het klembord zetten" staat aan
//   over:    nieuw | eerste | versiefout | modelfout | adresfout
//            nieuw: een eerdere versie was de laatst geziene, dus de kaart "Wat is er nieuw"
//            staat op Dicteren (standaard is de versie al gezien); eerste: een eerste
//            installatie, dus geen kaart en de versie wordt bewaard; versiefout: de versiegegevens
//            laden niet; modelfout: de modelgegevens laden niet; adresfout: een link naar
//            buiten de app opent niet. Open /over?mock=compleet voor het scherm Over Mondig.
//   bestanden: wachtrij | slepen | geenmodel | afgebroken | bestaat | automatisch | bewaarfout,
//            zie mock-bestanden.ts. Open
//            /bestanden?mock=compleet,wachtrij voor de rij uit het ontwerp.
//   vergadering: ruimte | alleen | opname | opnameruimte | opnamealleen | stil | uitschrijven |
//            klaarmoment | verslag | verslagruimte | verslagalleen | echo | onafgemaakt | veelwachtend |
//            driewachtend | twaalfwachtend | allesoorten | audionaast | audiofout | startfout |
//            geenapparaten, zie mock-vergadering.ts. Open
//            /vergadering?mock=compleet voor het scherm; alles werkt ook door te klikken.
//   videokaart: gpugeen | gpuingebouwd | gpuzoekfout | gpuaan | gpuprocessor | gputerugval |
//            gputestniet | gputestfaalt (Instellingen, Spraakmodellen, Meer opties).
//            Standaard is er een losse kaart (RTX 4090), staat de schakelaar uit en is er
//            nog geen motor geladen; de snelheidstest geeft na twee seconden "9,2 keer".
//            gpugeen: geen kaart; gpuingebouwd: alleen een ingebouwde chip; gpuzoekfout:
//            zoeken lukte niet; gpuaan: aan en in gebruik op de videokaart; gpuprocessor: in
//            gebruik op de processor; gputerugval: aan, maar teruggevallen op de processor;
//            gputestniet: de test zegt dat het niet loont; gputestfaalt: de videokaart faalt.
//   computer: geheugen8 | geheugen4 | weinigruimte | geenkaartproef (eerste-start-wizard en
//            "Computer opnieuw checken"; de computer is anders een laptop van 16 GB). Zonder
//            `compleet` staat er geen model en opent de wizard; `?mock=compleet` slaat hem over.
//            afstelschijfvol: "Advies overnemen" (Instellingen, Mondig opnieuw afstellen) faalt met
//            "te weinig ruimte". Standaard geeft het advies de videokaart en het nauwkeurige
//            model; `?mock=compleet,drie,gpugeen` geeft "Alles staat al goed".
//            download: schijfvol laat de download bij 42 procent stoppen met "De schijf is vol".
//            nooitgecontroleerd: er is geen laatste controle, dus op Modellen geen "Past bij deze
//            computer" (met `compleet` is er anders een controle van 2 november).
//   opschonen: opschonen | opschonenelk | opschoondubbel | opschoonterugval, zie mock-opschonen.ts
//            (met `notulen` voor een aanbieder). Ctrl+Shift+Spatie start een dictaat met de
//            tweede sneltoets. Met `opschonenelk,dictaat` schoont het automatische dictaat op.
//            dictaatopgeschoond: het nieuwste voorbeelddictaat is opgeschoond (geschiedenis).
//   eigentoets: de gebruiker koos zelf een sneltoets (Insert).
//            logboek: logfout (kopiëren lukt niet) | terugzetbezet (standaardsneltoets bezet)
//   meters:  meterfout | metergeenapp | meterstil | metermuziek | meteruitval | testgeluidfout,
//            zie mock-meter.ts (niveaumeters zonder opname en het testgeluid, fase UX-C).

import type { InvokeArgs } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { mockIPC } from '@tauri-apps/api/mocks';
import type { PlakUitkomst, PlakUitslag } from '../dictaat';
import { labelVanToets } from '../sneltoetsvangen';
import type { Oordeel } from '../sneltoets';
import { maakBestandenMock, ONBEKEND } from './mock-bestanden';
import { maakDictatenMock } from './mock-dictaten';
import { maakMeterMock } from './mock-meter';
import { maakOpschonenMock } from './mock-opschonen';
import type { NotulenInstellingen } from '../notulen';
import { maakVergaderMock } from './mock-vergadering';
import type { RijToestand } from '../downloadrij';
import type { Gebeurtenis, ProefResultaat, Toestand } from '../taak';

// Vaste voorbeelden van de drie uitkomsten, zodat de bevestiging en de meldingen in de gewone
// browser te zien zijn. De echte regels staan in Rust (`mondig-core`, sneltoets.rs) en worden
// hier bewust niet nagebouwd: F1 tot en met F12 los geeft een waarschuwing, een losse
// typtoets of Pause een fout, al het andere is goed.
/** De `aan` uit de argumenten van een schakelaar-commando; `standaard` als die ontbreekt. */
function leesAan(args: Args | undefined, standaard: boolean): boolean {
  const gelezen =
    args && !Array.isArray(args) && !(args instanceof ArrayBuffer) && !(args instanceof Uint8Array)
      ? args['aan']
      : standaard;
  return Boolean(gelezen);
}

function mockOordeel(toets: string): Oordeel {
  const klein = toets.toLowerCase();
  const functie = /^f(\d+)$/.exec(klein)?.[1];
  if (functie !== undefined && Number(functie) <= 12) {
    return { uitkomst: 'waarschuwing', soort: 'functietoets', toets: toets.toUpperCase() };
  }
  if (/^key[a-z]$/.test(klein)) return { uitkomst: 'fout', fout: 'losseToets' };
  if (klein.endsWith('pause')) return { uitkomst: 'fout', fout: 'werktNiet' };
  return { uitkomst: 'goed' };
}

// De modellen van de catalogus zoals Rust ze teruggeeft: de grootte is met het hulpmodel (Silero).
type Gebruik = 'dicteren' | 'bestanden';
interface MockModel {
  id: string;
  naam: string;
  grootte: number;
  nauwkeurigheid: number;
  snelheid: number;
  talen: string[];
  nietVoor: Gebruik[];
  /** Waar het model voor dient; standaard een spraakmodel. */
  soort?: 'spraak' | 'sprekers';
  /** De rol zoals Rust hem uitrekent (`model::modelrol`); geen bij het sprekermodel. */
  rol?: 'snel' | 'nauwkeurig' | 'extra_nauwkeurig';
}
const ULTRA = 'parakeet-ultra-q8';
const CANARY = 'canary-1b-v2-q8';
const WHISPER = 'whisper-large-v3-turbo-q8';
const SORTFORMER = 'sortformer-4spk-v2.1-q8';
const WHISPER_TALEN = [
  'af',
  'am',
  'ar',
  'as',
  'az',
  'ba',
  'be',
  'bg',
  'bn',
  'bo',
  'br',
  'bs',
  'ca',
  'cs',
  'cy',
  'da',
  'de',
  'el',
  'en',
  'es',
  'et',
  'eu',
  'fa',
  'fi',
  'fo',
  'fr',
  'gl',
  'gu',
  'haw',
  'ha',
  'he',
  'hi',
  'hr',
  'ht',
  'hu',
  'hy',
  'id',
  'is',
  'it',
  'ja',
  'jw',
  'ka',
  'kk',
  'km',
  'kn',
  'ko',
  'la',
  'lb',
  'ln',
  'lo',
  'lt',
  'lv',
  'mg',
  'mi',
  'mk',
  'ml',
  'mn',
  'mr',
  'ms',
  'mt',
  'my',
  'ne',
  'nl',
  'nn',
  'no',
  'oc',
  'pa',
  'pl',
  'ps',
  'pt',
  'ro',
  'ru',
  'sa',
  'sd',
  'si',
  'sk',
  'sl',
  'sn',
  'so',
  'sq',
  'sr',
  'su',
  'sv',
  'sw',
  'ta',
  'te',
  'tg',
  'tk',
  'tl',
  'tr',
  'tt',
  'uk',
  'ur',
  'uz',
  'vi',
  'yi',
  'yo',
  'yue',
  'zh'
];
const EUROPA = [
  'bg',
  'hr',
  'cs',
  'da',
  'nl',
  'en',
  'et',
  'fi',
  'fr',
  'de',
  'el',
  'hu',
  'it',
  'lv',
  'lt',
  'mt',
  'pl',
  'pt',
  'ro',
  'ru',
  'sk',
  'sl',
  'es',
  'sv',
  'uk'
];
const MODELLEN: MockModel[] = [
  {
    id: ULTRA,
    naam: 'Parakeet Ultra',
    grootte: 741_604_224,
    nauwkeurigheid: 3,
    snelheid: 5,
    talen: EUROPA,
    nietVoor: [],
    rol: 'snel'
  },
  {
    id: CANARY,
    naam: 'Canary 1b-v2',
    grootte: 1_145_531_072,
    nauwkeurigheid: 4,
    snelheid: 4,
    talen: EUROPA,
    nietVoor: [],
    rol: 'nauwkeurig'
  },
  {
    id: WHISPER,
    naam: 'Whisper large-v3 turbo',
    grootte: 887_622_784,
    nauwkeurigheid: 5,
    snelheid: 2,
    talen: WHISPER_TALEN,
    nietVoor: ['dicteren'],
    rol: 'extra_nauwkeurig'
  },
  {
    // Het sprekermodel (4b-D): geen spraakmodel, dus voor geen taak te kiezen.
    id: SORTFORMER,
    naam: 'Sortformer v2.1',
    grootte: 139_310_336,
    nauwkeurigheid: 1,
    snelheid: 1,
    talen: [],
    nietVoor: ['dicteren', 'bestanden'],
    soort: 'sprekers'
  }
];
const GROOTTE = (id: string): number => MODELLEN.find((m) => m.id === id)?.grootte ?? 0;
const VRIJ_RUIM = 212_000_000_000;
const VRIJ_KRAP = 310_000_000;

type Args = InvokeArgs | undefined;

// Dezelfde velden als modellen.json (naamsvermelding, ADR 0016), voor het scherm Over Mondig.
const CC = 'https://creativecommons.org/licenses/by/4.0/legalcode.en';
const MIT = 'https://opensource.org/license/mit';
const MOCK_VERMELDINGEN = [
  {
    id: 'parakeet-ultra-q8',
    naam: 'Parakeet Ultra (standaard, snel, 25 Europese talen)',
    hulpmodel: false,
    maker: 'Moondream',
    titel: 'Moondream Parakeet Ultra',
    copyright: null,
    bronUrl: 'https://huggingface.co/moondream/parakeet-ultra',
    ggufUrl: 'https://huggingface.co/handy-computer/parakeet-ultra-gguf',
    licentie: 'CC-BY-4.0',
    licentieUrl: CC,
    disclaimerUrl: CC + '#s5',
    wijziging:
      'Nagetraind door Moondream op parakeet-tdt-0.6b-v3 van NVIDIA. Daarna omgezet naar GGUF en gekwantiseerd (Q8_0) door handy-computer.'
  },
  {
    id: 'whisper-large-v3-turbo-q8',
    naam: 'Whisper large-v3 turbo (99 talen)',
    hulpmodel: false,
    maker: 'OpenAI',
    titel: 'whisper-large-v3-turbo',
    copyright: 'Copyright (c) 2022 OpenAI',
    bronUrl: 'https://huggingface.co/openai/whisper-large-v3-turbo',
    ggufUrl: 'https://huggingface.co/handy-computer/whisper-large-v3-turbo-gguf',
    licentie: 'MIT',
    licentieUrl: MIT,
    disclaimerUrl: MIT,
    wijziging:
      'Omgezet naar GGUF en gekwantiseerd (Q8_0) door handy-computer. De gewichten zijn niet opnieuw getraind.'
  },
  {
    id: 'silero-vad-v6.2',
    naam: 'Silero VAD v6.2 (stiltedetectie, hulpmodel)',
    hulpmodel: true,
    maker: 'Silero Team',
    titel: 'Silero VAD',
    copyright: 'Copyright (c) 2020-present Silero Team',
    bronUrl: 'https://github.com/snakers4/silero-vad',
    ggufUrl: 'https://huggingface.co/handy-computer/silero-vad-v6.2-gguf',
    licentie: 'MIT',
    licentieUrl: MIT,
    disclaimerUrl: MIT,
    wijziging:
      'Omgezet naar GGUF (F32, niet gekwantiseerd) door handy-computer. De gewichten zijn niet opnieuw getraind.'
  }
];

/** Het taak-id uit de argumenten van een commando. */
function taakUit(args: Args): number {
  if (
    args &&
    !Array.isArray(args) &&
    !(args instanceof ArrayBuffer) &&
    !(args instanceof Uint8Array)
  ) {
    return Number(args['taak']);
  }
  return 0;
}

export function installeerDevMock(): void {
  const opties = new Set(
    (new URLSearchParams(location.search).get('mock') ?? '').split(',').filter(Boolean)
  );
  const aanwezig: Record<string, number> = Object.fromEntries(MODELLEN.map((m) => [m.id, 0]));
  if (opties.has('compleet') || opties.has('canaryloopt')) aanwezig[ULTRA] = GROOTTE(ULTRA);
  if (opties.has('gedeeltelijk')) aanwezig[ULTRA] = 310_000_000;
  // Het sprekermodel staat er al (anders vraagt de schakelaar Sprekers herkennen om de download).
  if (opties.has('sprekermodel')) aanwezig[SORTFORMER] = GROOTTE(SORTFORMER);
  if (opties.has('drie')) {
    aanwezig[ULTRA] = GROOTTE(ULTRA);
    aanwezig[CANARY] = GROOTTE(CANARY);
    aanwezig[WHISPER] = GROOTTE(WHISPER);
  }
  // Wat de gebruiker al zag op de pagina Modellen.
  const gezien = new Set<string>(MODELLEN.map((m) => m.id));
  const keuzes: Partial<Record<Gebruik, string>> = {};
  const compleet = (id: string): boolean => (aanwezig[id] ?? 0) >= GROOTTE(id);
  // Dezelfde regels als `modelkeuze` in Rust (geldend_model), voor de mock.
  const geldend = (gebruik: Gebruik): string => {
    const gekozen = keuzes[gebruik];
    if (gekozen) return gekozen;
    if (gebruik === 'dicteren') return ULTRA;
    return compleet(CANARY) ? CANARY : geldend('dicteren');
  };
  const krap = opties.has('krap');
  let volgendId = 1;
  // De dictaatgeschiedenis en het laatste dictaat in het geheugen, zoals Rust ze bijhoudt.
  const dictaten = maakDictatenMock(opties);
  // De downloadrij, zoals `mondig_core::downloadrij`: na elkaar, nooit twee keer hetzelfde model.
  interface MockItem {
    id: number;
    model: string;
    toestand: RijToestand;
    voortgang: { binnen: number; totaal: number } | null;
  }
  let rij: MockItem[] = [];
  let rijTimer: number | null = null;
  // Het volgnummer van de rij, zoals in Rust: loopt op bij elke wijziging.
  let rijVersie = 0;
  const rijStand = (): { versie: number; items: MockItem[] } => ({
    versie: rijVersie,
    items: rij.map((i) => ({ ...i }))
  });
  let proefTimers: number[] = [];
  let dictaatTimers: number[] = [];
  // De laatste controle van "Mondig opnieuw afstellen", zoals Rust hem bewaart (`computer.rs`).
  // Met `compleet` is er een controle van 2 november; `nooitgecontroleerd`: nog nooit.
  let controle: {
    dicteren: string;
    bestanden: string;
    videokaart: string;
    datumMs: number;
  } | null =
    opties.has('compleet') && !opties.has('nooitgecontroleerd')
      ? {
          dicteren: ULTRA,
          bestanden: CANARY,
          videokaart: 'laten',
          datumMs: Date.UTC(2026, 10, 2, 9)
        }
      : null;

  // `eigentoets`: de gebruiker koos zelf een sneltoets (Insert), zoals in de gebruikerstest.
  let huidigeToets = opties.has('terugval')
    ? 'Ctrl+Alt+Space'
    : opties.has('eigentoets')
      ? 'Insert'
      : 'Ctrl+Space';
  let rusttijd = '5m';
  let alleenKlembord = opties.has('klembord');
  // Zoals in Rust staat "Stopwoorden weghalen" standaard aan; `stopwoordenuit` zet hem uit.
  let stopwoordenAan = !opties.has('stopwoordenuit');
  // Zoals in Rust staat "Sprekers herkennen" standaard aan; `sprekersuit` zet hem uit.
  let sprekersAan = !opties.has('sprekersuit');
  let videokaartAan = opties.has('gpuaan') || opties.has('gputerugval');
  let apparaatStand: string | null = opties.has('gpuaan')
    ? 'videokaart'
    : opties.has('gputerugval')
      ? 'terugval'
      : opties.has('gpuprocessor')
        ? 'processor'
        : null;
  const videokaarten = opties.has('gpugeen')
    ? []
    : opties.has('gpuingebouwd')
      ? [{ naam: 'Intel(R) Iris(R) Xe Graphics', soort: 'ingebouwd' }]
      : opties.has('gpuzoekfout')
        ? null
        : [{ naam: 'NVIDIA GeForce RTX 4090', soort: 'los' }];
  let opgeslagenMicrofoon: string | null = opties.has('micweg') ? 'bluetooth-weg' : null;
  let niveauTimer: number | null = null;
  let pilPositie = 'onder';
  let geluid = { aan: true, volume: 50 };
  let autostart = false;
  let verborgen = false;
  let afspeelsnelheid = 10;
  let verborgenIngesteld = false;
  const MOCK_VERSIE = '0.3.0-dev';
  let laatstGezien: string | null = opties.has('nieuw')
    ? '0.2.0'
    : opties.has('eerste')
      ? null
      : MOCK_VERSIE;
  const microfoons = [
    { id: 'akg', naam: 'AKG USB-headset' },
    { id: 'realtek', naam: 'Microfoon (Realtek Audio)' },
    { id: 'jabra', naam: 'Headset (Jabra Evolve2 65)' }
  ];

  const meldTaak = (taak: number, soort: Gebeurtenis['soort'], toestand: Toestand): void => {
    void emit('taak', { taak, soort, toestand } satisfies Gebeurtenis);
  };

  const vrijOp = (): number => (krap ? VRIJ_KRAP : VRIJ_RUIM);
  // Hoeveel ruimte er te weinig is voor wat nog binnen moet komen, met 20 procent marge en
  // minstens 100 MB, zoals in Rust.
  const tekortVoor = (id: string): number => {
    const nog = GROOTTE(id) - (aanwezig[id] ?? 0);
    const marge = Math.max(Math.ceil(nog * 0.2), 100_000_000);
    return compleet(id) ? 0 : Math.max(0, nog + marge - vrijOp());
  };
  // Per model de taken van "Downloaden en gebruiken", voor "Verder downloaden" (zoals in Rust).
  const daarnaOnthouden = new Map<string, string[]>();

  const status = (): Record<string, unknown> => {
    const id = geldend('dicteren');
    const grootte = GROOTTE(id);
    const binnen = aanwezig[id] ?? 0;
    return {
      id,
      status: binnen >= grootte ? 'compleet' : binnen > 0 ? 'gedeeltelijk' : 'ontbreekt',
      grootteBytes: grootte,
      aanwezigBytes: binnen,
      vrijBytes: vrijOp(),
      tekortBytes: tekortVoor(id),
      bron: 'huggingface.co'
    };
  };

  const meldModellenGewijzigd = (): void => {
    void emit('modellen-gewijzigd', null);
  };

  const modellenOverzicht = (): Record<string, unknown> => {
    const dicteren = geldend('dicteren');
    const bestanden = geldend('bestanden');
    const regels = MODELLEN.map((m) => {
      const binnen = aanwezig[m.id] ?? 0;
      const inGebruikVoor = (
        [
          ['dicteren', dicteren],
          ['bestanden', bestanden]
        ] as const
      )
        .filter(([, id]) => id === m.id)
        .map(([gebruik]) => gebruik);
      return {
        id: m.id,
        naam: m.naam,
        soort: m.soort ?? 'spraak',
        rol: m.rol ?? null,
        nauwkeurigheid: m.nauwkeurigheid,
        snelheid: m.snelheid,
        talen: m.talen,
        status: binnen >= m.grootte ? 'compleet' : binnen > 0 ? 'gedeeltelijk' : 'ontbreekt',
        grootteBytes: m.grootte,
        aanwezigBytes: binnen,
        tekortBytes: tekortVoor(m.id),
        nietVoor: m.nietVoor,
        inGebruikVoor,
        kanWeg:
          binnen > 0 &&
          inGebruikVoor.length === 0 &&
          !rij.some((i) => i.model === m.id && actief(i)),
        nieuw: (m.soort ?? 'spraak') === 'spraak' && !gezien.has(m.id),
        bron: 'huggingface.co',
        daarnaBijHervatten:
          binnen > 0 && binnen < m.grootte ? (daarnaOnthouden.get(m.id) ?? []) : []
      };
    });
    return {
      modellen: regels,
      dicteren,
      bestanden,
      downloadt: bezigItem()?.model ?? null,
      totaalBytes: Object.values(aanwezig).reduce((som, n) => som + n, 0),
      vrijBytes: vrijOp()
    };
  };

  const actief = (item: MockItem): boolean =>
    ['wacht', 'bezig', 'nieuwe_poging'].includes(item.toestand.type);
  const bezigItem = (): MockItem | undefined =>
    rij.find((i) => i.toestand.type === 'bezig' || i.toestand.type === 'nieuwe_poging');
  const meldRij = (): void => {
    rijVersie += 1;
    void emit('downloads-gewijzigd', rijStand());
    meldModellenGewijzigd();
  };
  const zet = (item: MockItem, toestand: RijToestand): void => {
    item.toestand = toestand;
    meldRij();
  };

  // Voert het eerste wachtende item uit, zoals de werkdraad in Rust.
  const volgende = (): void => {
    if (rijTimer !== null) return;
    const item = rij.find((i) => i.toestand.type === 'wacht');
    if (!item) return;
    const model = item.model;
    const totaal = GROOTTE(model);
    const stap = opties.has('snel') ? 50_000_000 : opties.has('traag') ? 1_000_000 : 5_000_000;
    let binnen = aanwezig[model] ?? 0;
    item.voortgang = { binnen, totaal };
    zet(item, { type: 'bezig' });
    meldTaak(item.id, 'download', {
      type: 'verwerkt',
      voortgang: Math.floor((binnen * 100) / totaal)
    });
    let pauze = 0;
    let pogingen = 0;
    const klaarMet = (toestand: RijToestand): void => {
      if (rijTimer !== null) window.clearInterval(rijTimer);
      rijTimer = null;
      zet(item, toestand);
      volgende();
    };
    rijTimer = window.setInterval(() => {
      if (pauze > 0) {
        pauze -= 1;
        return;
      }
      // De schijf loopt vol: een fout zonder nieuwe poging, met de vaste melding uit Rust
      // (`Probleem::SchijfVol`).
      if (opties.has('schijfvol') && binnen >= totaal * 0.42) {
        opties.delete('schijfvol');
        const melding =
          'De schijf is vol. Maak ruimte vrij en probeer het opnieuw; wat al binnen is, blijft bewaard.';
        meldTaak(item.id, 'download', { type: 'fout', melding, vervolg: 'opnieuw_proberen' });
        klaarMet({ type: 'fout', melding, vervolg: 'opnieuw_proberen' });
        return;
      }
      const haperen = opties.has('fout') || opties.has('hapering');
      if (haperen && binnen >= totaal * 0.42 && item.toestand.type === 'bezig') {
        const van = 5;
        pogingen += 1;
        if (opties.has('fout') && pogingen > 2) {
          opties.delete('fout');
          meldTaak(item.id, 'download', {
            type: 'fout',
            melding:
              'De verbinding viel steeds weg, ook na een paar nieuwe pogingen. Wat al binnen is, blijft bewaard. Probeer het opnieuw als je internet weer werkt.',
            vervolg: 'opnieuw_proberen'
          });
          klaarMet({
            type: 'fout',
            melding:
              'De verbinding viel steeds weg, ook na een paar nieuwe pogingen. Wat al binnen is, blijft bewaard. Probeer het opnieuw als je internet weer werkt.',
            vervolg: 'opnieuw_proberen'
          });
          return;
        }
        zet(item, { type: 'nieuwe_poging', poging: pogingen, van });
        // Een paar seconden wachten, zoals de oplopende wachttijd in Rust.
        pauze = 8;
        if (opties.has('hapering')) opties.delete('hapering');
        return;
      }
      binnen = Math.min(totaal, binnen + stap);
      aanwezig[model] = binnen;
      item.voortgang = { binnen, totaal };
      if (item.toestand.type === 'nieuwe_poging') zet(item, { type: 'bezig' });
      void emit('download-voortgang', { id: item.id, model, binnen, totaal });
      meldTaak(item.id, 'download', {
        type: 'verwerkt',
        voortgang: Math.floor((binnen * 100) / totaal)
      });
      if (opties.has('hash') && binnen >= totaal) {
        opties.delete('hash');
        aanwezig[model] = 0;
        klaarMet({
          type: 'fout',
          melding: 'Het gedownloade bestand is beschadigd en is weggegooid. Download het opnieuw.',
          vervolg: 'opnieuw_downloaden'
        });
      } else if (binnen >= totaal) {
        if (rijTimer !== null) window.clearInterval(rijTimer);
        // Een korte pauze voor de controle, zoals in de echte app.
        window.setTimeout(() => {
          // Eerst de keuzes zetten, dan melden, zoals de app het doet.
          for (const gebruik of daarnaOnthouden.get(model) ?? []) {
            keuzes[gebruik as Gebruik] = model;
          }
          daarnaOnthouden.delete(model);
          meldTaak(item.id, 'download', { type: 'klaar' });
          rijTimer = null;
          zet(item, { type: 'klaar' });
          volgende();
        }, 800);
      }
    }, 250);
  };

  const voegToe = (model: string): number => {
    const bestaand = rij.find((i) => i.model === model && actief(i));
    if (bestaand) return bestaand.id;
    rij = rij.filter((i) => i.model !== model);
    const id = volgendId++;
    gezien.add(model);
    rij.push({ id, model, toestand: { type: 'wacht' }, voortgang: null });
    meldRij();
    volgende();
    return id;
  };

  const stopModel = (model: string): void => {
    // Ook een onderbroken download: "Niet hervatten".
    const item = rij.find(
      (i) => i.model === model && (actief(i) || i.toestand.type === 'onderbroken')
    );
    if (!item) return;
    const loopt = item.toestand.type === 'bezig' || item.toestand.type === 'nieuwe_poging';
    if (loopt && rijTimer !== null) {
      window.clearInterval(rijTimer);
      rijTimer = null;
      meldTaak(item.id, 'download', { type: 'geannuleerd' });
    }
    zet(item, { type: 'gestopt' });
    volgende();
  };

  const stopProefTimers = (): void => {
    proefTimers.forEach((t) => {
      window.clearTimeout(t);
    });
    proefTimers = [];
  };

  const startProef = (): number => {
    const id = volgendId++;
    const traag = opties.has('traag');
    const na = (ms: number, werk: () => void): void => {
      proefTimers.push(window.setTimeout(werk, ms));
    };
    meldTaak(id, 'dictaat', { type: 'luistert' });
    na(500, () => {
      meldTaak(id, 'dictaat', { type: 'verwerkt', voortgang: null });
    });
    na(traag ? 6000 : 2000, () => {
      if (opties.has('procesfout')) {
        meldTaak(id, 'dictaat', {
          type: 'fout',
          melding: 'Het spraakprogramma is onverwacht gestopt. Probeer het opnieuw.',
          vervolg: 'opnieuw_proberen'
        });
        return;
      }
      const stil = opties.has('stil');
      const tekst = stil
        ? null
        : 'De gemeenteraad besluit om het plan voor de nieuwe sporthal uit te stellen tot na de zomer.';
      if (tekst) dictaten.nieuw(tekst);
      meldTaak(id, 'dictaat', { type: 'klaar' });
      void emit('proef-resultaat', {
        taak: id,
        uitkomst: stil ? 'geen_geluid' : 'tekst',
        tekst,
        duurMs: traag ? 5500 : 1400,
        soevereiniteit: 'lokaal'
      } satisfies ProefResultaat);
    });
    return id;
  };

  const stopNiveau = (): void => {
    if (niveauTimer !== null) window.clearInterval(niveauTimer);
    niveauTimer = null;
  };

  const stopDictaatTimers = (): void => {
    dictaatTimers.forEach((t) => {
      window.clearTimeout(t);
    });
    dictaatTimers = [];
    stopNiveau();
  };

  // Het niveau voor de meter in de pil, zoals de opnamelaag het twintig keer per seconde stuurt.
  const startNiveau = (): void => {
    stopNiveau();
    const begin = Date.now();
    niveauTimer = window.setInterval(() => {
      const t = Date.now() - begin;
      const golf = (Math.sin(t / 180) + 1) / 2;
      void emit('pil-niveau', 0.15 + 0.75 * golf * (0.5 + 0.5 * Math.random()));
    }, 50);
  };

  // Een dictaat met de sneltoets: luistert, verwerkt, klaar (of fout), zoals de app het meldt.
  const startDictaat = (metTweede = false): void => {
    const id = volgendId++;
    const na = (ms: number, werk: () => void): void => {
      dictaatTimers.push(window.setTimeout(werk, ms));
    };
    void emit('pil-taak', id);
    meldTaak(id, 'dictaat', { type: 'luistert' });
    startNiveau();
    const luisterduur = opties.has('dictaatlang') ? 560_000 : 3500;
    na(luisterduur, () => {
      stopNiveau();
      if (opties.has('dictaatblok')) {
        meldTaak(id, 'dictaat', {
          type: 'fout',
          melding:
            'Windows laat Mondig de microfoon niet gebruiken. Zet in de Windows-instellingen bij Privacy en beveiliging, Microfoon, de toegang voor apps en voor bureaublad-apps aan.',
          vervolg: 'microfooninstelling_openen'
        });
        return;
      }
      if (opties.has('dictaatfout')) {
        meldTaak(id, 'dictaat', {
          type: 'fout',
          melding: 'Mondig hoort niets. Controleer of je microfoon aan staat.',
          vervolg: 'microfooninstelling_openen'
        });
        return;
      }
      meldTaak(id, 'dictaat', { type: 'verwerkt', voortgang: null });
      // Zoals de app: na het uitschrijven eerst opschonen (als dat aan staat), dan plakken.
      const rondAf = (tekst: string | null, stil: boolean): void => {
        meldTaak(id, 'dictaat', { type: 'klaar' });
        void emit('dictaat-resultaat', {
          taak: id,
          uitkomst: stil ? 'geen_tekst' : 'tekst',
          tekst,
          duurMs: 1300,
          soevereiniteit: 'lokaal'
        } satisfies ProefResultaat);
        if (tekst) {
          // Plakken komt kort na de uitslag, zoals in de app.
          // Staat het proefveld op de pagina in focus, dan is Mondig zelf vooraan en plakt de
          // app niet (zoals in Rust): de tekst gaat dan in het proefveld.
          const inMondig =
            opties.has('plakinmondig') ||
            (document.activeElement instanceof HTMLElement &&
              document.activeElement.id === 'proefveld');
          const uitkomst: PlakUitkomst = inMondig
            ? 'in_mondig'
            : alleenKlembord
              ? 'op_klembord'
              : opties.has('beheer')
                ? 'beheerrechten'
                : opties.has('plakfout')
                  ? 'mislukt'
                  : 'geplakt';
          na(300, () => {
            void emit('plak-uitslag', {
              taak: id,
              uitkomst,
              vorigKlembord: 'bewaard'
            } satisfies PlakUitslag);
          });
        }
      };
      na(opties.has('snel') ? 500 : 2500, () => {
        const stil = opties.has('dictaatstil');
        const letterlijk =
          'ehm goedemorgen allemaal eh welkom bij de eh raadsvergadering we beginnen met de vaststelling van de agenda';
        const schoon =
          'Goedemorgen allemaal, welkom bij de raadsvergadering. We beginnen met de vaststelling van de agenda.';
        const plan = stil ? null : opschonen.plan(metTweede);
        if (plan === null) {
          if (!stil) dictaten.nieuw(schoon);
          rondAf(stil ? null : schoon, stil);
          return;
        }
        void emit('pil-opschonen', { taak: id, fase: { type: 'bezig', label: plan.label } });
        const terugval = opties.has('opschoonterugval');
        // Hooguit drie seconden, zoals de wachttijd van de kern; stoppen kan met het kruisje.
        let gewacht = 0;
        const wacht = (): void => {
          gewacht += 100;
          const klaar = opschonen.gestopt(id) || gewacht >= (terugval ? 3000 : 2500);
          if (!klaar) {
            na(100, wacht);
            return;
          }
          const opgeschoond = !terugval && !opschonen.gestopt(id);
          void emit('pil-opschonen', {
            taak: id,
            fase: { type: opgeschoond ? 'opgeschoond' : 'letterlijk' }
          });
          if (opgeschoond) {
            dictaten.nieuw(schoon, {
              letterlijk,
              door: { model: plan.model, aanbieder: plan.aanbieder, label: plan.label }
            });
          } else {
            dictaten.nieuw(letterlijk);
          }
          rondAf(opgeschoond ? schoon : letterlijk, false);
        };
        na(100, wacht);
      });
    });
  };

  const tekstArg = (args: Args, naam: string, terugval: string): string => {
    if (
      args &&
      !Array.isArray(args) &&
      !(args instanceof ArrayBuffer) &&
      !(args instanceof Uint8Array)
    ) {
      const waarde = args[naam];
      if (typeof waarde === 'string') return waarde;
    }
    return terugval;
  };

  const lijstArg = (args: Args, naam: string): string[] => {
    if (
      args &&
      !Array.isArray(args) &&
      !(args instanceof ArrayBuffer) &&
      !(args instanceof Uint8Array)
    ) {
      const waarde = args[naam];
      if (Array.isArray(waarde)) return waarde.filter((x): x is string => typeof x === 'string');
    }
    return [];
  };

  const microfoonLijst = (): Record<string, unknown> => {
    if (opties.has('geenmic')) {
      throw {
        melding: 'Mondig vindt geen microfoon. Sluit een microfoon of headset aan.',
        vervolg: 'microfooninstelling_openen'
      };
    }
    return {
      microfoons,
      standaardNaam: 'AKG USB-headset',
      opgeslagen: opgeslagenMicrofoon,
      opgeslagenAanwezig: microfoons.some((m) => m.id === opgeslagenMicrofoon)
    };
  };

  // De tweede sneltoets (ADR 0027): alleen aan bij "Met een eigen sneltoets".
  let tweedeToets = 'Ctrl+Shift+Space';
  const opschoonToets = (): Record<string, unknown> => {
    const label = labelVanToets(tweedeToets);
    const dubbel = opties.has('opschoondubbel');
    const aan = opschonen.stand() === 'eigenSneltoets' && !dubbel;
    return {
      actief: aan ? label : null,
      isTerugval: false,
      gewenst: label,
      geprobeerd: [label],
      dubbel
    };
  };

  const sneltoets = (): Record<string, unknown> => {
    if (opties.has('geensneltoets')) {
      return {
        actief: null,
        isTerugval: false,
        gewenst: 'Ctrl+Spatie',
        geprobeerd: ['Ctrl+Spatie', 'Ctrl+Alt+Spatie', 'Ctrl+Shift+Spatie']
      };
    }
    const terugval = opties.has('terugval') && huidigeToets === 'Ctrl+Alt+Space';
    const label = labelVanToets(huidigeToets);
    return {
      actief: label,
      isTerugval: terugval,
      gewenst: terugval ? 'Ctrl+Spatie' : label,
      geprobeerd: [label]
    };
  };

  const bestanden = maakBestandenMock(opties, () => volgendId++);
  const vergadering = maakVergaderMock(opties, () => volgendId++);
  // Opschonen leest de aanbieder uit de mock van Notulen, zoals Rust uit de instellingen.
  const opschonen = maakOpschonenMock(opties, () => {
    const antwoord = vergadering('notulen_instellingen', undefined);
    return antwoord === ONBEKEND ? null : (antwoord as NotulenInstellingen);
  });
  const meters = maakMeterMock(opties);

  mockIPC(
    (cmd: string, args: Args) => {
      // Meters eerst: een vergadering die start, stopt ze (voorrang), en geeft dan door.
      const meterAntwoord = meters(cmd, args);
      if (meterAntwoord !== ONBEKEND) return meterAntwoord;
      // Vergadering eerst: het hangt zijn eigen vraag "bestaat al" aan de beslissing van 4-C.
      const vergaderAntwoord = vergadering(cmd, args);
      if (vergaderAntwoord !== ONBEKEND) return vergaderAntwoord;
      const opschoonAntwoord = opschonen.handel(cmd, args);
      if (opschoonAntwoord !== ONBEKEND) return opschoonAntwoord;
      const dictatenAntwoord = dictaten.handel(cmd, args);
      if (dictatenAntwoord !== ONBEKEND) return dictatenAntwoord;
      const bestandenAntwoord = bestanden(cmd, args);
      if (bestandenAntwoord !== ONBEKEND) return bestandenAntwoord;
      switch (cmd) {
        case 'sneltoets_controleer':
          return mockOordeel(tekstArg(args, 'toets', ''));
        case 'model_status':
          if (opties.has('statusfout')) throw 'Er is geen gegevensmap gevonden op deze computer.';
          return status();
        case 'model_download_start': {
          const model = tekstArg(args, 'model', ULTRA);
          // `null` is verder downloaden: dan gelden de taken van de eerste start.
          const hervat =
            args !== undefined &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array) &&
            args['daarna'] === null;
          const daarna = hervat ? (daarnaOnthouden.get(model) ?? []) : lijstArg(args, 'daarna');
          if (!MODELLEN.some((m) => m.id === model)) {
            throw { melding: 'Dit model kent Mondig niet.', vervolg: 'geen' };
          }
          const alInDeRij = rij.find((i) => i.model === model && actief(i));
          if (alInDeRij) return alInDeRij.id;
          const beginnen = (): number => {
            if (compleet(model)) {
              throw { melding: 'Dit model staat al op je computer.', vervolg: 'geen' };
            }
            if (tekortVoor(model) > 0) {
              throw {
                melding: 'Er is te weinig ruimte op de schijf om dit model te downloaden.',
                vervolg: 'opnieuw_proberen'
              };
            }
            if (!hervat) daarnaOnthouden.set(model, daarna);
            return voegToe(model);
          };
          if (!opties.has('traagstart')) return beginnen();
          // Rust antwoordt pas na twee seconden: zo is "Wordt gestart…" te zien.
          return new Promise<number>((klaar, mis) => {
            window.setTimeout(() => {
              try {
                klaar(beginnen());
              } catch (fout) {
                // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- zoals Tauri: een foutmelding, geen Error
                mis(fout);
              }
            }, 2000);
          });
        }
        case 'download_stop': {
          const model =
            args !== undefined &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array) &&
            typeof args['model'] === 'string'
              ? args['model']
              : bezigItem()?.model;
          if (model) stopModel(model);
          return null;
        }
        case 'downloads_lijst':
          return rijStand();
        case 'download_lopend': {
          const item = bezigItem();
          return item
            ? {
                taak: item.id,
                model: item.model,
                toestand: {
                  type: 'verwerkt',
                  voortgang: Math.floor(((item.voortgang?.binnen ?? 0) * 100) / GROOTTE(item.model))
                },
                voortgang: item.voortgang
              }
            : null;
        }
        case 'modellen_overzicht':
          if (opties.has('modellenfout')) throw 'Geen gegevensmap in de dev-mock';
          return modellenOverzicht();
        case 'modellen_gezien':
          MODELLEN.forEach((m) => gezien.add(m.id));
          meldModellenGewijzigd();
          return null;
        case 'model_kiezen': {
          // Zoals Rust (`afstellen::model_kiezen`): de taak gaat weg bij andere downloads; staat
          // het model er, dan meteen in gebruik; komt het binnen, dan komt de taak erbij; anders
          // in de rij met deze taak.
          const gebruik = tekstArg(args, 'gebruik', 'dicteren') as Gebruik;
          const model = tekstArg(args, 'model', ULTRA);
          const gekozen = MODELLEN.find((m) => m.id === model);
          if (!gekozen || gekozen.nietVoor.includes(gebruik)) {
            throw { melding: 'Dit model is voor deze taak niet te kiezen.', vervolg: 'geen' };
          }
          for (const [ander, taken] of daarnaOnthouden) {
            if (ander !== model)
              daarnaOnthouden.set(
                ander,
                taken.filter((g) => g !== gebruik)
              );
          }
          if (compleet(model)) {
            keuzes[gebruik] = model;
            daarnaOnthouden.delete(model);
          } else if (rij.some((i) => i.model === model && actief(i))) {
            const al = daarnaOnthouden.get(model) ?? [];
            daarnaOnthouden.set(model, [...new Set([...al, gebruik])]);
          } else {
            if (tekortVoor(model) > 0) {
              throw {
                melding: 'Er is te weinig ruimte op de schijf om dit model te downloaden.',
                vervolg: 'opnieuw_proberen'
              };
            }
            daarnaOnthouden.set(model, [gebruik]);
            voegToe(model);
          }
          meldModellenGewijzigd();
          return { videokaartAan: false, gewisseld: [], downloadt: [] };
        }
        case 'model_gebruik_zet': {
          const gebruik = tekstArg(args, 'gebruik', 'dicteren') as Gebruik;
          const model = tekstArg(args, 'model', ULTRA);
          const gekozen = MODELLEN.find((m) => m.id === model);
          if (!gekozen) throw { melding: 'Dit model kent Mondig niet.', vervolg: 'geen' };
          if (gekozen.nietVoor.includes(gebruik)) {
            throw { melding: 'Dit model is voor deze taak niet te kiezen.', vervolg: 'geen' };
          }
          if (!compleet(model)) {
            throw {
              melding: 'Dit model staat nog niet helemaal op je computer. Download het eerst.',
              vervolg: 'opnieuw_downloaden'
            };
          }
          keuzes[gebruik] = model;
          meldModellenGewijzigd();
          return null;
        }
        case 'model_verwijder': {
          const model = tekstArg(args, 'model', '');
          if (geldend('dicteren') === model || geldend('bestanden') === model) {
            throw {
              melding:
                'Je gebruikt dit model nog. Kies eerst een ander model; anders kan Mondig niets uitschrijven.',
              vervolg: 'geen'
            };
          }
          if (rij.some((i) => i.model === model && actief(i))) {
            throw {
              melding: 'Dit model wordt nog gedownload. Stop eerst de download.',
              vervolg: 'geen'
            };
          }
          const vrij = aanwezig[model] ?? 0;
          aanwezig[model] = 0;
          // Zoals in Rust: een onderbroken of afgelopen download van dit model gaat uit de rij.
          const lengte = rij.length;
          rij = rij.filter((i) => i.model !== model || actief(i));
          if (rij.length !== lengte) meldRij();
          meldModellenGewijzigd();
          return vrij;
        }
        case 'sprekers_instelling':
          return {
            aan: sprekersAan,
            model: {
              id: SORTFORMER,
              status: compleet(SORTFORMER) ? 'compleet' : 'ontbreekt',
              grootteBytes: GROOTTE(SORTFORMER),
              aanwezigBytes: aanwezig[SORTFORMER] ?? 0,
              vrijBytes: vrijOp(),
              tekortBytes: tekortVoor(SORTFORMER),
              bron: 'huggingface.co'
            }
          };
        case 'sprekers_zet':
          sprekersAan =
            args !== undefined &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array) &&
            args['aan'] === true;
          return null;
        case 'proef_start':
          if (opties.has('geenmodel')) {
            throw {
              melding: 'Het spraakmodel is er nog niet. Download het eerst.',
              vervolg: 'opnieuw_downloaden'
            };
          }
          stopProefTimers();
          return startProef();
        case 'proef_annuleer':
          stopProefTimers();
          meldTaak(taakUit(args), 'dictaat', { type: 'geannuleerd' });
          return null;
        case 'sneltoets_status':
          return tekstArg(args, 'soort', 'dicteren') === 'opschonen'
            ? opschoonToets()
            : sneltoets();
        case 'sneltoets_vang_start':
          if (opties.has('vangfout')) {
            throw {
              melding: 'Mondig kan de sneltoets nu niet wijzigen. Probeer het opnieuw.',
              vervolg: 'opnieuw_proberen'
            };
          }
          return null;
        case 'sneltoets_vang_stop':
          return tekstArg(args, 'soort', 'dicteren') === 'opschonen'
            ? opschoonToets()
            : sneltoets();
        case 'sneltoets_wijzig': {
          const toets = tekstArg(args, 'toets', '');
          // Zoals Rust: de twee sneltoetsen van Mondig mogen niet gelijk zijn.
          if (tekstArg(args, 'soort', 'dicteren') === 'opschonen') {
            if (toets === huidigeToets) throw { soort: 'dubbel' };
            if (opties.has('bezet') && toets !== tweedeToets) throw { soort: 'bezet' };
            tweedeToets = toets;
            opties.delete('opschoondubbel');
            return opschoonToets();
          }
          if (opschonen.stand() === 'eigenSneltoets' && toets === tweedeToets) {
            throw { soort: 'dubbel' };
          }
          if (opties.has('bezet') && toets !== huidigeToets) {
            throw { soort: 'bezet' };
          }
          huidigeToets = toets;
          opties.delete('terugval');
          return sneltoets();
        }
        case 'alleen_klembord_instelling':
          return alleenKlembord;
        case 'alleen_klembord_zet':
          alleenKlembord = leesAan(args, false);
          return null;
        case 'stopwoorden_instelling':
          return stopwoordenAan;
        case 'stopwoorden_zet':
          stopwoordenAan = leesAan(args, true);
          return null;
        case 'versie_info':
          if (opties.has('versiefout')) throw 'Geen versiegegevens in de dev-mock';
          return {
            versie: MOCK_VERSIE,
            bouw: 'af75d93',
            datum: '2026-11-01',
            systeem: 'Windows 11 (build 26200)',
            processor: 'x86-64',
            model: {
              id: 'parakeet-ultra-q8',
              naam: 'Parakeet Ultra (standaard, snel, 25 Europese talen)'
            }
          };
        case 'werkproces_bouw':
          return '0.3.0-dev+af75d93 (protocol 4)';
        case 'model_vermeldingen':
          if (opties.has('modelfout')) throw 'Geen modelgegevens in de dev-mock';
          return MOCK_VERMELDINGEN;
        case 'open_vermeldingsadres':
          if (opties.has('adresfout')) throw 'Openen lukte niet';
          window.open(tekstArg(args, 'adres', ''), '_blank', 'noopener');
          return null;
        case 'laatst_geziene_versie':
          return laatstGezien;
        case 'laatst_geziene_versie_zet':
          laatstGezien = tekstArg(args, 'versie', MOCK_VERSIE);
          return null;
        case 'klembord_zet':
          return !opties.has('logfout');
        case 'videokaart_info':
          return { aan: videokaartAan, kaarten: videokaarten };
        case 'videokaart_zet':
          videokaartAan = Boolean(
            args &&
              !Array.isArray(args) &&
              !(args instanceof ArrayBuffer) &&
              !(args instanceof Uint8Array)
              ? args['aan']
              : false
          );
          // Zoals in Rust: de regel begint opnieuw; de motor laadt even later op het nieuwe
          // apparaat (hier na 800 ms, alsof er gedicteerd wordt).
          apparaatStand = null;
          void emit('videokaart-gewijzigd', videokaartAan);
          void emit('apparaat-gewijzigd', null);
          window.setTimeout(() => {
            apparaatStand = videokaartAan ? 'videokaart' : 'processor';
            void emit('apparaat-gewijzigd', apparaatStand);
          }, 800);
          return null;
        case 'apparaat_in_gebruik':
          return apparaatStand;
        case 'snelheidstest_start': {
          const uitkomst = opties.has('gputestfaalt')
            ? {
                oordeel: 'videokaart_faalde',
                keerSneller: null,
                processor: 0.082,
                videokaart: null
              }
            : opties.has('gputestniet')
              ? { oordeel: 'loont_niet', keerSneller: 0.8, processor: 0.08, videokaart: 0.1 }
              : { oordeel: 'loont', keerSneller: 9.2, processor: 0.082, videokaart: 0.0089 };
          return new Promise((klaar) => {
            window.setTimeout(() => {
              klaar(uitkomst);
            }, 2000);
          });
        }
        case 'laatste_controle':
          return controle;
        case 'laatste_controle_bewaar': {
          const velden =
            args &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array)
              ? args
              : {};
          controle = {
            dicteren: String(velden['dicteren']),
            bestanden: String(velden['bestanden']),
            videokaart: String(velden['videokaart']),
            datumMs: Date.now()
          };
          return controle;
        }
        case 'computer_check': {
          const proef =
            args &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array)
              ? (args['proef'] as { oordeel: string; keerSneller: number | null } | null)
              : null;
          const gib = opties.has('geheugen4') ? 3.8 : opties.has('geheugen8') ? 7.4 : 15.7;
          const apart = gib >= 7;
          const heeftLos = videokaarten?.some((k) => k.soort === 'los') ?? false;
          let stand = 'geen';
          if (videokaarten && videokaarten.length > 0) stand = heeftLos ? 'testen' : 'ingebouwd';
          if (proef && videokaarten && videokaarten.length > 0) {
            stand =
              proef.oordeel === 'loont'
                ? 'aanzetten'
                : proef.oordeel === 'loont_niet'
                  ? 'laten'
                  : 'faalde';
          }
          const vrij = opties.has('weinigruimte') ? 600_000_000 : 212_000_000_000;
          const tekort = (nodig: number): number =>
            Math.max(0, nodig + Math.max(nodig / 5, 100_000_000) - vrij);
          return new Promise((klaar) => {
            window.setTimeout(() => {
              klaar({
                computer: {
                  geheugenBytes: Math.round(gib * 1024 ** 3),
                  kernen: 8,
                  vrijBytes: vrij,
                  videokaarten
                },
                advies: {
                  geheugen:
                    gib < 3.5 ? 'te_weinig' : gib < 7 ? 'weinig' : gib < 14.5 ? 'genoeg' : 'ruim',
                  dicteren: ULTRA,
                  bestanden: apart ? CANARY : ULTRA,
                  bestandenApart: apart,
                  krap: apart && gib < 12,
                  tekortDicterenBytes: tekort(GROOTTE(ULTRA)),
                  tekortBeideBytes: tekort(GROOTTE(ULTRA) + (apart ? GROOTTE(CANARY) : 0)),
                  videokaart: { stand, keerSneller: proef?.keerSneller ?? null }
                },
                dicterenInGebruik: geldend('dicteren'),
                bestandenInGebruik: geldend('bestanden')
              });
            }, 600);
          });
        }
        case 'advies_overnemen': {
          // Zoals Rust (`afstellen.rs`): de videokaart meteen aan, een model dat er staat meteen in
          // gebruik, een model dat nog binnen moet komen in de downloadrij met zijn taken erbij.
          const punten =
            args &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array) &&
            Array.isArray(args['punten'])
              ? (args['punten'] as { soort: string; gebruik?: string; model?: string }[])
              : [];
          const uitkomst = {
            videokaartAan: false,
            gewisseld: [] as { gebruik: string; model: string }[],
            downloadt: [] as { model: string; taken: string[] }[]
          };
          const teHalen = new Map<string, string[]>();
          for (const punt of punten) {
            if (punt.soort !== 'model' || !punt.model || !punt.gebruik) continue;
            if (compleet(punt.model)) {
              keuzes[punt.gebruik as Gebruik] = punt.model;
              uitkomst.gewisseld.push({ gebruik: punt.gebruik, model: punt.model });
            } else {
              const taken = teHalen.get(punt.model) ?? [];
              if (!taken.includes(punt.gebruik)) taken.push(punt.gebruik);
              teHalen.set(punt.model, taken);
            }
          }
          if (opties.has('afstelschijfvol') && teHalen.size > 0) {
            throw {
              melding: 'Er is te weinig ruimte op de schijf om dit model te downloaden.',
              vervolg: 'opnieuw_proberen'
            };
          }
          for (const [model, taken] of teHalen) {
            const al = daarnaOnthouden.get(model) ?? [];
            daarnaOnthouden.set(model, [...new Set([...al, ...taken])]);
            voegToe(model);
            uitkomst.downloadt.push({ model, taken });
          }
          if (punten.some((p) => p.soort === 'videokaart')) {
            videokaartAan = true;
            void emit('videokaart-gewijzigd', true);
            uitkomst.videokaartAan = true;
            apparaatStand = null;
            void emit('apparaat-gewijzigd', null);
            window.setTimeout(() => {
              apparaatStand = 'videokaart';
              void emit('apparaat-gewijzigd', apparaatStand);
            }, 800);
          }
          meldModellenGewijzigd();
          return uitkomst;
        }
        case 'logboek_tekst':
          return 'Mondig 0.3.0 (bouw mock, 2026-11-02)\nSysteem: Windows 11, x86_64\n\n2026-11-02T15:04:05Z info app_gestart versie=0.3.0 os=windows\n';
        case 'logboek_map_open':
          return null;
        case 'instellingen_terugzetten':
          return { sneltoetsBezet: opties.has('terugzetbezet') };
        case 'rusttijd_instelling':
          return rusttijd;
        case 'rusttijd_zet':
          rusttijd = tekstArg(args, 'sleutel', '5m');
          return null;
        case 'microfoon_lijst':
          return microfoonLijst();
        case 'microfoon_kies': {
          opgeslagenMicrofoon = tekstArg(args, 'id', '') || null;
          return null;
        }
        case 'dictaat_annuleer':
          stopDictaatTimers();
          meldTaak(taakUit(args), 'dictaat', { type: 'geannuleerd' });
          return null;
        case 'dictaat_stop':
          return null;
        case 'pil_toon':
        case 'pil_verberg':
        case 'pil_open_mondig':
        case 'microfooninstelling_open':
          return null;
        case 'pil_stand':
          return { taak: null, luistert: false };
        case 'pil_positie_instelling':
          return pilPositie;
        case 'pil_positie_zet':
          pilPositie = tekstArg(args, 'plek', 'onder');
          return null;
        case 'geluid_instelling':
          return geluid;
        case 'geluid_zet':
          if (
            args &&
            !Array.isArray(args) &&
            !(args instanceof ArrayBuffer) &&
            !(args instanceof Uint8Array)
          ) {
            geluid = args['instelling'] as typeof geluid;
          }
          return null;
        case 'autostart_instelling':
          return autostart;
        case 'autostart_zet':
          autostart = Boolean(
            args &&
              !Array.isArray(args) &&
              !(args instanceof ArrayBuffer) &&
              !(args instanceof Uint8Array)
              ? args['aan']
              : false
          );
          // Zoals in Rust: autostart aan zet "Verborgen starten" mee aan, tenzij de gebruiker het
          // zelf al instelde.
          if (autostart && !verborgenIngesteld) {
            verborgen = true;
            verborgenIngesteld = true;
          }
          return null;
        case 'afspeelsnelheid_instelling':
          return afspeelsnelheid;
        case 'afspeelsnelheid_zet':
          afspeelsnelheid = Number(
            args &&
              !Array.isArray(args) &&
              !(args instanceof ArrayBuffer) &&
              !(args instanceof Uint8Array)
              ? args['snelheid']
              : 10
          );
          return null;
        case 'verborgen_starten_instelling':
          return verborgen;
        case 'verborgen_starten_zet':
          verborgen = Boolean(
            args &&
              !Array.isArray(args) &&
              !(args instanceof ArrayBuffer) &&
              !(args instanceof Uint8Array)
              ? args['aan']
              : false
          );
          verborgenIngesteld = true;
          return null;
        default:
          throw `Onbekend commando in de dev-mock: ${cmd}`;
      }
    },
    { shouldMockEvents: true }
  );
  // Laat de rest van de code denken dat het in de app draait.
  Object.assign(globalThis, { isTauri: true });
  // Canary komt al binnen als de pagina opent (ontwerp 5-A, scherm 2 en 3).
  if (opties.has('canaryloopt')) {
    aanwezig[CANARY] = Math.floor(GROOTTE(CANARY) * 0.38);
    daarnaOnthouden.set(CANARY, ['bestanden']);
    window.setTimeout(() => voegToe(CANARY), 300);
  }
  // De zijbalk met drie downloads: bezig, wacht en mislukt.
  if (opties.has('rij')) {
    aanwezig[CANARY] = Math.floor(GROOTTE(CANARY) * 0.38);
    aanwezig[SORTFORMER] = Math.floor(GROOTTE(SORTFORMER) * 0.6);
    opties.add('traag');
    rij = [
      {
        id: volgendId++,
        model: SORTFORMER,
        toestand: {
          type: 'fout',
          melding:
            'De verbinding viel steeds weg, ook na een paar nieuwe pogingen. Wat al binnen is, blijft bewaard. Probeer het opnieuw als je internet weer werkt.',
          vervolg: 'opnieuw_proberen'
        },
        voortgang: { binnen: aanwezig[SORTFORMER], totaal: GROOTTE(SORTFORMER) }
      }
    ];
    window.setTimeout(() => {
      voegToe(CANARY);
      voegToe(WHISPER);
    }, 300);
  }
  // Na een herstart: Canary was onderbroken toen Mondig sloot.
  if (opties.has('onderbroken')) {
    aanwezig[CANARY] = Math.floor(GROOTTE(CANARY) * 0.38);
    rij = [{ id: volgendId++, model: CANARY, toestand: { type: 'onderbroken' }, voortgang: null }];
  }
  if (
    ['dictaat', 'dictaatfout', 'dictaatstil', 'dictaatlang', 'dictaatblok'].some((o) =>
      opties.has(o)
    )
  ) {
    window.setTimeout(startDictaat, 1500);
  }
  // Ctrl+Spatie start een dictaat, zoals de globale sneltoets in de app. Niet tijdens het
  // vangen van een nieuwe sneltoets (daar roept het venster preventDefault aan).
  // Ctrl+Shift+Spatie is de tweede sneltoets: een opgeschoond dictaat bij "Met een eigen
  // sneltoets".
  window.addEventListener('keydown', (event) => {
    if (event.ctrlKey && event.code === 'Space' && !event.repeat && !event.defaultPrevented) {
      stopDictaatTimers();
      startDictaat(event.shiftKey && opschonen.stand() === 'eigenSneltoets');
    }
  });
}
