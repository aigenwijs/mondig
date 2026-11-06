<script lang="ts">
  import { onMount } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Folder from '@lucide/svelte/icons/folder';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import BewaarMelding from '#lib/components/BewaarMelding.svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { FORMATEN, bewarenOpenMap, formaatNaam, mapTekst } from '#lib/bewaren.js';
  import { bewaarStand } from '#lib/bewarenstand.svelte.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // De vaste rijen van de groep Opslaan (bestanden en vergaderingen samen): de map en het
  // standaardformaat. De rest staat achter "Meer opties" (InstellingOpslaanMeer). Elke keuze
  // wordt meteen in Rust bewaard; de map komt uit de mapkiezer, die Rust opent.

  const stand = $derived(bewaarStand.stand);
  const kaartTekst = $derived(mapTekst(stand?.map ?? null, t()));

  function zetFormaat(event: Event & { currentTarget: HTMLSelectElement }): void {
    const waarde = event.currentTarget.value;
    const formaat = FORMATEN.find((f) => f === waarde);
    if (formaat) void bewaarStand.zetFormaat(formaat, t().somsGingErIetsMis);
  }

  // Een fout bij openen staat onder de rij Map, net als een fout bij kiezen.
  async function openMap(): Promise<void> {
    try {
      await bewarenOpenMap();
    } catch (e) {
      bewaarStand.waar = 'map';
      bewaarStand.bewaard = false;
      bewaarStand.fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  onMount(() => {
    bewaarStand.laad(t().somsGingErIetsMis);
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="opslaan-in-kop"
      label={t().opslaanInKop}
      hulpId="opslaan-in-hulp"
      hulp={t().opslaanInHulp}
    />
    <BewaarMelding rij="map" />
  </div>
  <div class={rijBediening}>
    <button
      type="button"
      class="inline-flex h-9 w-full min-w-0 cursor-pointer items-center justify-between gap-2.5 rounded-klein border border-rand-veld bg-verhoogd pr-3 pl-3 text-basis text-tekst transition-colors hover:bg-hover sm:w-[18rem]"
      aria-label={t().mapKiezenAria(kaartTekst)}
      aria-describedby="opslaan-in-hulp"
      aria-disabled={bewaarStand.kiezen}
      title={stand?.map?.pad}
      onclick={() => void bewaarStand.kiesMap(t().somsGingErIetsMis)}
    >
      <span class="flex min-w-0 items-center gap-2">
        <Folder size={16} class="shrink-0 text-gedempt" aria-hidden="true" />
        <span class="truncate">{kaartTekst}</span>
      </span>
      <ChevronDown size={16} class="shrink-0 text-gedempt" aria-hidden="true" />
    </button>
    <Button variant="tweede" aria-label={t().mapOpenenAria(kaartTekst)} onclick={openMap}
      ><FolderOpen size={16} aria-hidden="true" />{t().mapOpenenKort}</Button
    >
  </div>
</div>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="formaat-kop"
      label={t().standaardformaatKop}
      hulpId="formaat-hulp"
      hulp={t().standaardformaatHulp}
    />
    <BewaarMelding rij="formaat" />
  </div>
  <div class={rijBediening}>
    <Keuze
      aria-labelledby="formaat-kop"
      aria-describedby="formaat-hulp"
      value={stand?.formaat ?? 'docx'}
      onchange={zetFormaat}
    >
      {#each FORMATEN as formaat (formaat)}
        <option value={formaat}>{formaatNaam(formaat, t())} (.{formaat})</option>
      {/each}
    </Keuze>
  </div>
</div>
