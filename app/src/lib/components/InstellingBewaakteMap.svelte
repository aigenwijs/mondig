<script lang="ts">
  import { onMount } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Folder from '@lucide/svelte/icons/folder';
  import BewaakMelding from '#lib/components/BewaakMelding.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { t } from '#lib';
  import { probleemTekst } from '#lib/bewaakte-map.js';
  import { bewaakStand } from '#lib/bewaakstand.svelte.js';
  import { mapTekst } from '#lib/bewaren.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';

  // De vaste rijen van de bewaakte map (B-1): aan of uit en de map. De rest staat achter "Meer
  // opties" (InstellingBewaakteMapMeer). Aanzetten zonder map opent eerst de mapkiezer, die
  // Rust opent; de interface geeft nooit een pad door.

  const stand = $derived(bewaakStand.stand);
  const kaartTekst = $derived(stand?.map ? mapTekst(stand.map, t()) : t().bewaakMapGeen);
  const probleem = $derived(probleemTekst(stand?.probleem ?? null, t()));

  onMount(() => {
    bewaakStand.laad(t().somsGingErIetsMis);
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="bewaak-aan-kop"
      label={t().bewaakAanKop}
      hulpId="bewaak-aan-hulp"
      hulp={t().bewaakAanHulp}
    />
    <BewaakMelding rij="aan" />
    {#if probleem}
      <p class="m-0 text-klein text-fout-tekst" role="status">{probleem}</p>
    {/if}
    {#if stand?.melding}
      <p class="m-0 text-klein text-fout-tekst" role="status">{stand.melding}</p>
    {/if}
  </div>
  <div class={rijBediening}>
    <Schakelaar
      aan={stand?.aan ?? false}
      wijzig={(aan: boolean) => void bewaakStand.zetAan(aan, t().somsGingErIetsMis)}
      labelledby="bewaak-aan-kop"
      describedby="bewaak-aan-hulp"
      bestuurd
    />
  </div>
</div>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="bewaak-map-kop"
      label={t().bewaakMapKop}
      hulpId="bewaak-map-hulp"
      hulp={t().bewaakMapHulp}
    />
    <BewaakMelding rij="map" />
  </div>
  <div class={rijBediening}>
    <button
      type="button"
      class="inline-flex h-9 w-full min-w-0 cursor-pointer items-center justify-between gap-2.5 rounded-klein border border-rand-veld bg-verhoogd pr-3 pl-3 text-basis text-tekst transition-colors hover:border-actie"
      aria-label={t().bewaakMapKiezenAria(kaartTekst)}
      aria-describedby="bewaak-map-hulp"
      aria-disabled={bewaakStand.kiezen}
      title={stand?.map?.pad}
      onclick={() => void bewaakStand.kiesMap(t().somsGingErIetsMis)}
    >
      <span class="flex min-w-0 items-center gap-2">
        <Folder size={16} class="shrink-0 text-gedempt" aria-hidden="true" />
        <span class="truncate">{kaartTekst}</span>
      </span>
      <ChevronDown size={16} class="shrink-0 text-gedempt" aria-hidden="true" />
    </button>
  </div>
</div>
