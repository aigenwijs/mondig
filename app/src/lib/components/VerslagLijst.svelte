<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Pause from '@lucide/svelte/icons/pause';
  import Play from '@lucide/svelte/icons/play';
  import Search from '@lucide/svelte/icons/search';
  import X from '@lucide/svelte/icons/x';
  import PersoonRegel from '#lib/components/PersoonRegel.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { Afspeler } from '#lib/afspelen.svelte.js';
  import { tijdcode } from '#lib/bestanden.js';
  import { foutUitCommando, type Foutmelding } from '#lib/taak.js';
  import {
    SNELHEDEN,
    bewerkToets,
    deelTekst,
    leesSnelheid,
    naarTreffer,
    neemtSpatieOver,
    vindTreffers,
    volgendeSleutel,
    vervangenRegel,
    type PersoonStand,
    type Treffer,
    type VerslagRegel
  } from '#lib/verslag.js';

  // De regels van een verslag of transcript, voor de vergadering (Opnemen) en voor Bestanden
  // (stroom V, V-1): een regel verbeteren, het stuk geluid erbij beluisteren en in de tekst
  // zoeken. Patroon van Vibe (MIT): regel bewerken (transcript-view.tsx), zoeken
  // (transcript-toolbar.tsx) en afspelen met een snelheid van 1, 1,5 of 2 keer.
  //
  // De tekst zelf staat aan de Rust-kant; dit onderdeel vraagt om een wijziging (`zetTekst`) en
  // toont daarna wat het scherm teruggeeft. Het geluid komt als WAV-bytes uit Rust (`haalAudio`),
  // zodat de webview geen bestandsrecht nodig heeft.

  let {
    regels,
    labels = false,
    label,
    lijstLabel,
    zetTekst,
    haalAudio,
    geenGeluid,
    persoon,
    zetVervangen,
    haalWeg,
    markeer,
    opgelicht = null
  }: {
    regels: VerslagRegel[];
    /** Heeft de lijst een kolom met de spreker? */
    labels?: boolean;
    /** De spreker van een regel, in de kolom ernaast. */
    label?: Snippet<[VerslagRegel]>;
    lijstLabel: string;
    /** Bewaart de nieuwe tekst van een regel; geeft de melding bij een fout, anders `null`. */
    zetTekst: (sleutel: number, tekst: string) => Promise<string | null>;
    /** Het geluid tussen twee tijden; `null` als de audio er niet meer is. */
    haalAudio: ((beginMs: number, eindMs: number) => Promise<ArrayBuffer>) | null;
    /** Waarom er niet te luisteren valt, voor als `haalAudio` `null` is. */
    geenGeluid: string;
    /** De stand van "Persoonsgegevens markeren en vervangen". */
    persoon: PersoonStand;
    /** Zet het vervangen aan of uit; geeft de melding bij een fout, anders `null`. */
    zetVervangen: (aan: boolean) => Promise<string | null>;
    /** Haalt de markering van een stuk tekst weg; geeft de melding bij een fout, anders `null`. */
    haalWeg: (tekst: string) => Promise<string | null>;
    /** Markeert zelf een stuk tekst; geeft de melding bij een fout, anders `null`. */
    markeer: (tekst: string) => Promise<string | null>;
    /** De regel die even oplicht, na een sprong vanaf een bronknop van de notulen. */
    opgelicht?: number | null;
  } = $props();

  // ---- Afspelen ----

  const afspeler = new Afspeler(
    (beginMs, eindMs) =>
      haalAudio
        ? haalAudio(beginMs, eindMs)
        : // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- een Foutmelding, zoals Rust die geeft
          Promise.reject({ melding: geenGeluid, vervolg: 'geen' } satisfies Foutmelding),
    // Een melding van Rust blijft staan; een fout van de browser (zoals "no supported source")
    // wordt een melding in gewone taal.
    (e) => foutUitCommando(e, t().verslagAfspelenMislukt).melding
  );

  function speel(regel: VerslagRegel): void {
    if (!haalAudio) return;
    void afspeler.speel(regel.sleutel, regel.startMs, regel.eindMs);
  }

  // ---- Bewerken ----

  /** De regel waarvan het tekstveld openstaat. */
  let bewerken = $state<number | null>(null);
  let concept = $state('');
  // Gezet zodra het bewerken klaar is, zodat de blur die erop volgt niet nog eens bewaart.
  let afgehandeld = true;
  let fout = $state<string | null>(null);
  let status = $state('');

  function start(regel: VerslagRegel): void {
    fout = null;
    status = '';
    concept = regel.tekst;
    afgehandeld = false;
    bewerken = regel.sleutel;
  }

  /** Zet de cursor achteraan, laat het veld met de tekst meegroeien en geeft het de focus. */
  function veld(node: HTMLTextAreaElement): { destroy: () => void } {
    const pas = (): void => {
      node.style.height = 'auto';
      node.style.height = `${String(node.scrollHeight)}px`;
    };
    node.addEventListener('input', pas);
    // Pas na het vullen van het veld: dan pas klopt de hoogte van de tekst.
    queueMicrotask(() => {
      pas();
      node.focus();
      node.setSelectionRange(node.value.length, node.value.length);
    });
    return {
      destroy: () => {
        node.removeEventListener('input', pas);
      }
    };
  }

  /**
   * Rondt het bewerken af. Bij `naarVolgende` (Enter) gaat het tekstveld meteen naar de volgende
   * regel; de wijziging wordt ondertussen bewaard. Een lege of ongewijzigde tekst laat de oude
   * staan, want een leeg veld is verloren tekst en geen bewerking.
   */
  async function rondAfWerk(
    sleutel: number,
    naarVolgende: boolean,
    focusTerug: boolean
  ): Promise<void> {
    if (afgehandeld || bewerken !== sleutel) return;
    afgehandeld = true;
    const nieuw = concept;
    const huidig = regels.find((r) => r.sleutel === sleutel);
    const volgende = naarVolgende ? volgendeSleutel(regels, sleutel) : null;
    const volgendeRegel = regels.find((r) => r.sleutel === volgende);
    if (volgendeRegel) {
      start(volgendeRegel);
    } else {
      bewerken = null;
      if (focusTerug) {
        await tick();
        document.getElementById(regelId(sleutel))?.focus();
      }
    }
    if (!huidig || nieuw.trim() === '' || nieuw === huidig.tekst) return;
    const melding = await zetTekst(sleutel, nieuw);
    if (melding === null) {
      status = t().verslagRegelBewaard;
    } else {
      fout = melding;
    }
  }

  /**
   * Rondt het bewerken af en onthoudt dat het bewaren loopt. Kopiëren en Opslaan wachten erop
   * (`wachtOpBewaren`), anders pakken ze nog de oude tekst als je net buiten een regel klikte.
   */
  function rondAf(sleutel: number, naarVolgende: boolean, focusTerug: boolean): Promise<void> {
    const werk = rondAfWerk(sleutel, naarVolgende, focusTerug);
    lopend = werk;
    return werk;
  }

  // Het bewaren van de laatste verbetering, dat nog kan lopen.
  let lopend: Promise<void> = Promise.resolve();

  /** Geeft een belofte die klaar is als een lopende verbetering is bewaard. */
  export function wachtOpBewaren(): Promise<void> {
    return lopend;
  }

  function annuleer(sleutel: number): void {
    if (afgehandeld || bewerken !== sleutel) return;
    afgehandeld = true;
    bewerken = null;
    void tick().then(() => document.getElementById(regelId(sleutel))?.focus());
  }

  function veldToets(event: KeyboardEvent, sleutel: number): void {
    const wat = bewerkToets(event.key, event.shiftKey, event.isComposing);
    if (wat === 'bewaar-volgende') {
      event.preventDefault();
      void rondAf(sleutel, true, true);
    } else if (wat === 'annuleer') {
      event.preventDefault();
      annuleer(sleutel);
    }
  }

  const regelId = (sleutel: number): string => `verslag-regel-${String(sleutel)}`;

  // In de stand Vervangen toont het verslag zelf al wat de ander krijgt: [BSN] in plaats van het
  // nummer. Zoeken en tonen werken op die tekst; verbeteren blijft op de echte tekst.
  const getoond = $derived(persoon.vervangen ? regels.map(vervangenRegel) : regels);

  // ---- Zoeken ----

  let zoekOpen = $state(false);
  let vraag = $state('');
  let huidig = $state(0);
  let zoekVeld = $state<HTMLInputElement | null>(null);
  let zoekKnop = $state<HTMLElement | null>(null);

  const treffers = $derived(
    vindTreffers(
      getoond.map((r) => r.tekst),
      vraag
    )
  );
  const actief = $derived(Math.min(huidig, Math.max(0, treffers.length - 1)));
  // Per regel (plek in de lijst) de treffers, en welke daarvan de actieve is (of -1).
  const perRegel = $derived.by(() => {
    const lijst: ({ treffers: Treffer[]; actief: number } | undefined)[] = [];
    treffers.forEach((treffer, i) => {
      const bij = lijst[treffer.regel] ?? { treffers: [], actief: -1 };
      if (i === actief) bij.actief = bij.treffers.length;
      bij.treffers.push(treffer);
      lijst[treffer.regel] = bij;
    });
    return lijst;
  });
  const telling = $derived(
    vraag === ''
      ? ''
      : treffers.length === 0
        ? t().verslagZoekGeen
        : t().verslagZoekTeller(actief + 1, treffers.length)
  );

  async function openZoeken(): Promise<void> {
    zoekOpen = true;
    await tick();
    zoekVeld?.focus();
    zoekVeld?.select();
  }

  async function sluitZoeken(): Promise<void> {
    zoekOpen = false;
    vraag = '';
    huidig = 0;
    await tick();
    zoekKnop?.focus();
  }

  async function ga(richting: 1 | -1): Promise<void> {
    if (treffers.length === 0) return;
    huidig = naarTreffer(actief, treffers.length, richting);
    await tick();
    document
      .querySelector('[data-actieve-treffer]')
      ?.scrollIntoView({ block: 'center', inline: 'nearest' });
  }

  function zoekToets(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === 'Enter') {
      event.preventDefault();
      void ga(event.shiftKey ? -1 : 1);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      void sluitZoeken();
    }
  }

  // ---- Persoonsgegevens markeren en vervangen ----

  let zelfVeld = $state('');
  let lijstEl = $state<HTMLElement | null>(null);

  // Een geselecteerd stuk tekst in het verslag komt in het veld "Zelf iets markeren".
  function opSelectie(): void {
    const selectie = window.getSelection();
    if (!selectie || selectie.isCollapsed || !lijstEl) return;
    // Alleen de zichtbare tekst: de verborgen woorden voor schermlezers ("gemarkeerd als ...") horen
    // er niet bij.
    const houder = document.createElement('div');
    houder.append(selectie.getRangeAt(0).cloneContents());
    houder.querySelectorAll('.sr-only').forEach((el) => {
      el.remove();
    });
    const tekst = houder.textContent.trim();
    if (tekst !== '' && !tekst.includes('\n') && lijstEl.contains(selectie.anchorNode)) {
      zelfVeld = tekst;
    }
  }

  // ---- Toetsen voor het hele verslag ----

  function opToets(event: KeyboardEvent): void {
    if (event.defaultPrevented) return;
    if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      void openZoeken();
      return;
    }
    // Spatie speelt of pauzeert, maar nooit in een tekstveld en niet op een knop of link: die
    // bedienen spatie zelf.
    if (
      event.key === ' ' &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey &&
      !event.shiftKey &&
      haalAudio &&
      !neemtSpatieOver(event.target)
    ) {
      event.preventDefault();
      const eerste = regels[0];
      if (!afspeler.schakel() && eerste) speel(eerste);
    }
  }

  onMount(() => {
    document.addEventListener('selectionchange', opSelectie);
    void leesSnelheid().then((snelheid) => {
      afspeler.neemSnelheidOver(snelheid);
    });
    return () => {
      document.removeEventListener('selectionchange', opSelectie);
      afspeler.stop();
    };
  });

  const tijdKnop =
    'group inline-flex min-h-8 cursor-pointer items-center gap-1 rounded-klein text-klein tabular-nums text-dim hover:text-actie data-[actief=true]:text-actie';
  const snelheidsKnop =
    'min-h-8 cursor-pointer rounded-klein px-1.5 text-klein tabular-nums text-gedempt hover:bg-hover aria-pressed:bg-hover aria-pressed:font-semibold aria-pressed:text-tekst';
</script>

<svelte:window onkeydown={opToets} />

<div class="flex flex-col gap-3">
  <!-- De werkbalk (N-C1): Zoeken en Snelheid in één balk; Zoeken klapt open tot een veld in
       dezelfde balk. Blijft bij het scrollen in beeld. -->
  <div
    class="sticky top-0 z-10 flex flex-wrap items-center gap-1 rounded-groot bg-vlak p-1"
    role={zoekOpen ? 'search' : 'toolbar'}
    aria-label={zoekOpen ? undefined : t().verslagWerkbalk}
  >
    {#if zoekOpen}
      <div
        class="flex min-w-40 flex-1 items-center gap-2 rounded-klein border border-actie bg-verhoogd pr-1 pl-2.5"
      >
        <Search size={16} class="shrink-0 text-dim" aria-hidden="true" />
        <input
          bind:this={zoekVeld}
          bind:value={vraag}
          type="text"
          autocomplete="off"
          spellcheck="false"
          aria-label={t().verslagZoekVeld}
          placeholder={t().verslagZoekVeld}
          class="h-8 min-w-0 flex-1 bg-transparent text-basis text-tekst outline-none placeholder:text-dim"
          oninput={() => {
            huidig = 0;
          }}
          onkeydown={zoekToets}
        />
        <span class="min-w-[4.5rem] text-right text-klein text-gedempt tabular-nums" role="status"
          >{telling}</span
        >
        <Button
          variant="spook"
          class="px-1.5"
          title={t().verslagZoekVorige}
          aria-label={t().verslagZoekVorige}
          aria-disabled={treffers.length === 0}
          onclick={() => {
            void ga(-1);
          }}><ChevronUp size={16} aria-hidden="true" /></Button
        >
        <Button
          variant="spook"
          class="px-1.5"
          title={t().verslagZoekVolgende}
          aria-label={t().verslagZoekVolgende}
          aria-disabled={treffers.length === 0}
          onclick={() => {
            void ga(1);
          }}><ChevronDown size={16} aria-hidden="true" /></Button
        >
        <Button
          variant="spook"
          class="px-1.5"
          title={t().verslagZoekSluiten}
          aria-label={t().verslagZoekSluiten}
          onclick={() => {
            void sluitZoeken();
          }}><X size={16} aria-hidden="true" /></Button
        >
      </div>
    {:else}
      <Button
        variant="spook"
        bind:ref={zoekKnop}
        title={t().verslagZoekenTitel}
        aria-keyshortcuts="Control+F"
        onclick={openZoeken}
        ><Search size={16} aria-hidden="true" /><span class="max-[639px]:sr-only"
          >{t().verslagZoeken}</span
        ></Button
      >
      {#if haalAudio}
        <span class="mx-1.5 my-1 w-px self-stretch bg-lijn" aria-hidden="true"></span>
        <div
          role="group"
          aria-label={t().verslagSnelheidKop}
          class="flex items-center gap-0.5 text-klein text-dim"
          title={t().verslagSnelheidTitel}
        >
          <span class="mr-1 max-[639px]:hidden" aria-hidden="true">{t().verslagSnelheidKop}</span>
          {#each SNELHEDEN as snelheid (snelheid)}
            <button
              type="button"
              class={snelheidsKnop}
              aria-pressed={afspeler.snelheid === snelheid}
              aria-label={t().verslagSnelheidAria(snelheid)}
              onclick={() => {
                afspeler.zetSnelheid(snelheid);
              }}>{t().verslagSnelheid(snelheid)}</button
            >
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <!-- Persoonsgegevens markeren en vervangen (V-2), als één regel. -->
  <PersoonRegel
    {regels}
    {persoon}
    {zetVervangen}
    {haalWeg}
    {markeer}
    bind:zelfVeld
    meldFout={(melding: string | null) => {
      fout = melding;
      if (melding === null) status = '';
    }}
    meldStatus={(zin: string) => {
      status = zin;
    }}
  />

  {#if fout}
    <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
  {/if}
  {#if afspeler.laden}
    <p class="m-0 text-klein text-gedempt">{t().verslagLaden}</p>
  {/if}
  {#if afspeler.fout}
    <p class="m-0 text-klein text-fout-tekst" role="alert">{afspeler.fout}</p>
  {/if}

  <ol
    bind:this={lijstEl}
    class="m-0 grid list-none gap-x-3 gap-y-3 p-0 text-midden leading-[1.55] {labels
      ? 'grid-cols-[3.75rem_fit-content(12rem)_1fr] max-[639px]:grid-cols-[3.75rem_1fr]'
      : 'grid-cols-[3.75rem_1fr]'}"
    aria-label={lijstLabel}
  >
    {#each regels as regel, plek (regel.sleutel)}
      {@const tijd = tijdcode(regel.startMs)}
      {@const speeltDeze = afspeler.sleutel === regel.sleutel && afspeler.speelt}
      {@const geladenDeze = afspeler.sleutel === regel.sleutel}
      {@const hier = perRegel[plek]}
      {@const zien = getoond[plek] ?? regel}
      <li
        class="col-span-full grid grid-cols-subgrid items-baseline {labels
          ? 'max-[639px]:col-span-2'
          : ''}"
      >
        {#if haalAudio}
          <button
            type="button"
            class={tijdKnop}
            data-actief={geladenDeze}
            aria-label={speeltDeze ? t().verslagPauzeren(tijd) : t().verslagAfspelenVanaf(tijd)}
            onclick={() => {
              speel(regel);
            }}
          >
            {#if speeltDeze}
              <Pause size={12} aria-hidden="true" />
            {:else}
              <Play
                size={12}
                class={geladenDeze
                  ? 'opacity-100'
                  : 'opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100'}
                aria-hidden="true"
              />
            {/if}{tijd}</button
          >
        {:else}
          <span class="text-klein text-dim tabular-nums" title={geenGeluid}>{tijd}</span>
        {/if}
        {#if labels}
          <span class="min-w-0"
            >{#if label}{@render label(regel)}{/if}</span
          >
        {/if}
        <div class="min-w-0 {labels ? 'max-[639px]:col-span-2' : ''}">
          {#if bewerken === regel.sleutel}
            <textarea
              use:veld
              bind:value={concept}
              rows="1"
              autocomplete="off"
              spellcheck="false"
              aria-label={t().verslagRegelVeld(tijd)}
              aria-describedby="verslag-veld-hulp"
              class="block w-full resize-none overflow-hidden rounded-klein border border-actie bg-verhoogd px-2 py-1 text-midden leading-[1.55] text-tekst"
              onkeydown={(event) => {
                veldToets(event, regel.sleutel);
              }}
              onblur={() => {
                void rondAf(regel.sleutel, false, false);
              }}></textarea>
            <p id="verslag-veld-hulp" class="m-0 mt-1 text-klein text-dim">
              {t().verslagVeldHulp}
            </p>
          {:else}
            <button
              type="button"
              id={regelId(regel.sleutel)}
              class="-ml-1.5 block w-[calc(100%+0.375rem)] cursor-text rounded-klein pl-1.5 text-left whitespace-pre-line hover:bg-hover {regel.veranderd ||
              regel.sleutel === opgelicht
                ? 'bg-nieuw'
                : ''}"
              data-veranderd={regel.veranderd ? '' : undefined}
              data-opgelicht={regel.sleutel === opgelicht ? '' : undefined}
              title={haalAudio ? t().verslagHintAfspelen : t().verslagHint}
              onclick={() => {
                start(regel);
              }}
              >{#each deelTekst(zien.tekst, hier?.treffers ?? [], hier?.actief ?? -1, zien.markeringen ?? []) as deel, i (i)}{#if deel.treffer || deel.soort}<mark
                    class="rounded-[3px] {deel.treffer
                      ? deel.actief
                        ? 'bg-actie text-op-actie'
                        : 'bg-waarsch/35 text-tekst'
                      : persoon.vervangen
                        ? 'bg-actie/15 font-medium whitespace-nowrap text-actie'
                        : 'bg-actie/15 text-tekst'} {deel.soort && !persoon.vervangen
                      ? 'underline decoration-actie decoration-dotted decoration-2 underline-offset-[3px]'
                      : ''}"
                    title={deel.soort ? t().persoonSoort(deel.soort) : undefined}
                    data-actieve-treffer={deel.actief ? '' : undefined}>{deel.tekst}</mark
                  >{#if deel.soort}<span class="sr-only"
                      >({t().persoonSoortAria(t().persoonSoort(deel.soort))})</span
                    >{/if}{:else}{deel.tekst}{/if}{/each}{#if regel.veranderd}<span class="sr-only"
                  >({t().opnieuwVeranderdAria})</span
                >{/if}</button
            >
          {/if}
        </div>
      </li>
    {/each}
  </ol>
</div>

<!-- Voor de schermlezer: een bewaarde regel en het klaarzetten van het geluid. -->
<div role="status" class="sr-only">{afspeler.laden ? t().verslagLaden : status}</div>
