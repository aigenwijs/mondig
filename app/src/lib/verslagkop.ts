import type { Berichten } from './i18n/typen';
import type { ModellenOverzicht } from './modellen';
import { duurMinuten, type BeurtInfo, type StandToestand } from './vergadering';

// De kop van het verslag na een opname (N-C1): de gegevensregel en het klaar-moment. Alleen
// pure hulpjes, zodat ze zonder scherm te testen zijn.

/** Hoe lang het klaar-moment staat voordat het vanzelf verdwijnt. */
export const KLAAR_MOMENT_MS = 6000;

/** Wat een model in het verslag is: sneller of nauwkeuriger dan de rest. */
export type ModelRol = 'snel' | 'nauwkeurig';

/**
 * De rol van het model waarmee een verslag is uitgeschreven, zoals Mondig elders over de modellen
 * praat: het model voor dicteren is "het snelle", het model voor opnames en bestanden (als dat een
 * ander is) "het nauwkeurige". We kijken dus welke taak het model heeft in het overzicht dat de
 * app al kent. Is het model geen van beide meer (de keuze veranderde sindsdien), dan beslist welke
 * balk hoger staat. Is dat gelijk, onbekend, of gebruikt Mondig voor alles hetzelfde model (dan is
 * er geen verschil te noemen), dan `null`: het scherm toont dan de naam.
 */
export function modelRol(
  model: string,
  overzicht: Pick<ModellenOverzicht, 'modellen' | 'dicteren' | 'bestanden'> | null | undefined
): ModelRol | null {
  const naam = model.trim().toLowerCase();
  if (naam === '' || !overzicht) return null;
  const regel =
    overzicht.modellen.find((m) => m.naam.toLowerCase() === naam) ??
    overzicht.modellen.find((m) => {
      const volledig = m.naam.toLowerCase();
      return volledig.startsWith(`${naam} `) || naam.startsWith(`${volledig} `);
    });
  if (!regel) return null;
  if (overzicht.dicteren === overzicht.bestanden) return null;
  if (regel.id === overzicht.bestanden) return 'nauwkeurig';
  if (regel.id === overzicht.dicteren) return 'snel';
  if (regel.snelheid > regel.nauwkeurigheid) return 'snel';
  if (regel.nauwkeurigheid > regel.snelheid) return 'nauwkeurig';
  return null;
}

/**
 * Hoeveel sprekers er in het verslag staan, maar alleen als de sprekers herkend zijn; anders
 * `null`, want "Jij en Anderen" is geen telling van mensen. Het telt de verschillende namen die het
 * verslag toont (zonder verschil in hoofdletters). Een kaal "Anderen" (zonder nummer en zonder
 * eigen naam) op een kant met herkende sprekers telt niet mee; een kant zonder nummers (online
 * Jij) wel. Dezelfde telling als `aantal_sprekers` in Rust, op de namen zoals Rust ze geeft.
 */
export function aantalSprekers(
  beurten: readonly Pick<BeurtInfo, 'kant' | 'sprekernummer' | 'spreker' | 'eigenNaam'>[]
): number | null {
  if (!beurten.some((b) => b.sprekernummer !== null)) return null;
  const metNummers = new Set(beurten.filter((b) => b.sprekernummer !== null).map((b) => b.kant));
  const naamVan = (b: (typeof beurten)[number]): string =>
    b.spreker ??
    (b.sprekernummer !== null
      ? `Spreker ${String(b.sprekernummer)}`
      : b.kant === 'jij'
        ? 'Jij'
        : 'Anderen');
  return new Set(
    beurten
      .filter((b) => b.sprekernummer !== null || b.eigenNaam || !metNummers.has(b.kant))
      .map((b) => naamVan(b).toLowerCase())
  ).size;
}

/** Wat het klaar-moment laat zien. */
export interface KlaarMoment {
  /** De duur van het gesprek. */
  gesprekMs: number | null;
  /** Hoe lang het uitschrijven duurde, voor zover het scherm dat zag; anders `null`. */
  uitschrijfMs: number | null;
}

/**
 * Wanneer het scherm het uitschrijven zag beginnen, bijgehouden bij elke nieuwe stand. De app
 * kent geen betrouwbare starttijd van het uitschrijven, dus tellen we alleen als dit scherm de
 * overgang naar `uitschrijven` zelf meemaakte. Opent het scherm midden in het uitschrijven (na
 * wisselen van scherm), dan blijft het `null` en toont het klaar-moment geen tijd: nooit een
 * verkeerde. Buiten het uitschrijven is het altijd `null`.
 */
export function volgUitschrijfStart(
  vorig: StandToestand['type'] | undefined,
  nu: StandToestand['type'],
  huidig: number | null,
  nuMs: number
): number | null {
  if (nu !== 'uitschrijven') return null;
  if (vorig === 'uitschrijven') return huidig;
  return vorig === undefined ? null : nuMs;
}

/**
 * Hoort bij deze wissel van stand een klaar-moment? Alleen als het scherm het uitschrijven zag
 * eindigen (van `uitschrijven` naar `verslag`). Een verslag dat er al stond, of dat een ander
 * scherm afrondde, krijgt geen strook. `gestartMs` is wanneer het scherm het uitschrijven voor het
 * eerst zag (`performance.now()`), `nuMs` de tijd van nu op dezelfde klok.
 */
export function bepaalKlaarMoment(
  vorig: StandToestand['type'] | undefined,
  nu: StandToestand,
  gestartMs: number | null,
  nuMs: number
): KlaarMoment | null {
  if (vorig !== 'uitschrijven' || nu.type !== 'verslag') return null;
  return {
    gesprekMs: nu.duurMs,
    uitschrijfMs: gestartMs === null ? null : Math.max(0, nuMs - gestartMs)
  };
}

type KlaarTeksten = Pick<
  Berichten,
  'klaarGesprek' | 'klaarUitgeschreven' | 'klaarUitgeschrevenKort' | 'klaarLokaal'
>;

/** "52 minuten gesprek, in 4 minuten uitgeschreven. Alles bleef op deze computer." */
export function klaarZin(klaar: KlaarMoment, t: KlaarTeksten): string {
  const delen: string[] = [];
  if (klaar.gesprekMs !== null) delen.push(t.klaarGesprek(duurMinuten(klaar.gesprekMs)));
  if (klaar.uitschrijfMs !== null) {
    const minuten = Math.round(klaar.uitschrijfMs / 60_000);
    delen.push(minuten < 1 ? t.klaarUitgeschrevenKort : t.klaarUitgeschreven(minuten));
  }
  const eerste = delen.join(', ');
  return eerste === '' ? t.klaarLokaal : `${eerste}. ${t.klaarLokaal}`;
}

/**
 * Laat `bij` na `ms` milliseconden lopen en geeft een functie die dat afbreekt. Voor het
 * klaar-moment: het verdwijnt vanzelf, tenzij het scherm eerder weggaat.
 */
export function startKlaarTimer(bij: () => void, ms: number = KLAAR_MOMENT_MS): () => void {
  const id = setTimeout(bij, ms);
  return () => {
    clearTimeout(id);
  };
}
