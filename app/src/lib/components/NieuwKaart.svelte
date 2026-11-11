<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { allePunten, eersteZin, uitgaveVoor, voorPlatform } from '#lib/changelog.js';
  import { vlak } from '#lib/stijl.js';
  import { versieStand } from '#lib/versie.svelte.js';
  import {
    kaartBesluit,
    laatstGezienVersie,
    versieZonderAchtervoegsel,
    zetLaatstGezienVersie
  } from '#lib/versie.js';
  import { haalPlatform } from '#lib/platform.js';
  import { uitgaven } from '#lib/wijzigingen.js';

  // "Wat is er nieuw" na een update (ADR 0016): één keer per nieuwe versie, rustig, op
  // Dicteren. De laatst geziene versie staat in de store van Mondig; er is geen netwerk nodig.

  let punten = $state<string[]>([]);
  let versie = $state('');
  let toon = $state(false);

  onMount(() => {
    void (async () => {
      await versieStand.laad();
      const info = versieStand.info;
      if (!info) return;
      // Kan de store niet gelezen worden, dan tonen we liever niets dan elke keer de kaart.
      const gezien = await laatstGezienVersie().catch(() => undefined);
      if (gezien === undefined) return;
      const besluit = kaartBesluit(gezien, info.versie);
      if (besluit === 'niets') return;
      if (besluit === 'bewaar') {
        // Eerste installatie: er is niets nieuws, dus geen kaart. Wel de versie onthouden.
        void zetLaatstGezienVersie(info.versie).catch(() => undefined);
        return;
      }
      const uitgave = uitgaveVoor(uitgaven, info.versie);
      // Punten voor alleen de Mac staan niet op Windows.
      const opMac = (await haalPlatform()) === 'mac';
      const lijst = uitgave ? allePunten(voorPlatform(uitgave, opMac)) : [];
      if (lijst.length === 0) {
        // Niets te melden voor deze versie: stil onthouden, geen lege kaart.
        void zetLaatstGezienVersie(info.versie).catch(() => undefined);
        return;
      }
      versie = info.versie;
      punten = lijst;
      toon = true;
    })();
  });

  async function sluit(): Promise<void> {
    toon = false;
    try {
      await zetLaatstGezienVersie(versie);
    } catch {
      // Lukt bewaren niet, dan komt de kaart de volgende keer terug. Geen foutmelding waard.
    }
  }
</script>

{#if toon}
  <!-- Eén regel (ontwerp N-C1, `.melding.een`): een ster, de eerste zin afgekapt met "…" (de hele zin
       staat in de title), "Lees meer" en een kruisje. -->
  <section
    class="{vlak} flex items-center gap-3 py-2.5 pr-2 pl-4 text-midden"
    aria-labelledby="nieuwkaart-kop"
  >
    <Sparkles size={16} class="shrink-0 text-actie" aria-hidden="true" />
    <p
      id="nieuwkaart-kop"
      class="m-0 min-w-0 flex-1 truncate"
      title="{t().nieuwRegelKop(versieZonderAchtervoegsel(versie))} {eersteZin(punten[0] ?? '')}"
    >
      <b class="font-semibold">{t().nieuwRegelKop(versieZonderAchtervoegsel(versie))}</b>
      {eersteZin(punten[0] ?? '')}
    </p>
    <Button href={resolve('/over')} variant="spook">{t().nieuwRegelLezen}</Button>
    <Button
      variant="spook"
      class="size-8 px-0"
      aria-label={t().sluiten}
      title={t().sluiten}
      onclick={sluit}><X size={16} aria-hidden="true" /></Button
    >
  </section>
{/if}
