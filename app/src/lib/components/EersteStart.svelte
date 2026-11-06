<script lang="ts">
  import { controleStand } from '#lib/controlestand.svelte.js';
  import { onMount, untrack } from 'svelte';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Download from '@lucide/svelte/icons/download';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import Woordmerk from '#lib/components/Woordmerk.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import Vinkvak from '#lib/components/Vinkvak.svelte';
  import { Progress } from '#lib/components/ui/progress/index.js';
  import { huidigeTaal, t } from '#lib';
  import {
    computerCheck,
    computerZin,
    standaardAdvies,
    videokaartAdviesTekst,
    wizardStapLog,
    type ComputerInfo
  } from '#lib/computer.js';
  import {
    downloadStand,
    stapNummer,
    takenEersteModel,
    teDownloaden,
    teHalenBytes,
    totaalVoortgang,
    WIZARD_STAPPEN,
    type WizardModel,
    type WizardStap
  } from '#lib/eerstestart.js';
  import { procentVan } from '#lib/downloadrij.js';
  import { leesbareGrootte, leesbareVoortgang } from '#lib/grootte.js';
  import { zoekModel, type ModelRegel } from '#lib/modellen.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { haalSneltoetsStatus } from '#lib/sneltoets.js';
  import { foutVak } from '#lib/stijl.js';
  import { snelheidstestStart, videokaartZet, type Vergelijking } from '#lib/videokaart.js';

  // De eerste-start-wizard (fase 5): waarom een
  // model, wat er past bij deze computer, downloaden, en de sneltoets. Hij verschijnt alleen
  // als er nog geen spraakmodel op de computer staat. Het downloaden zelf loopt via
  // `modellenStand`: de wizard zet zijn modellen meteen allebei in de downloadrij (ze komen na
  // elkaar binnen) en volgt daarna de stand van precies die modellen in de rij.

  let { onKlaar }: { onKlaar: () => void } = $props();

  const taal = $derived(huidigeTaal());
  let stap = $state<WizardStap>('waarom');

  // --- Stap 2: de computer lezen
  let info = $state<ComputerInfo | null>(null);
  let leesFout = $state(false);
  /** Is de computer echt uitgelezen? Niet bij "doorgaan met standaard": dan geen controle. */
  let uitgelezen = $state(false);
  let metTweede = $state(true);

  async function leesComputer(): Promise<void> {
    leesFout = false;
    try {
      info = await computerCheck(null);
      uitgelezen = true;
    } catch {
      leesFout = true;
    }
  }

  const modellen = $derived(modellenStand.overzicht?.modellen ?? []);
  const dicteerRegel = $derived(info ? zoekModel(modellen, info.advies.dicteren) : undefined);
  const bestandsRegel = $derived(info ? zoekModel(modellen, info.advies.bestanden) : undefined);
  const naam = (id: string): string => t().modelTekst(id)?.naam ?? id;
  const plan = $derived(info ? teDownloaden(info.advies, metTweede) : []);
  /** Hoeveel er in totaal nog binnen moet komen, voor de knop. */
  const teHalen = $derived(
    teHalenBytes(
      plan.flatMap((id) => {
        const regel = zoekModel(modellen, id);
        return regel
          ? [{ grootteBytes: regel.grootteBytes, startAanwezigBytes: regel.aanwezigBytes }]
          : [];
      })
    )
  );
  /** Wat per model op schijf stond toen op Downloaden werd geklikt: de basis van de voortgang. */
  let startAanwezig = $state<Record<string, number>>({});
  const geenRuimte = $derived(
    info !== null &&
      (metTweede ? info.advies.tekortBeideBytes : info.advies.tekortDicterenBytes) > 0
  );
  const tekort = $derived(
    info ? (metTweede ? info.advies.tekortBeideBytes : info.advies.tekortDicterenBytes) : 0
  );

  // --- Stap 3: downloaden, dan eventueel de proef met de videokaart
  /** Volgt de wizard zijn downloads (na "Downloaden", tot klaar of tot hij stopt)? */
  let bezig = $state(false);
  let testBezig = $state(false);
  let testMislukt = $state(false);
  let proef = $state<Vergelijking | null>(null);
  let kaartAan = $state(false);
  /** Modellen waarvoor de wizard het overzicht al eens opnieuw ophaalde na "klaar". */
  let gecontroleerd: string[] = [];

  /** Het eerste model doet beide taken tenzij het tweede model erbij komt. */
  function takenVoor(id: string): ('dicteren' | 'bestanden')[] {
    if (id !== info?.advies.dicteren) return ['bestanden'];
    return takenEersteModel(info.advies.bestandenApart && metTweede);
  }

  /** De stand van elk model uit het plan: op schijf, in de rij, en een eventuele fout. */
  const wizardModellen = $derived(
    plan.map((id): WizardModel => ({
      id,
      compleet: zoekModel(modellen, id)?.status === 'compleet',
      fase: modellenStand.knopFase(id),
      toestand: modellenStand.itemVoor(id)?.toestand.type ?? null,
      fout: modellenStand.foutVoor(id)?.melding ?? null
    }))
  );
  const stand = $derived(downloadStand(wizardModellen));

  /**
   * Zet alle modellen uit het plan die er nog niet staan in de downloadrij, in volgorde. Ze
   * komen na elkaar binnen; een model dat al in de rij staat, komt er niet nog eens bij.
   */
  async function zetInRij(): Promise<void> {
    gecontroleerd = [];
    bezig = true;
    for (const id of plan) {
      const regel = zoekModel(modellenStand.overzicht?.modellen ?? [], id);
      if (!regel || regel.status === 'compleet') continue;
      await modellenStand.start(regel, takenVoor(id));
    }
  }

  function startDownloads(): void {
    if (!info) return;
    // Het getal achter "van" blijft gelijk aan wat de knop noemde, ook als er tijdens het
    // downloaden meer op schijf komt of een poging opnieuw begint.
    startAanwezig = Object.fromEntries(
      plan.map((id) => [id, zoekModel(modellen, id)?.aanwezigBytes ?? 0])
    );
    stap = 'downloaden';
    void zetInRij();
  }

  /** Stopt de downloads van de wizard (wachtend en lopend); wat binnen is, blijft staan. */
  async function stopDownloads(): Promise<void> {
    bezig = false;
    for (const id of plan) {
      if (modellenStand.isActief(id)) await modellenStand.stop(id);
    }
  }

  // Volgt de stand van de eigen modellen in de rij. Klaar: afronden. Een item dat klaar meldt
  // terwijl het overzicht het model nog niet compleet ziet: één keer opnieuw ophalen.
  $effect(() => {
    const nu = stand;
    if (!bezig || stap !== 'downloaden') return;
    untrack(() => {
      if (nu.soort === 'klaar') {
        void afronden();
      } else if (nu.soort === 'controleren' && !gecontroleerd.includes(nu.model)) {
        gecontroleerd = [...gecontroleerd, nu.model];
        void modellenStand.laad();
      }
    });
  });

  async function afronden(): Promise<void> {
    if (!bezig) return;
    bezig = false;
    // De proef alleen met een losse videokaart die nog niet getest is (ADR 0011). Hij laat zich
    // niet onderbreken (het werkproces rekent een halve minuut door), dus er is geen Overslaan.
    if (info?.advies.videokaart.stand === 'testen') {
      testBezig = true;
      try {
        const uitkomst = await snelheidstestStart();
        proef = uitkomst;
        info = await computerCheck(uitkomst);
        kaartAan = info.advies.videokaart.stand === 'aanzetten';
      } catch {
        testMislukt = true;
      } finally {
        testBezig = false;
      }
    }
    // De wizard is ook een controle: Modellen zegt daarna welk model bij deze computer past.
    // Alleen als de computer echt is uitgelezen; met het standaardadvies weet Mondig dat niet.
    if (info && uitgelezen) void controleStand.bewaar(info.advies);
    await naarKlaar();
  }

  /** "Nu niet": de wizard sluit; het startscherm blijft om een model vragen. */
  function nuNiet(): void {
    void wizardStapLog('nu_niet');
    onKlaar();
  }

  // Elke stap komt in het logboek (alleen de vaste naam van de stap).
  $effect(() => {
    void wizardStapLog(testBezig ? 'proef' : stap);
  });

  /** Geen computergegevens (het lezen faalde): doorgaan met het standaardmodel. */
  function metStandaard(): void {
    info = standaardAdvies();
    uitgelezen = false;
    leesFout = false;
  }

  // --- Stap 4: klaar
  let toets = $state<string | null>(null);
  async function naarKlaar(): Promise<void> {
    stap = 'klaar';
    toets = (await haalSneltoetsStatus().catch(() => null))?.actief ?? null;
  }

  async function klaar(): Promise<void> {
    if (kaartAan) await videokaartZet(true).catch(() => undefined);
    onKlaar();
  }

  const vrijTekst = $derived(
    info?.computer.vrijBytes != null ? leesbareGrootte(info.computer.vrijBytes, taal) : null
  );
  const duur = $derived(
    dicteerRegel ? download_minuten(dicteerRegel.grootteBytes - dicteerRegel.aanwezigBytes) : null
  );

  /** Zelfde schatting als `mondig_core::hardware::download_minuten`: 100 en 20 Mbit/s. */
  function download_minuten(bytes: number): [number, number] {
    const per = (mbit: number): number =>
      Math.max(1, Math.ceil(bytes / (((mbit * 1_000_000) / 8) * 60)));
    return [per(100), per(20)];
  }

  function groottes(regel: ModelRegel | undefined): string {
    return regel ? leesbareGrootte(regel.grootteBytes, taal) : '';
  }

  onMount(() => {
    void leesComputer();
  });

  // Wat de downloadstap toont: het model dat binnenkomt (of wacht), met zijn voortgang.
  const toonModel = $derived(
    stand.soort === 'bezig' || stand.soort === 'controleren' ? stand.model : null
  );
  const toonItem = $derived(toonModel ? modellenStand.itemVoor(toonModel) : undefined);
  const toonFase = $derived(toonModel ? modellenStand.knopFase(toonModel) : 'start');
  // De voortgang van alle downloads samen, niet van het ene model dat nu binnenkomt: de knop
  // zei "1,8 GB", dus hier staat "van 1,8 GB".
  const totaal = $derived(
    totaalVoortgang(
      plan.flatMap((id) => {
        const regel = zoekModel(modellen, id);
        if (!regel) return [];
        return [
          {
            grootteBytes: regel.grootteBytes,
            aanwezigBytes: regel.aanwezigBytes,
            startAanwezigBytes: startAanwezig[id] ?? regel.aanwezigBytes,
            compleet: regel.status === 'compleet',
            binnenBytes: modellenStand.itemVoor(id)?.voortgang?.binnen ?? null
          }
        ];
      })
    )
  );
  const procent = $derived(
    stand.soort === 'controleren'
      ? 100
      : procentVan(totaal.totaal > 0 ? totaal : toonItem?.voortgang)
  );
  const voortgang = $derived(
    totaal.totaal > 0 && toonItem?.voortgang
      ? leesbareVoortgang(totaal.binnen, totaal.totaal, taal)
      : null
  );
  const resterend = $derived(toonModel ? modellenStand.resterendVoor(toonModel) : null);

  /** De stand van één model van het plan, in een paar woorden (voor de lijst onder de balk). */
  function standVan(m: WizardModel): string {
    if (m.compleet) return t().zijbalkKlaar;
    if (m.fase === 'wacht') return t().zijbalkWacht;
    if (m.fase === 'wordt_gestart') return t().wordtGestart;
    const toestand = modellenStand.itemVoor(m.id)?.toestand;
    if (toestand?.type === 'nieuwe_poging')
      return t().zijbalkOpnieuw(toestand.poging, toestand.van);
    if (m.fase === 'bezig')
      return `${String(procentVan(modellenStand.itemVoor(m.id)?.voortgang))}%`;
    if (m.fout) return t().zijbalkMislukt;
    if (toestand?.type === 'onderbroken') return t().zijbalkOnderbroken;
    return t().zijbalkGestopt;
  }

  const kolom = 'flex w-[540px] max-w-full flex-col gap-[18px]';
  const kop = 'm-0 text-kop leading-tight tracking-[-0.015em]';
  const lees = 'm-0 text-midden leading-[1.55] text-gedempt';
</script>

<div class="flex min-h-dvh flex-col bg-achtergrond">
  <header class="flex items-center px-8 pt-[22px]">
    <Woordmerk />
    <span class="ml-auto text-klein text-dim" aria-live="polite">
      {t().wizStap(stapNummer(stap), WIZARD_STAPPEN.length)}
    </span>
  </header>

  <main class="flex flex-1 flex-col items-center justify-center px-8 pb-10">
    <div class={kolom}>
      {#if stap === 'waarom'}
        <h1 class={kop}>{t().wizWaaromKop}</h1>
        <p class={lees}>{t().wizWaaromIntro}</p>
        <ul class="m-0 flex list-none flex-col divide-y divide-lijn rounded-groot bg-vlak p-0">
          <li class="flex gap-3.5 px-4 py-3.5">
            <ShieldCheck size={18} class="mt-0.5 text-actie" aria-hidden="true" />
            <div>
              <b class="mb-0.5 block text-midden font-semibold">{t().wizPuntLokaalKop}</b>
              <span class="text-basis leading-snug text-gedempt">{t().wizPuntLokaalTekst}</span>
            </div>
          </li>
          <li class="flex gap-3.5 px-4 py-3.5">
            <Download size={18} class="mt-0.5 text-actie" aria-hidden="true" />
            <div>
              <b class="mb-0.5 block text-midden font-semibold">{t().wizPuntDownloadKop}</b>
              <span class="text-basis leading-snug text-gedempt"
                >{t().wizPuntDownloadTekst(
                  groottes(dicteerRegel ?? zoekModel(modellen, 'parakeet-ultra-q8')),
                  duur ? t().wizDuur(duur[0], duur[1]) : t().wizDuur(2, 5)
                )}</span
              >
            </div>
          </li>
          <li class="flex gap-3.5 px-4 py-3.5">
            <HardDrive size={18} class="mt-0.5 text-actie" aria-hidden="true" />
            <div>
              <b class="mb-0.5 block text-midden font-semibold">{t().wizPuntRuimteKop}</b>
              <span class="text-basis leading-snug text-gedempt"
                >{vrijTekst ? t().wizPuntRuimteTekst(vrijTekst) : t().wizPuntRuimteOnbekend}</span
              >
            </div>
          </li>
        </ul>
        <div class="flex items-center gap-2.5">
          <Button variant="spook" onclick={nuNiet}>{t().wizNuNiet}</Button>
          <Button class="ml-auto" onclick={() => (stap = 'advies')}>{t().wizVerder}</Button>
        </div>
      {:else if stap === 'advies'}
        <h1 class={kop}>{t().wizAdviesKop}</h1>
        {#if leesFout}
          <div class={foutVak} role="alert">
            <span>{t().wizAdviesFout}</span>
            <div class="flex flex-wrap gap-2">
              <Button variant="tweede" onclick={() => void leesComputer()}
                >{t().opnieuwControleren}</Button
              >
              <Button onclick={metStandaard}>{t().wizDoorgaanStandaard}</Button>
            </div>
          </div>
          <div class="flex items-center gap-2.5">
            <Button variant="spook" onclick={() => (stap = 'waarom')}>{t().terug}</Button>
            <Button variant="spook" class="ml-auto" onclick={nuNiet}>{t().wizNuNiet}</Button>
          </div>
        {:else if !info}
          <p class={lees} role="status">{t().wizAdviesLeest}</p>
        {:else}
          <p class="m-0 text-basis text-dim">
            {t().wizAdviesGelezen(computerZin(info.computer, t()))}
          </p>
          <div class="flex flex-col divide-y divide-lijn rounded-groot bg-vlak">
            <div class="flex flex-col gap-1 px-4 py-3.5">
              <b class="text-groot font-semibold">{t().wizAdviesDicteren}</b>
              <span class="text-klein text-dim">{naam(info.advies.dicteren)}</span>
              <span class="text-basis text-gedempt"
                >{t().modelTekst(info.advies.dicteren)?.zin ?? ''}</span
              >
            </div>
            <div class="flex flex-col gap-1 px-4 py-3.5">
              <b class="text-groot font-semibold">{t().wizAdviesBestanden}</b>
              <span class="text-klein text-dim">{naam(info.advies.bestanden)}</span>
              <span class="text-basis text-gedempt">
                {info.advies.bestandenApart
                  ? info.advies.krap
                    ? t().wizBestandenKrap
                    : t().wizBestandenGoed
                  : t().wizBestandenZelfde}
              </span>
            </div>
          </div>
          {#if info.advies.bestandenApart}
            <Vinkvak bind:checked={metTweede}>
              {t().wizTweedeModel(groottes(bestandsRegel))}
              <span class="block text-klein text-gedempt">{t().wizTweedeUitleg}</span>
            </Vinkvak>
          {/if}
          {#if info.advies.geheugen === 'te_weinig' || info.advies.geheugen === 'weinig'}
            <p class="m-0 text-basis text-waarsch" role="status">{t().wizGeheugenWeinig}</p>
          {/if}
          {#if geenRuimte}
            <div class={foutVak} role="alert">
              <span>{t().wizRuimteTekort(leesbareGrootte(tekort, taal))}</span>
              <Button variant="tweede" onclick={() => void leesComputer()}
                >{t().opnieuwControleren}</Button
              >
            </div>
          {/if}
          <div class="flex items-center gap-2.5">
            <Button variant="spook" onclick={() => (stap = 'waarom')}>{t().terug}</Button>
            <Button
              class="ml-auto"
              aria-disabled={geenRuimte || !dicteerRegel}
              onclick={() => {
                if (!geenRuimte && dicteerRegel) startDownloads();
              }}
            >
              <Download size={16} aria-hidden="true" />{t().wizDownloadKnop(
                leesbareGrootte(teHalen, taal)
              )}
            </Button>
          </div>
        {/if}
      {:else if stap === 'downloaden'}
        {#if testBezig}
          <h1 class={kop}>{t().wizTestKop}</h1>
          <p class={lees} role="status">{t().wizTestUitleg}</p>
          <Progress value={null} aria-label={t().wizTestKop} />
        {:else}
          <h1 class={kop}>
            {stand.soort === 'fout'
              ? t().wizDownloadMislukt
              : toonModel
                ? t().wizDownloadKop(plan.length)
                : t().wizDownloadGestart}
          </h1>
          {#if stand.soort === 'fout'}
            <div class={foutVak} role="alert">
              <span>{stand.melding}</span>
              <Button onclick={() => void zetInRij()}>{t().opnieuwProberen}</Button>
            </div>
            <div class="flex">
              <Button variant="spook" onclick={nuNiet}>{t().wizNuNiet}</Button>
            </div>
          {:else if stand.soort === 'gestopt' || !bezig}
            <p class={lees} role="status">{t().wizDownloadGestopt}</p>
            <div class="flex items-center gap-2.5">
              <Button variant="spook" onclick={nuNiet}>{t().wizNuNiet}</Button>
              <Button class="ml-auto" onclick={() => void zetInRij()}>{t().verderDownloaden}</Button
              >
            </div>
          {:else}
            <div class="flex flex-col gap-2.5 rounded-groot bg-vlak px-[18px] py-4" role="status">
              <div class="flex items-baseline justify-between gap-3">
                <b class="text-groot font-semibold">
                  {toonFase === 'wacht'
                    ? t().downloadWacht
                    : toonItem?.toestand.type === 'nieuwe_poging'
                      ? t().zijbalkOpnieuw(toonItem.toestand.poging, toonItem.toestand.van)
                      : toonFase === 'wordt_gestart'
                        ? t().wordtGestart
                        : t().procentGedownload(procent)}
                </b>
                <span class="text-klein text-gedempt">
                  {voortgang ? t().wizDownloadVan(voortgang.binnen, voortgang.totaal) : ''}
                  {resterend ? ` · ${resterend}` : ''}
                </span>
              </div>
              <Progress value={procent} aria-label={t().wizDownloadKop(plan.length)} />
              {#if toonItem?.toestand.type === 'nieuwe_poging'}
                <span class="text-klein leading-relaxed text-gedempt"
                  >{t().downloadNieuwePoging(toonItem.toestand.poging, toonItem.toestand.van)}</span
                >
              {/if}
              {#if wizardModellen.length > 1}
                <!-- Beide modellen staan in de rij; ze komen na elkaar binnen. -->
                <ul
                  class="m-0 flex list-none flex-col gap-1 border-t border-lijn p-0 pt-2.5 text-klein"
                  data-wizard-rij
                >
                  {#each wizardModellen as m (m.id)}
                    <li class="flex items-center justify-between gap-3">
                      <span class="text-tekst">{naam(m.id)}</span>
                      <span class="text-gedempt">{standVan(m)}</span>
                    </li>
                  {/each}
                </ul>
              {/if}
              <span class="text-klein leading-relaxed text-dim">{t().wizDownloadTip}</span>
            </div>
            <div class="flex">
              <Button variant="spook" onclick={() => void stopDownloads()}>{t().annuleren}</Button>
            </div>
          {/if}
        {/if}
      {:else}
        <div class="flex items-center gap-3.5">
          <CircleCheck size={28} class="text-actie" aria-hidden="true" />
          <h1 class={kop}>{t().wizKlaarKop}</h1>
        </div>
        <p class={lees}>
          {toets ? t().wizKlaarSneltoets(toets) : t().wizKlaarSneltoetsGeen}
        </p>
        {#if info && info.advies.bestandenApart && !metTweede}
          <p class="m-0 text-basis leading-snug text-gedempt">{t().wizKlaarTweedeOvergeslagen}</p>
        {/if}
        {#if testMislukt}
          <p class="m-0 text-basis text-gedempt">{t().wizTestMislukt}</p>
        {/if}
        {#if proef && info}
          <div class="flex items-center gap-3 rounded-groot bg-vlak px-4 py-3.5">
            <div class="flex min-w-0 flex-1 flex-col gap-0.5">
              <b id="wiz-kaart-kop" class="text-basis font-medium">{t().wizKlaarKaart}</b>
              <span id="wiz-kaart-hulp" class="text-klein text-gedempt"
                >{info.advies.videokaart.stand === 'aanzetten'
                  ? t().wizKlaarKaartHulp
                  : videokaartAdviesTekst(info.advies, t(), taal)}</span
              >
            </div>
            <Schakelaar
              aan={kaartAan}
              wijzig={(nieuw: boolean) => (kaartAan = nieuw)}
              labelledby="wiz-kaart-kop"
              describedby="wiz-kaart-hulp"
              disabled={info.advies.videokaart.stand !== 'aanzetten'}
            />
          </div>
        {/if}
        <div class="flex">
          <Button class="ml-auto" onclick={() => void klaar()}>{t().wizAanDeSlag}</Button>
        </div>
      {/if}
    </div>
  </main>
</div>
