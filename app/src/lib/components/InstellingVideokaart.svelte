<script lang="ts">
  import { onMount } from 'svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';
  import {
    apparaatInGebruik,
    isBedienbaar,
    kaartTekst,
    luisterNaarApparaat,
    luisterNaarVideokaart,
    snelheidstestStart,
    standTekst,
    testTekst,
    videokaartInfo,
    videokaartZet,
    voorkeurKaart,
    type Rekenstand,
    type VideokaartInfo
  } from '#lib/videokaart.js';

  // "Videokaart gebruiken" (2-11-2026): een tussenstap tot de hardwarecheck van fase 5.
  // Alleen te bedienen als het werkproces een videokaart zag; daaronder staat wat de motoren
  // werkelijk gebruiken. "Snelheid testen" meet het bestandsmodel op beide apparaten.

  let info = $state<VideokaartInfo | null>(null);
  let stand = $state<Rekenstand | null>(null);
  let bewaard = $state(false);
  let fout = $state<string | null>(null);
  let testBezig = $state(false);
  let testUitkomst = $state<string | null>(null);

  const bedienbaar = $derived(info !== null && isBedienbaar(info));
  const heeftKaart = $derived(info !== null && voorkeurKaart(info.kaarten) !== null);
  const inGebruik = $derived(standTekst(stand, t()));

  async function wijzig(nieuw: boolean): Promise<void> {
    if (!info) return;
    const vorige = info.aan;
    info.aan = nieuw;
    bewaard = false;
    fout = null;
    try {
      await videokaartZet(nieuw);
      bewaard = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      info.aan = vorige;
    }
  }

  async function test(): Promise<void> {
    if (testBezig || !heeftKaart) return;
    testBezig = true;
    testUitkomst = null;
    fout = null;
    try {
      testUitkomst = testTekst(await snelheidstestStart(), t(), huidigeTaal());
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      testBezig = false;
    }
  }

  onMount(() => {
    videokaartInfo()
      .then((waarde) => {
        info = waarde;
      })
      .catch(() => {
        info = { aan: false, kaarten: null };
      });
    apparaatInGebruik()
      .then((waarde) => {
        stand = waarde;
      })
      .catch(() => {
        stand = null;
      });
    const stop = luisterNaarApparaat((nieuw) => {
      stand = nieuw;
    });
    // De schakelaar klopt ook als iets anders de kaart aan- of uitzet (Mondig opnieuw afstellen).
    const stopKaart = luisterNaarVideokaart((aan) => {
      if (info) info.aan = aan;
    });
    return () => {
      void stop.then((f) => {
        f();
      });
      void stopKaart.then((f) => {
        f();
      });
    };
  });
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="videokaart-kop"
      label={t().videokaartKop}
      hulpId="videokaart-hulp"
      hulp={t().videokaartHulp}
    />
    <p id="videokaart-kaart" class="m-0 text-klein text-dim">
      {info ? kaartTekst(info, t()) : t().videokaartZoeken}
    </p>
    <p class="m-0 text-klein text-gedempt" role="status">{inGebruik ?? ''}</p>
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <p class="m-0 text-klein text-gelukt" role="status">
      {testBezig ? t().snelheidBezig : (testUitkomst ?? (bewaard ? t().opgeslagen : ''))}
    </p>
  </div>
  <div class={rijBediening}>
    <Button
      variant="tweede"
      aria-disabled={!heeftKaart || testBezig}
      aria-describedby="videokaart-kaart"
      onclick={() => void test()}
    >
      {t().snelheidTesten}
    </Button>
    <Schakelaar
      aan={info?.aan ?? false}
      wijzig={(nieuw: boolean) => void wijzig(nieuw)}
      labelledby="videokaart-kop"
      describedby="videokaart-hulp videokaart-kaart"
      disabled={!bedienbaar}
    />
  </div>
</div>
