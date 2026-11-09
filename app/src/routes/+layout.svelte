<script lang="ts">
  import '../app.css';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import EersteStart from '#lib/components/EersteStart.svelte';
  import DictaatStrook from '#lib/components/DictaatStrook.svelte';
  import Zijbalk from '#lib/components/Zijbalk.svelte';
  import { wizardZichtbaar } from '#lib/eerstestart.js';
  import { startDictaatStand } from '#lib/dictaatstand.svelte.js';
  import { startDictatenStand } from '#lib/dictatenstand.svelte.js';
  import { modellenStand, startModellenStand } from '#lib/modellenstand.svelte.js';
  import { sprekers } from '#lib/sprekers.svelte.js';
  import { sprekerlabels, vergaderingZetLabels } from '#lib/vergadering.js';
  import { t } from '#lib';
  import { luisterNaarNavigatie } from '#lib/gebeurtenissen.js';
  import { bekendeRoute } from '#lib/navigatie.js';
  import type { LayoutProps } from './$types';

  let { children }: LayoutProps = $props();

  // De zwevende pil (sprint 3d) is een eigen venster op deze route: zonder zijbalk en strook.
  const isPil = $derived(page.url.pathname === '/pil');

  // De eerste-start-wizard: alleen als er geen spraakmodel staat, en daarna tot de gebruiker klaar is
  // of kiest voor "Nu niet". Eenmaal getoond blijft hij staan, ook als de download het model compleet maakt.
  let wizardGezien = $state(false);
  let wizardAfgesloten = $state(false);
  const toonWizard = $derived(
    !isPil && wizardZichtbaar(modellenStand.overzicht, wizardGezien, wizardAfgesloten)
  );
  $effect(() => {
    if (toonWizard) wizardGezien = true;
  });

  // "Sprekers herkennen" (4b-D): staat het sprekermodel er helemaal, dan gaat de schakelaar aan
  // waar de gebruiker om vroeg. Dat blijft kloppen als hij intussen naar een ander scherm ging.
  $effect(() => {
    void sprekers.volg();
  });

  // Kopiëren en Opslaan gebruiken de sprekernamen in de taal van de app ("You", "Others", "Speaker
  // 1"). Rust kent die taal niet, dus de interface geeft ze door, bij het starten en bij een
  // taalwissel. De pil heeft geen verslag en slaat dit over.
  $effect(() => {
    if (isPil) return;
    const labels = sprekerlabels(t());
    void vergaderingZetLabels(labels).catch(() => undefined);
  });

  onMount(() => {
    void sprekers.laad();
    // De ene luisteraar naar de dictaat-events, voor paneel en strook samen.
    const stopStand = startDictaatStand();
    // De dictaatgeschiedenis (ADR 0021): termijn en bewaarde dictaten, ververst bij elk event.
    const stopDictaten = startDictatenStand();
    // De modellen en de download die loopt, voor de zijbalk en de pagina Modellen.
    const stopModellen = startModellenStand();
    // Het menu in de systeembalk opent het venster op een bepaalde route. Alleen routes
    // die we kennen: het event is een verzoek, geen adres om blind te volgen.
    let weg = false;
    let stopNavigatie: (() => void) | null = null;
    void luisterNaarNavigatie((pad) => {
      const route = bekendeRoute(pad);
      if (route !== null) void goto(resolve(route));
    }).then((stop) => {
      if (weg) stop();
      else stopNavigatie = stop;
    });
    return () => {
      weg = true;
      stopStand();
      stopDictaten();
      stopModellen();
      stopNavigatie?.();
    };
  });
</script>

{#if isPil}
  {@render children()}
{:else if toonWizard}
  <EersteStart
    onKlaar={() => {
      wizardAfgesloten = true;
    }}
  />
{:else}
  <!-- Zijbalk links, het hoofdvlak rechts scrolt los ervan (ADR 0014). `relative` houdt
       onzichtbare tekst voor schermlezers (`sr-only`, absoluut gepositioneerd) binnen het
       vlak; anders wordt het hele venster langer en scrolt de zijbalk mee. -->
  <div class="relative flex h-dvh overflow-hidden">
    <Zijbalk />
    <div class="relative flex min-w-0 flex-1 flex-col overflow-y-auto">
      <DictaatStrook />
      <main
        class="flex w-full max-w-[760px] flex-1 flex-col gap-6 px-10 py-[30px] max-[639px]:gap-5 max-[639px]:px-5 max-[639px]:py-6"
      >
        {@render children()}
      </main>
    </div>
  </div>
{/if}
