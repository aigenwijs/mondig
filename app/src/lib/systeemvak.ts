import { invoke } from '@tauri-apps/api/core';
import type { Berichten } from './i18n/typen';

// Het menu van het systeemvak (tray) staat in Rust, maar Rust kent de taal van de app niet. De
// interface geeft de teksten door, bij het starten en bij een taalwissel (zoals de sprekernamen,
// `vergaderingZetLabels`). Zo staan alle vertalingen in `nl.ts` en `en.ts`.

/** De teksten van het menu; dezelfde velden als `Labels` in `systeemvak.rs`. */
export interface Systeemvaklabels {
  openen: string;
  opnameStarten: string;
  opnameStartenEerstAfmaken: string;
  opnameLoopt: string;
  stoppen: string;
  kopieer: string;
  map: string;
  instellingen: string;
  over: string;
  afsluiten: string;
  /** De tooltip tijdens een opname, met `{tijd}` voor de opnametijd. */
  tipOnline: string;
  tipRuimte: string;
  tipAlleen: string;
}

/** De menuteksten in de taal van `t`. */
export function systeemvaklabels(
  t: Pick<
    Berichten,
    | 'mondigOpenen'
    | 'vergaderStarten'
    | 'systeemvakStartenEerstAfmaken'
    | 'systeemvakOpnameLoopt'
    | 'pilStoppen'
    | 'systeemvakKopieer'
    | 'mapOpenenKort'
    | 'instellingen'
    | 'overMondig'
    | 'systeemvakAfsluiten'
    | 'systeemvakTipOnline'
    | 'systeemvakTipRuimte'
    | 'systeemvakTipAlleen'
  >
): Systeemvaklabels {
  return {
    openen: t.mondigOpenen,
    opnameStarten: t.vergaderStarten,
    opnameStartenEerstAfmaken: t.systeemvakStartenEerstAfmaken,
    opnameLoopt: t.systeemvakOpnameLoopt,
    stoppen: t.pilStoppen,
    kopieer: t.systeemvakKopieer,
    map: t.mapOpenenKort,
    instellingen: t.instellingen,
    over: t.overMondig,
    afsluiten: t.systeemvakAfsluiten,
    tipOnline: t.systeemvakTipOnline,
    tipRuimte: t.systeemvakTipRuimte,
    tipAlleen: t.systeemvakTipAlleen
  };
}

/** Geeft het systeemvak de menuteksten in de taal van de app; een wissel bouwt het menu opnieuw. */
export function systeemvakZetLabels(labels: Systeemvaklabels): Promise<void> {
  return invoke<undefined>('systeemvak_zet_labels', { labels });
}
