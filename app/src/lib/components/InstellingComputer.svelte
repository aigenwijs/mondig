<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import AfstellenRij from '#lib/components/AfstellenRij.svelte';
  import { huidigeTaal, t } from '#lib';
  import {
    adviesRegels,
    type AdviesOmgeving,
    aangevinkt,
    controleDatum,
    heeftIetsTeMelden,
    overgenomenTekst,
    ruimteMelding as bepaalRuimteMelding,
    type AdviesRegel,
    type AfstelStap,
    type AfstelToestand
  } from '#lib/afstellen.js';
  import { adviesOvernemen, computerCheck, computerZin, type ComputerInfo } from '#lib/computer.js';
  import { controleStand } from '#lib/controlestand.svelte.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { foutUitCommando } from '#lib/taak.js';
  import { snelheidstestStart, videokaartInfo, type Vergelijking } from '#lib/videokaart.js';

  // "Mondig opnieuw afstellen" (N-C1, N-C5): leest de computer opnieuw uit, doet de proef met de
  // videokaart als er een losse kaart is, laat het advies zien als lijst met vinkjes en neemt
  // over wat de gebruiker aangevinkt laat. Er begint niets tot "Advies overnemen". De snelheidstest
  // zelf kan niet halverwege stoppen (het werkproces rekent een halve minuut door): "Stoppen"
  // stopt het wachten en gooit de uitkomst weg.

  let toestand = $state<AfstelToestand>('rust');
  let stap = $state<AfstelStap>('computer');
  let toonKaartStap = $state(true);
  let info = $state<ComputerInfo | null>(null);
  let regels = $state<AdviesRegel[]>([]);
  let uit = $state<string[]>([]);
  let fout = $state<string | null>(null);
  let melding = $state<string | null>(null);
  let overnemenBezig = $state(false);
  let overgenomen = $state('');
  let gecontroleerd = $state(new Date());
  /** Telt op bij elke "Afstellen" en "Stoppen": een oudere ronde gooit zijn uitkomst weg. */
  let ronde = 0;
  /** De ronde waarvan de snelheidstest nog loopt (hij is na Stoppen niet te onderbreken). */
  let testRonde = $state<number | null>(null);
  /** De reden als de snelheidstest van deze ronde niet lukte; de videokaart blijft dan een open punt. */
  let kaartFout = $state<string | null>(null);

  const omgeving = (kaartAan: boolean): AdviesOmgeving => ({
    kaartAan,
    modellen: modellenStand.overzicht?.modellen ?? null,
    loopt: (id) => modellenStand.isActief(id),
    taal: huidigeTaal(),
    kaartFout
  });
  const ruimteMelding = $derived(
    toestand === 'advies' && info
      ? bepaalRuimteMelding(regels, new Set(uit), info.computer.vrijBytes, omgeving(false), t())
      : null
  );

  function bouwRegels(uitkomst: ComputerInfo, kaartAan: boolean): AdviesRegel[] {
    return adviesRegels(uitkomst, omgeving(kaartAan), t());
  }

  async function afstellen(): Promise<void> {
    if (toestand === 'bezig' || testRonde !== null) return;
    const mijn = ++ronde;
    kaartFout = null;
    fout = null;
    melding = null;
    uit = [];
    stap = 'computer';
    toonKaartStap = true;
    toestand = 'bezig';
    try {
      // Eerst het advies zonder proef: faalt de snelheidstest, dan blijft wat Mondig las staan.
      let uitkomst = await computerCheck(null);
      if (mijn !== ronde) return;
      const kaartAan = (await videokaartInfo()).aan;
      if (mijn !== ronde) return;
      if (uitkomst.advies.videokaart.stand === 'testen') {
        stap = 'kaart';
        testRonde = mijn;
        try {
          const proef: Vergelijking = await snelheidstestStart();
          if (testRonde === mijn) testRonde = null;
          if (mijn !== ronde) return;
          uitkomst = await computerCheck(proef);
        } catch (e) {
          if (testRonde === mijn) testRonde = null;
          if (mijn !== ronde) return;
          // De proef lukte niet: het advies blijft staan, met de videokaart als open punt en de reden.
          kaartFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
        }
      } else {
        toonKaartStap = false;
      }
      if (mijn !== ronde) return;
      stap = 'modellen';
      info = uitkomst;
      regels = bouwRegels(uitkomst, kaartAan);
      gecontroleerd = new Date();
      toestand = heeftIetsTeMelden(regels) ? 'advies' : 'goed';
      // Voor "Past bij deze computer" op Modellen: alleen een afgeronde ronde telt.
      void controleStand.bewaar(uitkomst.advies);
    } catch (e) {
      if (mijn !== ronde) return;
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      toestand = 'rust';
    }
  }

  function stoppen(): void {
    ronde++;
    melding = testRonde !== null ? t().afstelGestoptTest : t().afstelGestopt;
    fout = null;
    toestand = 'rust';
  }

  function wissel(sleutel: string, aan: boolean): void {
    uit = aan ? uit.filter((s) => s !== sleutel) : [...uit.filter((s) => s !== sleutel), sleutel];
  }

  async function overnemen(): Promise<void> {
    if (overnemenBezig) return;
    overnemenBezig = true;
    fout = null;
    try {
      const uitkomst = await adviesOvernemen(aangevinkt(regels, new Set(uit)));
      overgenomen = overgenomenTekst(uitkomst, t());
      toestand = 'overgenomen';
    } catch (e) {
      // Mislukt (bijvoorbeeld te weinig ruimte): het advies blijft staan, met de reden erbij.
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      overnemenBezig = false;
    }
  }

  // "Opnieuw controleren" op Modellen komt hier met #afstellen: dezelfde ronde, meteen gestart.
  // Daarna weg uit het adres, zodat terugnavigeren hem niet nog eens start.
  onMount(() => {
    if (page.url.hash !== '#afstellen') return;
    void goto(resolve('/instellingen'), { state: page.state, shallow: true, replace: true });
    void tick().then(() => {
      document.getElementById('sectie-computer')?.scrollIntoView({ block: 'start' });
      void afstellen();
    });
  });

  function terugNaarRust(): void {
    ronde++;
    toestand = 'rust';
    fout = null;
    melding = null;
  }
</script>

<div id="afstellen">
  <AfstellenRij
    {toestand}
    {stap}
    {toonKaartStap}
    computerZin={info ? computerZin(info.computer, t()) : ''}
    {regels}
    {uit}
    {melding}
    {fout}
    {overnemenBezig}
    {ruimteMelding}
    afstellenUitReden={testRonde !== null ? t().afstelTestRondtAf : null}
    {overgenomen}
    goedHulp={t().afstelGoedHulp(controleDatum(gecontroleerd, huidigeTaal()))}
    onAfstellen={() => void afstellen()}
    onStoppen={stoppen}
    onWissel={wissel}
    onOvernemen={() => void overnemen()}
    onNietNu={terugNaarRust}
    onSluiten={terugNaarRust}
  />
</div>
