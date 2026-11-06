<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { rij, rijBediening, rijLabel, rijNaam, rijUitleg } from '#lib/stijl.js';
  import { versieStand } from '#lib/versie.svelte.js';

  // De rij "Over Mondig" onderaan Instellingen (het patroon van Microsoft voor een
  // About-deel): naam en versie in de rij, de knop opent het scherm met wat er nieuw is,
  // licenties en privacy.

  onMount(() => {
    void versieStand.laad();
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <h3 id="over-rij-kop" class={rijNaam}>
      {versieStand.info ? `${t().appTitle} ${versieStand.info.versie}` : t().appTitle}
    </h3>
    <span id="over-rij-hulp" class={rijUitleg}>{t().overUitleg}</span>
  </div>
  <div class={rijBediening}>
    <Button href={resolve('/over')} variant="tweede" aria-describedby="over-rij-kop over-rij-hulp"
      >{t().overMondig}</Button
    >
  </div>
</div>
