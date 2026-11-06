// Gedeelde klassen voor de visuele taal van ADR 0014: groeperen met vlakken, niet met randen.

/** Een vlak dat bij elkaar horende inhoud groepeert. */
export const vlak = 'rounded-groot bg-vlak';

/** Een foutmelding: een eigen getint vlak, met één vervolgactie rechts. */
export const foutVak =
  'flex flex-wrap items-center justify-between gap-3 rounded-groot bg-fout-vlak px-4 py-3 text-fout-tekst';

/** Sectiekop: 13 px semibold in de tekstkleur, in zinsvorm (ADR 0014, aanvulling 2-11-2026). */
export const sectieKop = 'm-0 mb-2 font-tekst text-basis font-semibold text-tekst';

/** Paginatitel in Sora 22 px. */
export const paginaTitel = 'm-0 text-kop tracking-[-0.015em]';

/** Een rij in een instellingenvlak: de uitleg links, de bediening rechts. */
export const rij = 'flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3.5';
export const rijLabel = 'flex min-w-0 flex-[1_1_14rem] flex-col gap-0.5';
export const rijNaam = 'm-0 font-tekst text-basis leading-snug font-medium tracking-normal';
export const rijUitleg = 'm-0 text-klein text-gedempt';
// Op het kleinste venster schuift de bediening onder het label; met een keuzelijst erin neemt
// hij de volle breedte (ontwerp N-C1, 480×400).
export const rijBediening =
  'ml-auto flex max-w-full flex-wrap items-center gap-2 has-[select]:max-[639px]:w-full';

/** Een keuzelijst (veld van 36 px). Het pijltje komt van Keuze.svelte. */
export const keuzeVeld =
  'h-9 w-full min-w-0 cursor-pointer appearance-none truncate rounded-klein border border-rand-veld bg-verhoogd pr-9 pl-3 text-basis text-tekst transition-colors not-disabled:hover:bg-hover disabled:cursor-default disabled:opacity-50';

const chipBasis =
  'inline-flex h-[22px] items-center gap-[5px] rounded-klein px-2 font-tekst text-sectie font-medium whitespace-nowrap';
/** Een label of badge van 22 px, neutraal (Lokaal, Sneller). */
export const chip = `${chipBasis} bg-hover text-gedempt`;
/** Een label dat aanbeveelt: een ring in de actiekleur, zonder vlak. */
export const chipAanbevolen = `${chipBasis} text-actie ring-1 ring-actie ring-inset`;
/** De badge "Nieuw": een gevuld vlak in de actiekleur. */
export const chipNieuw = `${chipBasis} bg-actie font-semibold text-op-actie`;

/** Een melding op de pagina: een eigen vlak, de soort herken je aan het icoon of de tekstkleur. */
export const meldingVak =
  'flex items-start gap-3 rounded-groot bg-vlak px-3.5 py-3 text-basis leading-snug';
