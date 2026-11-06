<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Copy from '@lucide/svelte/icons/copy';
  import Info from '@lucide/svelte/icons/info';
  import Shield from '@lucide/svelte/icons/shield';
  import Square from '@lucide/svelte/icons/square';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    dictaatAnnuleer,
    dictaatStop,
    microfooninstellingOpen,
    pilOpenMondig,
    pilStand,
    pilToon,
    pilVerberg
  } from '#lib/commandos.js';
  import { formatDuur, type PlakUitkomst } from '#lib/dictaat.js';
  import { dictaatStand } from '#lib/dictaatstand.svelte.js';
  import {
    luisterNaarPilNiveau,
    luisterNaarPilOpschonen,
    luisterNaarPilTaak
  } from '#lib/gebeurtenissen.js';
  import { kopieerNaarKlembord } from '#lib/klembord.js';
  import { meldingTekst } from '#lib/taak.js';
  import { dictaatOpschonenStop, type PilOpschonen } from '#lib/opschonen.js';
  import {
    GEEN_TIJDEN,
    METER_STAAFJES,
    KLAAR_ZICHTBAAR_MS,
    pilActie,
    pilToestand,
    restSeconden,
    schuifNiveau,
    staafHoogte,
    volgTijden,
    volgendeTik,
    type KlaarSoort,
    type PilTijden
  } from '#lib/pil.js';

  // De zwevende pil (sprint 3d). Dit is een eigen venster
  // zonder activering: het neemt nooit de focus, zodat de tekst in je eigen programma
  // terechtkomt. Wat hier staat, beslist pil.ts; dit bestand tekent het, meet zichzelf en
  // vraagt Rust de pil op die maat te tonen.

  // Alleen in ontwikkeling: `?klok=540000` zet de klok negen minuten vooruit, om de
  // waarschuwing te zien zonder negen minuten te wachten.
  const klokVerschuiving = import.meta.env.DEV
    ? Number(new URLSearchParams(location.search).get('klok') ?? 0) || 0
    : 0;
  const klok = (): number => Date.now() + klokVerschuiving;

  let pilTaak = $state<number | null>(null);
  let gesloten = $state<number | null>(null);
  let niveaus = $state<number[]>(Array.from({ length: METER_STAAFJES }, () => 0));
  let tijden = $state<PilTijden>(GEEN_TIJDEN);
  let nu = $state(klok());
  /** Kopiëren gelukt voor dit taak-id: even een bevestiging, dan weg. */
  let gekopieerd = $state<number | null>(null);
  let kopieerMislukt = $state(false);
  let pil = $state<HTMLDivElement | null>(null);
  /** Het laatste bericht over opschonen (ADR 0027). */
  let opschonen = $state<PilOpschonen | null>(null);

  const weergave = $derived(dictaatStand.weergave);
  const plak = $derived(weergave.type === 'klaar' ? dictaatStand.plak : null);
  const taak = $derived(weergave.type === 'rust' ? null : weergave.taak);

  // De momenten (begin van verwerkt, afloop) bijhouden zodra de stand verandert.
  $effect.pre(() => {
    const volgende = volgTijden(tijden, weergave, plak, klok());
    if (volgende !== tijden) tijden = volgende;
  });

  const invoer = $derived({ weergave, plak, pilTaak, gesloten, tijden, nu, opschonen });
  const toestand = $derived(pilToestand(invoer));
  const actie = $derived(pilActie(toestand));
  const zichtbaar = $derived(toestand.type !== 'verborgen');

  // Een nieuw dictaat begint met een lege meter en zonder oude bevestiging.
  $effect(() => {
    if (weergave.type === 'luistert') {
      niveaus = Array.from({ length: METER_STAAFJES }, () => 0);
      kopieerMislukt = false;
    }
  });

  // De klok tikt alleen als er zonder event iets moet veranderen (duur, verwerkt na 1 s,
  // klaar na 1,5 s weg).
  $effect(() => {
    const wacht = volgendeTik({ ...invoer, nu: klok() });
    if (wacht === null) return;
    const timer = setTimeout(() => {
      nu = klok();
    }, wacht);
    return () => {
      clearTimeout(timer);
    };
  });

  // Tonen of verbergen, met de gemeten maat. Ook een verborgen venster rekent zijn opmaak
  // uit als we erom vragen, dus de maat klopt al voordat de pil verschijnt.
  let laatsteMaat = '';
  async function plaats(): Promise<void> {
    await tick();
    if (!zichtbaar || !pil) {
      if (laatsteMaat !== '') {
        laatsteMaat = '';
        await pilVerberg().catch(() => undefined);
      }
      return;
    }
    const vak = pil.getBoundingClientRect();
    const maat = `${String(Math.ceil(vak.width))}x${String(Math.ceil(vak.height))}`;
    if (maat === laatsteMaat) return;
    laatsteMaat = maat;
    await pilToon(Math.ceil(vak.width), Math.ceil(vak.height)).catch(() => undefined);
  }

  // Alleen bij een andere soort toestand opnieuw meten en plaatsen, niet bij elke tik van de
  // klok; een andere maat binnen dezelfde toestand vangt de ResizeObserver.
  const pilType = $derived(toestand.type);
  $effect(() => {
    if (pilType === 'verborgen' || pil !== null) void plaats();
  });

  onMount(() => {
    document.documentElement.classList.add('pil-venster');
    let weg = false;
    const opruimers: (() => void)[] = [];
    const houdVast = (stop: () => void): void => {
      if (weg) stop();
      else opruimers.push(stop);
    };
    void luisterNaarPilTaak((id) => {
      pilTaak = id;
      gesloten = null;
      gekopieerd = null;
      opschonen = null;
    }).then(houdVast);
    void luisterNaarPilOpschonen((bericht) => {
      opschonen = bericht;
    }).then(houdVast);
    // Events van vóór het laden (een dictaat vlak na het starten) zijn gemist: vraag de
    // stand op. Loopt de opname nog, dan begint de pil alsnog met "luistert".
    pilStand()
      .then((stand) => {
        if (stand.taak === null || pilTaak !== null) return;
        pilTaak = stand.taak;
        if (stand.luistert && dictaatStand.weergave.type === 'rust') {
          dictaatStand.opTaak({
            taak: stand.taak,
            soort: 'dictaat',
            toestand: { type: 'luistert' }
          });
        }
      })
      .catch(() => undefined);
    void luisterNaarPilNiveau((niveau) => {
      niveaus = schuifNiveau(niveaus, niveau);
    }).then(houdVast);
    // Verandert de maat (een lettertype laadt, de tekst breekt anders af), dan opnieuw.
    const waarnemer = new ResizeObserver(() => {
      void plaats();
    });
    if (pil) waarnemer.observe(pil);
    return () => {
      weg = true;
      waarnemer.disconnect();
      opruimers.forEach((stop) => {
        stop();
      });
    };
  });

  async function stop(): Promise<void> {
    if (taak !== null) await dictaatStop(taak).catch(() => undefined);
  }

  async function annuleer(): Promise<void> {
    if (taak !== null) await dictaatAnnuleer(taak).catch(() => undefined);
  }

  /** Het kruisje tijdens opschonen: stoppen, de letterlijke tekst wordt geplakt. */
  async function stopOpschonen(): Promise<void> {
    if (taak !== null) await dictaatOpschonenStop(taak).catch(() => undefined);
  }

  function sluit(): void {
    gesloten = taak;
  }

  async function kopieer(): Promise<void> {
    const tekst = dictaatStand.dictaat;
    if (tekst === null || taak === null) return;
    const id = taak;
    if (await kopieerNaarKlembord(tekst)) {
      gekopieerd = id;
      kopieerMislukt = false;
      // Kopiëren is "iets doen": na een korte bevestiging verdwijnt de pil.
      setTimeout(() => {
        if (gekopieerd === id) gesloten = id;
      }, KLAAR_ZICHTBAAR_MS);
    } else {
      kopieerMislukt = true;
    }
  }

  async function voerActieUit(): Promise<void> {
    if (actie === 'kopieren') return kopieer();
    if (actie === 'microfoon') await microfooninstellingOpen().catch(() => undefined);
    if (actie === 'mondig') await pilOpenMondig().catch(() => undefined);
    sluit();
  }

  function klaarTekst(soort: KlaarSoort): string {
    switch (soort) {
      case 'geplakt':
        return t().pilGeplakt;
      case 'waarschijnlijk':
        return t().pilWaarschijnlijk;
      case 'klembord':
        return t().pilOpKlembord;
      case 'in_mondig':
        return t().pilInMondig;
      case 'geen_tekst':
        return t().pilGeenTekst;
      case 'geen_geluid':
        return t().pilGeenGeluid;
      case 'verdacht':
        return t().pilVerdacht;
    }
  }

  function plakFoutTekst(uitkomst: PlakUitkomst): string {
    if (uitkomst === 'beheerrechten') return t().pilPlakBeheer;
    if (uitkomst === 'niet_gelezen') return t().pilPlakOnzeker;
    return t().pilPlakMislukt;
  }

  const foutTekst = $derived(
    toestand.type === 'fout'
      ? meldingTekst(toestand, t().appNietBijgewerkt)
      : toestand.type === 'plakfout'
        ? plakFoutTekst(toestand.uitkomst)
        : ''
  );
  const isFout = $derived(toestand.type === 'fout' || toestand.type === 'plakfout');
  const toonGekopieerd = $derived(gekopieerd !== null && gekopieerd === taak && isFout);

  const actieTekst = $derived(
    actie === 'kopieren'
      ? t().kopieren
      : actie === 'microfoon'
        ? t().instellingOpenen
        : actie === 'mondig'
          ? t().mondigOpenen
          : ''
  );

  // Wat de schermlezer hoort bij start en stop (de duur en de meter niet).
  const statusTekst = $derived(
    toestand.type === 'luistert'
      ? toestand.restMs === null
        ? t().dictaatLuistert
        : t().pilRest(restSeconden(toestand.restMs))
      : toestand.type === 'verwerkt'
        ? t().pilUitschrijven
        : toestand.type === 'opschonen'
          ? t().pilOpschonenStatus(t().opschonenLabel(toestand.label))
          : toestand.type === 'terugval'
            ? t().pilNietOpgeschoond(toestand.soort)
            : toestand.type === 'klaar'
              ? klaarTekst(toestand.soort)
              : toonGekopieerd
                ? t().pilGekopieerd
                : ''
  );
</script>

<svelte:head>
  <title>Mondig</title>
</svelte:head>

<!-- 8 px doorzichtige rand rond de pil, voor de schaduw; het venster is precies deze maat. -->
<div class="pil-donker fixed top-0 left-0 w-max p-2" bind:this={pil}>
  <!-- De statusregio staat er altijd (ook verborgen), zodat elke wijziging gemeld wordt. -->
  <p class="sr-only" role="status">{statusTekst}</p>

  {#if zichtbaar}
    <div
      class="flex min-h-11 max-w-[560px] items-center gap-3 rounded-[12px] border bg-achtergrond py-[7px] pr-2 pl-3.5 text-basis text-tekst shadow-pil {isFout
        ? 'border-fout-tekst'
        : 'border-lijn'}"
    >
      <span
        class="shrink-0 font-kop text-midden leading-none font-semibold tracking-[-0.045em]"
        aria-hidden="true">mondig<span class="text-actie">.</span></span
      >
      <span class="h-5 w-px shrink-0 bg-lijn" aria-hidden="true"></span>

      {#if toestand.type === 'luistert'}
        <span class="inline-flex h-[18px] shrink-0 items-end gap-[3px]" aria-hidden="true">
          {#each niveaus as niveau, i (i)}
            <b
              class="block w-1 rounded-[2px] bg-actie motion-safe:transition-[height] motion-safe:duration-75"
              style:height="{staafHoogte(niveau)}px"
            ></b>
          {/each}
        </span>
        <span
          class="shrink-0 font-kop font-semibold tabular-nums {toestand.restMs !== null
            ? 'text-waarsch'
            : ''}"
          aria-hidden="true">{formatDuur(toestand.duurMs)}</span
        >
        {#if toestand.restMs !== null}
          <span class="text-klein text-waarsch" aria-hidden="true"
            >{t().pilRest(restSeconden(toestand.restMs))}</span
          >
        {:else}
          <span class="inline-flex shrink-0 items-center gap-[5px] text-sectie text-gedempt">
            <Shield size={12} aria-hidden="true" />{t().pilLokaal}
          </span>
        {/if}
        <Button size="icoon" title={t().pilStoppen} aria-label={t().pilStoppen} onclick={stop}
          ><Square size={14} fill="currentColor" aria-hidden="true" /></Button
        >
        <Button
          variant="tweede"
          size="icoon"
          class="text-gedempt hover:text-tekst"
          title={t().annulerenMetEsc}
          aria-label={t().annulerenMetEsc}
          onclick={annuleer}><X size={15} aria-hidden="true" /></Button
        >
      {:else if toestand.type === 'afronden'}
        <!-- Net losgelaten: de meter stil en zonder draaiend teken, tot een seconde voorbij is. -->
        <span class="inline-flex h-[18px] shrink-0 items-end gap-[3px]" aria-hidden="true">
          {#each { length: METER_STAAFJES }, i (i)}
            <b class="block h-1 w-1 rounded-[2px] bg-dim"></b>
          {/each}
        </span>
        <span class="inline-flex shrink-0 items-center gap-[5px] text-sectie text-gedempt">
          <Shield size={12} aria-hidden="true" />{t().pilLokaal}
        </span>
        <Button
          variant="tweede"
          size="icoon"
          class="text-gedempt hover:text-tekst"
          title={t().annulerenMetEsc}
          aria-label={t().annulerenMetEsc}
          onclick={annuleer}><X size={15} aria-hidden="true" /></Button
        >
      {:else if toestand.type === 'verwerkt'}
        <span
          class="size-4 shrink-0 rounded-full border-2 border-lijn border-t-actie motion-safe:animate-spin"
          aria-hidden="true"
        ></span>
        <span>{t().pilUitschrijven}</span>
        <span class="inline-flex shrink-0 items-center gap-[5px] text-sectie text-gedempt">
          <Shield size={12} aria-hidden="true" />{t().pilLokaal}
        </span>
        <Button
          variant="tweede"
          size="icoon"
          class="text-gedempt hover:text-tekst"
          title={t().annulerenMetEsc}
          aria-label={t().annulerenMetEsc}
          onclick={annuleer}><X size={15} aria-hidden="true" /></Button
        >
      {:else if toestand.type === 'opschonen'}
        <!-- Opschonen (ADR 0027): het label wisselt van Lokaal naar dat van de aanbieder. -->
        <span
          class="size-4 shrink-0 rounded-full border-2 border-lijn border-t-actie motion-safe:animate-spin"
          aria-hidden="true"
        ></span>
        <span aria-hidden="true">{t().pilOpschonen}</span>
        <span
          class="inline-flex shrink-0 items-center gap-[5px] text-sectie text-gedempt"
          aria-hidden="true"
        >
          <Shield size={12} aria-hidden="true" />{t().opschonenLabel(toestand.label)}
        </span>
        <Button
          variant="tweede"
          size="icoon"
          class="text-gedempt hover:text-tekst"
          title={t().pilOpschonenStoppen}
          aria-label={t().pilOpschonenStoppen}
          onclick={stopOpschonen}><X size={15} aria-hidden="true" /></Button
        >
      {:else if toestand.type === 'terugval'}
        <Info size={15} class="shrink-0 text-gedempt" aria-hidden="true" />
        <span class="pr-1.5">{t().pilNietOpgeschoond(toestand.soort)}</span>
      {:else if toestand.type === 'klaar'}
        {#if toestand.soort === 'geplakt' || toestand.soort === 'waarschijnlijk'}
          <Check size={15} class="shrink-0 text-actie" aria-hidden="true" />
        {:else}
          <Info size={15} class="shrink-0 text-gedempt" aria-hidden="true" />
        {/if}
        <span>{klaarTekst(toestand.soort)}</span>
        <span class="shrink-0 pr-1.5 text-sectie text-gedempt">
          {toestand.soort === 'geplakt'
            ? `${t().pilDuur(toestand.duurMs / 1000)} · ${t().pilLokaal}`
            : t().pilLokaal}
        </span>
      {:else if isFout}
        <div class="contents" role="alert">
          {#if toonGekopieerd}
            <Check size={15} class="shrink-0 text-actie" aria-hidden="true" />
            <span>{t().pilGekopieerd}</span>
          {:else}
            <CircleAlert size={15} class="shrink-0 text-fout-tekst" aria-hidden="true" />
            <span>{kopieerMislukt ? t().kopierenMislukt : foutTekst}</span>
          {/if}
        </div>
        {#if actie !== null && !toonGekopieerd}
          <Button class="whitespace-nowrap" onclick={voerActieUit}>
            {#if actie === 'kopieren'}<Copy size={15} aria-hidden="true" />{/if}{actieTekst}
          </Button>
        {/if}
        <Button
          variant="spook"
          size="icoon"
          title={t().pilSluiten}
          aria-label={t().pilSluiten}
          onclick={sluit}><X size={15} aria-hidden="true" /></Button
        >
      {/if}
    </div>
  {/if}
</div>
