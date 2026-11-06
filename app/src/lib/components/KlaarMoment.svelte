<script lang="ts">
  import { untrack } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { startKlaarTimer } from '#lib/verslagkop.js';

  // Het klaar-moment (N-C1, ontwerp 3): direct na het uitschrijven een strook met een punt die
  // uitgroeit tot een vinkje, de duur van het gesprek en dat alles op deze computer bleef. Weg
  // na zes seconden of met het kruisje. Zonder geluid; de schermlezer krijgt "Het verslag is
  // klaar" van de pagina zelf. Bij `prefers-reduced-motion` staat er alleen het vinkje.

  let { zin, sluit }: { zin: string; sluit: () => void } = $props();

  $effect(() => untrack(() => startKlaarTimer(sluit)));
</script>

<div
  class="flex items-center gap-3.5 rounded-groot border border-lijn bg-vlak px-4 py-3.5"
  data-testid="klaarmoment"
>
  <span
    class="flex size-[30px] shrink-0 items-center justify-center rounded-full bg-actie text-op-actie punt"
    aria-hidden="true"
  >
    <Check size={18} strokeWidth={3} class="vink" />
  </span>
  <div class="flex min-w-0 flex-1 flex-col">
    <b class="font-kop font-semibold">{t().klaarKop}</b>
    <span class="text-klein text-gedempt">{zin}</span>
  </div>
  <Button
    variant="spook"
    size="icoon"
    title={t().vergaderMeldingSluiten}
    aria-label={t().vergaderMeldingSluiten}
    onclick={sluit}><X size={16} aria-hidden="true" /></Button
  >
</div>

<style>
  /* Alleen met beweging: de punt groeit uit, daarna verschijnt het vinkje. */
  @media (prefers-reduced-motion: no-preference) {
    .punt {
      animation: groei 0.6s cubic-bezier(0.16, 1, 0.3, 1);
    }
    .punt :global(.vink) {
      animation: vink 0.5s 0.35s both;
    }
  }
  @keyframes groei {
    from {
      transform: scale(0.25);
    }
  }
  @keyframes vink {
    from {
      opacity: 0;
      transform: scale(0.4);
    }
  }
</style>
