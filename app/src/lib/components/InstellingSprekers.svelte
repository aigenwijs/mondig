<script lang="ts">
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import SprekersMelding from '#lib/components/SprekersMelding.svelte';
  import { huidigeTaal, t } from '#lib';
  import { leesbareGrootte } from '#lib/grootte.js';
  import { sprekers } from '#lib/sprekers.svelte.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // De schakelaar "Sprekers herkennen" (4b-D, fase D-2). Eén instelling in de store, één poort
  // (`sprekers.svelte.ts`); dit onderdeel is alleen de weergave. Aanzetten zonder sprekermodel
  // vraagt eerst om de download. De schakelaar geldt voor de volgende vergadering of het volgende
  // bestand.
  //
  // `rij`: een gewone instellingenrij. `compact`: één regel met schakelaar en een korte hulptekst
  // (Bestanden onder "Bestanden kiezen", de kaart op Modellen). De grootte van het model staat er
  // alleen bij zolang het nog gedownload moet worden.
  let { variant = 'rij' }: { variant?: 'rij' | 'compact' } = $props();

  // Zolang het overzicht van de modellen er niet is, weet Mondig niet of er iets te downloaden
  // valt: dan de neutrale tekst, zonder grootte. De grootte komt uit het overzicht (catalogus).
  const model = $derived(sprekers.model);
  const grootte = $derived(
    model && model.status !== 'compleet'
      ? leesbareGrootte(Math.max(0, model.grootteBytes - model.aanwezigBytes), huidigeTaal())
      : null
  );
  const hulp = $derived(
    grootte === null ? t().sprekersCompactHulp : t().sprekersCompactDownload(grootte)
  );
</script>

{#if variant === 'compact'}
  <div class="flex flex-col gap-2">
    <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-klein">
      <Schakelaar
        aan={sprekers.schakelaar}
        wijzig={(aan: boolean) => void sprekers.zet(aan)}
        labelledby="sprekers-kop"
        describedby="sprekers-hulp"
        bestuurd
        disabled={sprekers.fase === 'wacht'}
      />
      <span id="sprekers-kop" class="font-medium text-tekst">{t().sprekersKop}</span>
      <span id="sprekers-hulp" class="text-dim">{hulp}</span>
    </div>
    <SprekersMelding />
  </div>
{:else}
  <div class={rij}>
    <div class={rijLabel}>
      <RijKop
        kopId="sprekers-kop"
        label={t().sprekersKop}
        hulpId="sprekers-hulp"
        hulp={grootte === null ? t().sprekersHulpAanwezig : t().sprekersHulp(grootte)}
      />
    </div>
    <div class={rijBediening}>
      <Schakelaar
        aan={sprekers.schakelaar}
        wijzig={(aan: boolean) => void sprekers.zet(aan)}
        labelledby="sprekers-kop"
        describedby="sprekers-hulp"
        bestuurd
        disabled={sprekers.fase === 'wacht'}
      />
    </div>
    <SprekersMelding />
  </div>
{/if}
