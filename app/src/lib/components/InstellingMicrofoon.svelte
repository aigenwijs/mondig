<script lang="ts">
  import { onMount } from 'svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import Streepjes from '#lib/components/Streepjes.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import {
    gekozenWaarde,
    microfoonIsWeg,
    microfoonKies,
    microfoonLijst,
    zelfdeLijst,
    type MicrofoonLijst
  } from '#lib/instellingen.js';
  import { METER_STREEPJES } from '#lib/meter.js';
  import { BEGINSTAND, maakMicTest, type MicTestStand } from '#lib/mictest.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { microfooninstellingOpen } from '#lib/commandos.js';
  import { foutUitCommando } from '#lib/taak.js';

  // De microfoon kiezen (direct toegepast, geen Opslaan), met "Testen" ernaast (N-C1, besluit 3).
  // De microfoon en de meter gaan pas aan na een klik op Testen en uit bij een klik op de meter,
  // na 15 seconden stilte en als je dit scherm verlaat. Er wordt niets bewaard of teruggespeeld.

  let lijst = $state<MicrofoonLijst | null>(null);
  let lijstFout = $state<string | null>(null);
  let bewaard = $state(false);
  let teststand = $state<MicTestStand>(BEGINSTAND);

  const waarde = $derived(lijst ? gekozenWaarde(lijst) : '');
  // De volledige naam van de gekozen microfoon, voor de tooltip als de naam toch wordt afgekapt.
  const gekozenNaam = $derived(
    lijst === null
      ? ''
      : waarde === ''
        ? t().apparaatStandaard(lijst.standaardNaam)
        : (lijst.microfoons.find((m) => m.id === waarde)?.naam ?? '')
  );
  const isWeg = $derived(lijst ? microfoonIsWeg(lijst) : false);

  // Tien streepjes, zoals elke meter bij een keuze.
  const test = maakMicTest(
    () => (waarde === '' ? null : waarde),
    (nieuw) => {
      teststand = nieuw;
    },
    undefined,
    METER_STREEPJES
  );

  /** Na een fout: de test stopt en begint opnieuw. */
  function opnieuw(): void {
    test.klaar();
    test.begin();
  }

  async function laad(): Promise<void> {
    try {
      const nieuw = await microfoonLijst();
      if (lijst === null || !zelfdeLijst(lijst, nieuw)) lijst = nieuw;
      lijstFout = null;
    } catch (e) {
      lijstFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function kies(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const id = event.currentTarget.value === '' ? null : event.currentTarget.value;
    bewaard = false;
    // Een ander apparaat is een nieuwe test: stop de lopende, de volgende klik meet de nieuwe.
    test.klaar();
    try {
      await microfoonKies(id);
      bewaard = true;
    } catch {
      // Lukt het bewaren niet, dan blijft de oude keuze gelden; de lijst hieronder laat dat zien.
    }
    await laad();
  }

  onMount(() => {
    void laad();
    // Het scherm verlaten zet de microfoon uit.
    return () => {
      test.sluit();
    };
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="microfoon-kop"
      label={t().microfoonKop}
      hulpId="microfoon-hulp"
      hulp={t().microfoonHulp}
    />
    {#if isWeg}
      <p class="m-0 text-klein text-gedempt">{t().microfoonWeg}</p>
    {/if}
    {#if lijstFout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{lijstFout}</p>
    {/if}
    {#if teststand.aan && !teststand.fout}
      <p class="m-0 text-klein text-gedempt" role="status">{t().micTestHint}</p>
    {/if}
    {#if teststand.fout}
      <p class="m-0 text-klein text-fout-tekst" role="status">{teststand.fout.melding}</p>
      <div class="flex flex-wrap gap-2">
        <Button variant="tweede" onclick={opnieuw}>{t().opnieuwProberen}</Button>
        {#if teststand.fout.vervolg === 'microfooninstelling_openen'}
          <Button variant="tweede" onclick={() => void microfooninstellingOpen()}
            >{t().microfooninstellingOpenen}</Button
          >
        {/if}
      </div>
    {/if}
    {#if !teststand.aan}
      <p class="m-0 text-klein text-gedempt empty:hidden" role="status">
        {teststand.einde === 'stilte'
          ? t().micTestStilte
          : teststand.einde === 'voorrang'
            ? t().meterVoorrang
            : ''}
      </p>
    {/if}
    <p class="m-0 text-klein text-gelukt empty:hidden" role="status">
      {bewaard ? t().opgeslagen : ''}
    </p>
  </div>
  <div class={rijBediening}>
    <Keuze
      breedte="sm:w-[16rem] max-[639px]:w-auto max-[639px]:min-w-0 max-[639px]:flex-1"
      title={gekozenNaam}
      aria-labelledby="microfoon-kop"
      aria-describedby="microfoon-hulp"
      value={waarde}
      onchange={kies}
      onfocus={laad}
    >
      <option value="">{t().apparaatStandaard(lijst?.standaardNaam ?? null)}</option>
      {#each lijst?.microfoons ?? [] as mic (mic.id)}
        <option value={mic.id} title={mic.naam}>{mic.naam}</option>
      {/each}
    </Keuze>
    {#if teststand.aan}
      <!-- Een klik op de meter stopt de test. -->
      <button
        type="button"
        class="inline-flex min-h-8 cursor-pointer items-center rounded-klein px-2 transition-colors hover:bg-hover"
        title={t().micTestStoppen}
        aria-label={t().micTestStoppen}
        onclick={() => {
          test.klaar();
        }}><Streepjes aan={teststand.streepjes} aantal={METER_STREEPJES} /></button
      >
    {:else}
      <Button
        variant="tweede"
        onclick={() => {
          test.begin();
        }}>{t().micTestInstellingen}</Button
      >
    {/if}
  </div>
</div>
