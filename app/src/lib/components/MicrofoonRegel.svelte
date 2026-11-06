<script lang="ts">
  import { onMount } from 'svelte';
  import Mic from '@lucide/svelte/icons/mic';
  import Streepjes from '#lib/components/Streepjes.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { gekozenWaarde, microfoonLijst, type MicrofoonLijst } from '#lib/instellingen.js';
  import { METER_STREEPJES_LANG } from '#lib/meter.js';
  import { BEGINSTAND, maakMicTest, type MicTestStand } from '#lib/mictest.js';

  // De microfoonregel onderaan de sneltoetskaart op Dicteren: het icoon, de naam van de
  // microfoon en "Microfoon testen". De microfoon en de meter gaan pas aan na die klik
  // (besluit 3) en weer uit bij "Klaar", na 15 seconden stilte en als je het scherm verlaat.
  // De microfoon kiezen kan onder Instellingen; hier staat alleen de naam.

  let lijst = $state<MicrofoonLijst | null>(null);
  let stand = $state<MicTestStand>(BEGINSTAND);

  const waarde = $derived(lijst ? gekozenWaarde(lijst) : '');
  const naam = $derived(
    lijst === null
      ? ''
      : waarde === ''
        ? t().apparaatStandaard(lijst.standaardNaam)
        : (lijst.microfoons.find((m) => m.id === waarde)?.naam ?? t().apparaatStandaard(null))
  );

  const test = maakMicTest(
    () => (waarde === '' ? null : waarde),
    (nieuw) => {
      stand = nieuw;
    }
  );

  function begin(): void {
    test.begin();
  }

  function klaar(): void {
    test.klaar();
  }

  onMount(() => {
    microfoonLijst()
      .then((nieuw) => {
        lijst = nieuw;
      })
      .catch(() => {
        lijst = null;
      });
    // Het scherm verlaten zet de microfoon uit.
    return () => {
      test.sluit();
    };
  });
</script>

<div
  class="flex min-h-[52px] flex-wrap items-center gap-x-2.5 gap-y-2 border-t border-lijn py-2 pr-2 pl-5 max-[639px]:pl-3.5"
>
  {#if stand.aan}
    <Streepjes aan={stand.streepjes} aantal={METER_STREEPJES_LANG} />
    <span
      class="min-w-[9.5rem] flex-1 text-klein {stand.fout ? 'text-fout-tekst' : 'text-gedempt'}"
      role="status"
    >
      {stand.fout ? stand.fout.melding : t().micTestHint}
    </span>
    <Button variant="tweede" onclick={klaar}>{t().micTestKlaar}</Button>
  {:else}
    <Mic size={16} class="shrink-0 text-dim" aria-hidden="true" />
    <span class="min-w-0 flex-1 truncate text-klein text-dim" title={naam}>{naam}</span>
    <Button variant="spook" onclick={begin}>{t().micTestKnop}</Button>
    <span class="basis-full text-klein text-dim empty:hidden" role="status">
      {stand.einde === 'stilte' ? t().micTestStilte : ''}
    </span>
  {/if}
</div>
