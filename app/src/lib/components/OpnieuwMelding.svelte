<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { rolTekst } from '#lib/opnieuw.js';
  import type { NaOpnieuw } from '#lib/vergadering.js';

  // De melding na het opnieuw uitschrijven (N-C4, ADR 0023, punten 2, 4 en 5): met welk model,
  // hoeveel regels er veranderden, hoeveel eigen markeringen niet meer in de tekst staan, en
  // "Vorige versie terugzetten" (kan tot het verslag sluit of opnieuw wordt uitgeschreven, wat je
  // ook wijzigt; de pagina vraagt eerst om bevestiging).

  let {
    na,
    terug,
    bezig = false
  }: { na: NaOpnieuw; terug: () => void; bezig?: boolean } = $props();
</script>

<div
  class="flex flex-wrap items-center gap-2.5 rounded-groot border border-lijn bg-nieuw py-2.5 pr-3 pl-3.5"
  role="status"
>
  <Check size={16} class="shrink-0 text-actie" aria-hidden="true" />
  <span class="flex min-w-0 flex-1 flex-col text-klein">
    <span>{t().opnieuwKlaar(rolTekst(na.rol, t()), na.veranderd)}</span>
    {#if na.verdwenen > 0}
      <span class="text-waarsch" data-testid="opnieuw-verdwenen"
        >{t().opnieuwVerdwenen(na.verdwenen)}</span
      >
    {/if}
  </span>
  <Button variant="spook" aria-disabled={bezig} onclick={terug}>{t().opnieuwTerugzetten}</Button>
</div>
