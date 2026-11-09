<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount, tick } from 'svelte';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Copy from '@lucide/svelte/icons/copy';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import X from '@lucide/svelte/icons/x';
  import DictatenLijst from '#lib/components/DictatenLijst.svelte';
  import InstellingSneltoets from '#lib/components/InstellingSneltoets.svelte';
  import MicrofoonRegel from '#lib/components/MicrofoonRegel.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import { dictaatAnnuleer, microfooninstellingOpen } from '#lib/commandos.js';
  import { bijnaVol, formatDuur, isActief, klokTijd, plakSoort } from '#lib/dictaat.js';
  import type { PlakUitkomst, Weergave } from '#lib/dictaat.js';
  import { dictaatStand } from '#lib/dictaatstand.svelte.js';
  import { laatsteLosTonen } from '#lib/dictaten.js';
  import { dictatenStand } from '#lib/dictatenstand.svelte.js';
  import { herstelFocus } from '#lib/focus.js';
  import { kopieerNaarKlembord } from '#lib/klembord.js';
  import { haalSneltoetsStatus, type SneltoetsStatus } from '#lib/sneltoets.js';
  import { opschonenInstellingen } from '#lib/opschonen.js';
  import { luisterNaarSneltoetsenGewijzigd } from '#lib/gebeurtenissen.js';
  import { foutVak, sectieKop, vlak } from '#lib/stijl.js';
  import { meldingTekst, type ProefResultaat } from '#lib/taak.js';

  // De toestand en de tekst komen uit de gedeelde stand (één bron, ook voor de strook op
  // andere schermen); dit paneel tekent ze alleen en regelt het kopiëren. `toonRust` is
  // onwaar zolang het spraakmodel ontbreekt: dan staat daar de downloadmelding.
  let { sneltoets, toonRust = true }: { sneltoets: SneltoetsStatus | null; toonRust?: boolean } =
    $props();

  const weergave = $derived<Weergave>(dictaatStand.weergave);
  const dictaat = $derived(dictaatStand.dictaat);
  const plak = $derived(weergave.type === 'klaar' ? dictaatStand.plak : null);
  const plakFout = $derived(plak !== null && plakSoort(plak) === 'fout');
  // De dictaatgeschiedenis (ADR 0021): bij 24 uur of 7 dagen staat hier de lijst "Je dictaten",
  // bij Niet bewaren alleen het laatste dictaat uit het geheugen, zoals voor de lijst bestond.
  const bewaart = $derived(dictatenStand.termijn !== 'niet');
  const lijstLeeg = $derived(
    bewaart && dictatenStand.geladen && dictatenStand.zichtbaar.length === 0
  );
  // Het laatste dictaat als los blok boven de lijst, als het daar niet als nieuwste in staat.
  const losLaatste = $derived(
    bewaart && laatsteLosTonen(dictatenStand.laatste, dictatenStand.zichtbaar)
      ? dictatenStand.laatste
      : null
  );
  const INSTELLING_DICTATEN = '#dictaten-bewaren';
  let nu = $state(Date.now());
  let gekopieerd = $state(false);
  // Het proefveld: de globale sneltoets plakt niet in Mondig zelf, dus vult het veld zichzelf
  // als het de focus had toen het dictaat begon en Mondig vooraan bleef.
  let proefveld = $state<HTMLTextAreaElement | null>(null);
  let proefOpen = $state(false);
  // Staat de lijst leeg (de eerste keer), dan staat het proefveld boven de lijst. Dat blijft zo
  // tot de gebruiker van scherm wisselt, ook als de lijst zich vult.
  let proefBoven = $state(false);
  // Boven de lijst zodra de lijst leeg is (ook voor het effect hieronder loopt) of was.
  const proefBovenNu = $derived(bewaart && (lijstLeeg || proefBoven));
  $effect(() => {
    if (lijstLeeg && !proefBoven) {
      proefBoven = true;
      proefOpen = true;
    }
  });
  let proefHadFocus = false;
  let inProefveld = $state(false);
  let kopieerMislukt = $state(false);
  // Pas na een seconde een draaiend teken: korter dan dat voelt een reactie direct (ADR 0012).
  let draait = $state(false);

  // De sneltoets die de kaart meldt na een wijziging gaat voor die van de pagina (die kent
  // alleen de stand bij het openen).
  let gewijzigd = $state<SneltoetsStatus | null>(null);
  function zetGewijzigd(stand: SneltoetsStatus | null): void {
    gewijzigd = stand;
  }
  const toets = $derived((gewijzigd ?? sneltoets)?.actief ?? null);
  const luistert = $derived(weergave.type === 'luistert' ? weergave : null);

  const kaart = `${vlak} px-5 py-4 text-midden`;

  // Dit paneel staat op het startscherm: zolang het er is, hoeft de strook op andere
  // schermen niets te melden over wat hier te zien is.
  // De tweede sneltoets (ADR 0027): alleen bij "Met een eigen sneltoets", en alleen als hij
  // echt aan staat. Een fout hier verbergt alleen de regel.
  let tweede = $state<string | null>(null);
  let tweedeVoorlezen = $state<string | null>(null);
  async function laadTweede(): Promise<void> {
    try {
      const info = await opschonenInstellingen();
      if (info.stand !== 'eigenSneltoets') {
        tweede = null;
        tweedeVoorlezen = null;
        return;
      }
      const stand = await haalSneltoetsStatus('opschonen');
      tweede = stand?.actief ?? null;
      tweedeVoorlezen = stand?.actiefVoorlezen ?? null;
    } catch {
      tweede = null;
      tweedeVoorlezen = null;
    }
  }

  onMount(() => {
    dictaatStand.zetStartscherm(true);
    void laadTweede();
    // Na elke wijziging van een sneltoets of van "Dictaten opschonen" opnieuw.
    let weg = false;
    let stop: (() => void) | null = null;
    void luisterNaarSneltoetsenGewijzigd(() => void laadTweede()).then((s) => {
      if (weg) s();
      else stop = s;
    });
    return () => {
      weg = true;
      stop?.();
      dictaatStand.zetStartscherm(false);
    };
  });

  // Een nieuw dictaat wist de melding van het vorige kopieeractie.
  $effect(() => {
    if (weergave.type === 'luistert') {
      gekopieerd = false;
      kopieerMislukt = false;
      inProefveld = false;
      proefHadFocus = proefveld !== null && document.activeElement === proefveld;
    }
  });

  // Mondig stond vooraan: de tekst staat nergens geplakt. Had het proefveld de focus, dan
  // komt de tekst daar, op de plek van de cursor.
  $effect(() => {
    if (plak !== 'in_mondig' || dictaat === null || !proefHadFocus || proefveld === null) return;
    if (inProefveld) return;
    inProefveld = true;
    proefHadFocus = false;
    proefveld.setRangeText(`${dictaat} `, proefveld.selectionStart, proefveld.selectionEnd, 'end');
  });

  /** Naar de rij Dictaten bewaren in Instellingen. */
  function naarDictatenBewaren(): void {
    void goto(resolve('/instellingen')).then(() => {
      // De rij staat nu op het scherm: scroll ernaartoe en zet de focus op de keuzelijst.
      document.getElementById('dictaten-bewaren')?.scrollIntoView({ block: 'center' });
      document.getElementById('dictaten-keuze')?.focus();
    });
  }

  /** Toont het proefveld en zet de cursor erin. */
  async function openProefveld(): Promise<void> {
    proefOpen = true;
    await tick();
    proefveld?.focus();
  }

  function uitkomstMelding(uitkomst: ProefResultaat['uitkomst']): string {
    if (uitkomst === 'geen_geluid') return t().geenGeluid;
    if (uitkomst === 'geen_tekst') return t().geenTekst;
    return t().tekstVerdacht;
  }

  function plakMelding(uitkomst: PlakUitkomst): string {
    switch (uitkomst) {
      case 'geplakt':
        return t().plakGeplakt;
      case 'waarschijnlijk_geplakt':
        return t().plakWaarschijnlijk;
      case 'op_klembord':
        return t().plakOpKlembord;
      case 'beheerrechten':
        return t().plakBeheerrechten;
      case 'toetsen_ingedrukt':
        return t().plakToetsen;
      case 'geen_venster':
        return t().plakGeenVenster;
      case 'niet_gelezen':
        return t().plakNietGelezen;
      case 'niet_geplakt':
        return t().plakNietGeplakt;
      case 'mislukt':
        return t().plakMislukt;
      case 'in_mondig':
        return inProefveld ? t().plakInProefveld : t().plakInMondig;
    }
  }

  async function annuleer(): Promise<void> {
    if (!isActief(weergave) || !('taak' in weergave)) return;
    try {
      await dictaatAnnuleer(weergave.taak);
    } catch {
      // Was het al afgelopen, dan meldt het toestandsmodel dat zelf.
    }
  }

  async function kopieer(tekst: string | null = dictaat): Promise<void> {
    if (tekst === null) return;
    kopieerMislukt = false;
    gekopieerd = await kopieerNaarKlembord(tekst);
    kopieerMislukt = !gekopieerd;
  }

  // De klok loopt alleen tijdens het luisteren; de duur in beeld is niet voor de schermlezer
  // (elke seconde een melding zou storen), de waarschuwing na negen minuten wel.
  $effect(() => {
    if (weergave.type !== 'luistert') return;
    nu = Date.now();
    const timer = setInterval(() => {
      nu = Date.now();
    }, 1000);
    return () => {
      clearInterval(timer);
    };
  });

  $effect(() => {
    if (weergave.type !== 'verwerkt') return;
    const timer = setTimeout(() => {
      draait = true;
    }, 1000);
    return () => {
      clearTimeout(timer);
      draait = false;
    };
  });

  // Verdwijnt de knop met focus bij een toestandswissel, dan naar de hoofdactie (UX-poort).
  // Alleen als de gebruiker in dit venster is: dicteert hij in een ander programma, dan
  // verplaatst Mondig niets.
  let vorigeType: Weergave['type'] | null = null;
  $effect(() => {
    const type = weergave.type;
    if (vorigeType !== null && type !== vorigeType && document.hasFocus()) {
      void tick().then(() => {
        herstelFocus(document);
      });
    }
    vorigeType = type;
  });
</script>

<div class="flex flex-col gap-6">
  {#if sneltoets?.actief === null}
    <div class={foutVak} role="alert">
      <p class="m-0 flex items-start gap-2.5">
        <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
        {t().sneltoetsGeen(sneltoets.geprobeerd.join(', '))}
      </p>
    </div>
  {:else if sneltoets?.isTerugval && sneltoets.actief}
    <p class="m-0 text-gedempt">
      {sneltoets.overgezet
        ? t().sneltoetsOvergezet(sneltoets.gewenst, sneltoets.actief)
        : t().sneltoetsTerugval(sneltoets.gewenst, sneltoets.actief)}
    </p>
  {/if}

  <!-- De luisterkaart (ontwerp B1). De statusregio staat er altijd, zodat een schermlezer elke
       wijziging meldt zonder dat de focus verspringt; Annuleren staat er buiten, in hetzelfde vlak. -->
  <div
    class={isActief(weergave)
      ? `${vlak} flex flex-wrap items-stretch justify-between gap-x-4 gap-y-3 px-5 py-[18px]`
      : 'contents'}
  >
    <!-- In rust staat alleen tekst voor schermlezers of een korte melding in deze regio; zonder
         zichtbare inhoud trekt de negatieve marge de ruimte van de `gap` weer weg. -->
    <div
      role="status"
      class="flex min-w-0 flex-col gap-3 empty:hidden {isActief(weergave)
        ? 'flex-[1_1_18rem]'
        : '[&:not(:has(>:not(.sr-only)))]:-mt-6'}"
    >
      {#if luistert}
        <div class="flex items-center gap-2.5">
          <span
            class="size-2 shrink-0 rounded-full bg-actie motion-safe:animate-pulse"
            aria-hidden="true"
          ></span>
          <b class="font-semibold">{t().dictaatLuistert}</b>
        </div>
        {#if toets}
          <span class="text-gedempt">{t().dictaatHoe(toets)}</span>
        {/if}
        {#if bijnaVol(luistert.sinds, nu)}
          <span class="font-medium">{t().dictaatBijnaVol}</span>
        {/if}
      {:else if weergave.type === 'verwerkt'}
        <div class="flex items-start gap-2.5">
          {#if draait}
            <LoaderCircle
              size={16}
              class="mt-0.5 shrink-0 text-gedempt motion-safe:animate-spin"
              aria-hidden="true"
            />
          {/if}
          <span>{t().bezigMetUitschrijven}</span>
        </div>
      {:else if weergave.type === 'klaar'}
        <!-- Zichtbaar staat dit naast Kopiëren, bij de tekst (zie hieronder). -->
        <p class="sr-only">
          {t().klaarIn(weergave.uitslag.duurMs / 1000)}{weergave.uitslag.soevereiniteit === 'lokaal'
            ? `. ${t().lokaalVerwerkt}`
            : ''}
        </p>
        {#if weergave.uitslag.tekst === null}
          <div class={kaart}>{uitkomstMelding(weergave.uitslag.uitkomst)}</div>
        {/if}
      {:else if weergave.type === 'geannuleerd'}
        <p class="m-0 text-gedempt">{t().dictaatGeannuleerd}</p>
      {/if}
    </div>

    {#if isActief(weergave)}
      <!-- De duur rechtsboven, Annuleren rechtsonder. De duur staat buiten de statusregio:
           elke seconde een melding zou storen. -->
      <div class="ml-auto flex flex-col items-end justify-between gap-3">
        {#if luistert}
          <span
            class="font-kop text-groot leading-none font-semibold tabular-nums"
            aria-hidden="true">{formatDuur(nu - luistert.sinds)}</span
          >
        {/if}
        <Button variant="tweede" onclick={annuleer} data-hoofdactie="">
          <X size={16} aria-hidden="true" />{t().annuleren}</Button
        >
      </div>
    {/if}
  </div>

  {#if toonRust && !isActief(weergave)}
    <!-- Sneltoets met de microfoonregel eronder, in één kaart. Wijzigen is dezelfde bediening
         als in Instellingen; de microfoon kies je daar, hier staat alleen de naam en de test. -->
    <InstellingSneltoets variant="kaart" bijStatus={zetGewijzigd} {tweede} {tweedeVoorlezen}>
      {#snippet onder()}
        <MicrofoonRegel />
      {/snippet}
    </InstellingSneltoets>
  {/if}

  {#if weergave.type === 'fout'}
    <div class={foutVak} role="alert">
      <p class="m-0 flex items-start gap-2.5">
        <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
        {meldingTekst(weergave, t().appNietBijgewerkt)}
      </p>
      {#if weergave.vervolg === 'opnieuw_proberen' && toets}
        <span>{t().dictaatOpnieuw(toets)}</span>
      {:else if weergave.vervolg === 'opnieuw_downloaden'}
        <Button href={resolve('/modellen')} data-hoofdactie="">{t().spraakmodelDownloaden}</Button>
      {:else if weergave.vervolg === 'ander_model_kiezen'}
        <Button href={resolve('/modellen')} data-hoofdactie="">{t().anderModelKiezen}</Button>
      {:else if weergave.vervolg === 'microfooninstelling_openen'}
        <!-- Privacy-instelling als Windows de microfoon blokkeert, anders Geluid (Rust kiest). -->
        <Button onclick={() => void microfooninstellingOpen()} data-hoofdactie=""
          >{t().microfooninstellingOpenen}</Button
        >
      {/if}
    </div>
  {/if}

  {#if plak !== null && plakFout}
    <!-- De tekst staat niet waar de gebruiker typt: een melding met één vervolgactie. -->
    <div class={foutVak} role="alert">
      <p class="m-0 flex items-start gap-2.5">
        <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
        {plakMelding(plak)}
      </p>
      <Button onclick={() => kopieer()} data-hoofdactie=""
        ><Copy size={16} aria-hidden="true" />{t().kopieren}</Button
      >
    </div>
  {/if}

  {#if !bewaart && dictaat !== null && !isActief(weergave)}
    <!-- Niet bewaren: het laatste dictaat uit het geheugen, met de weg naar de instelling. -->
    {@render laatsteKaart(dictaat, dictaatStand.dictaatOm, true)}
  {/if}

  {#if bewaart && !isActief(weergave)}
    <!-- Wat er met het dictaat van net gebeurde (klaar, geplakt of niet); de tekst zelf staat
         hieronder in de lijst. Altijd aanwezig, zodat een schermlezer het meldt. -->
    <div role="status" class="flex flex-col gap-1 text-klein [&:not(:has(>p))]:-mt-6">
      {#if weergave.type === 'klaar' && weergave.uitslag.tekst !== null}
        <p class="m-0 flex flex-wrap items-center gap-x-2.5 text-gedempt" aria-hidden="true">
          <span class="size-2 shrink-0 rounded-full bg-gelukt"></span>
          <span>{t().klaarIn(weergave.uitslag.duurMs / 1000)}</span>
          {#if weergave.uitslag.soevereiniteit === 'lokaal'}
            <span class="text-dim">&middot;</span>
            <span>{t().lokaalVerwerkt}</span>
          {/if}
        </p>
      {/if}
      {#if dictaat !== null && plak !== null && !plakFout}
        <p class="m-0 {plakSoort(plak) === 'gelukt' ? 'text-gedempt' : ''}">
          {plakMelding(plak)}
        </p>
      {/if}
      {#if dictaat !== null && plak !== null && dictaatStand.vorigKlembord === 'kwijt'}
        <p class="m-0">{t().plakVorigKlembordKwijt}</p>
      {:else if dictaat !== null && plak !== null && dictaatStand.vorigKlembord === 'onvolledig'}
        <p class="m-0">{t().plakVorigKlembordOnvolledig}</p>
      {/if}
      {#if gekopieerd}
        <p class="m-0 text-gelukt">{t().tekstGekopieerd}</p>
      {/if}
    </div>
  {/if}

  {#if toonRust && toets && proefBovenNu}
    <!-- Eerste keer: het proefveld staat meteen open en blijft staan, ook tijdens het dictaat
         en nadat de lijst gevuld is (anders zou de eerste proef nergens landen). -->
    {@render proefblok()}
  {/if}

  {#if bewaart && !isActief(weergave)}
    {#if dictatenStand.geladen && losLaatste !== null}
      <!-- Het laatste dictaat staat niet (als nieuwste) in de lijst, bijvoorbeeld omdat het
           schrijven naar schijf mislukte: de tekst staat dan alleen in het geheugen. -->
      {@render laatsteKaart(
        losLaatste.tekst,
        dictaat === losLaatste.tekst ? dictaatStand.dictaatOm : null,
        false
      )}
    {/if}

    {#if dictatenStand.geladen}
      <DictatenLijst
        dictaten={dictatenStand.zichtbaar}
        termijn={dictatenStand.termijn === '7d' ? '7d' : '24u'}
        bewaard={dictatenStand.bewaard}
        nu={dictatenStand.nu}
        bijVerwijder={(id: string) => dictatenStand.verwijder(id)}
        bijWisAlles={() => dictatenStand.wisAlles()}
        bijHoeLang={naarDictatenBewaren}
      />
    {/if}
  {/if}

  {#if toonRust && toets && !proefBovenNu}
    <!-- Het proefveld zit achter een knop: wie al dicteert, gebruikt het zelden. -->
    {#if proefOpen}
      {@render proefblok()}
    {:else}
      <div>
        <Button variant="spook" class="-ml-2" aria-expanded="false" onclick={openProefveld}
          >{t().proefveldOpenen}<ChevronRight size={16} aria-hidden="true" /></Button
        >
      </div>
    {/if}
  {/if}

  {#if kopieerMislukt}
    <div class={foutVak} role="alert">{t().kopierenMislukt}</div>
  {/if}
</div>

{#snippet proefblok()}
  <section class="flex flex-col" aria-labelledby="proefveld-kop">
    <h2 id="proefveld-kop" class={sectieKop}>{t().proefveldKop}</h2>
    <textarea
      bind:this={proefveld}
      id="proefveld"
      rows="3"
      aria-labelledby="proefveld-kop"
      placeholder={t().proefveldPlaats(toets ?? '')}
      class="min-h-[84px] w-full max-w-[680px] resize-y rounded-klein border border-rand-veld bg-verhoogd px-3 py-2.5 text-basis text-tekst placeholder:text-gedempt"
    ></textarea>
  </section>
{/snippet}

{#snippet laatsteKaart(tekst: string, om: number | null, metUitleg: boolean)}
  <!-- Het laatste dictaat (ontwerp N-0, "Niet bewaren"): kop met de tijd, een vlak met de tekst
       gedempt en een gewone tweede knop Kopiëren, en eventueel de dimme uitleg met Wijzigen. -->
  <section class="flex flex-col gap-2" aria-labelledby="laatste-dictaat-kop">
    <h2 id="laatste-dictaat-kop" class="m-0 font-tekst text-basis font-semibold text-tekst">
      {t().laatsteDictaatKop}{#if om !== null}<span class="ml-1 font-normal text-dim">
          · {klokTijd(om, huidigeTaal())}</span
        >{/if}
    </h2>
    <div class="{vlak} flex items-start gap-3 py-3 pr-3 pl-4">
      <span class="min-w-0 flex-1 text-gedempt whitespace-pre-wrap" lang="nl">{tekst}</span>
      <Button
        variant="tweede"
        onclick={() => kopieer(tekst)}
        data-hoofdactie={weergave.type === 'klaar' && !plakFout ? '' : undefined}
        ><Copy size={16} aria-hidden="true" />{t().kopieren}</Button
      >
    </div>
    {#if metUitleg}
      <!-- Alleen wat de gebruiker moet weten: een plakmelding die geen "gelukt" is (de tekst
           staat op het klembord of in het proefveld), en of het kopieren lukte. -->
      <div role="status" class="flex flex-col gap-1 text-klein empty:hidden">
        {#if plak !== null && !plakFout && plakSoort(plak) !== 'gelukt'}
          <p class="m-0">{plakMelding(plak)}</p>
        {/if}
        {#if plak !== null && dictaatStand.vorigKlembord === 'kwijt'}
          <p class="m-0">{t().plakVorigKlembordKwijt}</p>
        {:else if plak !== null && dictaatStand.vorigKlembord === 'onvolledig'}
          <p class="m-0">{t().plakVorigKlembordOnvolledig}</p>
        {/if}
        {#if gekopieerd}
          <p class="m-0 text-gelukt">{t().tekstGekopieerd}</p>
        {/if}
      </div>
      <p class="m-0 flex flex-wrap items-center gap-x-1 text-klein text-dim">
        {t().dictatenNietBewaard}
        <Button
          href={`${resolve('/instellingen')}${INSTELLING_DICTATEN}`}
          variant="spook"
          class="min-h-8 px-1.5 text-klein underline underline-offset-[3px]"
          >{t().dictatenWijzigen}</Button
        >
      </p>
    {/if}
  </section>
{/snippet}
