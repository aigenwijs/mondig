/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een foutmelding, geen Error */
// Dictaten opschonen (ADR 0027) in de dev-mock. Volgt Rust (app/src-tauri/src/opschonen.rs):
// aanzetten kan alleen met een aanbieder onder Notulen die klaar is, en de eerste keer per
// aanbieder alleen met een bevestiging. De aanbieder komt uit de mock van Notulen, zodat
// instellen onder Notulen hier meteen telt. Opties via `?mock=` (komma's ertussen):
//   opschonen:        Met een eigen sneltoets, al bevestigd voor de aanbieder van nu
//   opschonenelk:     Bij elk dictaat, al bevestigd
//   opschoondubbel:   de tweede sneltoets staat uit omdat hij gelijk is aan de gewone
//   opschoonterugval: het opschonen valt terug (te laat): de letterlijke tekst wordt geplakt
// Zonder deze opties staat opschonen uit en is er nog niets bevestigd. Met een aanbieder onder
// Notulen (`notulen` of `notulenollama`) is de keuzelijst te bedienen.

import type { InvokeArgs } from '@tauri-apps/api/core';
import type { NotulenAanbieder, NotulenInstellingen } from '../notulen';
import type { Label, OpschonenInfo, OpschoonStand } from '../opschonen';
import { ONBEKEND } from './mock-bestanden';

const arg = (args: InvokeArgs | undefined, sleutel: string): unknown =>
  args && !Array.isArray(args) && !(args instanceof ArrayBuffer) && !(args instanceof Uint8Array)
    ? args[sleutel]
    : undefined;

const fout = (melding: string, vervolg = 'geen'): never => {
  throw { melding, vervolg };
};

/** Mistral Small, zoals `opschonen::MISTRAL_MODEL` in de kern. */
const MISTRAL_OPSCHONEN = 'mistral-small-latest';

/** Wat Rust bij een aanbieder meestuurt (`Voorinstelling::naam` en `soevereiniteit`). */
const NAAM: Record<NotulenAanbieder, string> = { mistral: 'Mistral', ollama: 'Ollama' };
const LABEL: Record<NotulenAanbieder, Label> = { mistral: 'europees', ollama: 'lokaal' };

export interface OpschonenMock {
  handel: (cmd: string, args: InvokeArgs | undefined) => unknown;
  /** De stand die nu geldt. */
  stand: () => OpschoonStand;
  /**
   * Moet een dictaat opgeschoond worden? `metTweede`: begonnen met de tweede sneltoets. Geeft
   * het label en het model, of `null`.
   */
  plan: (metTweede: boolean) => { label: Label; model: string; aanbieder: string } | null;
  /** Het kruisje in de pil drukte op stoppen voor deze taak. */
  gestopt: (taak: number) => boolean;
}

export function maakOpschonenMock(
  opties: Set<string>,
  notulen: () => NotulenInstellingen | null
): OpschonenMock {
  let gekozen: OpschoonStand = opties.has('opschonenelk')
    ? 'bijElkDictaat'
    : opties.has('opschonen') || opties.has('opschoondubbel')
      ? 'eigenSneltoets'
      : 'uit';
  const bevestigd = new Set<NotulenAanbieder>();
  if (gekozen !== 'uit') {
    bevestigd.add('mistral');
    bevestigd.add('ollama');
  }
  let gestopteTaak: number | null = null;

  /** Zoals `bepaal_start`: de aanbieder als die klaar is voor gebruik. */
  const klaar = (): { aanbieder: NotulenAanbieder; model: string } | null => {
    const i = notulen();
    if (!i?.aanbieder) return null;
    if (i.aanbieder === 'mistral') {
      return i.trainingUit && i.sleutelStaat
        ? { aanbieder: 'mistral', model: MISTRAL_OPSCHONEN }
        : null;
    }
    return i.ollamaModel ? { aanbieder: 'ollama', model: i.ollamaModel } : null;
  };

  const info = (): OpschonenInfo => {
    const k = klaar();
    const isBevestigd = k !== null && bevestigd.has(k.aanbieder);
    return {
      stand: k !== null && isBevestigd ? gekozen : 'uit',
      aanbieder: k?.aanbieder ?? null,
      naam: k ? NAAM[k.aanbieder] : null,
      label: k ? LABEL[k.aanbieder] : null,
      model: k?.model ?? null,
      bevestigd: isBevestigd
    };
  };

  return {
    stand: () => info().stand,
    plan(metTweede) {
      const nu = info();
      const k = klaar();
      if (k === null) return null;
      const doet = nu.stand === 'bijElkDictaat' || (nu.stand === 'eigenSneltoets' && metTweede);
      return doet ? { label: LABEL[k.aanbieder], model: k.model, aanbieder: k.aanbieder } : null;
    },
    gestopt: (taak) => gestopteTaak === taak,
    handel(cmd, args) {
      switch (cmd) {
        case 'opschonen_instellingen':
          return info();
        case 'opschonen_zet_stand': {
          const stand = arg(args, 'stand');
          if (stand !== 'uit' && stand !== 'eigenSneltoets' && stand !== 'bijElkDictaat') {
            return fout('Deze keuze bestaat niet.');
          }
          if (stand !== 'uit') {
            const k = klaar();
            if (k === null) return fout('Kies eerst een taalmodel onder Notulen.');
            if (!bevestigd.has(k.aanbieder)) {
              if (arg(args, 'bevestigd') !== k.aanbieder) {
                return fout('Bevestig eerst waar de tekst heen gaat.', 'opnieuw_proberen');
              }
              bevestigd.add(k.aanbieder);
            }
          }
          gekozen = stand;
          return info();
        }
        case 'dictaat_opschonen_stop': {
          const taak = arg(args, 'taak');
          if (typeof taak === 'number') gestopteTaak = taak;
          return null;
        }
        default:
          return ONBEKEND;
      }
    }
  };
}
