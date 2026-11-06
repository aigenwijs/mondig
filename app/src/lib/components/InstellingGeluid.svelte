<script lang="ts">
  import { onMount } from 'svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import { geluidStand } from '#lib/geluidstand.svelte.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // Het geluidje bij starten en stoppen (sprint 3d): aan of uit, direct bewaard. Het volume
  // staat achter "Meer opties" (InstellingGeluidVolume). Bij aanzetten klinkt het startgeluid.

  onMount(() => {
    geluidStand.laad();
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop kopId="geluid-kop" label={t().geluidKop} hulpId="geluid-hulp" hulp={t().geluidHulp} />
    {#if geluidStand.fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{geluidStand.fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt empty:hidden" role="status">
      {geluidStand.bewaard ? t().opgeslagen : ''}
    </p>
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={geluidStand.instelling.aan}
      wijzig={(aan: boolean) =>
        void geluidStand.bewaar({ ...geluidStand.instelling, aan }, aan, t().somsGingErIetsMis)}
      labelledby="geluid-kop"
      describedby="geluid-hulp"
    />
  </div>
</div>
