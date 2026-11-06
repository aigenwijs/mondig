<script lang="ts">
  import { resolve } from '$app/paths';
  import { tick, untrack } from 'svelte';
  import Copy from '@lucide/svelte/icons/copy';
  import Pencil from '@lucide/svelte/icons/pencil';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import WaarheenVraag from '#lib/components/WaarheenVraag.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import { tijdcode } from '#lib/bestanden.js';
  import {
    aanbiederNaam,
    deelTekst,
    plekTekst,
    notulenKlaar,
    springNaarBron,
    voortgangProcent,
    type NotulenActiepunt,
    type NotulenBesluit,
    type NotulenBron,
    type NotulenInstellingen,
    type NotulenStand,
    type PuntSoort
  } from '#lib/notulen.js';
  import { vlak } from '#lib/stijl.js';

  // Het tabblad Notulen van het verslag (T3, ADR 0026): de lege staat met "Notulen maken", de
  // eenmalige vraag waar de tekst heen gaat, de voortgang met Stoppen, en de notulen zelf met bij
  // elk punt een bronknop die naar het moment in het tabblad Verslag springt. Mondig verandert
  // de tekst van een punt nooit; een punt dat niet in het verslag staat, blijft staan met een
  // oranje markering. Alles wat de notulen verandert, loopt via Rust; dit onderdeel toont en vraagt.

  let {
    stand,
    instellingen,
    vervangen,
    bevestigd,
    bezig = false,
    maak,
    stop,
    spring,
    kopieer,
    pasAan,
    haalWeg,
    melding = null
  }: {
    stand: NotulenStand;
    /** De instellingen van Notulen; `null` zolang ze geladen worden. */
    instellingen: NotulenInstellingen | null;
    /** Vervangt het verslag persoonsgegevens (de schakelaar van het verslag)? */
    vervangen: boolean;
    /** Heeft de gebruiker voor dit verslag al gezien waar de tekst heen gaat? */
    bevestigd: boolean;
    bezig?: boolean;
    /** Start het maken; `overschrijf` is waar als er onleesbare notulen staan en de gebruiker overschrijven koos. */
    maak: (overschrijf: boolean) => void;
    stop: () => void;
    /** Springt naar de beurt (vanaf 0) in het tabblad Verslag. */
    spring: (beurt: number) => void;
    kopieer: () => void;
    /** Past een punt aan; geeft een melding bij een fout, anders `null`. */
    pasAan: (
      soort: PuntSoort,
      id: number,
      tekst: string,
      wie: string | null,
      wanneer: string | null
    ) => Promise<string | null>;
    haalWeg: (soort: PuntSoort, id: number) => Promise<string | null>;
    melding?: string | null;
  } = $props();

  const klaar = $derived(notulenKlaar(instellingen));
  const info = $derived(stand.info);
  const locale = $derived(huidigeTaal() === 'nl' ? 'nl-NL' : 'en-GB');

  // De vraag "waar gaat de tekst heen?" staat open tot ze beantwoord is.
  let vraag = $state(false);
  const aanbieder = $derived(
    instellingen?.aanbieder ? aanbiederNaam(instellingen.aanbieder, t()) : ''
  );

  function vraagOfMaak(): void {
    // Notulen die deze versie niet kan lezen overschrijft Mondig nooit zonder te vragen.
    if (bevestigd && !stand.onleesbaar) maak(false);
    else vraag = true;
  }

  function bevestig(): void {
    vraag = false;
    maak(stand.onleesbaar);
  }

  // Zijn er nieuwe notulen (ander moment) of horen ze nu bij een andere versie van het verslag,
  // dan sluit de bewerkstand: de punten waar je aan werkte zijn niet meer dezelfde.
  const kenmerk = $derived(
    info === null ? null : `${String(info.momentMs)}:${String(info.vorigeVersie)}`
  );
  let vorigKenmerk: string | null = null;
  $effect(() => {
    const nu = kenmerk;
    untrack(() => {
      if (nu !== vorigKenmerk) {
        bewerken = null;
        vorigKenmerk = nu;
      }
    });
  });

  // Een tweede klik terwijl een wijziging nog loopt, wordt genegeerd.
  let werkt = $state(false);

  // Het punt dat bewerkt wordt, en de velden daarvan.
  let bewerken = $state<{ soort: PuntSoort; id: number } | null>(null);
  let wat = $state('');
  let wie = $state('');
  let wanneer = $state('');
  let fout = $state<string | null>(null);
  let status = $state('');

  function startBesluit(b: NotulenBesluit): void {
    fout = null;
    bewerken = { soort: 'besluit', id: b.id };
    wat = b.tekst;
  }

  function startActie(a: NotulenActiepunt): void {
    fout = null;
    bewerken = { soort: 'actiepunt', id: a.id };
    wat = a.wat;
    wie = a.wie;
    wanneer = a.wanneer;
  }

  const punt = (soort: PuntSoort, id: number): string => `notulen-${soort}-${String(id)}`;

  async function bewaar(): Promise<void> {
    if (bewerken === null || werkt) return;
    werkt = true;
    const huidig = bewerken;
    const melding = await pasAan(
      huidig.soort,
      huidig.id,
      wat,
      huidig.soort === 'actiepunt' ? wie : null,
      huidig.soort === 'actiepunt' ? wanneer : null
    ).finally(() => {
      werkt = false;
    });
    if (melding === null) {
      bewerken = null;
      status = t().notulenPuntGewijzigd;
      await tick();
      document.getElementById(punt(huidig.soort, huidig.id))?.focus();
    } else {
      fout = melding;
    }
  }

  async function weg(soort: PuntSoort, id: number): Promise<void> {
    if (werkt) return;
    werkt = true;
    const melding = await haalWeg(soort, id).finally(() => {
      werkt = false;
    });
    if (melding === null) {
      bewerken = null;
      status = t().notulenPuntWeg;
    } else {
      fout = melding;
    }
  }

  function moment(ms: number): string {
    return new Intl.DateTimeFormat(locale, {
      day: 'numeric',
      month: 'long',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false
    }).format(new Date(ms));
  }

  const soortNaam = (soort: PuntSoort): string =>
    soort === 'besluit' ? t().notulenSoortBesluit : t().notulenSoortActiepunt;

  const procent = $derived(stand.maken ? voortgangProcent(stand.maken) : null);
  const nietGevonden = $derived(info?.nietGevonden ?? 0);

  const kopKlasse = 'm-0 mb-1.5 font-tekst text-basis font-semibold text-tekst';
  const markering =
    'inline-flex items-center gap-1 rounded-klein bg-waarsch/15 px-1.5 text-klein font-medium text-waarsch';
  const veldKlasse =
    'h-9 w-full min-w-0 rounded-klein border border-rand-veld bg-verhoogd px-3 text-basis text-tekst';
</script>

<!-- Eén bronknop: "▸ 0:28". Zonder beurt (niet teruggevonden, of een vorige versie) is er geen knop. -->
{#snippet bron(b: NotulenBron)}
  {#if b.beurt !== null && b.ms !== null}
    {@const tijd = tijdcode(b.ms)}
    <button
      type="button"
      class="inline-flex min-h-8 min-w-8 shrink-0 cursor-pointer items-center justify-center gap-1 rounded-klein px-2 text-klein text-actie tabular-nums hover:bg-hover"
      title={t().notulenBronCitaat(b.citaat)}
      aria-label={t().notulenBronAria(tijd)}
      onclick={() => springNaarBron(b, spring)}
    >
      <span aria-hidden="true">▸</span>{tijd}
    </button>
  {:else if !b.gevonden}
    <span class={markering}>
      <TriangleAlert size={12} aria-hidden="true" />{t().notulenNietTeruggevonden}
    </span>
  {/if}
{/snippet}

{#snippet knoppen(soort: PuntSoort, id: number, start: () => void)}
  <span class="ml-auto flex shrink-0 gap-0.5">
    <Button
      variant="spook"
      size="icoon"
      title={t().notulenAanpassen}
      aria-label={t().notulenPuntAanpassenAria(soortNaam(soort))}
      aria-disabled={bezig || werkt}
      onclick={start}><Pencil size={14} aria-hidden="true" /></Button
    >
    <Button
      variant="spook"
      size="icoon"
      title={t().notulenWeghalen}
      aria-label={t().notulenPuntWeghalenAria(soortNaam(soort))}
      aria-disabled={bezig}
      onclick={() => void weg(soort, id)}><Trash2 size={14} aria-hidden="true" /></Button
    >
  </span>
{/snippet}

<div class="flex flex-col gap-4">
  {#if melding}
    <p class="m-0 text-klein text-gedempt" role="status">{melding}</p>
  {/if}

  {#if stand.maken}
    <!-- Bezig: de voortgang per deel en Stoppen. Het verslag zelf blijft te lezen. -->
    <div class="{vlak} flex flex-col gap-2.5 px-4 py-3.5" role="status">
      <div class="flex items-center gap-2.5">
        <span
          class="size-3.5 shrink-0 rounded-full border-2 border-lijn border-t-actie motion-safe:animate-spin"
          aria-hidden="true"
        ></span>
        <b class="min-w-0 flex-1 font-semibold">{t().notulenBezigKop}</b>
        <Button variant="spook" aria-disabled={bezig} onclick={stop}>{t().notulenStoppen}</Button>
      </div>
      <p class="m-0 text-gedempt">{deelTekst(stand.maken, t())}</p>
      {#if procent !== null}
        <div
          class="h-1.5 overflow-hidden rounded-full bg-hover"
          role="progressbar"
          aria-label={t().notulenBezigKop}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={procent}
        >
          <div class="h-full bg-actie transition-[width]" style:width="{String(procent)}%"></div>
        </div>
      {/if}
    </div>
  {:else if info}
    {#if info.vorigeVersie}
      <div
        class="flex flex-wrap items-center gap-2.5 rounded-groot border border-lijn bg-nieuw py-2.5 pr-3 pl-3.5"
        role="status"
      >
        <TriangleAlert size={16} class="shrink-0 text-waarsch" aria-hidden="true" />
        <span class="min-w-0 flex-1 text-klein">{t().notulenVorigeVersie}</span>
        <Button variant="tweede" aria-disabled={bezig} onclick={vraagOfMaak}
          >{t().notulenOpnieuwMaken}</Button
        >
      </div>
    {/if}

    {#if nietGevonden > 0}
      <p class="m-0 flex items-start gap-2 text-klein text-waarsch">
        <TriangleAlert
          size={14}
          class="mt-0.5 shrink-0"
          aria-hidden="true"
        />{t().notulenMarkeringTelling(nietGevonden)}
      </p>
    {/if}

    <section aria-labelledby="notulen-samenvatting">
      <h3 id="notulen-samenvatting" class={kopKlasse}>{t().notulenSamenvatting}</h3>
      <p class="m-0 whitespace-pre-line">{info.samenvatting}</p>
      <p class="m-0 mt-1 text-klein text-dim">{t().notulenSamenvattingNota}</p>
    </section>

    <section aria-labelledby="notulen-besluiten">
      <h3 id="notulen-besluiten" class={kopKlasse}>{t().notulenBesluiten}</h3>
      {#if info.besluiten.length === 0}
        <p class="m-0 text-gedempt">{t().notulenGeenBesluiten}</p>
      {:else}
        <ul class="m-0 flex list-none flex-col gap-2.5 p-0">
          {#each info.besluiten as besluit (besluit.id)}
            <li class="flex flex-col gap-1">
              {#if bewerken?.soort === 'besluit' && bewerken.id === besluit.id}
                <label class="flex flex-col gap-1 text-klein text-gedempt">
                  {t().notulenVeldTekst}
                  <textarea
                    bind:value={wat}
                    rows="3"
                    class="w-full resize-y rounded-klein border border-rand-veld bg-verhoogd px-3 py-2 text-basis text-tekst"
                  ></textarea>
                </label>
                <div class="flex flex-wrap gap-2">
                  <Button aria-disabled={bezig || werkt} onclick={() => void bewaar()}
                    >{t().notulenPuntBewaren}</Button
                  >
                  <Button
                    variant="spook"
                    onclick={() => {
                      bewerken = null;
                    }}>{t().annuleren}</Button
                  >
                </div>
              {:else}
                <div class="flex flex-wrap items-start gap-x-2 gap-y-0.5">
                  <span
                    class="min-w-0 flex-[1_1_16rem] pt-1"
                    id={punt('besluit', besluit.id)}
                    tabindex="-1">{besluit.tekst}</span
                  >
                  {@render bron(besluit.bron)}
                  {@render knoppen('besluit', besluit.id, () => {
                    startBesluit(besluit);
                  })}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section aria-labelledby="notulen-acties">
      <h3 id="notulen-acties" class={kopKlasse}>{t().notulenActiepunten}</h3>
      {#if info.actiepunten.length === 0}
        <p class="m-0 text-gedempt">{t().notulenGeenActiepunten}</p>
      {:else}
        <ul class="m-0 flex list-none flex-col gap-3 p-0">
          {#each info.actiepunten as actie (actie.id)}
            <li class="flex flex-col gap-1">
              {#if bewerken?.soort === 'actiepunt' && bewerken.id === actie.id}
                <label class="flex flex-col gap-1 text-klein text-gedempt">
                  {t().notulenVeldWat}
                  <textarea
                    bind:value={wat}
                    rows="2"
                    class="w-full resize-y rounded-klein border border-rand-veld bg-verhoogd px-3 py-2 text-basis text-tekst"
                  ></textarea>
                </label>
                <div class="grid grid-cols-2 gap-2 max-[639px]:grid-cols-1">
                  <label class="flex flex-col gap-1 text-klein text-gedempt">
                    {t().notulenVeldWie}
                    <input bind:value={wie} class={veldKlasse} autocomplete="off" />
                  </label>
                  <label class="flex flex-col gap-1 text-klein text-gedempt">
                    {t().notulenVeldWanneer}
                    <input bind:value={wanneer} class={veldKlasse} autocomplete="off" />
                  </label>
                </div>
                <div class="flex flex-wrap gap-2">
                  <Button aria-disabled={bezig || werkt} onclick={() => void bewaar()}
                    >{t().notulenPuntBewaren}</Button
                  >
                  <Button
                    variant="spook"
                    onclick={() => {
                      bewerken = null;
                    }}>{t().annuleren}</Button
                  >
                </div>
              {:else}
                <div class="flex flex-wrap items-start gap-x-2 gap-y-0.5">
                  <span
                    class="min-w-0 flex-[1_1_16rem] pt-1"
                    id={punt('actiepunt', actie.id)}
                    tabindex="-1">{actie.wat}</span
                  >
                  {@render bron(actie.bron)}
                  {@render knoppen('actiepunt', actie.id, () => {
                    startActie(actie);
                  })}
                </div>
                <p
                  class="m-0 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-klein text-gedempt"
                >
                  <span>{t().notulenWie}:</span>
                  {#if actie.wie.trim() === ''}
                    <span class="text-dim">{t().notulenNietGenoemd}</span>
                  {:else}
                    <span class="text-tekst">{actie.wie}</span>
                  {/if}
                  {#if actie.wieNietTeruggevonden}
                    <span class={markering}
                      ><TriangleAlert size={12} aria-hidden="true" />{t()
                        .notulenVeldNietGevonden}</span
                    >
                  {/if}
                  <span aria-hidden="true">·</span>
                  <span>{t().notulenWanneer}:</span>
                  {#if actie.wanneer.trim() === ''}
                    <span class="text-dim">{t().notulenNietGenoemd}</span>
                  {:else}
                    <span class="text-tekst">{actie.wanneer}</span>
                  {/if}
                  {#if actie.wanneerNietTeruggevonden}
                    <span class={markering}
                      ><TriangleAlert size={12} aria-hidden="true" />{t()
                        .notulenVeldNietGevonden}</span
                    >
                  {/if}
                </p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}

    <div class="flex flex-wrap items-center gap-2 border-t border-lijn pt-3">
      <p class="m-0 min-w-0 flex-1 text-klein text-dim">
        {t().notulenGemaaktMet(plekTekst(info.aanbieder, info.model, t()), moment(info.momentMs))}
      </p>
      <Button variant="tweede" onclick={kopieer}
        ><Copy size={16} aria-hidden="true" />{t().notulenKopieren}</Button
      >
      {#if !info.vorigeVersie}
        <Button variant="tweede" aria-disabled={bezig} onclick={vraagOfMaak}
          >{t().notulenOpnieuwMaken}</Button
        >
      {/if}
    </div>
  {:else}
    <!-- Nog geen notulen. -->
    {#if stand.onleesbaar}
      <p class="m-0 text-klein text-gedempt" role="note">{t().notulenOnleesbaar}</p>
    {/if}
    <div class="{vlak} flex flex-col items-start gap-2.5 px-4 py-4">
      <b class="font-semibold">{t().notulenLeegKop}</b>
      <p class="m-0 text-gedempt">{t().notulenLeegTekst}</p>
      {#if klaar === 'ok'}
        {#if !vraag}
          <Button aria-disabled={bezig} onclick={vraagOfMaak}>{t().notulenMaken}</Button>
        {/if}
      {:else if klaar !== null}
        <p class="m-0 text-gedempt">
          {klaar === 'niet-ingesteld' ? t().notulenNietIngesteld : t().notulenOnvolledig}
        </p>
        <Button variant="tweede" href={`${resolve('/instellingen')}#notulen`}
          >{t().notulenNaarInstellingen}</Button
        >
      {/if}
    </div>
  {/if}

  {#if vraag}
    <WaarheenVraag
      id="notulen-vraag"
      tekst={t().notulenVraagTekst(aanbieder, vervangen)}
      knop={stand.onleesbaar ? t().notulenOverschrijven : t().notulenMaken}
      {bezig}
      bijBevestig={bevestig}
      bijAnnuleer={() => {
        vraag = false;
      }}
    >
      {#snippet extra()}
        {#if stand.onleesbaar}
          <span class="text-waarsch">{t().notulenOverschrijfVraag}</span>
        {/if}
      {/snippet}
    </WaarheenVraag>
  {/if}

  <div role="status" class="sr-only">{status}</div>
</div>
