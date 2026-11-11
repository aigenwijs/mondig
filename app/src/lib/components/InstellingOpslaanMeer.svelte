<script lang="ts">
  import { onMount } from 'svelte';
  import BewaarMelding from '#lib/components/BewaarMelding.svelte';
  import InstellingBestandsnaam from '#lib/components/InstellingBestandsnaam.svelte';
  import InstellingKlantnamen from '#lib/components/InstellingKlantnamen.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { t } from '#lib';
  import { bewaarStand } from '#lib/bewarenstand.svelte.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // De rijen van Opslaan achter "Meer opties": de bestandsnaam, de klantnamen, de tijden in het document,
  // automatisch opslaan en de opname als geluidsbestand naast een vergaderverslag.

  const stand = $derived(bewaarStand.stand);

  onMount(() => {
    bewaarStand.laad(t().somsGingErIetsMis);
  });
</script>

<InstellingBestandsnaam />

<InstellingKlantnamen />

<div class={rij}>
  <div class={rijLabel}>
    <RijKop kopId="tijden-kop" label={t().tijdenKop} hulpId="tijden-hulp" hulp={t().tijdenHulp} />
    <BewaarMelding rij="tijden" />
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.tijden ?? true}
      wijzig={(aan: boolean) => void bewaarStand.zetTijden(aan, t().somsGingErIetsMis)}
      labelledby="tijden-kop"
      describedby="tijden-hulp"
    />
  </div>
</div>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="automatisch-kop"
      label={t().automatischKop}
      hulpId="automatisch-hulp"
      hulp={t().automatischHulp}
    />
    <BewaarMelding rij="automatisch" />
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.automatisch ?? false}
      wijzig={(aan: boolean) => void bewaarStand.zetAutomatisch(aan, t().somsGingErIetsMis)}
      labelledby="automatisch-kop"
      describedby="automatisch-hulp"
    />
  </div>
</div>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="audio-kop"
      label={t().audioNaastKop}
      hulpId="audio-hulp"
      hulp={t().audioNaastHulp}
    />
    <BewaarMelding rij="audio" />
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.audio ?? false}
      wijzig={(aan: boolean) => void bewaarStand.zetAudio(aan, t().somsGingErIetsMis)}
      labelledby="audio-kop"
      describedby="audio-hulp"
    />
  </div>
</div>
