<script lang="ts">
  import { onMount } from 'svelte';
  import BewaakMelding from '#lib/components/BewaakMelding.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { t } from '#lib';
  import { bewaakStand } from '#lib/bewaakstand.svelte.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // De rijen van de bewaakte map achter "Meer opties": submappen, overslaan als er al een
  // transcript is, en het transcript naast de opname.

  const stand = $derived(bewaakStand.stand);

  onMount(() => {
    bewaakStand.laad(t().somsGingErIetsMis);
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="bewaak-submappen-kop"
      label={t().bewaakSubmappenKop}
      hulpId="bewaak-submappen-hulp"
      hulp={t().bewaakSubmappenHulp}
    />
    <BewaakMelding rij="submappen" />
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.submappen ?? false}
      wijzig={(aan: boolean) => void bewaakStand.zetSubmappen(aan, t().somsGingErIetsMis)}
      labelledby="bewaak-submappen-kop"
      describedby="bewaak-submappen-hulp"
    />
  </div>
</div>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="bewaak-overslaan-kop"
      label={t().bewaakOverslaanKop}
      hulpId="bewaak-overslaan-hulp"
      hulp={t().bewaakOverslaanHulp}
    />
    <BewaakMelding rij="overslaan" />
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.overslaan ?? true}
      wijzig={(aan: boolean) => void bewaakStand.zetOverslaan(aan, t().somsGingErIetsMis)}
      labelledby="bewaak-overslaan-kop"
      describedby="bewaak-overslaan-hulp"
    />
  </div>
</div>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="bewaak-naast-kop"
      label={t().bewaakNaastKop}
      hulpId="bewaak-naast-hulp"
      hulp={t().bewaakNaastHulp}
    />
    <BewaakMelding rij="naast" />
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.naast ?? false}
      wijzig={(aan: boolean) => void bewaakStand.zetNaast(aan, t().somsGingErIetsMis)}
      labelledby="bewaak-naast-kop"
      describedby="bewaak-naast-hulp"
    />
  </div>
</div>
