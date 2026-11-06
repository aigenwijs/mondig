<script lang="ts">
  import { onMount } from 'svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import {
    RUSTTIJD_SLEUTELS,
    STANDAARD_RUSTTIJD,
    isRusttijdSleutel,
    rusttijdInstelling,
    rusttijdZet,
    type RusttijdSleutel
  } from '#lib/instellingen.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Hoe lang het spraakmodel na het laatste dictaat in het geheugen blijft (ADR 0012).
  // Direct toegepast: de motorbeheerder krijgt de nieuwe rusttijd zonder herstart.

  let waarde = $state<RusttijdSleutel>(STANDAARD_RUSTTIJD);
  let bewaard = $state(false);
  let fout = $state<string | null>(null);

  function label(sleutel: RusttijdSleutel): string {
    switch (sleutel) {
      case '5m':
        return t().rusttijd5m;
      case '15m':
        return t().rusttijd15m;
      case '1u':
        return t().rusttijd1u;
      case 'nooit':
        return t().rusttijdNooit;
    }
  }

  async function wijzig(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const gekozen = event.currentTarget.value;
    if (!isRusttijdSleutel(gekozen)) return;
    bewaard = false;
    fout = null;
    try {
      await rusttijdZet(gekozen);
      waarde = gekozen;
      bewaard = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      // Het menu laat weer zien wat echt geldt.
      event.currentTarget.value = waarde;
    }
  }

  onMount(() => {
    rusttijdInstelling()
      .then((sleutel) => {
        waarde = sleutel;
      })
      .catch(() => {
        waarde = STANDAARD_RUSTTIJD;
      });
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="geheugen-kop"
      label={t().geheugenKop}
      hulpId="geheugen-hulp"
      hulp={t().geheugenHulp}
    />
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
  </div>
  <div class={rijBediening}>
    <Keuze
      breedte="sm:w-[20rem]"
      aria-labelledby="geheugen-kop"
      aria-describedby="geheugen-hulp"
      value={waarde}
      onchange={wijzig}
    >
      {#each RUSTTIJD_SLEUTELS as sleutel (sleutel)}
        <option value={sleutel}>{label(sleutel)}</option>
      {/each}
    </Keuze>
  </div>
</div>
