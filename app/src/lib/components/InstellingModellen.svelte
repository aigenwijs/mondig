<script lang="ts">
  import { resolve } from '$app/paths';
  import { Button } from '#lib/components/ui/button/index.js';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // Eén verwijsrij naar de pagina Modellen (sprint 5-A): de keuze zelf staat op één plek.
  // Rollen in plaats van namen: de namen staan op Modellen.
  const overzicht = $derived(modellenStand.overzicht);
  const hulp = $derived(
    overzicht
      ? t().instellingModellenHulp(overzicht.dicteren !== overzicht.bestanden)
      : t().instellingModellenKies
  );
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop kopId="modellen-kop" label={t().instellingModellenKop} hulpId="modellen-hulp" {hulp} />
  </div>
  <div class={rijBediening}>
    <Button
      variant="tweede"
      href={resolve('/modellen')}
      aria-describedby="modellen-kop modellen-hulp">{t().instellingModellenKnop}</Button
    >
  </div>
</div>
