<script lang="ts">
  import InstellingComputer from '#lib/components/InstellingComputer.svelte';
  import InstellingProblemen from '#lib/components/InstellingProblemen.svelte';
  import InstellingBewaartermijn from '#lib/components/InstellingBewaartermijn.svelte';
  import InstellingBewaakteMap from '#lib/components/InstellingBewaakteMap.svelte';
  import InstellingBewaakteMapMeer from '#lib/components/InstellingBewaakteMapMeer.svelte';
  import InstellingAutostart from '#lib/components/InstellingAutostart.svelte';
  import InstellingGeheugen from '#lib/components/InstellingGeheugen.svelte';
  import InstellingGeluid from '#lib/components/InstellingGeluid.svelte';
  import InstellingGeluidVolume from '#lib/components/InstellingGeluidVolume.svelte';
  import InstellingDictatenBewaren from '#lib/components/InstellingDictatenBewaren.svelte';
  import InstellingKlembord from '#lib/components/InstellingKlembord.svelte';
  import InstellingStopwoorden from '#lib/components/InstellingStopwoorden.svelte';
  import InstellingMicrofoon from '#lib/components/InstellingMicrofoon.svelte';
  import InstellingModellen from '#lib/components/InstellingModellen.svelte';
  import InstellingNotulen from '#lib/components/InstellingNotulen.svelte';
  import InstellingOpschonen from '#lib/components/InstellingOpschonen.svelte';
  import InstellingOpslaan from '#lib/components/InstellingOpslaan.svelte';
  import InstellingOpslaanMeer from '#lib/components/InstellingOpslaanMeer.svelte';
  import InstellingOver from '#lib/components/InstellingOver.svelte';
  import InstellingPositie from '#lib/components/InstellingPositie.svelte';
  import InstellingSneltoets from '#lib/components/InstellingSneltoets.svelte';
  import InstellingSprekers from '#lib/components/InstellingSprekers.svelte';
  import InstellingVideokaart from '#lib/components/InstellingVideokaart.svelte';
  import MeerOpties from '#lib/components/MeerOpties.svelte';
  import { t } from '#lib';
  import { paginaTitel, sectieKop, vlak } from '#lib/stijl.js';

  // Groepen in de volgorde van gebruik (ontwerp N-C1): Deze computer (opnieuw afstellen en
  // Modellen) bovenaan, dan Dicteren, Opslaan, Map in de gaten houden, Opstarten en Problemen
  // oplossen, en onderaan Over Mondig. Wat je zelden verandert zit per groep achter "Meer
  // opties", één niveau diep. De groep Spraakmodellen is opgegaan in de rij Modellen.
</script>

<svelte:head>
  <title>{t().instellingen} - {t().appTitle}</title>
</svelte:head>

<h1 class={paginaTitel}>{t().instellingen}</h1>

<section aria-labelledby="sectie-computer">
  <h2 id="sectie-computer" class={sectieKop}>{t().groepDezeComputer}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingComputer />
    <InstellingModellen />
    <MeerOpties>
      <InstellingGeheugen />
      <InstellingVideokaart />
    </MeerOpties>
  </div>
</section>

<section aria-labelledby="sectie-dicteren">
  <h2 id="sectie-dicteren" class={sectieKop}>{t().groepDicteren}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingSneltoets />
    <InstellingMicrofoon />
    <InstellingDictatenBewaren />
    <InstellingOpschonen />
    <InstellingPositie />
    <InstellingGeluid />
    <MeerOpties>
      <InstellingGeluidVolume />
      <InstellingKlembord />
      <InstellingStopwoorden />
    </MeerOpties>
  </div>
</section>

<section aria-labelledby="sectie-opslaan">
  <h2 id="sectie-opslaan" class={sectieKop}>{t().groepOpslaan}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingOpslaan />
    <InstellingBewaartermijn />
    <InstellingSprekers />
    <MeerOpties>
      <InstellingOpslaanMeer />
    </MeerOpties>
  </div>
</section>

<section id="notulen" aria-labelledby="sectie-notulen">
  <h2 id="sectie-notulen" class={sectieKop}>{t().groepNotulen}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingNotulen />
  </div>
</section>

<section aria-labelledby="sectie-bewaakte-map">
  <h2 id="sectie-bewaakte-map" class={sectieKop}>{t().groepBewaakteMap}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingBewaakteMap />
    <MeerOpties>
      <InstellingBewaakteMapMeer />
    </MeerOpties>
  </div>
</section>

<section aria-labelledby="sectie-opstarten">
  <h2 id="sectie-opstarten" class={sectieKop}>{t().groepOpstarten}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingAutostart />
  </div>
</section>

<section aria-labelledby="sectie-problemen">
  <h2 id="sectie-problemen" class={sectieKop}>{t().groepProblemen}</h2>
  <div class="{vlak} divide-y divide-lijn">
    <InstellingProblemen />
  </div>
</section>

<!-- Onderaan, zoals de Windows-richtlijnen voor instellingen het vragen: Over Mondig. -->
<div class={vlak}>
  <InstellingOver />
</div>
