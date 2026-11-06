<script lang="ts">
  import { t } from '#lib';
  import { bewaakStand, type BewaakRij } from '#lib/bewaakstand.svelte.js';

  // "Opgeslagen." of de fout, onder de rij die het laatst wijzigde.
  let { rij }: { rij: BewaakRij } = $props();
</script>

{#if bewaakStand.fout && bewaakStand.waar === rij}
  <p class="m-0 text-klein text-fout-tekst" role="alert">{bewaakStand.fout}</p>
{/if}
<p class="m-0 text-klein text-gelukt empty:hidden" role="status">
  {bewaakStand.bewaard && bewaakStand.waar === rij ? t().opgeslagen : ''}
</p>
