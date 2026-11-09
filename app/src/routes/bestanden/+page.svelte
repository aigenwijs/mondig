<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount, tick } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FileText from '@lucide/svelte/icons/file-text';
  import Film from '@lucide/svelte/icons/film';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Upload from '@lucide/svelte/icons/upload';
  import InstellingSprekers from '#lib/components/InstellingSprekers.svelte';
  import Transcript from '#lib/components/Transcript.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    bestandAnnuleer,
    bestandenKies,
    bestandenStand,
    bestandTranscript,
    bestandWeghalen,
    duurTekst,
    foutVanStand,
    isNieuwer,
    luisterNaarSlepen,
    luisterNaarStand,
    stand,
    type BestandenStand,
    type BestandInfo,
    type BestandTranscript
  } from '#lib/bestanden.js';
  import { bewarenOpenMap, mapTekst, type VerwijderUitslag } from '#lib/bewaren.js';
  import { bewaarStand } from '#lib/bewarenstand.svelte.js';
  import { luisterNaarTaken } from '#lib/gebeurtenissen.js';
  import {
    resterendeTijdTekst,
    schatResterendeSeconden,
    type Meting
  } from '#lib/resterendetijd.js';
  import { bereikteDrempel } from '#lib/schermlezer.js';
  import { schatWachtrij } from '#lib/wachtrijtijd.js';
  import { foutVak, paginaTitel, vlak } from '#lib/stijl.js';
  import { foutUitCommando, meldingTekst, type Foutmelding, type Gebeurtenis } from '#lib/taak.js';

  // Eén scherm met drie standen: leeg met slepen of
  // kiezen, de wachtrij, en het transcript van één bestand (Transcript.svelte, met bewaren en
  // exporteren uit 4-C).
  //
  // De lijst komt alleen uit standen van Rust (het event `bestanden-gewijzigd` en de
  // antwoorden van de commando's), elk met een volgnummer: een oudere stand wordt genegeerd,
  // zodat een laat antwoord een eindtoestand niet terugdraait.

  let lijst = $state<BestandInfo[]>([]);
  let versie = -1;
  let geladen = $state(false);
  let geopend = $state<BestandTranscript | null>(null);
  // Een fout van het toevoegen (uit de stand) en een fout van dit scherm zelf.
  let standFout = $state<Foutmelding | null>(null);
  let schermFout = $state<Foutmelding | null>(null);
  const fout = $derived(schermFout ?? standFout);
  let slepen = $state(false);
  let kiezen = $state(false);
  // Per bestand de tekst "nog ongeveer ...", als er genoeg gemeten is.
  let resterend = $state<Record<number, string | null>>({});
  // De resttijd van de hele wachtrij ("Nog ongeveer 12 minuten voor 4 bestanden"), of niets
  // zolang er te kort gemeten is of er maar één bestand is.
  let wachtrijTekst = $state<string | null>(null);
  // Voor de schermlezer: voortgang bij 25, 50, 75 en 100 procent, klaar en fouten.
  let schermlezerZin = $state('');
  // Na Verwijderen: wat er weg is en wat bleef staan.
  let lijstMelding = $state<string | null>(null);
  let kiesKnop = $state<HTMLElement | null>(null);

  // Geen state: alleen gelezen bij elke voortgang.
  const metingen: Record<number, Meting[]> = {};
  const vorigProcent: Record<number, number> = {};
  const sprekersGemeld: Record<number, boolean> = {};

  /** Neemt een stand over als die nieuwer is dan wat er staat. */
  function neemOver(nieuw: BestandenStand): void {
    if (!isNieuwer(nieuw, versie)) return;
    versie = nieuw.versie;
    lijst = nieuw.lijst;
    standFout = foutVanStand(nieuw, t());
    werkWachtrijBij();
  }

  /** Rekent de resttijd van de hele wachtrij opnieuw uit, met de klok van deze pagina. */
  function werkWachtrijBij(): void {
    const schatting = schatWachtrij(lijst, (taak) => {
      const eerste = metingen[taak]?.[0];
      return eerste
        ? { ms: performance.now() - eerste.tijd, vanafProcent: eerste.binnen }
        : undefined;
    });
    wachtrijTekst =
      schatting === null
        ? null
        : t().wachtrijRest(resterendeTijdTekst(schatting.seconden, t()), schatting.aantal);
  }

  async function laad(): Promise<void> {
    try {
      neemOver(await bestandenStand());
    } catch {
      // Lukt de stand niet, dan blijft wat er stond.
    }
  }

  // De gebeurtenissen zelf alleen voor de resterende tijd en de schermlezer; de lijst komt
  // uit de stand.
  function opTaak(gebeurtenis: Gebeurtenis): void {
    if (gebeurtenis.soort !== 'bestand') return;
    const { taak, toestand } = gebeurtenis;
    const naam = lijst.find((b) => b.taak === taak)?.naam ?? '';
    if (toestand.type === 'verwerkt' && toestand.stap === 'sprekers') {
      // Het rekenen van het sprekermodel meldt geen voortgang: geen tijd, wel een zin.
      resterend[taak] = null;
      if (!sprekersGemeld[taak]) {
        sprekersGemeld[taak] = true;
        schermlezerZin = t().vergaderSprekersSchermlezer;
      }
    } else if (toestand.type === 'verwerkt' && toestand.voortgang !== null) {
      const reeks = metingen[taak] ?? [];
      reeks.push({ tijd: performance.now(), binnen: toestand.voortgang });
      if (reeks.length > 80) reeks.shift();
      metingen[taak] = reeks;
      const seconden = schatResterendeSeconden(reeks, 100);
      resterend[taak] = seconden === null ? null : resterendeTijdTekst(seconden, t());
      const drempel = bereikteDrempel(vorigProcent[taak] ?? 0, toestand.voortgang);
      vorigProcent[taak] = toestand.voortgang;
      if (drempel !== null && drempel < 100) {
        schermlezerZin = t().bestandProcentSchermlezer(naam, drempel);
      }
    } else if (toestand.type === 'klaar') {
      schermlezerZin = t().bestandKlaarSchermlezer(naam);
    } else if (toestand.type === 'fout') {
      schermlezerZin = t().bestandFoutSchermlezer(naam, toestand.melding);
    }
    werkWachtrijBij();
  }

  async function kies(): Promise<void> {
    if (kiezen) return;
    kiezen = true;
    schermFout = null;
    try {
      neemOver(await bestandenKies());
    } catch (e) {
      schermFout = foutUitCommando(e, t().somsGingErIetsMis);
    } finally {
      kiezen = false;
    }
  }

  async function annuleer(taak: number): Promise<void> {
    try {
      await bestandAnnuleer(taak);
    } catch {
      // Al afgelopen: de stand zet de rij goed.
    }
  }

  async function weghalen(taak: number): Promise<void> {
    try {
      neemOver(await bestandWeghalen(taak));
    } catch {
      await laad();
    }
    await tick();
    // De knop met focus is weg: naar de hoofdactie.
    kiesKnop?.focus();
  }

  /** Opent de opslagmap in de Verkenner; de fout komt in gewone taal boven de lijst. */
  async function openMap(): Promise<void> {
    try {
      await bewarenOpenMap();
    } catch (e) {
      schermFout = foutUitCommando(e, t().somsGingErIetsMis);
    }
  }

  async function open(taak: number): Promise<void> {
    lijstMelding = null;
    const transcript = await bestandTranscript(taak).catch(() => null);
    if (transcript === null) {
      schermFout = { melding: t().transcriptNietGevonden, vervolg: 'geen' };
      return;
    }
    schermFout = null;
    geopend = transcript;
    await tick();
    document.getElementById('transcript-kop')?.focus();
  }

  async function terug(): Promise<void> {
    const was = geopend?.taak;
    geopend = null;
    await tick();
    if (was !== undefined) document.getElementById(`openen-${String(was)}`)?.focus();
  }

  /** Na Verwijderen: terug naar de lijst, met wat er weg is en wat bleef staan. */
  async function naVerwijderen(uitslag: VerwijderUitslag): Promise<void> {
    neemOver(uitslag.stand);
    const delen = [
      uitslag.verwijderd.length > 0
        ? t().verwijderdMelding(uitslag.verwijderd.length)
        : t().verwijderdZonderBestanden
    ];
    if (uitslag.gebleven.length > 0) delen.push(t().verwijderGebleven(uitslag.gebleven.join(', ')));
    lijstMelding = delen.join(' ');
    geopend = null;
    await tick();
    kiesKnop?.focus();
  }

  onMount(() => {
    let weg = false;
    const opruimers: (() => void)[] = [];
    const houdVast = (opruimen: () => void): void => {
      if (weg) opruimen();
      else opruimers.push(opruimen);
    };
    // Eerst alle luisteraars, dan pas de stand ophalen: zo gaat er niets verloren tussen
    // die twee, ook geen fout van slepen vanaf een ander scherm (die staat in de stand).
    void Promise.all([
      luisterNaarTaken(opTaak).then(houdVast),
      luisterNaarStand(neemOver).then(houdVast),
      luisterNaarSlepen((boven) => {
        slepen = boven;
      }).then(houdVast)
    ])
      .then(laad)
      .then(() => {
        if (bewaarStand.stand === null) bewaarStand.laad(t().somsGingErIetsMis);
        geladen = true;
      });
    return () => {
      weg = true;
      opruimers.forEach((opruimen) => {
        opruimen();
      });
    };
  });

  // Waar het uitschrift komt, uit de instelling Opslaan ("Documenten\Mondig").
  const waarTekst = $derived.by(() => {
    const instellingen = bewaarStand.stand;
    if (instellingen?.map == null) return null;
    const map = mapTekst(instellingen.map, t());
    return instellingen.automatisch ? t().bestandenKomtIn(map) : t().bestandenOpslaanIn(map);
  });

  // De hoogtes van de streepjes in de lege staat (px); daarna volgt de punt van het woordmerk.
  const GOLFJE = [12, 22, 34, 18, 28, 14, 8];

  const kopRegel = 'flex flex-wrap items-end gap-3';
  const rijKlasse = 'flex items-center gap-3.5 px-4 py-3 [&+&]:border-t [&+&]:border-lijn';
  const vierkant =
    'grid size-[34px] shrink-0 place-items-center rounded-klein bg-achtergrond text-dim';
</script>

<svelte:head>
  <title>{t().menuBestanden} - {t().appTitle}</title>
</svelte:head>

{#snippet sprekersRegel()}
  <InstellingSprekers variant="compact" />
{/snippet}

{#snippet foutIcoon()}
  <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
{/snippet}

{#snippet foutMelding()}
  {#if fout}
    <div class={foutVak} role="alert">
      <p class="m-0 flex items-start gap-2.5">
        {@render foutIcoon()}{meldingTekst(fout, t().appNietBijgewerkt)}
      </p>
      {#if fout.vervolg === 'opnieuw_downloaden'}
        <Button href={resolve('/modellen')}>{t().spraakmodelDownloaden}</Button>
      {/if}
    </div>
  {/if}
{/snippet}

{#if geopend}
  <!-- 3. Het transcript van één bestand, met bewaren en exporteren (4-C). -->
  {#key geopend.taak}
    <Transcript transcript={geopend} {terug} verwijderd={naVerwijderen} />
  {/key}
{:else}
  <div class={kopRegel}>
    <h1 class={paginaTitel}>{t().menuBestanden}</h1>
    <div class="ml-auto flex flex-wrap items-center gap-2">
      <!-- Op het kleinste venster alleen het icoon, met tooltip en label. -->
      <Button
        variant="tweede"
        class="max-[639px]:w-8 max-[639px]:px-0"
        title={t().bestandenUitschriftenOpenen}
        aria-label={t().bestandenUitschriftenOpenen}
        onclick={openMap}
        ><FolderOpen size={16} aria-hidden="true" /><span class="max-[639px]:hidden"
          >{t().bestandenUitschriftenOpenen}</span
        ></Button
      >
      {#if lijst.length > 0}
        <Button variant="tweede" bind:ref={kiesKnop} onclick={kies} aria-disabled={kiezen}
          ><Upload size={16} aria-hidden="true" />{t().bestandenMeerToevoegen}</Button
        >
      {/if}
    </div>
  </div>

  {@render foutMelding()}
  <p class="m-0 text-klein text-gedempt empty:hidden" role="status">{lijstMelding ?? ''}</p>

  {#if !geladen}
    <p class="m-0 text-gedempt" role="status">{t().gegevensOphalen}</p>
  {:else if lijst.length === 0}
    <!-- 1. Leeg: slepen of kiezen. Slepen gaat via het venster (Rust), niet via HTML. De streepjes
         eindigen in de punt van het woordmerk en bewegen alleen tijdens het slepen. -->
    <div
      class="flex min-h-[220px] flex-1 flex-col items-center justify-center gap-2.5 rounded-groot border-[1.5px] border-dashed p-5 text-center transition-colors {slepen
        ? 'border-actie bg-nieuw'
        : 'border-rand-veld'}"
    >
      <div class="mb-1 flex h-10 items-center gap-[3px]" aria-hidden="true">
        {#each GOLFJE as hoogte, i (i)}
          <i
            class="block w-[5px] rounded-[2px] transition-colors {slepen
              ? 'bg-actie motion-safe:animate-[dans_.9s_ease-in-out_infinite]'
              : 'bg-meter-leeg'}"
            style:height="{hoogte}px"
            style:animation-delay="{i * 0.1}s"
          ></i>
        {/each}
        <i class="ml-1 block size-[9px] rounded-full bg-actie"></i>
      </div>
      <h2 class="m-0 text-kop2 max-[639px]:text-[16px]">
        {slepen ? t().bestandenLoslaten : t().bestandenSleepKop}
      </h2>
      <p class="m-0 text-gedempt">
        {slepen ? t().bestandenLoslatenUitleg : t().bestandenSleepUitleg}
      </p>
      {#if !slepen && waarTekst !== null}
        <p class="m-0 text-klein text-dim">{waarTekst}</p>
      {/if}
      {#if !slepen}
        <Button class="mt-1.5" bind:ref={kiesKnop} onclick={kies} aria-disabled={kiezen}
          >{t().bestandenKiezen}</Button
        >
        <!-- Geldt voor de bestanden die hierna binnenkomen (zelfde instelling als bij
             vergaderingen en op Modellen). -->
        <div class="mt-2">{@render sprekersRegel()}</div>
      {/if}
    </div>
  {:else}
    <!-- 2. De wachtrij. -->
    {#if slepen}
      <div
        class="rounded-groot border-[1.5px] border-dashed border-actie bg-vlak px-4 py-3 text-center text-gedempt"
      >
        {t().bestandenLoslaten}
      </div>
    {/if}
    {#if wachtrijTekst}
      <p class="m-0 text-klein text-gedempt">{wachtrijTekst}</p>
    {/if}
    <ul class="{vlak} m-0 list-none p-0" aria-label={t().bestandenLijstLabel}>
      {#each lijst as bestand (bestand.taak)}
        {@const toestand = bestand.toestand}
        {@const isFout = toestand.type === 'fout'}
        {@const loopt = toestand.type === 'verwerkt'}
        {@const wacht = toestand.type === 'wacht'}
        <li class={rijKlasse}>
          <div class="{vierkant} {isFout ? 'text-fout-tekst' : ''}">
            {#if isFout}
              <CircleAlert size={16} aria-hidden="true" />
            {:else if toestand.type === 'klaar'}
              <Check size={16} aria-hidden="true" />
            {:else if bestand.video}
              <Film size={16} aria-hidden="true" />
            {:else}
              <FileText size={16} aria-hidden="true" />
            {/if}
          </div>
          <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <span class="truncate text-basis font-medium">{bestand.naam}</span>
            <span class="text-klein {isFout ? 'text-fout-tekst' : 'text-gedempt'}">
              {#if bestand.duurMs !== null && !isFout}
                {duurTekst(bestand.duurMs)}<span class="mx-1.5 text-dim" aria-hidden="true">·</span>
              {/if}{stand(bestand, resterend[bestand.taak] ?? null, t())}{#if bestand.uitMap}<span
                  class="mx-1.5 text-dim"
                  aria-hidden="true">·</span
                >{t().bestandUitMap}{/if}{#if bestand.opgeslagen}<span
                  class="mx-1.5 text-dim"
                  aria-hidden="true">·</span
                >{bestand.opgeslagenVerouderd
                  ? t().bestandOpgeslagenVerouderd(bestand.opgeslagen)
                  : t().bestandOpgeslagenAls(bestand.opgeslagen)}{/if}
            </span>
            {#if bestand.bewaarFout}
              <span class="text-klein text-fout-tekst"
                >{t().bestandBewaarFout(bestand.bewaarFout)}</span
              >
            {/if}
          </div>
          <div class="ml-auto flex shrink-0 items-center gap-2">
            {#if loopt}
              <div
                class="h-1.5 w-[140px] overflow-hidden rounded-full bg-hover max-[639px]:w-16"
                aria-hidden="true"
              >
                <div
                  class="h-full bg-actie transition-[width] {toestand.stap === 'sprekers'
                    ? 'motion-safe:animate-pulse'
                    : ''}"
                  style:width="{toestand.stap === 'sprekers' ? 100 : (toestand.voortgang ?? 0)}%"
                ></div>
              </div>
            {/if}
            {#if toestand.type === 'klaar' && bestand.heeftTranscript}
              <Button
                variant="tweede"
                id="openen-{bestand.taak}"
                aria-label={t().bestandOpenenAria(bestand.naam)}
                onclick={() => open(bestand.taak)}>{t().bestandOpenen}</Button
              >
            {/if}
            {#if loopt}
              <Button
                variant="spook"
                title={t().bestandAnnulerenKort}
                aria-label={t().bestandAnnuleren(bestand.naam)}
                onclick={() => annuleer(bestand.taak)}
                ><Trash2 size={16} aria-hidden="true" /></Button
              >
            {:else}
              <Button
                variant="spook"
                title={wacht ? t().bestandUitRijKort : t().bestandWeghalenKort}
                aria-label={wacht
                  ? t().bestandUitRij(bestand.naam)
                  : t().bestandWeghalen(bestand.naam)}
                onclick={() => weghalen(bestand.taak)}
                ><Trash2 size={16} aria-hidden="true" /></Button
              >
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    <p class="m-0 text-klein text-dim">{t().bestandenDicterenGaatVoor}</p>
    {@render sprekersRegel()}
  {/if}
{/if}

<!-- Voortgang en uitkomsten voor de schermlezer, zonder de focus te verplaatsen (WCAG 4.1.3). -->
<div role="status" class="sr-only">{schermlezerZin}</div>
