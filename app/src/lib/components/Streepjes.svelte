<script lang="ts">
  import { METER_STREEPJES } from '#lib/meter.js';

  // De streepjes van een niveaumeter, alleen om te tekenen. Eén meterstijl voor het hele
  // programma: tien streepjes bij een keuze, twintig (`lang`) tijdens het opnemen en testen.
  // Een stille of bezette microfoon geeft `aan = 0`: een lege meter.
  interface Props {
    /** Hoeveel streepjes aan staan. */
    aan: number;
    /** Het aantal streepjes; 20 geeft de langere, hogere meter. */
    aantal?: number;
  }
  let { aan, aantal = METER_STREEPJES }: Props = $props();

  const nummers = $derived(Array.from({ length: aantal }, (_x, i) => i));
  const lang = $derived(aantal > METER_STREEPJES);
</script>

<span class="inline-flex flex-none items-center gap-0.5" aria-hidden="true">
  {#each nummers as i (i)}
    <i
      class="block rounded-[1px] {lang ? 'h-[18px] w-1.5 max-[639px]:w-1' : 'h-3.5 w-1'} {i < aan
        ? 'bg-actie'
        : 'bg-meter-leeg'}"
    ></i>
  {/each}
</span>
