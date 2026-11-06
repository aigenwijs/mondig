<script lang="ts">
  import { onMount } from 'svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { t } from '#lib';
  import {
    autostartInstelling,
    autostartZet,
    verborgenStartenInstelling,
    verborgenStartenZet
  } from '#lib/instellingen.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Starten bij aanmelden (sprint 3d), standaard uit. De stand komt uit Windows zelf; de
  // vraag bij de eerste start volgt in fase 5. Eronder, ingesprongen en alleen zichtbaar als
  // starten bij aanmelden aan staat, "Geminimaliseerd starten" (sprint 5-B): zonder
  // hoofdvenster, alleen in het systeemvak. Dat geldt alleen bij aanmelden. Zet je starten bij
  // aanmelden aan en had je dit nog nooit zelf ingesteld, dan zet Rust het mee aan; daarom
  // lezen we het daarna opnieuw.

  let aan = $state(false);
  let bewaard = $state(false);
  let fout = $state<string | null>(null);

  let verborgen = $state(false);
  let verborgenBewaard = $state(false);
  let verborgenFout = $state<string | null>(null);

  async function wijzig(nieuw: boolean): Promise<void> {
    const vorige = aan;
    aan = nieuw;
    bewaard = false;
    fout = null;
    try {
      await autostartZet(nieuw);
      bewaard = true;
      verborgen = await verborgenStartenInstelling();
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      aan = vorige;
    }
  }

  async function wijzigVerborgen(nieuw: boolean): Promise<void> {
    const vorige = verborgen;
    verborgen = nieuw;
    verborgenBewaard = false;
    verborgenFout = null;
    try {
      await verborgenStartenZet(nieuw);
      verborgenBewaard = true;
    } catch (e) {
      verborgenFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      verborgen = vorige;
    }
  }

  onMount(() => {
    autostartInstelling()
      .then((waarde) => {
        aan = waarde;
      })
      .catch(() => {
        aan = false;
      });
    verborgenStartenInstelling()
      .then((waarde) => {
        verborgen = waarde;
      })
      .catch(() => {
        verborgen = false;
      });
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="autostart-kop"
      label={t().autostartKop}
      hulpId="autostart-hulp"
      hulp={t().autostartHulp}
    />
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
  </div>
  <div class={rijBediening}>
    <Schakelaar
      {aan}
      wijzig={(nieuw: boolean) => void wijzig(nieuw)}
      labelledby="autostart-kop"
      describedby="autostart-hulp"
    />
  </div>
</div>

{#if aan}
  <div class="{rij} pl-8!">
    <div class={rijLabel}>
      <RijKop
        kopId="verborgen-kop"
        label={t().verborgenKop}
        hulpId="verborgen-hulp"
        hulp={t().verborgenHulp}
      />
      {#if verborgenFout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{verborgenFout}</p>
      {/if}
      <p class="m-0 text-klein text-gelukt" role="status">
        {verborgenBewaard ? t().opgeslagen : ''}
      </p>
    </div>
    <div class={rijBediening}>
      <Schakelaar
        aan={verborgen}
        wijzig={(nieuw: boolean) => void wijzigVerborgen(nieuw)}
        labelledby="verborgen-kop"
        describedby="verborgen-hulp"
      />
    </div>
  </div>
{/if}
