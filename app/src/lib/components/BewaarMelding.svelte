<script lang="ts">
  import { t } from '#lib';
  import { bewaarStand, type BewaarRij } from '#lib/bewarenstand.svelte.js';

  // "Opgeslagen." of de fout, onder de rij die het laatst wijzigde.
  let { rij }: { rij: BewaarRij } = $props();
</script>

{#if bewaarStand.fout && bewaarStand.waar === rij}
  <p class="m-0 text-klein text-fout-tekst" role="alert">{bewaarStand.fout}</p>
{/if}
<p class="m-0 text-klein text-gelukt empty:hidden" role="status">
  {bewaarStand.bewaard && bewaarStand.waar === rij ? t().opgeslagen : ''}
</p>
