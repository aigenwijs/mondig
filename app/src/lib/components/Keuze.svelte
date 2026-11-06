<script lang="ts">
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import type { Snippet } from 'svelte';
  import type { HTMLSelectAttributes } from 'svelte/elements';
  import { keuzeVeld } from '#lib/stijl.js';

  // Een gewone keuzelijst in de stijl van de app, met een Lucide-pijltje in plaats van het
  // pijltje van het systeem. Het gedrag blijft dat van <select>.
  let {
    breedte = 'sm:w-[18rem]',
    children,
    ...rest
  }: HTMLSelectAttributes & { breedte?: string; children: Snippet } = $props();
</script>

<span class="relative inline-flex w-full {breedte}">
  <select class={keuzeVeld} {...rest}>
    {@render children()}
  </select>
  <ChevronDown
    size={16}
    class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-gedempt"
    aria-hidden="true"
  />
</span>
