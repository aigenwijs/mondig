import { isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PlakUitslag } from './dictaat';
import type { PilOpschonen } from './opschonen';
import type { RijStand } from './downloadrij';
import type { DownloadVoortgang, Gebeurtenis, ProefResultaat } from './taak';

// Dunne laag om de events van de app: `taak`, `download-voortgang`, `downloads-gewijzigd`,
// `modellen-gewijzigd`, `proef-resultaat`,
// `dictaat-resultaat`, `plak-uitslag`, `dictaten-gewijzigd`, `navigeer`,
// `sneltoetsen-gewijzigd`, en voor de pil `pil-taak`, `pil-niveau` en `pil-opschonen`.
// Buiten Tauri (de gewone browser) bestaan ze niet; dan gebeurt er niets.

const GEEN_EVENTS: Promise<UnlistenFn> = Promise.resolve(() => undefined);

/** Elke wijziging van elke taak. */
export function luisterNaarTaken(bij: (gebeurtenis: Gebeurtenis) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<Gebeurtenis>('taak', (event) => {
    bij(event.payload);
  });
}

/** Byte-voortgang van een download, hooguit vier keer per seconde. */
export function luisterNaarDownloadVoortgang(
  bij: (voortgang: DownloadVoortgang) => void
): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<DownloadVoortgang>('download-voortgang', (event) => {
    bij(event.payload);
  });
}

/** De downloadrij is veranderd: de hele rij met zijn volgnummer. */
export function luisterNaarDownloads(bij: (stand: RijStand) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<RijStand>('downloads-gewijzigd', (event) => {
    bij(event.payload);
  });
}

/** De lijst met modellen, de keuzes of een download zijn veranderd. */
export function luisterNaarModellenGewijzigd(bij: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen('modellen-gewijzigd', () => {
    bij();
  });
}

/** De uitslag van een dictaat met de sneltoets. */
export function luisterNaarDictaatResultaat(
  bij: (resultaat: ProefResultaat) => void
): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<ProefResultaat>('dictaat-resultaat', (event) => {
    bij(event.payload);
  });
}

/** Hoe het plakken van een dictaat afliep. */
export function luisterNaarPlakUitslag(bij: (uitslag: PlakUitslag) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<PlakUitslag>('plak-uitslag', (event) => {
    bij(event.payload);
  });
}

/** De bewaarde dictaten of de termijn zijn veranderd; de interface haalt de lijst opnieuw op. */
export function luisterNaarDictatenGewijzigd(bij: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen('dictaten-gewijzigd', () => {
    bij();
  });
}

/** De app vraagt de interface naar een andere route (menu in de systeembalk). */
export function luisterNaarNavigatie(bij: (pad: string) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<string>('navigeer', (event) => {
    bij(event.payload);
  });
}

/** Het taak-id van een dictaat met de sneltoets: alleen dat krijgt de pil. */
export function luisterNaarPilTaak(bij: (taak: number) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<number>('pil-taak', (event) => {
    bij(event.payload);
  });
}

/** Het niveau tijdens een dictaat, 0 tot 1, ongeveer twintig keer per seconde. */
export function luisterNaarPilNiveau(bij: (niveau: number) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<number>('pil-niveau', (event) => {
    bij(event.payload);
  });
}

/** Hoe het opschonen van een dictaat loopt (ADR 0027): bezig met het label, of de afloop. */
export function luisterNaarPilOpschonen(bij: (bericht: PilOpschonen) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<PilOpschonen>('pil-opschonen', (event) => {
    bij(event.payload);
  });
}

/** Een sneltoets veranderde (de gewone of de tweede, ADR 0027); haal de stand opnieuw op. */
export function luisterNaarSneltoetsenGewijzigd(bij: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen('sneltoetsen-gewijzigd', () => {
    bij();
  });
}
