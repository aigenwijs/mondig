<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount, tick } from 'svelte';
  import InstellingSneltoets from '#lib/components/InstellingSneltoets.svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import WaarheenVraag from '#lib/components/WaarheenVraag.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    OPSCHOON_STANDEN,
    opschonenInstellingen,
    opschonenZetStand,
    vraagEerst,
    type OpschonenInfo,
    type OpschoonStand
  } from '#lib/opschonen.js';
  import { rij, rijBediening, rijLabel, rijNaam, rijUitleg } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // "Dictaten opschonen" (ADR 0027): Uit (standaard), Met een eigen sneltoets, of Bij elk
  // dictaat. Alleen met een taalmodel onder Notulen; zonder staat de keuzelijst uit met een
  // verwijzing. De eerste keer per aanbieder eerst de vraag waar de tekst heen gaat. Bij "Met een
  // eigen sneltoets" staat de tweede sneltoets eronder, met dezelfde bediening als de gewone.
  // Direct bewaard, zonder Opslaan; Rust controleert alles nog eens.

  let { begin = null }: { begin?: OpschonenInfo | null } = $props();

  // `begin` is alleen de beginstand (voor de tests); daarna beslist Rust.
  // svelte-ignore state_referenced_locally
  let info = $state<OpschonenInfo | null>(begin);
  let bezig = $state(false);
  let fout = $state<string | null>(null);
  let bewaard = $state(false);
  // De stand waarvoor de vraag "Waar gaat de tekst heen?" open staat.
  let wacht = $state<OpschoonStand | null>(null);

  const waarde = $derived(info?.stand ?? 'uit');
  const aanbieder = $derived(info?.aanbieder ?? null);
  const kanKiezen = $derived(aanbieder !== null);

  function keuzeLabel(stand: OpschoonStand): string {
    switch (stand) {
      case 'uit':
        return t().opschonenKeuzeUit;
      case 'eigenSneltoets':
        return t().opschonenKeuzeSneltoets;
      case 'bijElkDictaat':
        return t().opschonenKeuzeElk;
    }
  }

  // De naam en het label van de aanbieder komen uit Rust.
  const naam = $derived(info?.naam ?? '');
  const label = $derived(info?.label ?? null);
  const uitleg = $derived(
    aanbieder === null || label === null
      ? t().opschonenUitleg
      : label === 'lokaal'
        ? t().opschonenUitlegLokaal
        : t().opschonenUitlegNaar(naam, t().opschonenLabel(label))
  );

  async function laad(): Promise<void> {
    try {
      info = await opschonenInstellingen();
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function zet(stand: OpschoonStand, bevestigd: boolean): Promise<void> {
    bewaard = false;
    fout = null;
    bezig = true;
    try {
      info = await opschonenZetStand(stand, bevestigd ? aanbieder : null);
      bewaard = true;
    } catch (e) {
      const melding = foutUitCommando(e, t().somsGingErIetsMis).melding;
      await laad();
      fout = melding;
    } finally {
      bezig = false;
    }
  }

  async function wijzig(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const lijst = event.currentTarget;
    const gekozen = OPSCHOON_STANDEN.find((s) => s === lijst.value);
    if (gekozen === undefined || info === null || gekozen === waarde) return;
    // Het menu laat tot na de vraag zien wat nu geldt.
    lijst.value = waarde;
    if (vraagEerst(info, gekozen)) {
      wacht = gekozen;
      await tick();
      document.getElementById('opschonen-vraag-annuleer')?.focus();
      return;
    }
    await zet(gekozen, false);
  }

  async function bevestig(): Promise<void> {
    const doel = wacht;
    if (doel === null) return;
    wacht = null;
    await zet(doel, true);
    await tick();
    document.getElementById('opschonen-keuze')?.focus();
  }

  async function annuleer(): Promise<void> {
    wacht = null;
    await tick();
    document.getElementById('opschonen-keuze')?.focus();
  }

  onMount(() => {
    void laad();
  });
</script>

<div id="opschonen">
  <div class={rij}>
    <div class={rijLabel}>
      <h3 id="opschonen-kop" class={rijNaam}>{t().opschonenKop}</h3>
      <span id="opschonen-hulp" class={rijUitleg}>{uitleg}</span>
      {#if info !== null && !kanKiezen}
        <span class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1.5">
          <span class="text-klein text-tekst">{t().opschonenGeenAanbieder}</span>
          <Button variant="tweede" href={`${resolve('/instellingen')}#notulen`}
            >{t().opschonenNaarNotulen}</Button
          >
        </span>
      {/if}
      {#if fout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
      {/if}
      <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
    </div>
    <div class={rijBediening}>
      <Keuze
        id="opschonen-keuze"
        aria-labelledby="opschonen-kop"
        aria-describedby="opschonen-hulp"
        breedte="sm:w-[15rem]"
        value={waarde}
        disabled={bezig || !kanKiezen}
        onchange={wijzig}
      >
        {#each OPSCHOON_STANDEN as stand (stand)}
          <option value={stand}>{keuzeLabel(stand)}</option>
        {/each}
      </Keuze>
    </div>
  </div>
  {#if wacht !== null && aanbieder !== null && label !== null}
    <WaarheenVraag
      id="opschonen-vraag"
      klas="mx-4 mb-3.5 bg-hover"
      tekst={t().opschonenVraagTekst(naam, t().opschonenLabel(label))}
      knop={t().opschonenAanzetten}
      {bezig}
      bijBevestig={bevestig}
      bijAnnuleer={annuleer}
    />
  {/if}
  {#if waarde === 'eigenSneltoets'}
    <!-- De tweede sneltoets hoort bij deze keuze: geen eigen scheidingslijn. -->
    <InstellingSneltoets soort="opschonen" />
  {/if}
</div>
