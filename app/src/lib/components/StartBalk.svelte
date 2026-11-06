<script lang="ts">
  import type { Snippet } from 'svelte';

  // De balk met "Opname starten": direct onder de apparaten, vóór "Nog uit te schrijven" en
  // "Eerder opgenomen" (gebruikerstest na N-C). Een gewone balk in de pagina, niet zwevend: zo staat
  // de knop waar je kijkt als je wilt beginnen, en komt hij in de tabvolgorde vóór de lijsten.
  // Op een smal venster vult de knop de breedte. De balk is alleen de houder.
  let { children }: { children: Snippet } = $props();
</script>

<div data-startbalk="" class="flex items-center gap-3.5">
  {@render children()}
</div>
