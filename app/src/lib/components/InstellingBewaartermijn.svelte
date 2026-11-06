<script lang="ts">
  import { onMount } from 'svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import { t } from '#lib';
  import {
    BEWAAR_DAGEN,
    STANDAARD_BEWAAR_DAGEN,
    bewaartermijnInstelling,
    bewaartermijnZet,
    type BewaarDagen
  } from '#lib/vergadering.js';
  import { rij, rijBediening, rijLabel, rijNaam, rijUitleg } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Hoe lang een vergaderopname en haar verslag in de tijdelijke map blijven staan voor Mondig ze
  // wist (ADR 0008, aanvulling 4b-C, en ADR 0022). Direct bewaard, zonder "Opslaan".

  let waarde = $state<BewaarDagen>(STANDAARD_BEWAAR_DAGEN);
  let bewaard = $state(false);
  let fout = $state<string | null>(null);

  function label(dagen: BewaarDagen): string {
    switch (dagen) {
      case 1:
        return t().bewaar1;
      case 7:
        return t().bewaar7;
      case 30:
        return t().bewaar30;
    }
  }

  async function wijzig(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const gekozen = BEWAAR_DAGEN.find((d) => String(d) === event.currentTarget.value);
    if (gekozen === undefined) return;
    bewaard = false;
    fout = null;
    try {
      await bewaartermijnZet(gekozen);
      waarde = gekozen;
      bewaard = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      // Het menu laat weer zien wat echt geldt.
      event.currentTarget.value = String(waarde);
    }
  }

  onMount(() => {
    bewaartermijnInstelling()
      .then((dagen) => {
        waarde = dagen;
      })
      .catch((e: unknown) => {
        // Onleesbaar: Mondig wist dan niets. Wie een bewaartijd kiest, herstelt de instelling.
        waarde = STANDAARD_BEWAAR_DAGEN;
        fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      });
  });
</script>

<div id="bewaartermijn" class={rij}>
  <div class={rijLabel}>
    <h3 id="bewaar-kop" class={rijNaam}>{t().bewaarKop}</h3>
    <span id="bewaar-hulp" class={rijUitleg}>{t().bewaarHulp}</span>
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
  </div>
  <div class={rijBediening}>
    <Keuze
      aria-labelledby="bewaar-kop"
      aria-describedby="bewaar-hulp"
      value={String(waarde)}
      onchange={wijzig}
    >
      {#each BEWAAR_DAGEN as dagen (dagen)}
        <option value={String(dagen)}>{label(dagen)}</option>
      {/each}
    </Keuze>
  </div>
</div>
