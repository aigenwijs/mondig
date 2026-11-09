/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een gewone waarde (een foutmelding), geen Error */
// De dev-mock voor Notulen (T3), los van mock-vergadering.ts. Hij volgt Rust: dezelfde commando's,
// dezelfde weigeringen en dezelfde vormen (app/src-tauri/src/notulen.rs en
// vergadering/notulen_maken.rs). Opties via `?mock=` (komma's ertussen):
//   notulen:        Notulen maken is ingesteld: Mistral, sleutel opgeslagen, training uit aangevinkt
//   notulenollama:  idem, maar met Ollama op deze computer (een model gekozen)
//   ollamauit:      Ollama draait niet
//   notulenbezig:   het maken loopt al (deel 2 van 3) en blijft staan
//   notulenklaar:   de notulen staan er: twee besluiten met bron, één niet teruggevonden, en twee
//                   actiepunten waarvan één met een wie die niet in het verslag staat (zonder
//                   sprekers, zoals een opname in de ruimte waar niemand een spreker aanwees, is die
//                   wie leeg: het verslag zegt dan niet wie "ik" is, zoals in Rust)
//   notulenvorige:  idem, maar gemaakt bij de vorige versie van het verslag (bronnen niet klikbaar)
// Zonder opties is Notulen maken niet ingesteld; instellen in Instellingen werkt gewoon. Maken
// duurt in de mock ongeveer vier seconden, in drie delen. De sleutel gaat heen en komt nooit terug.

import type { InvokeArgs } from '@tauri-apps/api/core';
import { MISTRAL_NAUWKEURIG, MISTRAL_SNEL } from '../notulen';
import type {
  NotulenActiepunt,
  NotulenAanbieder,
  NotulenBron,
  NotulenInfo,
  NotulenInstellingen,
  NotulenStand,
  OllamaStand,
  PuntSoort
} from '../notulen';
import type { Foutmelding } from '../taak';
import type { BeurtInfo, Vergadersoort } from '../vergadering';
import { ONBEKEND } from './mock-bestanden';

const NOTULEN_BEZIG: Foutmelding = {
  melding:
    'Mondig maakt notulen van dit verslag. Verbeteren kan weer als het klaar is, of als je stopt.',
  vervolg: 'geen'
};

const fout = (melding: string): Foutmelding => ({ melding, vervolg: 'geen' });

const arg = (args: InvokeArgs | undefined, sleutel: string): unknown =>
  args && !Array.isArray(args) && !(args instanceof ArrayBuffer) && !(args instanceof Uint8Array)
    ? args[sleutel]
    : undefined;

const tekstArg = (args: InvokeArgs | undefined, sleutel: string): string => {
  const waarde = arg(args, sleutel);
  return typeof waarde === 'string' ? waarde : '';
};

export interface NotulenMock {
  /** De notulen en het maken ervan in de stand van het verslag. */
  stand: () => NotulenStand;
  /** Maakt Mondig nu notulen? Dan weigeren wijzigen en sluiten. */
  bezig: () => boolean;
  /** Het verslag is gesloten of een ander verslag is geopend: vergeet de notulen. */
  vergeet: () => void;
  /** De notulen zoals ze nu zijn, om bij "Ongedaan maken" terug te zetten. */
  bewaar: () => NotulenInfo | null;
  /** Zet notulen terug die `bewaar` gaf. */
  zetTerug: (bewaard: NotulenInfo | null) => void;
  /**
   * Na een wijziging aan de sprekers, zoals `BewaardeNotulen::volg` in Rust: de bronnen wijzen
   * naar de nieuwe plek van samengevoegde beurten, en "Wie" heet zoals de spreker nu heet.
   */
  volg: (oud: BeurtInfo[], nieuw: BeurtInfo[]) => void;
  /** De commando's van Notulen; `ONBEKEND` voor de rest. */
  handel: (cmd: string, args: InvokeArgs | undefined) => unknown;
}

export function maakNotulenMock(
  opties: Set<string>,
  melden: () => void,
  beurten: () => BeurtInfo[],
  nieuweStand: () => unknown,
  vervangenNu: () => boolean = () => false,
  bijNieuw: () => void = () => undefined,
  soort: () => Vergadersoort = () => 'online',
  uitvoer: (tekst: string) => string = (tekst) => tekst,
  uitvoerNaam: (naam: string) => string = (naam) => naam
): NotulenMock {
  const ollamaKeuze = opties.has('notulenollama');
  const klaarOptie = opties.has('notulenklaar') || opties.has('notulenvorige');
  const ingesteld =
    opties.has('notulen') ||
    ollamaKeuze ||
    klaarOptie ||
    opties.has('notulenbezig') ||
    opties.has('notulenvorige');

  // Instellingen. De sleutel zelf bestaat in de mock niet: er is alleen "staat er een".
  let aanbieder: NotulenAanbieder | null = ollamaKeuze ? 'ollama' : ingesteld ? 'mistral' : null;
  let mistralModel = MISTRAL_NAUWKEURIG;
  let ollamaModel: string | null = ollamaKeuze ? 'qwen3:8b' : null;
  let trainingUit = ingesteld && !ollamaKeuze;
  let sleutelStaat = ingesteld && !ollamaKeuze;

  const instellingen = (): NotulenInstellingen => ({
    aanbieder,
    mistralModel,
    ollamaModel,
    trainingUit,
    sleutelStaat,
    sleutelFout: null
  });

  // De notulen van het verslag dat open is.
  let info: NotulenInfo | null = null;
  let maken: { klaar: number; totaal: number } | null = null;
  let timer: number | null = null;

  const bron = (index: number, citaat: string, gevonden = true, vorige = false): NotulenBron => {
    const lijst = beurten();
    const beurt = lijst[Math.min(index, Math.max(0, lijst.length - 1))];
    return {
      gevonden,
      beurt: gevonden && !vorige && beurt ? beurt.index : null,
      ms: gevonden && !vorige && beurt ? beurt.startMs : null,
      citaat
    };
  };

  // Heeft deze beurt een label voor het model (Rust: `Invoer::van`)? Online en bij Alleen ik elke
  // beurt; in de ruimte alleen een beurt die iemand aanwees (de mock geeft zo'n beurt een spreker).
  // Bij een regel zonder label is geen enkele "wie" de spreker: "ik" is dan niemand.
  const heeftLabel = (b: BeurtInfo | undefined): boolean =>
    soort() !== 'ruimte' || (b !== undefined && b.spreker !== null);

  const voorbeeld = (vorige: boolean): NotulenInfo => {
    const bronBeurt = beurten()[6] ?? beurten().at(-1);
    const zonderLabel = !heeftLabel(bronBeurt);
    const actiepunten: NotulenActiepunt[] = [
      {
        id: 1,
        wat: 'De omwonenden uitnodigen met een brief en een bericht in het buurthuis',
        wie: '',
        wanneer: '',
        watGetoond: '',
        wieGetoond: '',
        wanneerGetoond: '',
        wieNietTeruggevonden: false,
        wanneerNietTeruggevonden: false,
        bron: bron(4, 'Dan nodigen we de omwonenden uit met een brief', true, vorige)
      },
      {
        id: 2,
        wat: 'Een uitnodiging sturen naar de speeltuinvereniging',
        wie: zonderLabel ? '' : 'Jan',
        wanneer: 'deze week',
        watGetoond: '',
        wieGetoond: '',
        wanneerGetoond: '',
        wieNietTeruggevonden: !zonderLabel,
        wanneerNietTeruggevonden: false,
        bron: bron(6, 'Ik stuur hen deze week een uitnodiging', true, vorige)
      }
    ];
    return {
      samenvattingGetoond: '',
      samenvatting:
        'De bijeenkomst over het nieuwe speelveld wordt op een donderdagavond gehouden in de aula van de wijkschool. Omwonenden krijgen een brief en de speeltuinvereniging wordt uitgenodigd.',
      besluiten: [
        {
          id: 1,
          tekst: 'De bijeenkomst wordt op een donderdagavond gehouden.',
          tekstGetoond: '',
          bron: bron(1, 'bijeenkomst op een donderdagavond te houden', true, vorige)
        },
        {
          id: 2,
          tekst: 'De aula van de wijkschool is de locatie.',
          tekstGetoond: '',
          bron: bron(3, 'De aula heeft plek voor zo’n tachtig mensen', true, vorige)
        },
        {
          id: 3,
          tekst: 'De gemeente betaalt de zaalhuur.',
          tekstGetoond: '',
          bron: bron(3, 'De gemeente betaalt de huur van de zaal', false)
        }
      ],
      actiepunten,
      aanbieder: ollamaKeuze ? 'ollama' : 'mistral',
      model: ollamaKeuze ? 'qwen3:8b' : MISTRAL_NAUWKEURIG,
      momentMs: Date.now() - 4 * 60_000,
      taal: 'nl',
      vorigeVersie: vorige,
      gevonden: 4,
      nietGevonden: 1
    };
  };

  // Zoals Rust (`notulen_info`): met Vervangen aan staat op het scherm wat Kopiëren en Opslaan
  // geven. De echte tekst blijft in `info`, want bewerken begint daarmee. Het citaat is alleen om
  // te lezen en wordt direct vervangen.
  const toon = (n: NotulenInfo): NotulenInfo => {
    const bron = (b: NotulenBron): NotulenBron => ({ ...b, citaat: uitvoer(b.citaat) });
    return {
      ...n,
      samenvattingGetoond: uitvoer(n.samenvatting),
      besluiten: n.besluiten.map((b) => ({
        ...b,
        tekstGetoond: uitvoer(b.tekst),
        bron: bron(b.bron)
      })),
      actiepunten: n.actiepunten.map((a) => ({
        ...a,
        watGetoond: uitvoer(a.wat),
        wieGetoond: uitvoerNaam(a.wie),
        wanneerGetoond: uitvoer(a.wanneer),
        bron: bron(a.bron)
      }))
    };
  };

  // De beurten van het verslag staan er pas als het verslag er is: het voorbeeld wordt bij de
  // eerste stand gemaakt.
  let wacht: 'klaar' | 'vorige' | null = opties.has('notulenvorige')
    ? 'vorige'
    : opties.has('notulenklaar')
      ? 'klaar'
      : null;
  const bouwWachtende = (): void => {
    if (wacht !== null && beurten().length > 0) {
      info = voorbeeld(wacht === 'vorige');
      wacht = null;
    }
  };
  if (opties.has('notulenbezig')) maken = { klaar: 1, totaal: 3 };

  const stop = (): void => {
    if (timer !== null) window.clearInterval(timer);
    timer = null;
  };

  const start = (): void => {
    maken = { klaar: 0, totaal: 3 };
    stop();
    timer = window.setInterval(() => {
      if (maken === null) return;
      maken = { ...maken, klaar: maken.klaar + 1 };
      if (maken.klaar >= maken.totaal) {
        stop();
        maken = null;
        info = voorbeeld(false);
        // Zoals Rust: nieuwe notulen laten "Ongedaan maken" van een naam vervallen.
        bijNieuw();
      }
      melden();
    }, 1300);
  };

  const ollama = (): OllamaStand =>
    opties.has('ollamauit')
      ? { draait: false, modellen: [], melding: null }
      : { draait: true, modellen: ['qwen3:8b', 'ministral-3:8b'], melding: null };

  const weigerAlsBezig = (): void => {
    if (maken !== null) throw NOTULEN_BEZIG;
  };

  const geenNotulen = (): never => {
    throw fout('Dit verslag heeft geen notulen die Mondig kan aanpassen.');
  };

  return {
    stand: () => {
      bouwWachtende();
      return {
        info: info === null ? null : toon(structuredClone(info)),
        onleesbaar: false,
        maken: maken === null ? null : { ...maken }
      };
    },
    bezig: () => maken !== null,
    vergeet: () => {
      wacht = null;
      stop();
      info = null;
      maken = null;
    },
    bewaar: () => {
      bouwWachtende();
      return info === null ? null : structuredClone(info);
    },
    zetTerug: (bewaard) => {
      info = bewaard === null ? null : structuredClone(bewaard);
    },
    volg: (oud, nieuw) => {
      bouwWachtende();
      if (info === null || info.vorigeVersie) return;
      // Per oude beurt de nieuwe beurt van dezelfde kant die zijn tijden en tekst omvat.
      const kaart: number[] = [];
      let plek = 0;
      for (const b of oud) {
        const past = (n: BeurtInfo | undefined): boolean =>
          n?.kant === b.kant &&
          n.startMs <= b.startMs &&
          b.eindMs <= n.eindMs &&
          n.tekst.includes(b.tekst);
        while (plek < nieuw.length && !past(nieuw[plek])) plek += 1;
        if (plek >= nieuw.length) return;
        kaart.push(plek);
      }
      const verplaats = (b: NotulenBron): NotulenBron => {
        const n = b.beurt === null ? undefined : nieuw[kaart[b.beurt] ?? -1];
        return n ? { ...b, beurt: n.index, ms: n.startMs } : b;
      };
      // Per oude naam de nieuwe, alleen als die voor alle beurten gelijk is.
      const namen = new Map<string, string | null>();
      oud.forEach((b, i) => {
        const van = b.spreker ?? (b.kant === 'jij' ? 'Jij' : 'Anderen');
        const naar = nieuw[kaart[i] ?? -1]?.spreker ?? null;
        const andereKant = b.kant === 'jij' ? 'Anderen' : 'Jij';
        const doel = naar === andereKant ? null : naar;
        const eerder = namen.get(van);
        namen.set(van, eerder === undefined || eerder === doel ? doel : null);
      });
      const nieuweNaam = (wie: string): string | null => {
        if (wie.trim() === '') return null;
        for (const [van, naar] of namen) {
          if (naar !== null && naar !== van && van.toLowerCase() === wie.trim().toLowerCase()) {
            return naar;
          }
        }
        return null;
      };
      info = {
        ...info,
        besluiten: info.besluiten.map((p) => ({ ...p, bron: verplaats(p.bron) })),
        actiepunten: info.actiepunten.map((a) => {
          const naar = nieuweNaam(a.wie);
          return {
            ...a,
            wie: naar ?? a.wie,
            wieNietTeruggevonden: naar === null ? a.wieNietTeruggevonden : false,
            bron: verplaats(a.bron)
          };
        })
      };
    },
    handel: (cmd, args) => {
      bouwWachtende();
      switch (cmd) {
        case 'notulen_instellingen':
          return instellingen();
        case 'notulen_zet_aanbieder': {
          const waarde = arg(args, 'aanbieder');
          aanbieder = waarde === 'mistral' || waarde === 'ollama' ? waarde : null;
          return instellingen();
        }
        case 'notulen_zet_model': {
          const model = tekstArg(args, 'model');
          if (arg(args, 'aanbieder') === 'ollama') ollamaModel = model;
          else if (model === MISTRAL_NAUWKEURIG || model === MISTRAL_SNEL) mistralModel = model;
          else throw fout('Dit model kent Mondig niet. Kies een model uit de lijst.');
          return instellingen();
        }
        case 'notulen_zet_training':
          trainingUit = arg(args, 'uit') === true;
          return instellingen();
        case 'notulen_sleutel_zet': {
          const waarde = tekstArg(args, 'waarde').trim();
          if (waarde === '' || /\s/.test(waarde)) {
            throw fout('Dit lijkt geen geldige sleutel: hij is leeg, te lang of bevat spaties.');
          }
          sleutelStaat = true;
          // Een nieuwe sleutel haalt het vinkje over training weg, zoals in Rust.
          trainingUit = false;
          return instellingen();
        }
        case 'notulen_sleutel_wis':
          sleutelStaat = false;
          trainingUit = false;
          return instellingen();
        case 'notulen_sleutel_test':
          return new Promise((klaar) => {
            window.setTimeout(() => {
              klaar(null);
            }, 700);
          });
        case 'notulen_ollama_stand':
          return ollama();
        case 'vergadering_notulen_maken': {
          if (aanbieder === null) {
            throw fout(
              'Je hebt nog niet gekozen waar Mondig notulen maakt. Dat kies je bij Instellingen, onder Notulen.'
            );
          }
          if (aanbieder === 'mistral' && !trainingUit) {
            throw fout(
              'Zet bij Instellingen, onder Notulen, het vinkje dat training uit staat voor je sleutel. Zonder dat maakt Mondig geen notulen via Mistral.'
            );
          }
          if (aanbieder === 'mistral' && !sleutelStaat) {
            throw fout(
              'Er is nog geen sleutel voor Mistral ingevuld. Vul hem in bij Instellingen, onder Notulen.'
            );
          }
          if (maken !== null) throw fout('Mondig maakt al notulen van dit verslag.');
          const bevestigd = arg(args, 'bevestigd') as { aanbieder?: string; vervangen?: boolean };
          if (bevestigd.aanbieder !== aanbieder || bevestigd.vervangen !== vervangenNu()) {
            throw fout(
              'Waar de tekst heen gaat of het vervangen van persoonsgegevens is veranderd. Bevestig het opnieuw.'
            );
          }
          start();
          return nieuweStand();
        }
        case 'vergadering_notulen_stoppen':
          stop();
          maken = null;
          return nieuweStand();
        case 'vergadering_notulen_punt_aanpassen': {
          weigerAlsBezig();
          if (info === null) return geenNotulen();
          const soort = arg(args, 'soort') as PuntSoort;
          const id = Number(arg(args, 'id'));
          const tekst = tekstArg(args, 'tekst').split(/\s+/).filter(Boolean).join(' ');
          if (tekst === '') {
            throw fout('Een punt kan niet leeg zijn. Haal het weg als je het niet wilt houden.');
          }
          if (soort === 'besluit') {
            const punt = info.besluiten.find((p) => p.id === id);
            if (!punt) throw fout('Dit punt staat niet meer in de notulen.');
            punt.tekst = tekst;
          } else {
            const punt = info.actiepunten.find((p) => p.id === id);
            if (!punt) throw fout('Dit punt staat niet meer in de notulen.');
            const wie = tekstArg(args, 'wie').trim();
            const wanneer = tekstArg(args, 'wanneer').trim();
            // Een veld dat de gebruiker zelf verandert, verliest zijn markering.
            if (wie !== punt.wie) punt.wieNietTeruggevonden = false;
            if (wanneer !== punt.wanneer) punt.wanneerNietTeruggevonden = false;
            punt.wat = tekst;
            punt.wie = wie;
            punt.wanneer = wanneer;
          }
          return nieuweStand();
        }
        case 'vergadering_notulen_punt_weghalen': {
          weigerAlsBezig();
          if (info === null) return geenNotulen();
          const id = Number(arg(args, 'id'));
          const lijst = arg(args, 'soort') === 'besluit' ? info.besluiten : info.actiepunten;
          const index = lijst.findIndex((p) => p.id === id);
          if (index < 0) {
            throw fout('Dit punt staat niet meer in de notulen.');
          }
          lijst.splice(index, 1);
          return nieuweStand();
        }
        case 'vergadering_notulen_kopieer':
          if (info === null) throw fout('Dit verslag heeft nog geen notulen.');
          return null;
        default:
          return ONBEKEND;
      }
    }
  };
}

export { NOTULEN_BEZIG };
