<script lang="ts">
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { voortgangTekst } from '#lib/opnieuw.js';
  import type { OpnieuwStand } from '#lib/vergadering.js';

  // De balk tijdens het opnieuw uitschrijven (N-C4, ADR 0023, punt 7): wat er gebeurt, het
  // percentage en de resterende tijd, dat verbeteren weer kan als het klaar is, en Stoppen. Het
  // oude verslag blijft eronder te lezen, te kopiëren en op te slaan.

  let {
    stand,
    stop,
    bezig = false
  }: { stand: OpnieuwStand; stop: () => void; bezig?: boolean } = $props();
</script>

<div class="flex items-center gap-2.5 rounded-groot bg-vlak px-3 py-2.5 text-klein" role="status">
  <span
    class="size-3.5 shrink-0 rounded-full border-2 border-lijn border-t-actie motion-safe:animate-spin"
    aria-hidden="true"
  ></span>
  <span class="min-w-0 flex-1"
    >{voortgangTekst(stand.rol, stand.voortgang, stand.restMs, t())}
    <span class="text-dim">{t().opnieuwBezigHulp}</span></span
  >
  <Button variant="spook" aria-disabled={bezig} onclick={stop}>{t().opnieuwStoppen}</Button>
</div>
