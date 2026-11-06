<script lang="ts">
  import { onMount } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import Keuze from '#lib/components/Keuze.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Vinkvak from '#lib/components/Vinkvak.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    MISTRAL_NAUWKEURIG,
    MISTRAL_SNEL,
    bewaarSleutel,
    notulenInstellingen,
    notulenOllamaStand,
    notulenSleutelTest,
    notulenSleutelWis,
    notulenSleutelZet,
    notulenZetAanbieder,
    notulenZetModel,
    notulenZetTraining,
    type NotulenAanbieder,
    type NotulenInstellingen,
    type OllamaStand
  } from '#lib/notulen.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // De groep Notulen in Instellingen (T3, ADR 0024 en 0025): waar de tekst van een verslag heen
  // gaat, bij Mistral de sleutel (gemaskeerd; na bewaren alleen "Opgeslagen", met Testen en
  // Wissen), het vinkje dat training uit staat met uitleg, en het model. Bij Ollama de stand van
  // Ollama. De sleutel gaat alleen van dit veld naar Rust en komt nooit terug; na bewaren is het
  // veld leeg. Alles wordt direct toegepast, er is geen Opslaan.

  let { begin = null }: { begin?: NotulenInstellingen | null } = $props();

  // `begin` is alleen de beginstand (en voor de tests); daarna beslist Rust.
  // svelte-ignore state_referenced_locally
  let i = $state<NotulenInstellingen | null>(begin);
  let ollama = $state<OllamaStand | null>(null);
  let ollamaKijkt = $state(false);
  let sleutel = $state('');
  let nieuweSleutel = $state(false);
  let bezig = $state(false);
  let fout = $state<string | null>(null);
  let melding = $state<string | null>(null);
  let testen = $state(false);
  let uitlegOpen = $state(false);

  const toon = (e: unknown): string => foutUitCommando(e, t().somsGingErIetsMis).melding;

  async function laad(): Promise<void> {
    try {
      i = await notulenInstellingen();
    } catch (e) {
      fout = toon(e);
    }
  }

  // Elke vraag aan Ollama krijgt een nummer: een oudere uitslag mag een nieuwere nooit overschrijven.
  let ollamaTeller = 0;

  async function kijkNaarOllama(): Promise<void> {
    const mijn = ++ollamaTeller;
    ollamaKijkt = true;
    let uitslag: OllamaStand;
    try {
      uitslag = await notulenOllamaStand();
    } catch (e) {
      uitslag = { draait: false, modellen: [], melding: toon(e) };
    }
    if (mijn !== ollamaTeller) return;
    ollama = uitslag;
    ollamaKijkt = false;
  }

  /** Voert een wijziging uit en neemt de nieuwe instellingen over. */
  async function voer(werk: () => Promise<NotulenInstellingen>, goed?: string): Promise<boolean> {
    if (bezig) return false;
    bezig = true;
    fout = null;
    melding = null;
    try {
      i = await werk();
      melding = goed ?? null;
      return true;
    } catch (e) {
      // Na een mislukte wijziging toont het scherm weer wat er echt in Rust staat.
      const melding = toon(e);
      await laad();
      fout = melding;
      return false;
    } finally {
      bezig = false;
    }
  }

  /** Zet een keuzelijst weer op wat in Rust staat, ook als de gebruiker er iets anders in koos. */
  const synchroniseer = (lijst: HTMLSelectElement, waarde: string): void => {
    lijst.value = waarde;
  };

  async function kiesAanbieder(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const lijst = event.currentTarget;
    const waarde = lijst.value;
    const aanbieder: NotulenAanbieder | null =
      waarde === 'mistral' || waarde === 'ollama' ? waarde : null;
    await voer(() => notulenZetAanbieder(aanbieder));
    synchroniseer(lijst, i?.aanbieder ?? '');
  }

  async function kiesMistralModel(
    event: Event & { currentTarget: HTMLSelectElement }
  ): Promise<void> {
    const lijst = event.currentTarget;
    await voer(() => notulenZetModel('mistral', lijst.value));
    synchroniseer(lijst, i?.mistralModel === MISTRAL_SNEL ? MISTRAL_SNEL : MISTRAL_NAUWKEURIG);
  }

  async function kiesOllamaModel(
    event: Event & { currentTarget: HTMLSelectElement }
  ): Promise<void> {
    const lijst = event.currentTarget;
    if (lijst.value === '') return;
    await voer(() => notulenZetModel('ollama', lijst.value));
    synchroniseer(lijst, i?.ollamaModel ?? '');
  }

  async function bewaar(): Promise<void> {
    if (bezig || sleutel.trim() === '') return;
    bezig = true;
    fout = null;
    melding = null;
    const uitslag = await bewaarSleutel(sleutel, notulenSleutelZet);
    // Na bewaren is het veld leeg; bij een fout blijft de tekst staan om te verbeteren.
    sleutel = uitslag.veld;
    if (uitslag.instellingen) {
      i = uitslag.instellingen;
      nieuweSleutel = false;
      melding = t().notulenSleutelOpgeslagen;
    } else {
      fout = toon(uitslag.fout);
    }
    bezig = false;
  }

  async function test(): Promise<void> {
    if (bezig || testen) return;
    testen = true;
    fout = null;
    melding = null;
    try {
      await notulenSleutelTest();
      melding = t().notulenSleutelTestOk;
    } catch (e) {
      fout = toon(e);
    } finally {
      testen = false;
    }
  }

  async function wis(): Promise<void> {
    if (await voer(() => notulenSleutelWis(), t().notulenSleutelGewist)) nieuweSleutel = false;
  }

  onMount(() => {
    if (i === null) void laad();
  });

  // Bij het laden van de instellingen kan Ollama de gekozen aanbieder zijn.
  $effect(() => {
    if (i?.aanbieder === 'ollama' && ollama === null && !ollamaKijkt) void kijkNaarOllama();
  });

  const waarde = $derived(i?.aanbieder ?? '');
  const invoerKlasse =
    'h-9 w-full min-w-0 rounded-klein border border-rand-veld bg-verhoogd px-3 text-basis text-tekst sm:w-[18rem]';
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="notulen-kop"
      label={t().notulenKop}
      hulpId="notulen-hulp"
      hulp={t().notulenHulp}
    />
  </div>
  <div class={rijBediening}>
    <Keuze
      breedte="sm:w-[16rem]"
      aria-labelledby="notulen-kop"
      aria-describedby="notulen-hulp"
      value={waarde}
      disabled={i === null || bezig}
      onchange={kiesAanbieder}
    >
      <option value="">{t().notulenKeuzeGeen}</option>
      <option value="mistral">{t().notulenKeuzeMistral}</option>
      <option value="ollama">{t().notulenKeuzeOllama}</option>
    </Keuze>
  </div>
</div>

{#if i?.aanbieder === 'mistral'}
  <div class={rij}>
    <div class={rijLabel}>
      <RijKop
        kopId="notulen-sleutel-kop"
        label={t().notulenSleutelKop}
        hulpId="notulen-sleutel-hulp"
        hulp={t().notulenSleutelHulp}
      />
      {#if i.sleutelFout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{i.sleutelFout}</p>
      {/if}
    </div>
    <div class={rijBediening}>
      {#if i.sleutelStaat && !nieuweSleutel}
        <span class="inline-flex items-center gap-1.5 text-basis text-gedempt">
          <Check size={16} class="text-gelukt" aria-hidden="true" />{t().notulenSleutelOpgeslagen}
        </span>
        <Button variant="tweede" aria-disabled={testen || bezig} onclick={() => void test()}
          >{testen ? t().notulenSleutelTestBezig : t().notulenSleutelTesten}</Button
        >
        <Button
          variant="spook"
          aria-disabled={bezig}
          onclick={() => {
            nieuweSleutel = true;
          }}>{t().notulenSleutelVervangen}</Button
        >
        <Button variant="spook" aria-disabled={bezig} onclick={() => void wis()}
          >{t().notulenSleutelWissen}</Button
        >
      {:else}
        <!-- Gemaskeerd: de sleutel staat nooit leesbaar op het scherm. -->
        <input
          type="password"
          class={invoerKlasse}
          bind:value={sleutel}
          disabled={bezig}
          autocomplete="off"
          spellcheck="false"
          placeholder={t().notulenSleutelPlaats}
          aria-label={t().notulenSleutelVeld}
          aria-describedby="notulen-sleutel-hulp"
          onkeydown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void bewaar();
            }
          }}
        />
        <Button aria-disabled={bezig || sleutel.trim() === ''} onclick={() => void bewaar()}
          >{t().notulenSleutelBewaren}</Button
        >
        {#if i.sleutelStaat}
          <Button
            variant="spook"
            onclick={() => {
              nieuweSleutel = false;
              sleutel = '';
            }}>{t().annuleren}</Button
          >
        {/if}
      {/if}
    </div>
  </div>

  <div class="flex flex-col gap-1 px-4 py-3">
    <Vinkvak
      bind:checked={
        () => i?.trainingUit ?? false, (uit: boolean) => void voer(() => notulenZetTraining(uit))
      }
      disabled={bezig}
    >
      <b class="font-medium">{t().notulenTrainingVinkje}</b>
    </Vinkvak>
    <div class="ml-[30px]">
      <button
        type="button"
        class="min-h-8 cursor-pointer text-klein text-gedempt underline underline-offset-[3px] hover:text-tekst"
        aria-expanded={uitlegOpen}
        aria-controls="notulen-training-uitleg"
        onclick={() => {
          uitlegOpen = !uitlegOpen;
        }}>{t().notulenTrainingWaarom}</button
      >
      {#if uitlegOpen}
        <p id="notulen-training-uitleg" class="m-0 mt-1 text-klein text-gedempt">
          {t().notulenTrainingUitleg}
        </p>
      {/if}
    </div>
  </div>

  <div class={rij}>
    <div class={rijLabel}>
      <RijKop kopId="notulen-model-kop" label={t().notulenModelKop} />
    </div>
    <div class={rijBediening}>
      <Keuze
        breedte="sm:w-[16rem]"
        aria-labelledby="notulen-model-kop"
        value={i.mistralModel === MISTRAL_SNEL ? MISTRAL_SNEL : MISTRAL_NAUWKEURIG}
        disabled={bezig}
        onchange={(event: Event & { currentTarget: HTMLSelectElement }) =>
          void kiesMistralModel(event)}
      >
        <option value={MISTRAL_NAUWKEURIG}>{t().notulenModelNauwkeurig}</option>
        <option value={MISTRAL_SNEL}>{t().notulenModelSnel}</option>
      </Keuze>
    </div>
  </div>
{:else if i?.aanbieder === 'ollama'}
  <div class={rij}>
    <div class={rijLabel}>
      <RijKop
        kopId="notulen-ollama-kop"
        label={t().notulenOllamaModelKop}
        hulpId="notulen-ollama-hulp"
        hulp={t().notulenOllamaDuur}
      />
      <p class="m-0 text-klein text-gedempt" role="status">
        {#if ollamaKijkt || ollama === null}
          {t().notulenOllamaKijkt}
        {:else if !ollama.draait}
          {ollama.melding ?? t().notulenOllamaDraaitNiet}
        {:else if ollama.modellen.length === 0}
          {t().notulenOllamaGeenModellen}
        {:else}
          {t().notulenOllamaDraait}
        {/if}
      </p>
    </div>
    <div class={rijBediening}>
      {#if ollama?.draait && ollama.modellen.length > 0}
        <Keuze
          breedte="sm:w-[16rem]"
          aria-labelledby="notulen-ollama-kop"
          aria-describedby="notulen-ollama-hulp"
          value={i.ollamaModel ?? ''}
          disabled={bezig}
          onchange={(event: Event & { currentTarget: HTMLSelectElement }) =>
            void kiesOllamaModel(event)}
        >
          {#if i.ollamaModel === null}
            <option value="">{t().notulenOllamaKies}</option>
          {/if}
          {#each ollama.modellen as model (model)}
            <option value={model}>{model}</option>
          {/each}
        </Keuze>
      {/if}
      <Button variant="tweede" aria-disabled={ollamaKijkt} onclick={() => void kijkNaarOllama()}
        >{t().notulenOllamaOpnieuwKijken}</Button
      >
    </div>
  </div>
{/if}

<div class="empty:hidden" role="status">
  {#if fout}
    <p class="m-0 px-4 py-2.5 text-klein text-fout-tekst" role="alert">{fout}</p>
  {:else if melding}
    <p class="m-0 px-4 py-2.5 text-klein text-gelukt">{melding}</p>
  {/if}
</div>
