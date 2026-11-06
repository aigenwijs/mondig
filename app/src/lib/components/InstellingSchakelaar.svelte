<script lang="ts">
  import { onMount } from 'svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { t } from '#lib';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Een instellingenrij met één aan/uit-schakelaar die direct wordt bewaard. Voor de rijen
  // die alleen "lees" en "zet" verschillen (Alleen op het klembord zetten, Stopwoorden
  // weghalen). Mislukt het bewaren, dan laat de schakelaar weer zien wat echt geldt.

  interface Props {
    /** Id van de kop; de schakelaar verwijst ernaar (`aria-labelledby`). */
    kopId: string;
    /** Id van de uitleg; de schakelaar verwijst ernaar (`aria-describedby`). */
    hulpId: string;
    /** De naam van de instelling. */
    label: string;
    /** De uitleg in gewone taal. */
    hulp: string;
    /** Leest de bewaarde stand. */
    lees: () => Promise<boolean>;
    /** Bewaart een nieuwe stand. */
    zet: (aan: boolean) => Promise<void>;
    /** De stand zolang die nog niet gelezen is of niet te lezen is. */
    standaard: boolean;
  }
  let { kopId, hulpId, label, hulp, lees, zet, standaard }: Props = $props();

  // `standaard` is bedoeld als beginwaarde; de stand zelf wordt hier bijgehouden.
  // svelte-ignore state_referenced_locally
  let aan = $state(standaard);
  let bewaard = $state(false);
  let fout = $state<string | null>(null);

  async function wijzig(nieuw: boolean): Promise<void> {
    const vorige = aan;
    aan = nieuw;
    bewaard = false;
    fout = null;
    try {
      await zet(nieuw);
      bewaard = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      aan = vorige;
    }
  }

  onMount(() => {
    lees()
      .then((waarde) => {
        aan = waarde;
      })
      .catch(() => {
        aan = standaard;
      });
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop {kopId} {label} {hulpId} {hulp} />
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
  </div>
  <div class={rijBediening}>
    <Schakelaar
      {aan}
      wijzig={(nieuw: boolean) => void wijzig(nieuw)}
      labelledby={kopId}
      describedby={hulpId}
    />
  </div>
</div>
