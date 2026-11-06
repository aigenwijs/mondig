<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { vlak } from '#lib/stijl.js';

  // De vraag "Waar gaat de tekst heen?" (ADR 0024 punt 5, ADR 0027 punt 5): een kaart met de
  // uitleg, een knop om door te gaan en Annuleren. Voor notulen maken en voor dictaten opschonen;
  // wat er daarna gebeurt, beslist de ouder (en Rust controleert het nog eens).

  interface Props {
    /** Voorvoegsel van de ids, zodat de schermlezer kop en uitleg vindt. */
    id: string;
    tekst: string;
    knop: string;
    bezig?: boolean;
    /** Wat er nog onder de uitleg moet, zoals de vraag over overschrijven. */
    extra?: Snippet;
    klas?: string;
    bijBevestig: () => void;
    bijAnnuleer: () => void;
  }
  let {
    id,
    tekst,
    knop,
    bezig = false,
    extra,
    klas = '',
    bijBevestig,
    bijAnnuleer
  }: Props = $props();
</script>

<div
  class="{vlak} flex flex-col gap-2.5 px-4 py-3.5 {klas}"
  role="alertdialog"
  aria-labelledby="{id}-kop"
  aria-describedby="{id}-tekst"
>
  <b id="{id}-kop" class="font-semibold">{t().notulenVraagKop}</b>
  <span id="{id}-tekst" class="text-gedempt">{tekst}</span>
  {#if extra}{@render extra()}{/if}
  <div class="flex flex-wrap gap-2">
    <Button id="{id}-ja" aria-disabled={bezig} onclick={bijBevestig}>{knop}</Button>
    <Button id="{id}-annuleer" variant="spook" onclick={bijAnnuleer}>{t().annuleren}</Button>
  </div>
</div>
