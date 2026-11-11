<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Toetsen from '#lib/components/Toetsen.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import {
    alsWijzigFout,
    haalSneltoetsInfo,
    haalSneltoetsStatus,
    maakVolgorde,
    sneltoetsControleer,
    sneltoetsVangStart,
    sneltoetsVangStop,
    sneltoetsWijzig,
    type SneltoetsFoutSoort,
    type SneltoetsInfo,
    type SneltoetsSoort,
    type SneltoetsStatus
  } from '#lib/sneltoets.js';
  import { luisterNaarSneltoetsenGewijzigd } from '#lib/gebeurtenissen.js';
  import {
    LEGE_VANG,
    bouwToets,
    drukIn,
    isAfgerond,
    labelVanToets,
    laatLos,
    liveLabel,
    liveVoorlezen,
    type Vangstand
  } from '#lib/sneltoetsvangen.js';
  import { foutVak, rij, rijBediening, rijLabel, vlak } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Een andere sneltoets kiezen. Na "Wijzigen" vangt het venster de toetsen; Rust schort de
  // globale sneltoets op zolang dat duurt en zet hem altijd terug (ook als het venster
  // sluit of de focus verliest). Vastleggen gebeurt als alle toetsen los zijn.

  // Op Dicteren staat dezelfde bediening als kaart bovenaan (`variant="kaart"`): grote toetsen
  // met uitleg en Wijzigen. `bijStatus` meldt de sneltoets die nu geldt, zodat de pagina de
  // nieuwe toets in zijn teksten gebruikt.
  //
  // `soort="opschonen"` is de tweede sneltoets onder "Dictaten opschonen" (ADR 0027): dezelfde
  // bediening, met een eigen standaard. Rust weigert hem als hij gelijk is aan de gewone toets.
  //
  // Het platform en de standaarden komen uit Rust (`sneltoets_info`): op de Mac zijn de toetsen
  // andere (Option+Spatie) en staan ze als symbolen (⌥Spatie).
  interface Props {
    variant?: 'rij' | 'kaart';
    soort?: SneltoetsSoort;
    bijStatus?: (status: SneltoetsStatus | null) => void;
    /** Alleen bij de kaart: een regel onder een lijn in dezelfde kaart (de microfoon). */
    onder?: Snippet;
    /** Alleen bij de kaart: de tweede sneltoets (opgeschoond), als die aan staat. */
    tweede?: string | null;
    /** Dezelfde tweede sneltoets voluit voor een schermlezer (op de Mac `Option Shift Spatie`). */
    tweedeVoorlezen?: string | null;
  }
  let {
    variant = 'rij',
    soort = 'dicteren',
    bijStatus,
    onder,
    tweede = null,
    tweedeVoorlezen = null
  }: Props = $props();

  // De soort verandert niet tijdens het leven van dit onderdeel.
  // svelte-ignore state_referenced_locally
  const isOpschonen = soort === 'opschonen';
  const id = isOpschonen ? 'opschoontoets' : 'sneltoets';

  // Het platform en de standaard van Rust. Tot het antwoord er is (of buiten de app) geldt
  // Windows voor de weergave en is er geen standaard om naar te herstellen.
  let info = $state<SneltoetsInfo | null>(null);
  const platform = $derived(info?.platform ?? 'windows');
  const mac = $derived(platform === 'mac');
  const standaardToets = $derived(isOpschonen ? info?.standaardOpschonen : info?.standaard);

  type Modus = 'rust' | 'vangt' | 'bezig';

  let status = $state<SneltoetsStatus | null>(null);
  let modus = $state<Modus>('rust');
  let vang = $state<Vangstand>(LEGE_VANG);
  // Waarom de combinatie die je net indrukte niet kan.
  let hint = $state<string | null>(null);
  // De toets die al door een ander programma gebruikt wordt (melding 5).
  let bezetToets = $state<string | null>(null);
  let andereFout = $state<string | null>(null);
  let bevestiging = $state<string | null>(null);
  let afgebroken = $state(false);
  let vangvak = $state<HTMLElement | null>(null);
  let wijzigKnop = $state<HTMLElement | null>(null);
  // Een functietoets (F1 tot en met F12) wacht op bevestiging voor hij geregistreerd wordt.
  let wacht = $state<{ gewoneToets: string; toets: string; label: string } | null>(null);
  let tochKnop = $state<HTMLElement | null>(null);

  $effect(() => {
    bijStatus?.(status);
  });

  const standaardLabel = $derived(
    (isOpschonen ? info?.standaardOpschonenLabel : info?.standaardLabel) ?? ''
  );
  const live = $derived(liveLabel(vang.vast.length > 0 ? vang.vast : vang.piek, platform));
  const liveSr = $derived(liveVoorlezen(vang.vast.length > 0 ? vang.vast : vang.piek, platform));
  // Zonder standaard (nog geladen, of buiten de app) valt er niets te herstellen.
  const isStandaard = $derived(info === null || status?.actief === standaardLabel);

  function foutTekst(fout: SneltoetsFoutSoort, toets: string): string {
    switch (fout) {
      case 'windowsToets':
        return t().sneltoetsFoutWindows;
      case 'losseToets':
        return t().sneltoetsFoutLosseToets(mac);
      case 'shiftAlleen':
        return t().sneltoetsFoutShiftAlleen(mac);
      case 'leeg':
      case 'geenHoofdtoets':
        // Het voorbeeld is de standaardtoets van dit platform, uit Rust.
        return t().sneltoetsFoutHoofdtoets(info?.standaardLabel ?? 'Ctrl+Spatie');
      case 'meerdereHoofdtoetsen':
        return t().sneltoetsFoutMeerdere;
      case 'escNietToegestaan':
      case 'onbekend':
        return t().sneltoetsFoutOnbekend;
      case 'gereserveerd':
        return t().sneltoetsFoutGereserveerd;
      case 'werktNiet':
        return t().sneltoetsFoutWerktNiet(toets, mac);
    }
  }

  // Elk antwoord van Rust dat de status of de uitslag van een poging zet, vraagt eerst een
  // nummer. Komt een ouder antwoord later binnen (bijvoorbeeld het stoppen van het vangen na
  // "Toch gebruiken"), dan wordt het genegeerd en overschrijft het de nieuwe toestand niet.
  const volgorde = maakVolgorde();

  async function begin(): Promise<void> {
    // Een antwoord dat nog onderweg is (bijvoorbeeld het stoppen na "Toch gebruiken") hoort
    // bij de vorige ronde en mag niets meer zetten.
    volgorde.ongeldig();
    bezetToets = null;
    andereFout = null;
    bevestiging = null;
    afgebroken = false;
    hint = null;
    wacht = null;
    try {
      // Eerst opschorten: vangen met een nog actieve Ctrl+Spatie start een dictaat.
      await sneltoetsVangStart();
    } catch (e) {
      andereFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      return;
    }
    vang = LEGE_VANG;
    modus = 'vangt';
    await tick();
    vangvak?.focus();
  }

  /** Stopt het vangen zonder te wijzigen; de oude toets komt terug. */
  async function stopVangen(doorFocusverlies: boolean): Promise<void> {
    if (modus !== 'vangt') return;
    modus = 'rust';
    afgebroken = doorFocusverlies;
    const isNogHuidig = volgorde.nieuw();
    try {
      const stand = await sneltoetsVangStop(soort);
      if (isNogHuidig()) status = stand;
    } catch {
      // Rust herstelt de toets ook zelf als het venster sluit of de focus verliest.
    }
    if (!doorFocusverlies && wacht === null) {
      await tick();
      wijzigKnop?.focus();
    }
  }

  /** Een functietoets mag, maar pas na "Toch gebruiken". Intussen komt de oude toets terug. */
  async function vraagBevestiging(
    gewoneToets: string,
    toets: string,
    label: string
  ): Promise<void> {
    wacht = { gewoneToets, toets, label };
    await stopVangen(false);
    await tick();
    tochKnop?.focus();
  }

  function bevestig(): void {
    if (wacht === null) return;
    const { toets, label } = wacht;
    wacht = null;
    void instellen(toets, label);
  }

  /** Registreert een toets: nieuw gevangen, of het herstellen van de standaard. */
  async function instellen(toets: string, label: string): Promise<void> {
    modus = 'bezig';
    const isNogHuidig = volgorde.nieuw();
    let melding: { bezet: boolean; tekst: string } | null = null;
    let stand: SneltoetsStatus | null;
    try {
      stand = await sneltoetsWijzig(toets, soort);
    } catch (e) {
      const wijzigFout = alsWijzigFout(e);
      if (wijzigFout?.soort === 'bezet') melding = { bezet: true, tekst: '' };
      else if (wijzigFout?.soort === 'dubbel') {
        melding = {
          bezet: false,
          tekst: isOpschonen ? t().opschoonToetsDubbel : t().sneltoetsDubbelMetOpschonen
        };
      } else if (wijzigFout?.soort === 'geweigerd') {
        melding = { bezet: false, tekst: foutTekst(wijzigFout.fout, toets) };
      } else melding = { bezet: false, tekst: foutUitCommando(e, t().somsGingErIetsMis).melding };
      // Rust zette de oude toets terug; de stand ophalen laat zien wat er nu geldt.
      stand = await haalSneltoetsStatus(soort).catch(() => null);
    }
    // Is er intussen iets nieuws begonnen, dan hoort dit antwoord niet meer bij het scherm.
    if (!isNogHuidig()) return;
    if (stand !== null) status = stand;
    if (melding === null)
      bevestiging = isOpschonen ? t().opschoonToetsIngesteld(label) : t().sneltoetsIngesteld(label);
    else if (melding.bezet) bezetToets = label;
    else andereFout = melding.tekst;
    modus = 'rust';
    await tick();
    wijzigKnop?.focus();
  }

  async function legVast(): Promise<void> {
    const toets = bouwToets(vang.piek);
    // De poging is verstuurd: begin schoon, ook als het antwoord ongeldig wordt of mislukt.
    vang = LEGE_VANG;
    const isNogHuidig = volgorde.nieuw();
    let oordeel;
    try {
      oordeel = await sneltoetsControleer(toets);
    } catch (e) {
      if (!isNogHuidig() || modus !== 'vangt') return;
      // Stoppen is veiliger dan doorvangen zonder oordeel: de oude toets komt terug en de
      // gebruiker ziet waarom.
      andereFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      void stopVangen(false);
      return;
    }
    // Intussen is er een toets bijgekomen, of het vangen is gestopt: dit oordeel is verouderd.
    if (!isNogHuidig() || modus !== 'vangt') return;
    if (oordeel.uitkomst === 'fout') {
      // Blijf vangen: de gebruiker probeert het meteen opnieuw.
      hint = foutTekst(oordeel.fout, toets);
      return;
    }
    const label = labelVanToets(toets, platform);
    if (oordeel.uitkomst === 'waarschuwing') {
      void vraagBevestiging(oordeel.toets, toets, label);
      return;
    }
    void instellen(toets, label);
  }

  async function herstel(): Promise<void> {
    wacht = null;
    bezetToets = null;
    andereFout = null;
    bevestiging = null;
    afgebroken = false;
    if (modus === 'vangt') {
      // De toets is nog opgeschort; wijzigen naar de standaard zet hem meteen goed.
      modus = 'bezig';
    }
    // Zonder standaard van Rust (nog niet geladen) valt er niets te herstellen.
    if (standaardToets === undefined) return;
    await instellen(standaardToets, standaardLabel);
  }

  // Zolang we vangen, lopen alle toetsen langs deze handlers (in de vangfase, vóór de
  // knop of pagina eraan komt) en doet geen enkele toets nog iets anders.
  $effect(() => {
    if (modus !== 'vangt') return;
    const omlaag = (event: KeyboardEvent): void => {
      event.preventDefault();
      if (event.code === 'Escape') {
        void stopVangen(false);
        return;
      }
      if (event.repeat || !event.code) return;
      hint = null;
      // Een oordeel dat nog onderweg is hoort bij de vorige poging.
      volgorde.ongeldig();
      vang = drukIn(vang, event.code);
    };
    const omhoog = (event: KeyboardEvent): void => {
      event.preventDefault();
      if (!event.code) return;
      vang = laatLos(vang, event.code);
      if (isAfgerond(vang)) void legVast();
    };
    const focusWeg = (): void => {
      void stopVangen(true);
    };
    window.addEventListener('keydown', omlaag, true);
    window.addEventListener('keyup', omhoog, true);
    window.addEventListener('blur', focusWeg);
    return () => {
      window.removeEventListener('keydown', omlaag, true);
      window.removeEventListener('keyup', omhoog, true);
      window.removeEventListener('blur', focusWeg);
    };
  });

  /** Haalt de stand opnieuw op, maar niet midden in het vangen of wijzigen. */
  function ververs(): void {
    const isNogHuidig = volgorde.nieuw();
    haalSneltoetsStatus(soort)
      .then((stand) => {
        if (isNogHuidig() && modus === 'rust') status = stand;
      })
      .catch(() => {
        if (isNogHuidig() && modus === 'rust') status = null;
      });
  }

  onMount(() => {
    ververs();
    void haalSneltoetsInfo()
      .then((antwoord) => {
        info = antwoord;
      })
      .catch(() => undefined);
    // De andere sneltoets of de keuze "Dictaten opschonen" kan deze stand veranderen.
    let weg = false;
    let stop: (() => void) | null = null;
    void luisterNaarSneltoetsenGewijzigd(ververs).then((s) => {
      if (weg) s();
      else stop = s;
    });
    return () => {
      weg = true;
      stop?.();
      // Het scherm verlaten tijdens het vangen: de oude toets terug.
      if (modus !== 'rust') sneltoetsVangStop(soort).catch(() => undefined);
    };
  });
</script>

{#snippet wijzigKnopTekening()}
  <Button
    variant="tweede"
    bind:ref={wijzigKnop}
    onclick={begin}
    aria-describedby="{id}-kop {id}-hulp"
    aria-disabled={modus === 'bezig' ? 'true' : undefined}>{t().sneltoetsWijzigen}</Button
  >
{/snippet}

<!-- Als rij is de buitenste div een gewoon blok: zo ziet de divide-y van Instellingen hem als kind en komt de scheidingslijn naar Microfoon. -->
<div class={variant === 'kaart' ? `${vlak} flex flex-col` : undefined}>
  <div
    class={variant === 'kaart'
      ? 'flex flex-col gap-3.5 px-5 py-[18px] max-[639px]:px-3.5'
      : `${rij} flex-col items-stretch!`}
  >
    <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
      {#if variant === 'kaart'}
        <!-- De kaart: toetsen, uitleg en Wijzigen op één regel. De kop is er voor schermlezers. -->
        <span id="sneltoets-kop" class="sr-only">{t().sneltoetsKop}</span>
        {#if modus !== 'vangt'}
          <div class="flex min-w-0 flex-[1_1_100%] flex-wrap items-center gap-x-5 gap-y-2.5">
            {#if status?.actief}
              <span class="sr-only"
                >{t().sneltoetsUitleg(status.actiefVoorlezen ?? status.actief)}</span
              >
              <span aria-hidden="true"><Toetsen label={status.actief} groot /></span>
              <span
                id="sneltoets-hulp"
                class="min-w-[14rem] flex-[1_1_14rem] text-midden text-gedempt max-[639px]:order-3 max-[639px]:min-w-0 max-[639px]:basis-full max-[639px]:text-klein"
                aria-hidden="true">{t().dicterenRustUitleg}</span
              >
            {:else if status !== null}
              <span
                id="sneltoets-hulp"
                class="min-w-[14rem] flex-[1_1_14rem] text-midden text-gedempt"
                >{t().dicterenGeenToets}</span
              >
            {:else}
              <span id="sneltoets-hulp" class="min-w-[14rem] flex-[1_1_14rem]"></span>
            {/if}
            {#if bezetToets !== null}
              <span class="text-klein text-gedempt">{t().sneltoetsOngewijzigd}</span>
            {/if}
            <span class="ml-auto flex items-center gap-1">
              {#if status !== null && !isStandaard}
                <!-- Klein naast Wijzigen, alleen als er een eigen sneltoets staat. -->
                <Button
                  variant="spook"
                  class="text-klein font-medium text-gedempt"
                  title={t().sneltoetsHerstellen(standaardLabel)}
                  aria-label={t().sneltoetsHerstellen(standaardLabel)}
                  onclick={herstel}>{t().sneltoetsHerstellenKort}</Button
                >
              {/if}
              {@render wijzigKnopTekening()}
            </span>
            {#if tweede && status?.actief}
              <!-- De tweede sneltoets (ADR 0027) op een eigen regel: de toetsjes en wat hij doet. -->
              <span
                class="flex basis-full items-center gap-2 text-klein text-gedempt max-[639px]:order-4"
              >
                <span class="sr-only"
                  >{t().opschoonToetsOpDicterenAria(tweedeVoorlezen ?? tweede)}</span
                >
                <span class="flex items-center gap-2" aria-hidden="true">
                  <Toetsen label={tweede} />
                  <span>{t().opschoonToetsOpDicteren}</span>
                </span>
              </span>
            {/if}
          </div>
        {/if}
      {:else}
        <div class={rijLabel}>
          <RijKop
            kopId="{id}-kop"
            label={isOpschonen ? t().opschoonToetsKop : t().sneltoetsKop}
            hulpId="{id}-hulp"
            hulp={isOpschonen ? t().opschoonToetsHulp : t().sneltoetsHoudTik}
          />
          {#if isOpschonen && status !== null && !status.actief && modus === 'rust'}
            <!-- De tweede toets staat uit: gelijk aan de gewone, of bezet door een ander programma. -->
            <span class="text-klein text-waarsch"
              >{status.dubbel
                ? t().opschoonToetsDubbel
                : t().opschoonToetsBezet(status.gewenst)}</span
            >
          {/if}
        </div>
        {#if modus !== 'vangt'}
          <div class={rijBediening}>
            {#if status?.actief}
              <span class="sr-only">{status.actiefVoorlezen ?? status.actief}</span>
              <span aria-hidden="true"><Toetsen label={status.actief} /></span>
            {:else}
              <b class="font-semibold">-</b>
            {/if}
            {#if bezetToets !== null}
              <span class="text-klein text-gedempt">{t().sneltoetsOngewijzigd}</span>
            {/if}
            {@render wijzigKnopTekening()}
          </div>
        {/if}
      {/if}
    </div>

    {#if wacht !== null}
      <div class={foutVak} role="alert">
        <div class="flex items-start gap-2.5">
          <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
          <div>
            <b class="mb-0.5 block font-semibold">{t().sneltoetsFunctieKop(wacht.label)}</b>
            {t().sneltoetsFunctieTekst(wacht.gewoneToets)}
          </div>
        </div>
        <div class="flex flex-wrap gap-2">
          <Button variant="tweede" bind:ref={tochKnop} onclick={bevestig}
            >{t().sneltoetsToch}</Button
          >
          <Button variant="tweede" onclick={begin}>{t().sneltoetsAndere}</Button>
        </div>
      </div>
    {/if}
    {#if bezetToets !== null}
      <div class={foutVak} role="alert">
        <div class="flex items-start gap-2.5">
          <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
          <div>
            <b class="mb-0.5 block font-semibold">{t().sneltoetsBezetKop}</b>
            {t().sneltoetsBezetTekst(bezetToets)}
          </div>
        </div>
        <Button variant="tweede" onclick={begin}>{t().opnieuwProberen}</Button>
      </div>
    {/if}
    {#if andereFout !== null}
      <div class={foutVak} role="alert">
        <p class="m-0 flex items-start gap-2.5">
          <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
          {andereFout}
        </p>
      </div>
    {/if}

    {#if modus === 'vangt'}
      <div
        bind:this={vangvak}
        tabindex="-1"
        role="status"
        class="flex flex-col items-center gap-2 rounded-groot border-2 border-dashed border-actie bg-verhoogd p-5 text-center text-midden"
      >
        {t().sneltoetsVang}
        <span class="flex min-h-[34px] items-center justify-center" lang="nl">
          {#if live}
            <span class="sr-only">{liveSr}</span>
            <span aria-hidden="true"><Toetsen label={live} groot /></span>
          {/if}
        </span>
        {#if hint}
          <span class="block font-medium text-fout-tekst">{hint}</span>
        {/if}
        <span class="text-klein text-gedempt">{t().sneltoetsVangHulp}</span>
      </div>
    {/if}

    {#if modus === 'vangt' || (variant === 'rij' && !isStandaard)}
      <Button variant="spook" class="-ml-2 self-start" onclick={herstel}
        >{t().sneltoetsHerstellen(standaardLabel)}</Button
      >
    {/if}

    <!-- Altijd aanwezig voor de bevestiging na het wijzigen of afbreken. -->
    <p
      class="m-0 text-klein text-gelukt"
      class:-mt-2={modus === 'vangt' || !bevestiging}
      role="status"
    >
      {#if modus !== 'vangt'}{bevestiging ?? ''}{/if}
    </p>
    {#if afgebroken && modus === 'rust'}
      <p class="m-0 text-klein text-gedempt">{t().sneltoetsVangAfgebroken}</p>
    {/if}
  </div>
  {#if variant === 'kaart' && onder}
    {@render onder()}
  {/if}
</div>
