<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import { Button } from '#lib/components/ui/button/index.js';
  import ModelBeheerKaart from '#lib/components/ModelBeheerKaart.svelte';
  import ModelFunctieKaart from '#lib/components/ModelFunctieKaart.svelte';
  import SprekersKaart from '#lib/components/SprekersKaart.svelte';
  import { huidigeTaal, t } from '#lib';
  import { controleDatum } from '#lib/afstellen.js';
  import { controleStand } from '#lib/controlestand.svelte.js';
  import { leesbareGrootte } from '#lib/grootte.js';
  import { GEBRUIKEN, beheerIndeling } from '#lib/modellen.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { foutVak, paginaTitel, sectieKop } from '#lib/stijl.js';

  // De pagina Modellen, opnieuw ingedeeld na de gebruikerstest van N-C (voorbeeld: Handy). Bovenaan
  // "Wat Mondig gebruikt": per functie een kaart met de rol van het gekozen model en Wijzigen,
  // plus Sprekers herkennen. Erbij een regel over de laatste controle van "Mondig opnieuw
  // afstellen"; "Opnieuw controleren" start dezelfde ronde in Instellingen. Daaronder alleen
  // beheer: wat op deze computer staat en wat te downloaden is. Modelnamen staan alleen hier.

  const taal = $derived(huidigeTaal());
  const overzicht = $derived(modellenStand.overzicht);
  const beheer = $derived(beheerIndeling(overzicht?.modellen ?? []));
  const controle = $derived(controleStand.controle);

  // Wat nieuw was toen de pagina opende, blijft zo zolang je er bent; daarna is het gezien.
  let nieuweIds = $state<string[] | null>(null);
  $effect(() => {
    if (overzicht && nieuweIds === null) {
      nieuweIds = overzicht.modellen.filter((m) => m.nieuw).map((m) => m.id);
      void modellenStand.markeerGezien();
    }
  });

  // Wie hier is, ziet een fout of een onderbroken download bij het model: in de zijbalk hoeft
  // hij dan niet meer te blijven staan. Ook voor wat er gebeurt terwijl de pagina open is.
  $effect(() => {
    modellenStand.markeerBekeken();
  });

  onMount(() => {
    void modellenStand.laad();
    void controleStand.laad();
  });

  const afstellen = `${resolve('/instellingen')}#afstellen`;
</script>

<svelte:head>
  <title>{t().modellenTitel} - {t().appTitle}</title>
</svelte:head>

<h1 class={paginaTitel}>{t().modellenTitel}</h1>
<p class="-mt-2 mb-0 max-w-[560px] text-klein leading-normal text-gedempt">
  {t().modellenPaginaUitleg}
</p>

{#if modellenStand.laadFout}
  <div class={foutVak} role="alert">
    <p class="m-0 flex items-start gap-2.5">
      <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />{t().modellenLaadFout}
    </p>
    <Button onclick={() => void modellenStand.laad()}>{t().opnieuwControleren}</Button>
  </div>
{:else if !overzicht}
  <p class="m-0 text-gedempt" role="status">{t().gegevensOphalen}</p>
{:else}
  <section aria-labelledby="wat-mondig-gebruikt" class="flex flex-col gap-2">
    <div class="flex flex-col gap-0.5">
      <h2 id="wat-mondig-gebruikt" class="{sectieKop} mb-0">{t().watMondigGebruiktKop}</h2>
      {#if controleStand.geladen}
        <!-- De laatste controle: datum en de weg naar dezelfde ronde als in Instellingen. -->
        <p class="m-0 flex flex-wrap items-center gap-x-1.5 text-klein text-dim" data-controle="">
          {#if controle}
            <CircleCheck size={14} class="text-actie" aria-hidden="true" />
            <span>{t().gecontroleerdOp(controleDatum(new Date(controle.datumMs), taal))}</span>
            <span aria-hidden="true">·</span>
            <Button href={afstellen} variant="spook" class="min-h-6 px-1 text-klein"
              >{t().opnieuwControlerenComputer}</Button
            >
          {:else}
            <Button href={afstellen} variant="spook" class="min-h-6 px-1 text-klein"
              >{t().laatComputerBekijken}</Button
            >
          {/if}
        </p>
      {/if}
    </div>
    <div class="grid grid-cols-3 gap-3 max-[639px]:grid-cols-1">
      {#each GEBRUIKEN as gebruik (gebruik)}
        <ModelFunctieKaart {gebruik} />
      {/each}
      <SprekersKaart />
    </div>
  </section>

  {#each [{ sleutel: 'op', kop: t().groepOpDezeComputer, lijst: beheer.opDezeComputer }, { sleutel: 'download', kop: t().groepTeDownloaden, lijst: beheer.teDownloaden }] as groep (groep.sleutel)}
    {#if groep.lijst.length > 0}
      <section aria-labelledby="groep-{groep.sleutel}" class="flex flex-col gap-2">
        <h2 id="groep-{groep.sleutel}" class="{sectieKop} mb-0">{groep.kop}</h2>
        {#each groep.lijst as regel (regel.id)}
          <ModelBeheerKaart {regel} nieuw={nieuweIds?.includes(regel.id) ?? false} />
        {/each}
      </section>
    {/if}
  {/each}

  <p class="m-0 flex items-center gap-2 text-klein text-gedempt">
    <HardDrive size={16} aria-hidden="true" /><span
      >{t().modellenTotaalVoor}<b class="font-semibold text-tekst"
        >{leesbareGrootte(overzicht.totaalBytes, taal)}</b
      >{t().modellenTotaalNa}</span
    >
  </p>
{/if}

<!-- Voor schermlezers: alleen bij 25, 50, 75 en 100 procent een zin. De focus blijft waar hij is. -->
<div role="status" class="sr-only">{modellenStand.schermlezerZin}</div>
