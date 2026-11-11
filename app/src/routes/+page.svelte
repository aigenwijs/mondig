<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Package from '@lucide/svelte/icons/package';
  import { Button } from '#lib/components/ui/button/index.js';
  import DictaatPaneel from '#lib/components/DictaatPaneel.svelte';
  import NieuwKaart from '#lib/components/NieuwKaart.svelte';
  import ToegangMelding from '#lib/components/ToegangMelding.svelte';
  import { huidigeTaal, t } from '#lib';
  import { leesbareGrootte } from '#lib/grootte.js';
  import { haalModelStatus, type ModelOverzicht } from '#lib/modelstatus.js';
  import { haalSneltoetsStatus, type SneltoetsStatus } from '#lib/sneltoets.js';
  import { foutVak, paginaTitel, vlak } from '#lib/stijl.js';

  // Pas na het ophalen weten we of het model er is; tot dan tonen we geen melding.
  let model = $state<ModelOverzicht | null>(null);
  // Lukt het ophalen niet, dan zeggen we dat, in plaats van te doen alsof het model ontbreekt.
  let statusFout = $state(false);
  // De sneltoets die Mondig bij de start registreerde (of `null` buiten de app).
  let sneltoets = $state<SneltoetsStatus | null>(null);

  async function laadStatus(): Promise<void> {
    statusFout = false;
    try {
      model = await haalModelStatus();
    } catch {
      statusFout = true;
    }
  }

  onMount(() => {
    void laadStatus();
    haalSneltoetsStatus()
      .then((status) => {
        sneltoets = status;
      })
      .catch(() => {
        sneltoets = null;
      });
  });

  const modelCompleet = $derived(model?.status === 'compleet');
</script>

<svelte:head>
  <title>{t().appTitle}</title>
</svelte:head>

<h1 class={paginaTitel}>{t().homeDictate}</h1>

<NieuwKaart />

<!-- Op de Mac: uitleg als Mondig nog niet mag plakken (recht op toegankelijkheid). -->
<ToegangMelding />

{#if statusFout}
  <div class={foutVak} role="alert">
    <p class="m-0 flex items-start gap-2.5">
      <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
      {t().modelStatusFout}
    </p>
    <Button onclick={laadStatus}>{t().opnieuwControleren}</Button>
  </div>
{:else if model && model.status !== 'compleet'}
  <div
    class="{vlak} flex flex-wrap items-center justify-between gap-4 px-5 py-4 text-midden"
    role="status"
  >
    <div class="flex items-start gap-3">
      <Package size={16} class="mt-1 shrink-0 text-gedempt" aria-hidden="true" />
      <div>
        {#if model.status === 'gedeeltelijk'}
          <!-- Een deel staat er al (bijvoorbeeld Parakeet zonder Silero): alleen de rest komt binnen. -->
          <b class="mb-0.5 block font-semibold">{t().modelAanvullenKop}</b>
          <span class="text-gedempt"
            >{t().modelAanvullenTekst(
              leesbareGrootte(Math.max(0, model.grootteBytes - model.aanwezigBytes), huidigeTaal())
            )}</span
          >
        {:else}
          <b class="mb-0.5 block font-semibold">{t().modelNodigKop}</b>
          <span class="text-gedempt"
            >{t().modelNodigTekst(leesbareGrootte(model.grootteBytes, huidigeTaal()))}</span
          >
        {/if}
      </div>
    </div>
    <Button href={resolve('/modellen')}
      >{model.status === 'gedeeltelijk' ? t().verderDownloaden : t().downloaden}</Button
    >
  </div>
{/if}

<!-- Zolang de status nog niet binnen is, tonen we de rust al: meestal is het model er. -->
<DictaatPaneel {sneltoets} toonRust={model === null ? !statusFout : modelCompleet} />
