<script lang="ts">
  import { tick } from 'svelte';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Download from '@lucide/svelte/icons/download';
  import Globe from '@lucide/svelte/icons/globe';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import Info from '@lucide/svelte/icons/info';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.js';
  import ModelBalkjes from '#lib/components/ModelBalkjes.svelte';
  import { Progress } from '#lib/components/ui/progress/index.js';
  import { huidigeTaal, t } from '#lib';
  import { leesbareGrootte, leesbareVoortgang } from '#lib/grootte.js';
  import { GEBRUIKEN, rijStand, talenSamenvatting, type ModelRegel } from '#lib/modellen.js';
  import { procentVan } from '#lib/downloadrij.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { chipNieuw, meldingVak, vlak } from '#lib/stijl.js';
  import type { Foutmelding } from '#lib/taak.js';

  // Eén model in de beheerlijst van Modellen (gebruikerstest na N-C, naar het voorbeeld van Handy):
  // rol en naam, één zin, rechts de balkjes, onderaan talen, grootte en Verwijderen of Downloaden.
  // Kiezen voor een functie gebeurt in de kaarten bovenaan (Wijzigen); hier alleen beheer. Een
  // model in gebruik krijgt een rand en zegt waarvoor.

  interface Props {
    regel: ModelRegel;
    /** Moet de badge "Nieuw" erop? (Staat vast zolang de pagina open is.) */
    nieuw: boolean;
  }
  let { regel, nieuw }: Props = $props();

  const taal = $derived(huidigeTaal());
  const tekst = $derived(t().modelTekst(regel.id));
  const naam = $derived(tekst?.naam ?? regel.naam);
  const rol = $derived(
    regel.soort === 'sprekers' ? t().sprekermodelNaam : t().modelRolNaam(regel.rol)
  );
  const item = $derived(modellenStand.itemVoor(regel.id));
  const stand = $derived(rijStand(regel, modellenStand.isActief(regel.id)));
  const fase = $derived(modellenStand.knopFase(regel.id));
  const procent = $derived(procentVan(item?.voortgang));
  const resterend = $derived(modellenStand.resterendVoor(regel.id));
  const onderbroken = $derived(item?.toestand.type === 'onderbroken');
  const downloadFout = $derived(modellenStand.foutVoor(regel.id));
  const talen = $derived(talenSamenvatting(regel));
  const rest = $derived(Math.max(0, regel.grootteBytes - regel.aanwezigBytes));
  const vrijBytes = $derived(modellenStand.overzicht?.vrijBytes ?? null);
  const inGebruik = $derived(regel.status === 'compleet' && regel.inGebruikVoor.length > 0);
  // "Verder downloaden" maakt af wat bij de eerste start gekozen werd; zegt dat als het een
  // model in gebruik neemt, zodat niemand raadt wat er na afloop verandert.
  const naHervatten = $derived(
    stand.download === 'verder' && regel.daarnaBijHervatten.length > 0
      ? t().naHervattenGebruik(
          regel.daarnaBijHervatten.includes('dicteren'),
          regel.daarnaBijHervatten.includes('bestanden')
        )
      : null
  );

  let bevestig = $state(false);
  let bezig = $state(false);
  let actieFout = $state<Foutmelding | null>(null);
  let verwijderKnop = $state<HTMLElement | null>(null);

  function gebruikTekst(): string {
    const voor = regel.inGebruikVoor;
    if (voor.length === GEBRUIKEN.length) return t().inGebruikVoorBeide;
    return voor.includes('dicteren') ? t().inGebruikVoorDicteren : t().inGebruikVoorBestanden;
  }

  async function voerUit(werk: () => Promise<Foutmelding | null>): Promise<void> {
    if (bezig) return;
    bezig = true;
    actieFout = null;
    try {
      actieFout = await werk();
    } finally {
      bezig = false;
    }
  }

  async function download(): Promise<void> {
    // aria-disabled houdt een klik niet tegen; een tweede klik terwijl hij start, doet niets.
    if (stand.teWeinigRuimte || fase !== 'start') return;
    // Een gewone download neemt het model niet in gebruik; dat kies je bovenaan bij Wijzigen.
    // "Verder downloaden" (null) maakt de keuze van de eerste start af.
    await voerUit(() => modellenStand.start(regel, stand.download === 'verder' ? null : []));
  }

  async function verwijder(): Promise<void> {
    bevestig = false;
    await voerUit(() => modellenStand.verwijder(regel.id));
  }

  async function annuleerBevestiging(): Promise<void> {
    bevestig = false;
    await tick();
    verwijderKnop?.focus();
  }
</script>

<div
  class="{vlak} flex flex-col gap-2.5 border px-4 pt-3.5 pb-2.5 {inGebruik
    ? 'border-actie'
    : 'border-transparent'}"
  data-model={regel.id}
>
  <div class="flex flex-wrap items-start gap-x-4 gap-y-2">
    <div class="flex min-w-0 flex-[1_1_16rem] flex-col gap-1">
      <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <h3 class="m-0 font-kop text-midden font-semibold">{rol}</h3>
        <span class="text-klein text-dim">{naam}</span>
        {#if nieuw}<span class="{chipNieuw} self-center">{t().labelNieuw}</span>{/if}
      </div>
      {#if inGebruik}
        <span class="inline-flex items-center gap-1.5 text-klein font-medium text-actie">
          <span class="inline-block size-2 flex-none rounded-full bg-actie" aria-hidden="true"
          ></span>
          {gebruikTekst()}
        </span>
      {/if}
      {#if tekst}
        <p class="m-0 text-klein leading-snug text-gedempt">{tekst.zin}</p>
      {/if}
    </div>
    {#if regel.soort === 'spraak'}
      <ModelBalkjes nauwkeurigheid={regel.nauwkeurigheid} snelheid={regel.snelheid} />
    {/if}
  </div>

  {#if naHervatten}
    <p class="m-0 flex items-start gap-[7px] text-klein leading-snug text-dim">
      <Info size={14} class="mt-px shrink-0" aria-hidden="true" />{naHervatten}
    </p>
  {/if}

  {#if stand.bezig && item}
    {@const getallen = item.voortgang
      ? leesbareVoortgang(item.voortgang.binnen, item.voortgang.totaal, taal)
      : null}
    <div class="flex flex-col gap-1.5" data-download>
      <div class="flex items-baseline justify-between gap-3 text-klein">
        <span class="font-medium text-tekst">
          {#if item.toestand.type === 'wacht'}
            {t().downloadWacht}
          {:else if item.toestand.type === 'nieuwe_poging'}
            {t().downloadNieuwePoging(item.toestand.poging, item.toestand.van)}
          {:else if getallen}
            {[
              t().downloadVoortgang(getallen.binnen, getallen.totaal),
              procent >= 100
                ? t().downloadControleren
                : resterend
                  ? resterend.charAt(0).toLowerCase() + resterend.slice(1)
                  : ''
            ]
              .filter(Boolean)
              .join(', ')}
          {:else}
            {t().modelBezigOphalen}
          {/if}
        </span>
        {#if item.toestand.type !== 'wacht'}
          <span class="font-semibold text-actie" aria-hidden="true">{procent}%</span>
        {/if}
      </div>
      {#if item.toestand.type !== 'wacht'}
        <Progress value={procent} aria-label={t().downloadVoortgangNaam} class="h-2.5 bg-lijn" />
      {/if}
    </div>
  {/if}

  <!-- Onderaan: talen links, grootte en de actie rechts, onder een lijn. -->
  <div
    class="flex flex-wrap items-center gap-x-3.5 gap-y-1 border-t border-lijn pt-2 text-klein text-dim"
  >
    {#if regel.soort === 'spraak'}
      <span class="inline-flex items-center gap-[5px]"
        ><Globe size={14} aria-hidden="true" />{t().talenAantal(
          talen.aantal,
          talen.turksArabisch
        )}</span
      >
    {/if}
    <span class="ml-auto inline-flex items-center gap-[5px]">
      <HardDrive size={14} aria-hidden="true" />{regel.status === 'gedeeltelijk' && !stand.bezig
        ? t().gedeeltelijkGedownload(
            leesbareVoortgang(regel.aanwezigBytes, regel.grootteBytes, taal).binnen,
            leesbareGrootte(regel.grootteBytes, taal)
          )
        : leesbareGrootte(regel.grootteBytes, taal)}
    </span>

    {#if stand.bezig}
      <Button
        variant="spook"
        class="text-klein"
        onclick={() => void modellenStand.stop(regel.id)}
        data-hoofdactie=""><X size={14} aria-hidden="true" />{t().stoppenMetDownloaden}</Button
      >
    {/if}

    {#if stand.download}
      <Button
        variant="tweede"
        class="text-klein"
        onclick={download}
        aria-disabled={stand.teWeinigRuimte || bezig || fase !== 'start' ? 'true' : undefined}
        aria-describedby={onderbroken ? `onderbroken-${regel.id}` : undefined}
        ><Download size={14} aria-hidden="true" />{fase === 'wordt_gestart'
          ? t().wordtGestart
          : onderbroken
            ? t().hervatten
            : stand.download === 'verder'
              ? t().verderDownloadenGrootte(leesbareGrootte(rest, taal))
              : t().downloaden}</Button
      >
      {#if onderbroken && fase === 'start'}
        <span id="onderbroken-{regel.id}" class="text-klein text-dim"
          >{t().downloadOnderbroken}</span
        >
        <!-- Niet hervatten: de melding verdwijnt, ook na een herstart; het deel blijft staan. -->
        <Button variant="spook" class="text-klein" onclick={() => void modellenStand.stop(regel.id)}
          >{t().nietHervatten}</Button
        >
      {/if}
    {/if}

    {#if stand.verwijderen && !bevestig}
      <Button
        variant="spook"
        class="text-klein {stand.verwijderen === 'aan'
          ? 'not-aria-disabled:hover:text-fout-tekst'
          : ''}"
        bind:ref={verwijderKnop}
        onclick={stand.verwijderen === 'aan'
          ? () => {
              bevestig = true;
            }
          : undefined}
        aria-disabled={stand.verwijderen === 'uit' || bezig ? 'true' : undefined}
        title={stand.verwijderen === 'uit' ? t().nietWeg : undefined}
        aria-describedby={stand.verwijderen === 'uit' ? `niet-weg-${regel.id}` : undefined}
        ><Trash2 size={14} aria-hidden="true" />{t().verwijderen}</Button
      >
      {#if stand.verwijderen === 'uit'}<span id="niet-weg-{regel.id}" class="sr-only"
          >{t().nietWeg}</span
        >{/if}
    {/if}
  </div>

  {#if bevestig}
    <div
      class="flex flex-wrap items-center justify-end gap-2 rounded-klein bg-verhoogd p-2.5 text-klein font-medium text-tekst"
      role="group"
      aria-label={t().verwijderen}
    >
      <span class="mr-auto"
        >{t().verwijderenBevestigen(leesbareGrootte(regel.aanwezigBytes, taal))}</span
      >
      <Button variant="spook" class="text-klein" onclick={annuleerBevestiging}
        >{t().annuleren}</Button
      >
      <Button
        variant="tweede"
        class="text-klein text-fout-tekst"
        onclick={verwijder}
        data-hoofdactie=""><Trash2 size={14} aria-hidden="true" />{t().verwijderen}</Button
      >
    </div>
  {/if}

  {#if stand.teWeinigRuimte}
    <div class="{meldingVak} bg-verhoogd" role="alert">
      <HardDrive size={16} class="mt-0.5 shrink-0 text-waarsch" aria-hidden="true" />
      <div class="flex flex-col gap-1">
        <b class="font-semibold">{t().ruimteTekortKop}</b>
        <span class="text-gedempt"
          >{vrijBytes === null
            ? t().ruimteTekortOnbekend
            : t().ruimteTekortTekst(
                leesbareGrootte(rest, taal),
                leesbareGrootte(vrijBytes, taal)
              )}</span
        >
        <span>
          <Button variant="tweede" class="mt-1 text-klein" onclick={() => void modellenStand.laad()}
            >{t().opnieuwControleren}</Button
          >
        </span>
      </div>
    </div>
  {/if}

  {#if actieFout ?? downloadFout}
    {@const fout = actieFout ?? downloadFout}
    {#if fout}
      <div class="{meldingVak} bg-verhoogd" role="alert">
        <CircleAlert size={16} class="mt-0.5 shrink-0 text-fout-tekst" aria-hidden="true" />
        <div class="flex flex-col gap-1">
          {#if !actieFout}<b class="font-semibold">{t().downloadMisluktKop}</b>{/if}
          <span class="text-gedempt">{fout.melding}</span>
          {#if !actieFout && (fout.vervolg === 'opnieuw_proberen' || fout.vervolg === 'opnieuw_downloaden') && stand.download}
            <span>
              <Button class="mt-1 text-klein" onclick={() => void download()} data-hoofdactie=""
                >{fout.vervolg === 'opnieuw_downloaden'
                  ? t().opnieuwDownloaden
                  : t().opnieuwProberen}</Button
              >
            </span>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>
