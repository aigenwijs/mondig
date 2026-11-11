<script lang="ts">
  import { onMount } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import Copy from '@lucide/svelte/icons/copy';
  import Onderdelen from '#lib/components/Onderdelen.svelte';
  import Extern from '#lib/components/Extern.svelte';
  import Woordmerk from '#lib/components/Woordmerk.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    allePunten,
    beperkPunten,
    EERSTE_PUNTEN,
    details,
    eerdereUitgavenVoor,
    eersteZin,
    isDevVersie,
    uitgaveVoor,
    voorPlatform
  } from '#lib/changelog.js';
  import { kopieerNaarKlembord } from '#lib/klembord.js';
  import { paginaTitel, rijNaam, sectieKop, vlak } from '#lib/stijl.js';
  import { versieStand } from '#lib/versie.svelte.js';
  import {
    haalModelVermeldingen,
    haalWerkprocesBouw,
    versieGegevensTekst,
    korteDatum,
    type ModelVermelding
  } from '#lib/versie.js';
  import { haalPlatform } from '#lib/platform.js';
  import { uitgaven } from '#lib/wijzigingen.js';
  import { apparaatInGebruik } from '#lib/videokaart.js';

  // Over Mondig (ADR 0016, stijl ADR 0014): kop met versie, wat er nieuw is, de
  // spraakmodellen, onderdelen van anderen, privacy en copyright. Alles komt uit de app
  // zelf; er is geen netwerk voor nodig.

  let modellen = $state<ModelVermelding[] | null>(null);
  let modellenFout = $state(false);
  let gekopieerd = $state(false);
  let kopieerMislukt = $state(false);
  // Mac-punten staan alleen op de Mac; tot Rust het platform meldt (of buiten de app) geldt Windows.
  let opMac = $state(false);

  onMount(() => {
    void versieStand.laad();
    void haalPlatform().then((platform) => {
      opMac = platform === 'mac';
    });
    haalModelVermeldingen()
      .then((lijst) => {
        modellen = lijst;
      })
      .catch(() => {
        modellenFout = true;
      });
  });

  const info = $derived(versieStand.info);
  const uitgave = $derived.by(() => {
    const gevonden = info ? uitgaveVoor(uitgaven, info.versie) : null;
    return gevonden ? voorPlatform(gevonden, opMac) : null;
  });
  // De eerdere uitgaven, voor "Alle wijzigingen" (de huidige staat er al boven).
  const andere = $derived(info ? eerdereUitgavenVoor(uitgaven, info.versie, opMac) : []);
  // Eerst de eerste punten, elk met alleen zijn eerste zin (wat je merkt); "Alle wijzigingen
  // tonen" klapt de rest uit, nu met de details achter elke zin (bij -dev is dat veel).
  let alles = $state(false);
  const beperkt = $derived(uitgave ? beperkPunten(uitgave, EERSTE_PUNTEN) : null);
  const heeftDetails = $derived(
    uitgave ? allePunten(uitgave).some((p) => details(p) !== '') : false
  );
  const getoond = $derived(
    alles || !uitgave ? (uitgave?.categorieen ?? []) : (beperkt?.categorieen ?? [])
  );

  async function kopieer(): Promise<void> {
    if (!info) return;
    gekopieerd = false;
    kopieerMislukt = false;
    // Vers opvragen: het werkproces kan sinds het openen van dit scherm gestart zijn.
    const werkproces = await haalWerkprocesBouw().catch(() => null);
    const apparaat = await apparaatInGebruik().catch(() => null);
    gekopieerd = await kopieerNaarKlembord(versieGegevensTekst(info, werkproces, apparaat));
    kopieerMislukt = !gekopieerd;
  }

  const kaart = `${vlak} flex flex-col gap-4 px-5 py-4`;
  const rijen = 'm-0 grid grid-cols-1 gap-x-6 gap-y-1 min-[34rem]:grid-cols-[max-content_1fr]';
  const dt = 'text-gedempt';
  const dd = 'm-0 mb-2 min-w-0 min-[34rem]:mb-0';
</script>

<svelte:head>
  <title>{t().overMondig} - {t().appTitle}</title>
</svelte:head>

<h1 class={paginaTitel}>{t().overMondig}</h1>

<section class={kaart} aria-labelledby="over-kop">
  <h2 id="over-kop" class="m-0 flex items-center">
    <Woordmerk />
  </h2>
  {#if info}
    <p class="m-0 text-midden">{t().versieRegelTekst(info.versie)}</p>
    <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
      <Button variant="tweede" onclick={kopieer}>
        <Copy size={16} aria-hidden="true" />{t().versieKopieren}
      </Button>
      <span class="text-klein text-gedempt">{t().versieKopierenUitleg}</span>
    </div>
    <p class="m-0 flex items-center gap-2 text-klein text-gelukt" role="status">
      {#if gekopieerd}<Check size={14} aria-hidden="true" />{t().versieGekopieerd}{/if}
    </p>
    {#if kopieerMislukt}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{t().kopierenMislukt}</p>
    {/if}
  {:else if versieStand.mislukt}
    <p class="m-0 text-fout-tekst" role="alert">{t().versieFout}</p>
  {:else}
    <p class="m-0 text-gedempt" role="status">{t().versieLaden}</p>
  {/if}
</section>

<section aria-labelledby="sectie-nieuw" class="flex flex-col gap-3">
  <h2 id="sectie-nieuw" class="{sectieKop} !mb-0">{t().nieuwKop}</h2>
  {#if info}
    <div class="{vlak} flex flex-col gap-4 px-5 py-4">
      {#if isDevVersie(info.versie)}
        <p class="m-0 text-klein text-gedempt">{t().nieuwNogNiet}</p>
      {:else if uitgave}
        <p class="m-0 text-klein text-gedempt">{t().nieuwVersie(info.versie)}</p>
      {/if}
      {#if uitgave && uitgave.categorieen.length > 0}
        {#each getoond as categorie (categorie.naam)}
          <div>
            <h3 class="{rijNaam} mb-1.5">{categorie.naam}</h3>
            <ul class="m-0 flex list-disc flex-col gap-1.5 pl-5 text-midden">
              {#each categorie.punten as punt, i (i)}
                {#if alles && details(punt) !== ''}
                  <li>{eersteZin(punt)} <span class="text-gedempt">{details(punt)}</span></li>
                {:else}
                  <li>{eersteZin(punt)}</li>
                {/if}
              {/each}
            </ul>
          </div>
        {/each}
        {#if beperkt && (beperkt.verborgen > 0 || heeftDetails)}
          <div>
            <Button
              variant="tweede"
              aria-expanded={alles}
              onclick={() => {
                alles = !alles;
              }}>{alles ? t().minderTonen : t().alleWijzigingenTonen}</Button
            >
          </div>
        {/if}
      {:else}
        <p class="m-0 text-gedempt">{t().nieuwGeen}</p>
      {/if}
    </div>
    {#if andere.length > 0}
      <details class={vlak}>
        <summary class="cursor-pointer px-5 py-3 font-medium hover:bg-hover">
          {t().eerdereUitgaven}
        </summary>
        <div class="flex flex-col gap-5 px-5 pt-1 pb-4">
          {#each andere as u (u.kop)}
            <div>
              <h3 class="{rijNaam} mb-1">
                {u.kop}{u.datum ? ` (${u.datum})` : ''}
              </h3>
              {#each u.categorieen as categorie (categorie.naam)}
                <p class="m-0 mt-2 text-klein font-semibold text-gedempt">{categorie.naam}</p>
                <ul class="m-0 flex list-disc flex-col gap-1.5 pl-5 text-midden">
                  {#each categorie.punten as punt, i (i)}
                    <li>{punt}</li>
                  {/each}
                </ul>
              {/each}
            </div>
          {/each}
        </div>
      </details>
    {/if}
  {/if}
</section>

<section aria-labelledby="sectie-modellen" class="flex flex-col gap-3">
  <h2 id="sectie-modellen" class="{sectieKop} !mb-0">{t().modellenKop}</h2>
  <p class="m-0 max-w-[600px] text-gedempt">{t().modellenUitleg}</p>
  {#if modellenFout}
    <p class="m-0 text-fout-tekst" role="alert">{t().modellenFout}</p>
  {:else if modellen}
    <div class="flex flex-col gap-3">
      {#each modellen as m (m.id)}
        <article class={kaart} aria-labelledby="model-{m.id}">
          <h3 id="model-{m.id}" class="m-0 text-kop2">
            {m.naam}
            {#if m.hulpmodel}<span class="ml-1 text-klein font-normal text-dim"
                >({t().hulpmodel})</span
              >{/if}
          </h3>
          <dl class={rijen}>
            <dt class={dt}>{t().vermMaker}</dt>
            <dd class={dd}>{m.maker}, {m.titel}</dd>
            <dt class={dt}>{t().vermCopyright}</dt>
            <dd class={dd}>{m.copyright ?? t().vermGeenCopyright}</dd>
            <dt class={dt}>{t().vermOorspronkelijk}</dt>
            <dd class={dd}><Extern href={m.bronUrl} /></dd>
            <dt class={dt}>{t().vermLicentie}</dt>
            <dd class={dd}>{m.licentie}, <Extern href={m.licentieUrl} /></dd>
            <dt class={dt}>{t().vermDisclaimer}</dt>
            <dd class={dd}><Extern href={m.disclaimerUrl} tekst={t().vermDisclaimerTekst} /></dd>
            <dt class={dt}>{t().vermGewijzigd}</dt>
            <dd class={dd}>{m.wijziging}</dd>
            <dt class={dt}>{t().vermOmgezet}</dt>
            <dd class="m-0 min-w-0"><Extern href={m.ggufUrl} /></dd>
          </dl>
        </article>
      {/each}
    </div>
  {/if}
</section>

<section aria-labelledby="sectie-onderdelen" class="flex flex-col gap-3">
  <h2 id="sectie-onderdelen" class="{sectieKop} !mb-0">{t().onderdelenKop}</h2>
  <p class="m-0 max-w-[600px] text-gedempt">{t().onderdelenUitleg}</p>
  <Onderdelen />
</section>

<section aria-labelledby="sectie-privacy" class="flex flex-col gap-3">
  <h2 id="sectie-privacy" class="{sectieKop} !mb-0">{t().privacyKop}</h2>
  <p class="m-0 text-midden">{t().privacyTekst}</p>
</section>

<!-- De bouwcode staat niet in de kop maar hier, voor wie hem bij een supportvraag nodig heeft. -->
<p class="m-0 mt-2 text-klein text-dim">
  {t().copyrightRegel}{#if info}<span class="mx-1.5" aria-hidden="true">·</span>{t().bouwRegelTekst(
      info.bouw,
      korteDatum(info.datum)
    )}{/if}
</p>
