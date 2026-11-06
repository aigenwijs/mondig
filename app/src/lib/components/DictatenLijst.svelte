<script lang="ts">
  import { tick } from 'svelte';
  import { DropdownMenu } from 'bits-ui';
  import Check from '@lucide/svelte/icons/check';
  import Copy from '@lucide/svelte/icons/copy';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import { klokTijd } from '#lib/dictaat.js';
  import { datumKop, groepeerPerDag, type BewaardDictaat, type DagGroep } from '#lib/dictaten.js';
  import { kopieerNaarKlembord } from '#lib/klembord.js';
  import { modelNaam } from '#lib/opschonen.js';
  import { foutVak, vlak } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // "Je dictaten" (ADR 0021, ontwerp N-0 sectie 1): de bewaarde dictaten per dag, een regel per
  // dictaat met de tijd en de tekst afgekapt; een klik klapt de hele tekst open. Kopiëren en het
  // prullenbakje staan bij aanwijzen, bij toetsenbordfocus en bij een opengeklapte regel. De
  // lijst tekent alleen wat de ouder geeft; Rust is de bron (zie dictatenstand.svelte.ts).
  // Een opgeschoond dictaat (ADR 0027) toont opengeklapt een grijze regel met wie het opschoonde
  // en de knop "Letterlijke tekst kopiëren".

  interface Props {
    /** Nieuwste eerst. */
    dictaten: BewaardDictaat[];
    /** De termijn, voor "24 uur bewaard" en de zin van de lege staat. Niet bewaren komt hier niet. */
    termijn: '24u' | '7d';
    /** Het aantal bestanden op schijf: zoveel gaat weg bij Alles wissen. */
    bewaard: number;
    /** Het moment van nu, voor Vandaag en Gisteren; standaard de klok. */
    nu?: number;
    bijVerwijder: (id: string) => Promise<void>;
    bijWisAlles: () => Promise<void>;
    /** Naar Instellingen, de rij Dictaten bewaren. */
    bijHoeLang: () => void;
    /** Welk dictaat bij het tekenen al open staat (voor de tests). */
    beginOpen?: string | null;
  }
  let {
    dictaten,
    termijn,
    bewaard,
    nu = Date.now(),
    bijVerwijder,
    bijWisAlles,
    bijHoeLang,
    beginOpen = null
  }: Props = $props();

  const taal = $derived(huidigeTaal());
  const groepen = $derived<DagGroep[]>(groepeerPerDag(dictaten, nu));
  const termijnTekst = $derived(termijn === '24u' ? t().dictatenTermijn24u : t().dictatenTermijn7d);

  // svelte-ignore state_referenced_locally
  let open = $state<string | null>(beginOpen);
  // Het dictaat waarvan de letterlijke tekst net gekopieerd is.
  let letterlijkGekopieerd = $state<string | null>(null);
  let gekopieerd = $state<string | null>(null);
  let vraagWissen = $state(false);
  let fout = $state<string | null>(null);
  // Voor de schermlezer: wat er net gebeurde, zonder de focus te verplaatsen (WCAG 4.1.3).
  let melding = $state('');
  // Elk vinkje (tekst of letterlijke tekst) heeft een eigen timer.
  const timers: Record<'tekst' | 'letterlijk', ReturnType<typeof setTimeout> | undefined> = {
    tekst: undefined,
    letterlijk: undefined
  };

  const menuInhoud =
    'z-50 min-w-[210px] rounded-groot border border-lijn bg-verhoogd p-1.5 shadow-[0_8px_24px_rgba(0,0,0,0.35)]';
  const menuItem =
    'flex h-8 cursor-pointer items-center rounded-klein px-2.5 text-basis text-tekst outline-none select-none data-highlighted:bg-hover';
  const gevaarKnop = 'bg-fout-tekst text-achtergrond not-aria-disabled:hover:bg-fout-tekst/90';

  function wissel(id: string): void {
    open = open === id ? null : id;
  }

  /** Kopieert een tekst en zet twee seconden het vinkje van deze soort bij dit dictaat. */
  async function kopieerMetVinkje(
    soort: 'tekst' | 'letterlijk',
    id: string,
    tekst: string
  ): Promise<void> {
    fout = null;
    if (!(await kopieerNaarKlembord(tekst))) {
      fout = t().kopierenMislukt;
      return;
    }
    const zet = (waarde: string | null): void => {
      if (soort === 'tekst') gekopieerd = waarde;
      else letterlijkGekopieerd = waarde;
    };
    zet(id);
    melding = soort === 'tekst' ? t().dictatenGekopieerd : t().dictatenLetterlijkGekopieerd;
    clearTimeout(timers[soort]);
    timers[soort] = setTimeout(() => {
      zet(null);
    }, 2000);
  }

  function kopieer(dictaat: BewaardDictaat): Promise<void> {
    return kopieerMetVinkje('tekst', dictaat.id, dictaat.tekst);
  }

  async function kopieerLetterlijk(dictaat: BewaardDictaat): Promise<void> {
    if (dictaat.letterlijk === undefined) return;
    await kopieerMetVinkje('letterlijk', dictaat.id, dictaat.letterlijk);
  }

  /** De rij die na het verwijderen de focus krijgt: de volgende, anders de vorige. */
  function buurVan(id: string): string | null {
    const i = dictaten.findIndex((d) => d.id === id);
    return dictaten[i + 1]?.id ?? dictaten[i - 1]?.id ?? null;
  }

  function rijKnop(id: string): HTMLElement | null {
    return document.getElementById(`dictaat-${id}`);
  }

  async function verwijder(dictaat: BewaardDictaat): Promise<void> {
    fout = null;
    const buur = buurVan(dictaat.id);
    try {
      await bijVerwijder(dictaat.id);
      melding = t().dictatenVerwijderd;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
    if (open === dictaat.id) open = null;
    await tick();
    (buur === null ? document.getElementById('dictaten-menu') : rijKnop(buur))?.focus();
  }

  async function vraag(): Promise<void> {
    vraagWissen = true;
    await tick();
    document.getElementById('dictaten-wis-annuleer')?.focus();
  }

  async function annuleerWissen(): Promise<void> {
    vraagWissen = false;
    await tick();
    document.getElementById('dictaten-menu')?.focus();
  }

  async function bevestigWissen(): Promise<void> {
    fout = null;
    vraagWissen = false;
    try {
      await bijWisAlles();
      melding = t().dictatenAlleGewist;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
    open = null;
    await tick();
    document.getElementById('dictaten-menu')?.focus();
  }
</script>

<section class="flex flex-col gap-2.5" aria-labelledby="dictaten-kop">
  <div class="flex items-center gap-2">
    <h2 id="dictaten-kop" class="m-0 font-tekst text-basis font-semibold text-tekst">
      {t().dictatenKop}
    </h2>
    <span class="ml-auto text-klein text-dim max-[639px]:hidden"
      >{t().dictatenBewaard(termijnTekst)}</span
    >
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button
            {...props}
            id="dictaten-menu"
            variant="spook"
            size="icoon"
            class="max-[639px]:ml-auto"
            aria-label={t().dictatenMenu}
            title={t().dictatenMenu}
          >
            <Ellipsis size={16} aria-hidden="true" />
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          sideOffset={6}
          class={menuInhoud}
          onCloseAutoFocus={(e) => {
            // De bevestigvraag pakt de focus zelf.
            if (vraagWissen) e.preventDefault();
          }}
        >
          <DropdownMenu.Item class={menuItem} onSelect={bijHoeLang}
            >{t().dictatenHoeLang}</DropdownMenu.Item
          >
          {#if bewaard > 0}
            <DropdownMenu.Separator class="my-1 h-px bg-lijn" />
            <DropdownMenu.Item
              class="{menuItem} text-fout-tekst"
              onSelect={() => {
                void vraag();
              }}>{t().dictatenAllesWissen}</DropdownMenu.Item
            >
          {/if}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  </div>

  {#if vraagWissen}
    <div
      class="{vlak} flex flex-col gap-2.5 px-4 py-3.5"
      role="alertdialog"
      aria-labelledby="dictaten-wis-kop"
      aria-describedby="dictaten-wis-uitleg"
    >
      <b id="dictaten-wis-kop" class="font-semibold">{t().dictatenAllesWissenKop}</b>
      <span id="dictaten-wis-uitleg" class="text-gedempt">{t().dictatenWisVraag(bewaard)}</span>
      <div class="flex flex-wrap gap-2">
        <Button class={gevaarKnop} onclick={bevestigWissen}>{t().dictatenWisBevestig}</Button>
        <Button id="dictaten-wis-annuleer" variant="spook" onclick={annuleerWissen}
          >{t().annuleren}</Button
        >
      </div>
    </div>
  {/if}

  {#if dictaten.length === 0}
    <!-- De lege staat: de streepjes van de meter lopen uit in de punt van het woordmerk. -->
    <div class="{vlak} flex flex-col items-center gap-2 px-5 py-6 text-center">
      <span class="flex h-[34px] items-center gap-[3px]" aria-hidden="true">
        {#each [10, 18, 28, 16, 24, 12] as hoogte (hoogte)}
          <i class="block w-[5px] rounded-[2px] bg-meter-leeg" style="height: {hoogte}px"></i>
        {/each}
        <i class="ml-1 block size-2 rounded-full bg-actie"></i>
      </span>
      <b class="font-semibold">{t().dictatenLeegKop}</b>
      <span class="max-w-[420px] text-klein text-gedempt"
        >{t().dictatenLeegTekst(termijnTekst)}</span
      >
    </div>
  {:else}
    <div class="{vlak} overflow-hidden pb-1">
      {#each groepen as groep (groep.sleutel)}
        <h3 class="m-0 px-4 pt-3 pb-1 font-tekst text-sectie font-medium text-dim">
          {groep.soort === 'vandaag'
            ? t().dictatenDagVandaag
            : groep.soort === 'gisteren'
              ? t().dictatenDagGisteren
              : datumKop(groep.tijdUnixMs, taal)}
        </h3>
        <ul class="m-0 flex list-none flex-col p-0">
          {#each groep.dictaten as dictaat (dictaat.id)}
            {@const tijd = klokTijd(dictaat.tijdUnixMs, taal)}
            {@const uit = open === dictaat.id}
            <li class="group flex min-h-12 items-start gap-1 pr-2 pl-1 hover:bg-hover">
              <div class="flex min-w-0 flex-1 flex-col">
                <button
                  type="button"
                  id="dictaat-{dictaat.id}"
                  class="flex min-h-12 min-w-0 flex-1 cursor-pointer items-start gap-3 rounded-klein bg-transparent py-3 pl-3 text-left text-basis text-tekst"
                  aria-expanded={uit}
                  onclick={() => {
                    wissel(dictaat.id);
                  }}
                >
                  <span class="w-11 shrink-0 text-klein text-dim tabular-nums">{tijd}</span>
                  <span
                    class="min-w-0 flex-1 {uit ? 'break-words whitespace-pre-wrap' : 'truncate'}"
                    lang="nl">{dictaat.tekst}</span
                  >
                </button>
                {#if uit && dictaat.letterlijk !== undefined && dictaat.opgeschoondDoor}
                  <!-- Wie het opschoonde, en de letterlijke tekst terug (ADR 0027 punt 4). -->
                  <span
                    class="-mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 pb-2.5 pl-[4.25rem] text-klein text-dim"
                  >
                    <span
                      >{dictaat.opgeschoondDoor.label
                        ? t().dictatenOpgeschoondDoor(
                            modelNaam(dictaat.opgeschoondDoor.model),
                            t().opschonenLabel(dictaat.opgeschoondDoor.label)
                          )
                        : t().dictatenOpgeschoondDoorZonderLabel(
                            modelNaam(dictaat.opgeschoondDoor.model)
                          )}</span
                    >
                    <Button
                      variant="spook"
                      class="h-7 px-2 text-klein font-medium text-gedempt"
                      aria-label={t().dictatenLetterlijkKopierenAria(tijd)}
                      onclick={() => kopieerLetterlijk(dictaat)}
                    >
                      {#if letterlijkGekopieerd === dictaat.id}
                        <Check size={14} class="text-gelukt" aria-hidden="true" />
                      {/if}
                      {t().dictatenLetterlijkKopieren}
                    </Button>
                  </span>
                {/if}
              </div>
              <span
                class="flex shrink-0 items-center gap-0.5 self-start pt-2 {uit
                  ? ''
                  : 'opacity-0 group-focus-within:opacity-100 group-hover:opacity-100'}"
              >
                <Button
                  variant="spook"
                  size="icoon"
                  aria-label={t().dictatenKopieerAria(tijd)}
                  title={t().kopieren}
                  onclick={() => kopieer(dictaat)}
                >
                  {#if gekopieerd === dictaat.id}
                    <Check size={16} class="text-gelukt" aria-hidden="true" />
                  {:else}
                    <Copy size={16} aria-hidden="true" />
                  {/if}
                </Button>
                <Button
                  variant="spook"
                  size="icoon"
                  aria-label={t().dictatenVerwijderAria(tijd)}
                  title={t().verwijderen}
                  onclick={() => verwijder(dictaat)}
                >
                  <Trash2 size={16} aria-hidden="true" />
                </Button>
              </span>
            </li>
          {/each}
        </ul>
      {/each}
    </div>
  {/if}

  {#if fout}
    <div class={foutVak} role="alert">{fout}</div>
  {/if}
  <div role="status" class="sr-only">{melding}</div>
</section>
