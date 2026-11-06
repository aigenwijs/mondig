<script lang="ts">
  import { onMount } from 'svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import { pilPositieInstelling, pilPositieZet, type PilPositie } from '#lib/instellingen.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Waar de zwevende pil verschijnt (sprint 3d): onder (standaard) of boven, op het scherm
  // waar je werkt. Direct bewaard; geldt de volgende keer dat de pil verschijnt.

  let waarde = $state<PilPositie>('onder');
  let bewaard = $state(false);
  let fout = $state<string | null>(null);

  async function wijzig(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const gekozen = event.currentTarget.value === 'boven' ? 'boven' : 'onder';
    bewaard = false;
    fout = null;
    try {
      await pilPositieZet(gekozen);
      waarde = gekozen;
      bewaard = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      // Het menu laat weer zien wat echt geldt.
      event.currentTarget.value = waarde;
    }
  }

  onMount(() => {
    pilPositieInstelling()
      .then((plek) => {
        waarde = plek;
      })
      .catch(() => {
        waarde = 'onder';
      });
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="positie-kop"
      label={t().positieKop}
      hulpId="positie-hulp"
      hulp={t().positieHulp}
    />
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
  </div>
  <div class={rijBediening}>
    <Keuze
      breedte="sm:w-[20rem]"
      aria-labelledby="positie-kop"
      aria-describedby="positie-hulp"
      value={waarde}
      onchange={wijzig}
    >
      <option value="onder">{t().positieOnder}</option>
      <option value="boven">{t().positieBoven}</option>
    </Keuze>
  </div>
</div>
