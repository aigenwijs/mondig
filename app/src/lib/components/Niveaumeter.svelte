<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '#lib';
  import Streepjes from '#lib/components/Streepjes.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import {
    METER_STREEPJES,
    startMeter,
    streepjesAan,
    type Meetbron,
    type MeterEinde
  } from '#lib/meter.js';
  import type { Foutmelding } from '#lib/taak.js';

  // Een live niveaumeter van tien streepjes voor de microfoon of het geluid van de computer.
  // De meter start zodra de component er is, krijgt een nieuwe meter als het apparaat
  // verandert en stopt netjes als de component verdwijnt (de helper `startMeter` stuurt de
  // hartslag en pauzeert zodra het venster verborgen is). Los van Instellingen, zodat ook
  // Dicteren en Opnemen hem kunnen gebruiken.

  interface Props {
    bron: Meetbron;
    /** Het id van het apparaat, of `null` voor de standaard. */
    apparaat: string | null;
    /**
     * Toont de component zelf de melding als de meter weigert of faalt? Zet op `false` om de
     * melding elders te tonen via `fout` en `opnieuw()`.
     */
    meldingTonen?: boolean;
    /** De laatste fout van de meter, of `null` als hij loopt. */
    fout?: Foutmelding | null;
  }
  let { bron, apparaat, meldingTonen = true, fout = $bindable(null) }: Props = $props();

  // Hoeveel streepjes aan staan; verandert hooguit tien keer, ook al komt het niveau 20 keer
  // per seconde binnen.
  let streepjes = $state(0);
  // Wat een schermlezer voorleest: hooguit eens per seconde bijgewerkt.
  let gemeld = $state(0);
  // Telt op bij `opnieuw()`, zodat het effect hieronder een nieuwe meter start.
  let poging = $state(0);
  // Waarom de app de meter stopte zonder foutmelding (dictaat of opname loopt).
  let reden = $state<MeterEinde['reden'] | null>(null);

  const label = $derived(bron === 'microfoon' ? t().microfoonMeterAria : t().systeemMeterAria);
  const meldTekst = $derived(
    fout
      ? fout.melding
      : reden === 'voorrang'
        ? t().meterVoorrang
        : reden !== null
          ? t().meterGestopt
          : ''
  );

  /** Start de meter opnieuw, bijvoorbeeld na "Opnieuw proberen". */
  export function opnieuw(): void {
    poging += 1;
  }

  // Een eerdere meter van dezelfde bron stopt asynchroon in de app; een korte pauze voor een
  // nieuwe start voorkomt dat die stop de nieuwe meter raakt. De eerste start wacht niet.
  let eerderGestart = false;

  $effect(() => {
    // Afhankelijkheden: een ander apparaat, een andere bron of "opnieuw" start een nieuwe meter.
    const kies = { bron, apparaat, poging };
    fout = null;
    reden = null;
    streepjes = 0;
    let meter: ReturnType<typeof startMeter> | null = null;
    const vertraging = eerderGestart ? 150 : 0;
    eerderGestart = true;
    const timer = setTimeout(() => {
      meter = startMeter({
        bron: kies.bron,
        apparaat: kies.apparaat,
        bijNiveau: (niveau) => {
          streepjes = streepjesAan(niveau);
        },
        bijFout: (f) => {
          fout = f;
        },
        bijEinde: (einde) => {
          reden = einde.reden;
          if (einde.fout) fout = einde.fout;
        }
      });
    }, vertraging);
    return () => {
      clearTimeout(timer);
      meter?.stop();
      streepjes = 0;
    };
  });

  onMount(() => {
    const timer = setInterval(() => {
      gemeld = streepjes;
    }, 1000);
    return () => {
      clearInterval(timer);
    };
  });
</script>

<span class="inline-flex flex-col items-end gap-1">
  <span
    class="inline-flex items-center gap-0.5"
    role="meter"
    aria-label={label}
    aria-valuemin={0}
    aria-valuemax={METER_STREEPJES}
    aria-valuenow={gemeld}
    aria-valuetext={t().meterWaarde(gemeld, METER_STREEPJES)}
  >
    <Streepjes aan={streepjes} />
  </span>
  {#if meldingTonen && meldTekst}
    <span class="flex max-w-64 flex-col items-end gap-1 text-right text-klein text-gedempt">
      <span role="status">{meldTekst}</span>
      <Button variant="spook" onclick={opnieuw}>{t().opnieuwProberen}</Button>
    </span>
  {/if}
</span>
