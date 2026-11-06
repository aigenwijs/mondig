<script lang="ts">
  import { onMount, tick } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Copy from '@lucide/svelte/icons/copy';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import OpslaanKnop from '#lib/components/OpslaanKnop.svelte';
  import VerslagLijst from '#lib/components/VerslagLijst.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import {
    bestandAudioStuk,
    bestandPersoonMarkeer,
    bestandPersoonVervangen,
    bestandPersoonWeghalen,
    bestandZetTekst,
    duurTekst,
    naamZonderExtensie,
    transcriptTekst,
    type BestandTranscript,
    type TranscriptRegel
  } from '#lib/bestanden.js';
  import {
    bestandBewaar,
    bestandBewaarBeslis,
    bestandOpenOpgeslagen,
    bestandOpgeslagenLijst,
    bestandToonOpgeslagen,
    bestandVerwijder,
    bestandZetLabel,
    bewarenInstellingen,
    mapTekst,
    volgnummer,
    type BewaarUitslag,
    type Formaat,
    type Keuze,
    type MapInfo,
    type OpgeslagenInfo,
    type VerwijderUitslag
  } from '#lib/bewaren.js';
  import { kopieerNaarKlembord } from '#lib/klembord.js';
  import type { PersoonStand, VerslagRegel } from '#lib/verslag.js';
  import { foutVak, paginaTitel, vlak } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Het transcript van één bestand (ontwerp 4-B, scherm 3, en 4-C, schermen 2 tot 4): kopiëren,
  // opslaan met de splitknop (standaardformaat, of een ander formaat of een andere plek onder
  // het pijltje), het label, de melding "Opgeslagen" met Openen en Map tonen, de vraag als de
  // naam al bestaat, en Verwijderen met bevestiging. Paden komen nooit uit de interface: Rust
  // kiest de map (instelling of mapkiezer) en onthoudt welke bestanden het zelf maakte.

  let {
    transcript,
    terug,
    verwijderd
  }: {
    transcript: BestandTranscript;
    terug: () => void;
    verwijderd: (uitslag: VerwijderUitslag) => void;
  } = $props();

  let standaard = $state<Formaat>('docx');
  let label = $state('');
  let bewaardLabel = '';
  // De bestanden die Verwijderen weghaalt, uit Rust op het moment van de bevestiging.
  let teVerwijderen = $state<OpgeslagenInfo[]>([]);
  let bezig = $state(false);
  let melding = $state<string | null>(null);
  let fout = $state<string | null>(null);
  let opgeslagen = $state<{ naam: string; map: MapInfo } | null>(null);
  let vraag = $state<{ vraag: number; naam: string; voorstel: string } | null>(null);
  let verwijderVraag = $state(false);
  let opslaanKnop = $state<HTMLElement | null>(null);
  let verwijderKnop = $state<HTMLElement | null>(null);

  const taal = $derived(huidigeTaal());

  // De regels zoals ze nu zijn: na een verbetering geeft Rust ze opnieuw, met de nieuwe tekst.
  let regels = $state<TranscriptRegel[]>([]);
  let lijst = $state<{ wachtOpBewaren: () => Promise<void> } | undefined>();
  let persoon = $state<PersoonStand>({ vervangen: false, eigen: [] });

  $effect.pre(() => {
    label = transcript.label;
    bewaardLabel = transcript.label;
    regels = transcript.regels;
    persoon = transcript.persoon;
  });

  function datumTekst(ms: number): string {
    return new Intl.DateTimeFormat(taal === 'nl' ? 'nl-NL' : 'en-GB', {
      day: 'numeric',
      month: 'short',
      year: 'numeric'
    }).format(new Date(ms));
  }

  function wis(): void {
    melding = null;
    fout = null;
    opgeslagen = null;
    vraag = null;
    verwijderVraag = false;
  }

  async function kopieer(): Promise<void> {
    // Een verbetering waar je net uit klikte, wordt eerst bewaard; anders kopieer je de oude tekst.
    await lijst?.wachtOpBewaren();
    wis();
    const gelukt = await kopieerNaarKlembord(transcriptTekst({ ...transcript, regels }));
    melding = gelukt ? t().tekstGekopieerd : t().kopierenMislukt;
  }

  /**
   * Verbetert de tekst van een regel. Wat al is opgeslagen klopt dan niet meer met het scherm,
   * dus de melding "Opgeslagen" gaat weg. Geeft de melding bij een fout, anders `null`.
   */
  async function zetRegelTekst(index: number, tekst: string): Promise<string | null> {
    try {
      regels = await bestandZetTekst(transcript.taak, index, tekst);
      wis();
      return null;
    } catch (e) {
      return foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  /**
   * Past de markeringen van persoonsgegevens aan en neemt wat Rust teruggeeft over. Wat al is
   * opgeslagen klopt dan niet meer, dus de melding "Opgeslagen" gaat weg. Geeft de melding bij een
   * fout, anders `null`.
   */
  async function wijzigPersoon(werk: () => Promise<BestandTranscript>): Promise<string | null> {
    try {
      const nieuw = await werk();
      regels = nieuw.regels;
      persoon = nieuw.persoon;
      wis();
      return null;
    } catch (e) {
      return foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  /** Bewaart het label als het veranderd is; het moet er zijn voor de naam en de gegevens. */
  async function bewaarLabel(): Promise<void> {
    const nieuw = label.trim();
    if (nieuw === bewaardLabel) return;
    await bestandZetLabel(transcript.taak, nieuw);
    bewaardLabel = nieuw;
  }

  async function verwerk(uitslag: BewaarUitslag): Promise<void> {
    if (uitslag.type === 'opgeslagen') {
      opgeslagen = { naam: uitslag.naam, map: uitslag.map };
      await tick();
      opslaanKnop?.focus();
    } else if (uitslag.type === 'bestaatAl') {
      vraag = { vraag: uitslag.vraag, naam: uitslag.naam, voorstel: uitslag.voorstel };
      await tick();
      document.getElementById('bestaat-al-eerste')?.focus();
    }
  }

  async function opslaan(formaat: Formaat | null, anderePlek: boolean): Promise<void> {
    if (bezig) return;
    await lijst?.wachtOpBewaren();
    wis();
    bezig = true;
    try {
      await bewaarLabel();
      await verwerk(await bestandBewaar(transcript.taak, formaat, anderePlek));
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
    }
  }

  async function beslis(keuze: Keuze): Promise<void> {
    if (!vraag || bezig) return;
    const open = vraag.vraag;
    vraag = null;
    bezig = true;
    try {
      await verwerk(await bestandBewaarBeslis(open, keuze));
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
    }
    if (keuze === 'annuleren') {
      await tick();
      opslaanKnop?.focus();
    }
  }

  async function openen(): Promise<void> {
    try {
      await bestandOpenOpgeslagen(transcript.taak);
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function toonMap(): Promise<void> {
    try {
      await bestandToonOpgeslagen(transcript.taak);
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function vraagVerwijderen(): Promise<void> {
    wis();
    try {
      teVerwijderen = await bestandOpgeslagenLijst(transcript.taak);
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      return;
    }
    verwijderVraag = true;
    await tick();
    document.getElementById('verwijder-annuleer')?.focus();
  }

  async function annuleerVerwijderen(): Promise<void> {
    verwijderVraag = false;
    await tick();
    verwijderKnop?.focus();
  }

  async function verwijder(): Promise<void> {
    if (bezig) return;
    bezig = true;
    try {
      const uitslag = await bestandVerwijder(transcript.taak);
      if (uitslag.weg) {
        verwijderd(uitslag);
        return;
      }
      // Een bestand was open of kon niet weg: het transcript blijft, zodat het later kan.
      verwijderVraag = false;
      const delen: string[] = [];
      if (uitslag.inGebruik.length > 0) {
        delen.push(t().verwijderInGebruik(uitslag.inGebruik.join(', ')));
      }
      if (uitslag.mislukt.length > 0) delen.push(t().verwijderMislukt(uitslag.mislukt.join(', ')));
      fout = delen.join(' ');
      if (uitslag.verwijderd.length > 0) melding = t().verwijderdMelding(uitslag.verwijderd.length);
      await tick();
      verwijderKnop?.focus();
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      verwijderVraag = false;
    } finally {
      bezig = false;
    }
  }

  onMount(() => {
    bewarenInstellingen()
      .then((instellingen) => {
        standaard = instellingen.formaat;
      })
      .catch(() => {
        standaard = 'docx';
      });
  });

  const kopRegel = 'flex flex-wrap items-end gap-3';
</script>

{#snippet foutIcoon()}
  <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
{/snippet}

<div class="flex flex-col gap-[18px]">
  <div>
    <Button variant="spook" class="-ml-2 text-klein" onclick={terug}
      ><ArrowLeft size={14} aria-hidden="true" /><span class="sr-only"
        >{t().terugNaarBestanden}</span
      ><span aria-hidden="true">{t().menuBestanden}</span></Button
    >
  </div>
  <div class={kopRegel}>
    <h1
      id="transcript-kop"
      class="{paginaTitel} min-w-0 flex-[1_1_16rem] break-words"
      tabindex="-1"
    >
      {naamZonderExtensie(transcript.naam)}
    </h1>
    <div class="ml-auto flex flex-wrap gap-2">
      <Button variant="tweede" onclick={kopieer}
        ><Copy size={16} aria-hidden="true" />{t().kopieren}</Button
      >
      <!-- De splitknop: links het standaardformaat, rechts het menu (ontwerp 4-C, scherm 2). -->
      <OpslaanKnop
        {standaard}
        {bezig}
        bind:ref={opslaanKnop}
        opslaan={(formaat: Formaat | null, anderePlek: boolean) => {
          void opslaan(formaat, anderePlek);
        }}
      />
    </div>
  </div>

  <!-- Wat er net gebeurde: kopiëren, bezig, opgeslagen, de vraag, of een fout. -->
  <p class="m-0 text-klein text-gedempt empty:hidden" role="status">
    {bezig ? t().opslaanBezig : (melding ?? '')}
  </p>
  {#if fout}
    <div class={foutVak} role="alert">
      <p class="m-0 flex items-start gap-2.5">{@render foutIcoon()}{fout}</p>
    </div>
  {/if}
  <div role="status" class="empty:hidden">
    {#if opgeslagen}
      <div class="flex flex-wrap items-center gap-2.5 rounded-groot bg-vlak px-3.5 py-2.5">
        <span class="size-2 shrink-0 rounded-full bg-gelukt" aria-hidden="true"></span>
        <span class="min-w-0 flex-1 break-words"
          ><b class="font-semibold">{t().opgeslagenKop}</b>
          {t().opgeslagenAlsIn(opgeslagen.naam, mapTekst(opgeslagen.map, t()))}</span
        >
        <span class="ml-auto flex gap-1.5">
          <Button variant="tweede" onclick={openen}>{t().opgeslagenOpenen}</Button>
          <Button variant="spook" onclick={toonMap}>{t().opgeslagenMapTonen}</Button>
        </span>
      </div>
    {/if}
  </div>
  {#if vraag}
    <div
      class="{vlak} flex flex-col gap-2.5 px-4 py-3.5"
      role="alertdialog"
      aria-labelledby="bestaat-al-kop"
      aria-describedby="bestaat-al-naam"
    >
      <b id="bestaat-al-kop" class="font-semibold">{t().bestaatAlKop}</b>
      <span id="bestaat-al-naam" class="break-words text-gedempt">{vraag.naam}</span>
      <div class="flex flex-wrap gap-2">
        <Button id="bestaat-al-eerste" onclick={() => beslis('nummer')}
          >{t().opslaanAlsNummer(volgnummer(vraag.voorstel))}</Button
        >
        <Button variant="tweede" onclick={() => beslis('vervangen')}>{t().vervangen}</Button>
        <Button variant="spook" onclick={() => beslis('annuleren')}>{t().annuleren}</Button>
      </div>
    </div>
  {/if}

  <!-- Metadata als gedempte tekst met middenpunten, niet als pilletjes. -->
  <p class="m-0 flex flex-wrap items-center gap-x-1.5 text-klein text-dim">
    <span>{duurTekst(transcript.duurMs)}</span><span aria-hidden="true">·</span>
    <span>{datumTekst(transcript.klaarOpMs)}</span><span aria-hidden="true">·</span>
    <span class="inline-flex items-center gap-1" title={t().uitgeschrevenMet(transcript.model)}
      ><ShieldCheck size={14} aria-hidden="true" />{t().lokaalVerwerkt}<span class="sr-only"
        >, {t().uitgeschrevenMet(transcript.model)}</span
      ></span
    >
  </p>

  <div class="{vlak} flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3.5">
    <div class="flex min-w-0 flex-[1_1_14rem] flex-col gap-0.5">
      <label for="transcript-label" class="m-0 text-basis leading-snug font-medium"
        >{t().labelKop}</label
      >
      <span id="transcript-label-hulp" class="m-0 text-klein text-gedempt">{t().labelHulp}</span>
    </div>
    <input
      id="transcript-label"
      type="text"
      maxlength="80"
      autocomplete="off"
      spellcheck="false"
      class="ml-auto h-9 w-full min-w-0 rounded-klein border border-rand-veld bg-verhoogd px-3 text-basis text-tekst placeholder:text-dim sm:w-[16rem]"
      placeholder={t().labelVoorbeeld}
      aria-describedby="transcript-label-hulp"
      bind:value={label}
      onchange={() => {
        bewaarLabel().catch((e: unknown) => {
          fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
        });
      }}
    />
  </div>

  {#if transcript.waarschuwing}
    <p class="m-0 flex items-start gap-2.5 text-fout-tekst" role="note">
      {@render foutIcoon()}{transcript.waarschuwing}
    </p>
  {/if}
  {#if transcript.melding}
    <p class="m-0 text-gedempt">{transcript.melding}</p>
  {/if}
  {#if regels.length === 0}
    {#if !transcript.melding}<p class="m-0 text-gedempt">{t().transcriptGeenTekst}</p>{/if}
  {:else}
    <VerslagLijst
      bind:this={lijst}
      regels={regels.map((r) => ({
        sleutel: r.index,
        startMs: r.startMs,
        eindMs: r.eindMs,
        tekst: r.tekst,
        markeringen: r.markeringen
      }))}
      labels={regels.some((r) => r.spreker)}
      lijstLabel={t().transcriptRegelsLabel}
      zetTekst={zetRegelTekst}
      {persoon}
      zetVervangen={(aan: boolean) =>
        wijzigPersoon(() => bestandPersoonVervangen(transcript.taak, aan))}
      haalWeg={(tekst: string) =>
        wijzigPersoon(() => bestandPersoonWeghalen(transcript.taak, tekst))}
      markeer={(tekst: string) =>
        wijzigPersoon(() => bestandPersoonMarkeer(transcript.taak, tekst))}
      haalAudio={transcript.audioAanwezig
        ? (beginMs: number, eindMs: number) => bestandAudioStuk(transcript.taak, beginMs, eindMs)
        : null}
      geenGeluid={t().verslagGeenGeluidBestand}
    >
      {#snippet label(regel: VerslagRegel)}
        {@const spreker = regels.find((r) => r.index === regel.sleutel)?.spreker}
        {#if spreker}<b class="font-semibold">{spreker}</b>{/if}
      {/snippet}
    </VerslagLijst>
  {/if}

  <!-- Verwijderen (ontwerp 4-B): alleen wat Mondig zelf opsloeg, met bevestiging. -->
  <div class="flex flex-col items-start gap-2.5 border-t border-lijn pt-4">
    {#if verwijderVraag}
      <div
        class="{vlak} flex w-full flex-col gap-2.5 px-4 py-3.5"
        role="alertdialog"
        aria-labelledby="verwijder-kop"
        aria-describedby="verwijder-uitleg"
      >
        <b id="verwijder-kop" class="font-semibold">{t().verwijderKop}</b>
        <span id="verwijder-uitleg" class="break-words text-gedempt"
          >{teVerwijderen.length > 0
            ? t().verwijderMetBestanden(
                teVerwijderen.map((b) => t().bestandInMap(b.naam, mapTekst(b.map, t()))).join(', ')
              )
            : t().verwijderZonderBestanden}</span
        >
        <div class="flex flex-wrap gap-2">
          <Button
            class="bg-fout-tekst text-achtergrond not-aria-disabled:hover:bg-fout-tekst/90"
            aria-disabled={bezig}
            onclick={verwijder}>{t().verwijderBevestig}</Button
          >
          <Button id="verwijder-annuleer" variant="spook" onclick={annuleerVerwijderen}
            >{t().annuleren}</Button
          >
        </div>
      </div>
    {:else}
      <Button
        variant="spook"
        class="-ml-2 text-fout-tekst"
        bind:ref={verwijderKnop}
        onclick={vraagVerwijderen}>{t().verwijderenKnop}</Button
      >
    {/if}
  </div>
</div>
