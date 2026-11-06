<script lang="ts">
  import { onMount } from 'svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import { geluidStand } from '#lib/geluidstand.svelte.js';
  import { volumeVeilig } from '#lib/instellingen.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // Het volume van het geluidje, achter "Meer opties". Na het schuiven klinkt het startgeluid,
  // zodat je hoort hoe hard het is.

  function schuif(event: Event & { currentTarget: HTMLInputElement }): void {
    const volume = volumeVeilig(Number(event.currentTarget.value));
    void geluidStand.bewaar({ ...geluidStand.instelling, volume }, true, t().somsGingErIetsMis);
  }

  onMount(() => {
    geluidStand.laad();
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop kopId="volume-kop" label={t().geluidVolumeKop} />
  </div>
  <div class={rijBediening}>
    <input
      type="range"
      min="0"
      max="100"
      step="5"
      class="h-1.5 w-40 cursor-pointer accent-[var(--actie)] disabled:cursor-default disabled:opacity-50"
      value={geluidStand.instelling.volume}
      disabled={!geluidStand.instelling.aan}
      aria-labelledby="volume-kop"
      aria-valuetext="{String(geluidStand.instelling.volume)}%"
      onchange={schuif}
    />
  </div>
</div>
