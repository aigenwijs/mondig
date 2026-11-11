<script lang="ts">
  import Users from '@lucide/svelte/icons/users';
  import { Button } from '#lib/components/ui/button/index.js';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import SprekersMelding from '#lib/components/SprekersMelding.svelte';
  import { huidigeTaal, t } from '#lib';
  import { leesbareGrootte } from '#lib/grootte.js';
  import { sprekers } from '#lib/sprekers.svelte.js';
  import { vlak } from '#lib/stijl.js';

  // De kaart "Sprekers herkennen" onder "Wat Mondig gebruikt" (gebruikerstest na N-C): dezelfde
  // instelling en dezelfde poort als in Instellingen en op Bestanden (`sprekers.svelte.ts`), met
  // de stand van het sprekermodel erbij. Geen eigen logica: aanzetten zonder model vraagt eerst
  // om de download, zoals overal.

  const model = $derived(sprekers.model);
  const naam = $derived(model ? (t().modelTekst(model.id)?.naam ?? model.naam) : '');
  // Is het sprekermodel er nog niet, dan staat hier een knop "Downloaden (139 MB)" in plaats van
  // de schakelaar (UI-check punt 26): de schakelaar zou stilletjes een download starten. Na de
  // download springt de poort (`sprekers.svelte.ts`) de schakelaar zelf aan. De grootte komt uit
  // de catalogus, min wat er al binnen is.
  const nogTeDownloaden = $derived(model !== undefined && model.status !== 'compleet');
  const grootte = $derived(
    model
      ? leesbareGrootte(Math.max(0, model.grootteBytes - model.aanwezigBytes), huidigeTaal())
      : ''
  );
  const stand = $derived.by(() => {
    if (!model) return '';
    return model.status === 'compleet' ? t().sprekersModelAanwezig : t().sprekersModelNiet;
  });
</script>

<!-- Opgebouwd als de functiekaarten ernaast: kop, waarom, rol en naam, de stand, en onderaan de
     schakelaar op de plek van Wijzigen, zodat de kop op één regel blijft en de kaarten gelijk
     lopen. -->
<div class="{vlak} flex flex-col gap-1.5 px-4 py-3.5" data-functie="sprekers">
  <span
    id="sprekers-kop"
    class="flex items-center gap-1.5 text-klein font-medium whitespace-nowrap text-gedempt"
  >
    <Users size={14} class="shrink-0" aria-hidden="true" />{t().sprekersKop}
  </span>
  <span class="text-klein text-dim">{t().functieSprekersWaarom}</span>
  {#if model}
    <span class="mt-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
      <span class="font-kop text-midden font-semibold">{t().sprekermodelNaam}</span>
      <span class="text-klein text-dim">{naam}</span>
    </span>
  {/if}
  <span id="sprekers-hulp" class="text-klein leading-snug text-gedempt">{stand}</span>
  <SprekersMelding />
  <div class="mt-auto flex min-h-8 items-center justify-end pt-1.5">
    {#if nogTeDownloaden}
      <Button
        variant="tweede"
        class="text-klein"
        aria-describedby="sprekers-hulp"
        aria-disabled={sprekers.fase === 'wacht'}
        onclick={() => {
          if (sprekers.fase !== 'wacht') void sprekers.download();
        }}>{t().sprekersDownloaden(grootte)}</Button
      >
    {:else}
      <Schakelaar
        aan={sprekers.schakelaar}
        wijzig={(aan: boolean) => void sprekers.zet(aan)}
        labelledby="sprekers-kop"
        describedby="sprekers-hulp"
        bestuurd
        disabled={sprekers.fase === 'wacht'}
      />
    {/if}
  </div>
</div>
