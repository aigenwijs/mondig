<script lang="ts">
  import { resolve } from '$app/paths';
  import { onMount, tick, untrack } from 'svelte';
  import { Dialog, DropdownMenu, RadioGroup } from 'bits-ui';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Copy from '@lucide/svelte/icons/copy';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Headphones from '@lucide/svelte/icons/headphones';
  import Info from '@lucide/svelte/icons/info';
  import Pencil from '@lucide/svelte/icons/pencil';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Monitor from '@lucide/svelte/icons/monitor';
  import NotulenPaneel from '#lib/components/NotulenPaneel.svelte';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Square from '@lucide/svelte/icons/square';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import User from '@lucide/svelte/icons/user';
  import Users from '@lucide/svelte/icons/users';
  import X from '@lucide/svelte/icons/x';
  import EerderOpgenomen from '#lib/components/EerderOpgenomen.svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import KlaarMoment from '#lib/components/KlaarMoment.svelte';
  import Niveaumeter from '#lib/components/Niveaumeter.svelte';
  import OpnieuwDialoog from '#lib/components/OpnieuwDialoog.svelte';
  import OpnieuwMelding from '#lib/components/OpnieuwMelding.svelte';
  import OpnieuwVoortgang from '#lib/components/OpnieuwVoortgang.svelte';
  import OpslaanKnop from '#lib/components/OpslaanKnop.svelte';
  import RingIcoon from '#lib/components/RingIcoon.svelte';
  import SprekerNaam from '#lib/components/SprekerNaam.svelte';
  import StartBalk from '#lib/components/StartBalk.svelte';
  import Streepjes from '#lib/components/Streepjes.svelte';
  import VerslagLijst from '#lib/components/VerslagLijst.svelte';
  import VerslagTitel from '#lib/components/VerslagTitel.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import {
    bestandBewaarBeslis,
    bestandOpenOpgeslagen,
    bestandToonOpgeslagen,
    bewarenInstellingen,
    bewarenOpenMap,
    mapTekst,
    volgnummer,
    type BewaarUitslag,
    type Formaat,
    type Keuze as BewaarKeuze,
    type MapInfo
  } from '#lib/bewaren.js';
  import { microfooninstellingOpen } from '#lib/commandos.js';
  import {
    OPLICHT_MS,
    bevestigingGeldt,
    notulenInstellingen,
    vergaderingNotulenKopieer,
    vergaderingNotulenMaken,
    vergaderingNotulenPuntAanpassen,
    vergaderingNotulenPuntWeghalen,
    vergaderingNotulenStoppen,
    type Bevestiging,
    type NotulenInstellingen,
    type PuntSoort
  } from '#lib/notulen.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import {
    aantalSprekers,
    bepaalKlaarMoment,
    volgUitschrijfStart,
    klaarZin,
    modelRol,
    type KlaarMoment as KlaarGegevens
  } from '#lib/verslagkop.js';
  import type { VerslagRegel } from '#lib/verslag.js';
  import { bewaarTotTekst, standaardTitel, verslagTitel } from '#lib/verslagtitel.js';
  import { eersteKeuze, opnieuwGeslaagd } from '#lib/opnieuw.js';
  import { luisterNaarTaken } from '#lib/gebeurtenissen.js';
  import { METER_STREEPJES_LANG, streepjesAan, testgeluidSpeel } from '#lib/meter.js';
  import {
    gekozenWaarde,
    microfoonLijst,
    zelfdeLijst,
    type MicrofoonLijst
  } from '#lib/instellingen.js';
  import {
    resterendeTijdTekst,
    schatResterendeSeconden,
    type Meting
  } from '#lib/resterendetijd.js';
  import { bereikteDrempel } from '#lib/schermlezer.js';
  import { foutVak, paginaTitel, rij, rijBediening, rijNaam, vlak } from '#lib/stijl.js';
  import { foutUitCommando, meldingTekst, type Foutmelding, type Gebeurtenis } from '#lib/taak.js';
  import {
    SOORTEN,
    aankondiging,
    andereSoorten,
    bewaartermijnInstelling,
    dagVan,
    duurMinuten,
    isNieuwer,
    beurtNaamGetoond,
    kanAanSpreker,
    klokTijd,
    luisterNaarNiveau,
    luisterNaarStand,
    luisterNaarStartGevraagd,
    metSysteemgeluid,
    naamMelding,
    nieuweStap,
    opnamesVanSoort,
    schermVan,
    sprekersVan,
    zonderSpreker,
    toontLabels,
    vergaderingAfspeelapparaten,
    vergaderingAudioBewaren,
    vergaderingAudioStuk,
    vergaderingGeefAanSpreker,
    vergaderingHerstel,
    vergaderingKopieer,
    vergaderingLater,
    vergaderingNaam,
    vergaderingOpnieuwKeuzes,
    vergaderingOpnieuwStart,
    vergaderingOpnieuwStoppen,
    vergaderingPersoonMarkeer,
    vergaderingPersoonVervangen,
    vergaderingPersoonWeghalen,
    vergaderingOpslaan,
    vergaderingSluiten,
    vergaderingStand,
    vergaderingStart,
    vergaderingStartGevraagd,
    vergaderingStapTerug,
    vergaderingStop,
    vergaderingUitschrijven,
    vergaderingVoegSprekersSamen,
    vergaderingWeggooien,
    vergaderingVerslagNaam,
    vergaderingVerslagOpenen,
    vergaderingVerslagWissen,
    vergaderingVorigeTerugzetten,
    vergaderingZetTekst,
    vraagtHerinnering,
    WACHTEND_ZICHTBAAR,
    wistBinnenkort,
    zichtbareOpnames,
    type AfspeelInfo,
    type BeurtInfo,
    type OpnieuwKeuzes,
    type Niveaus,
    type OpnameInfo,
    type SprekerInfo,
    type Sprekerstap,
    type VergaderingStand,
    type VerslagInfo,
    type Vergadersoort
  } from '#lib/vergadering.js';

  // Het scherm Opnemen, voorheen Vergadering (herzien in fase UX-D): kiezen, opnemen, uitschrijven en het verslag
  // met namen, in één route. Drie soorten: online, in de ruimte en Alleen ik. Wat er te zien is volgt de stand van
  // Rust (`vergadering-gewijzigd`); dit scherm houdt alleen bij wat Rust niet weet: de keuzes
  // voor de start, wat er met het verslag is gedaan, en welke vraag er open staat.
  //
  // Zoals Bestanden: een oudere stand wordt genegeerd (volgnummer), en de schermlezer krijgt
  // een korte zin bij start, stop, klaar en voortgang, zonder dat de focus verspringt. De
  // niveaumeters melden tien keer per seconde niets; alleen de stiltewaarschuwing is een
  // statusmelding.

  let stand = $state<VergaderingStand | null>(null);

  // Het verslag heeft twee tabbladen: Verslag en Notulen (T3). Het tabblad Notulen toont de
  // instellingen van Notulen (waar de tekst heen gaat) en onthoudt per verslag of de gebruiker de
  // vraag "waar gaat de tekst heen?" al beantwoordde. De bronknop van een punt springt naar het
  // tabblad Verslag en laat die beurt even oplichten.
  let tab = $state<'verslag' | 'notulen'>('verslag');
  let notulenIns = $state<NotulenInstellingen | null>(null);
  let notulenBevestigd = $state<(Bevestiging & { taak: number }) | null>(null);
  let oplicht = $state<number | null>(null);
  let oplichtTimer: number | undefined;
  let versie = -1;
  let geladen = $state(false);
  let schermFout = $state<Foutmelding | null>(null);
  const toestand = $derived(stand?.toestand ?? { type: 'klaar' as const });
  const scherm = $derived(schermVan(toestand));
  const fout = $derived(schermFout ?? stand?.fout ?? null);
  const taal = $derived(huidigeTaal());

  // De keuzes voor de start.
  let soort = $state<Vergadersoort>('online');
  let soortGezet = false;
  let micLijst = $state<MicrofoonLijst | null>(null);
  let gekozenMic = $state<string | null>(null);
  let afspeelLijst = $state<AfspeelInfo[]>([]);
  let afspeelFout = $state(false);
  let afspeelGeladen = false;
  let gekozenAfspeel = $state('');
  let herinnering = $state(false);
  let bezig = $state(false);
  // Alleen voor de tekst "Bezig met opslaan": bij Sluiten of Weggooien hoort die niet.
  let opslaanActief = $state(false);
  let startKnop = $state<HTMLElement | null>(null);
  let herinneringStart = $state<HTMLElement | null>(null);
  // Is de microfoonlijst gelezen (of mislukt)? Pas dan start de meter, op de juiste microfoon.
  let micKlaar = $state(false);

  // De meters vóór de start (fase UX-C): ze lopen zolang dit startscherm in beeld is en gaan
  // uit zodra de opname start, zodat de opname de microfoon voor zich heeft.
  let metersAan = $state(true);
  let micMeterFout = $state<Foutmelding | null>(null);
  let sysMeterFout = $state<Foutmelding | null>(null);
  let micMeter = $state<{ opnieuw: () => void } | undefined>();
  let sysMeter = $state<{ opnieuw: () => void } | undefined>();
  let testBezig = $state(false);
  let testFout = $state<string | null>(null);

  // De lijst "Nog uit te schrijven": hooguit drie regels, de rest na "Alle N tonen".
  let alleTonen = $state(false);

  // De opname: de klok loopt lokaal door tussen twee standen, het niveau komt uit events.
  let klokBasis = $state<{ duurMs: number; ontvangen: number } | null>(null);
  let tik = $state(performance.now());
  let niveaus = $state<Niveaus | null>(null);
  const duurNu = $derived(klokBasis ? klokBasis.duurMs + (tik - klokBasis.ontvangen) : 0);

  // Uitschrijven.
  let procentEvent = $state<{ taak: number; procent: number } | null>(null);
  let resterend = $state<string | null>(null);
  // Geen state: alleen gelezen bij elke voortgang.
  const metingen: Record<number, Meting[]> = {};
  const vorigProcent: Record<number, number> = {};
  // Bij de stap Sprekers herkennen (4b-D) toont het scherm geen percentage: het rekenen meldt
  // geen voortgang, dus een getal zou lang op hetzelfde blijven staan.
  const sprekersStap = $derived(toestand.type === 'uitschrijven' && toestand.stap === 'sprekers');
  const voortgang = $derived(
    toestand.type === 'uitschrijven' && !sprekersStap
      ? procentEvent?.taak === toestand.taak
        ? procentEvent.procent
        : toestand.voortgang
      : null
  );

  // Een vraag "Weggooien?": `id` is de opname uit de lijst, `null` die van nu.
  let weggooi = $state<{ id: string | null } | null>(null);

  // Het verslag.
  let standaardFormaat = $state<Formaat>('docx');
  let verslagMelding = $state<string | null>(null);
  // Opnieuw uitschrijven (N-C4): de dialoog, de keuzes uit Rust en het gekozen model.
  let opnieuwOpen = $state(false);
  let opnieuwKeuzes = $state<OpnieuwKeuzes | null>(null);
  let opnieuwFout = $state<string | null>(null);
  let opnieuwGekozen = $state<string | null>(null);
  let verslagFout = $state<string | null>(null);
  let opgeslagen = $state<{ naam: string; map: MapInfo; audio: string | null } | null>(null);
  let bestaatAl = $state<{ vraag: number; naam: string; voorstel: string } | null>(null);
  let audioBezig = $state(false);
  let echoToon = $state(true);
  let opslaanKnop = $state<HTMLElement | null>(null);
  // De naam van het verslag terwijl hij wordt bewerkt; `null` als dat niet zo is.
  let bewerkNaam = $state<string | null>(null);
  // Staat de vraag "Verslag wissen?" open voor het verslag dat nu open is?
  let verslagWisVraag = $state(false);
  // De vraag voor "Vorige versie terugzetten": terugzetten zet ook je latere wijzigingen terug.
  let vorigeVraag = $state(false);
  // De bewaartermijn in dagen, voor de regel onder "Eerder opgenomen".
  let bewaarDagen = $state<number | null>(null);
  // Het klaar-moment direct na het uitschrijven (N-C1) en wanneer het scherm het uitschrijven zag
  // beginnen (`performance.now()`), voor "in 4 minuten uitgeschreven".
  let klaarMoment = $state<KlaarGegevens | null>(null);
  let uitschrijfStart: number | null = null;
  let verslagTaak = -1;
  // De melding na een wijziging aan de sprekers, met "Ongedaan maken" zolang Rust die stap nog
  // heeft (`sprekerstap`); een andere wijziging laat hem daar vervallen, en dan ook hier.
  let stapZin = $state<string | null>(null);
  let verslagLijst = $state<{ wachtOpBewaren: () => Promise<void> } | undefined>();

  // Voor de schermlezer.
  let schermlezerZin = $state('');
  const meld = (zin: string): void => {
    schermlezerZin = zin;
  };

  // De tijd van nu voor de melding bij opnames die bijna worden gewist.
  let nuMs = $state(Date.now());
  const wachtend = $derived(stand?.opnames ?? []);
  const verslagen = $derived(stand?.verslagen ?? []);
  // Alleen de opnames van de soort in de schakelaar; de andere soorten staan eronder als een
  // regel per soort, zodat geen opname onopgemerkt vanzelf gewist wordt.
  const vanSoort = $derived(opnamesVanSoort(wachtend, soort));
  const zichtbaar = $derived(zichtbareOpnames(vanSoort, alleTonen));
  const anderen = $derived(andereSoorten(wachtend, soort, nuMs));
  const microfoonWaarde = $derived(
    gekozenMic ?? (micLijst === null ? '' : gekozenWaarde(micLijst))
  );
  // Is er een bewaarde microfoon die nog is aangesloten? Dan kan de start niet "de standaard
  // van Windows" vragen (Rust neemt dan de bewaarde), dus staat die keuze er niet bij.
  const heeftBewaardeMic = $derived(micLijst?.opgeslagen != null && micLijst.opgeslagenAanwezig);

  function kiesMicrofoon(event: Event & { currentTarget: HTMLSelectElement }): void {
    gekozenMic = event.currentTarget.value;
  }

  function kiesAfspeel(event: Event & { currentTarget: HTMLSelectElement }): void {
    gekozenAfspeel = event.currentTarget.value;
  }

  /** Zet de focus op het kopje van de nieuwe toestand, of op de startknop bij terug naar het begin. */
  async function focusNaWissel(nieuw: string): Promise<void> {
    await tick();
    const doel = nieuw === 'klaar' ? startKnop : document.getElementById('vergader-focus');
    doel?.focus();
  }

  /** Neemt een stand over als die nieuwer is dan wat er staat. */
  function neemOver(nieuw: VergaderingStand): void {
    if (!isNieuwer(nieuw, versie)) return;
    const vorig = stand;
    versie = nieuw.versie;
    stand = nieuw;
    if (!soortGezet) {
      soort = nieuw.laatsteSoort;
      soortGezet = true;
    }
    const nu = nieuw.toestand;
    const vorigType = vorig?.toestand.type;
    const wissel = vorigType !== undefined && vorigType !== nu.type;
    if (nu.type === 'neemtOp') {
      klokBasis = { duurMs: nu.duurMs, ontvangen: performance.now() };
      niveaus = nu.niveaus;
    } else {
      niveaus = null;
    }
    if (wissel) {
      weggooi = null;
      verslagWisVraag = false;
      vorigeVraag = false;
      bewerkNaam = null;
      schermFout = null;
    }
    // Het uitschrijven telt alleen als dit scherm het zag beginnen; anders geen tijd tonen.
    const klaar = bepaalKlaarMoment(vorigType, nu, uitschrijfStart, performance.now());
    uitschrijfStart = volgUitschrijfStart(vorigType, nu.type, uitschrijfStart, performance.now());
    if (nu.type !== 'verslag') klaarMoment = null;
    if (nu.type === 'verslag' && nu.taak !== verslagTaak) {
      klaarMoment = klaar;
      verslagTaak = nu.taak;
      verslagMelding = null;
      verslagFout = null;
      opgeslagen = null;
      bestaatAl = null;
      echoToon = true;
      verslagWisVraag = false;
      vorigeVraag = false;
      bewerkNaam = null;
      audioBezig = false;
      tab = 'verslag';
      oplicht = null;
      stapZin = null;
    }
    // Na een geslaagd opnieuw uitschrijven staat er andere tekst: het bestand dat eerder is
    // opgeslagen heeft de oude, dus "Openen" mag er niet meer naar wijzen.
    if (opnieuwGeslaagd(vorig?.toestand, nu)) opgeslagen = null;
    // Nieuwe notulen horen niet bij een bestand dat al is opgeslagen: dan is "Opgeslagen als..."
    // niet meer waar.
    if (
      nu.type === 'verslag' &&
      vorig?.toestand.type === 'verslag' &&
      vorig.toestand.notulen.info?.momentMs !== nu.notulen.info?.momentMs
    ) {
      opgeslagen = null;
    }
    if (nu.type === 'uitschrijven' && vorig?.toestand.type !== 'uitschrijven') {
      resterend = null;
      procentEvent = null;
    }
    const wat = aankondiging(vorig, nieuw);
    const wasSprekersStap =
      vorig?.toestand.type === 'uitschrijven' && vorig.toestand.stap === 'sprekers';
    if (nu.type === 'uitschrijven' && nu.stap === 'sprekers' && !wasSprekersStap) {
      meld(t().vergaderSprekersSchermlezer);
    } else if (wat === 'gestart') meld(t().vergaderGestartSchermlezer);
    else if (wat === 'uitschrijven') meld(t().vergaderUitschrijvenSchermlezer);
    else if (wat === 'verslag') meld(t().vergaderKlaarSchermlezer);
    if (geladen && wissel) void focusNaWissel(nu.type);
  }

  async function laad(): Promise<void> {
    try {
      neemOver(await vergaderingStand());
    } catch {
      // Lukt de stand niet, dan blijft wat er stond.
    }
  }

  async function laadMicrofoons(): Promise<void> {
    try {
      const nieuw = await microfoonLijst();
      if (micLijst === null || !zelfdeLijst(micLijst, nieuw)) micLijst = nieuw;
    } catch {
      // Zonder lijst blijft de keuze op de standaard.
    } finally {
      micKlaar = true;
    }
  }

  async function laadAfspeel(): Promise<void> {
    try {
      afspeelLijst = await vergaderingAfspeelapparaten();
      afspeelFout = false;
    } catch {
      afspeelLijst = [];
      afspeelFout = true;
    }
  }

  function opTaak(gebeurtenis: Gebeurtenis): void {
    if (gebeurtenis.soort !== 'vergadering') return;
    const { taak, toestand: taakToestand } = gebeurtenis;
    if (taakToestand.type !== 'verwerkt' || taakToestand.voortgang === null) return;
    // De voortgang van de stap Sprekers is die van het lezen van het spoor: geen percentage.
    if (taakToestand.stap === 'sprekers') return;
    procentEvent = { taak, procent: taakToestand.voortgang };
    const reeks = metingen[taak] ?? [];
    reeks.push({ tijd: performance.now(), binnen: taakToestand.voortgang });
    if (reeks.length > 80) reeks.shift();
    metingen[taak] = reeks;
    const seconden = schatResterendeSeconden(reeks, 100);
    resterend = seconden === null ? null : resterendeTijdTekst(seconden, t());
    const drempel = bereikteDrempel(vorigProcent[taak] ?? 0, taakToestand.voortgang);
    vorigProcent[taak] = taakToestand.voortgang;
    if (drempel !== null && drempel < 100) meld(t().vergaderProcentSchermlezer(drempel));
  }

  /** Voert een commando uit dat een nieuwe stand geeft; een fout komt op het scherm. */
  async function voerUit(werk: () => Promise<VergaderingStand>): Promise<boolean> {
    if (bezig) return false;
    bezig = true;
    schermFout = null;
    try {
      neemOver(await werk());
      return true;
    } catch (e) {
      const melding = foutUitCommando(e, t().somsGingErIetsMis);
      // Eerst de stand verversen: een wissel van toestand wist een oude fout, deze niet.
      await laad();
      schermFout = melding;
      return false;
    } finally {
      bezig = false;
    }
  }

  // ---- Starten ----

  // "Opname starten" in het systeemvak (N-C5): het venster opent op dit scherm en zet de vraag
  // "Vertel het de anderen" klaar. Het verzoek ligt in Rust tot dit scherm het ophaalt, zodat het
  // ook werkt als het scherm er nog niet stond toen het menu werd gebruikt.
  let startGevraagd = $state<Vergadersoort | null>(null);

  async function haalStartVraag(): Promise<void> {
    const gevraagd = await vergaderingStartGevraagd().catch(() => null);
    if (gevraagd !== null) startGevraagd = gevraagd;
  }

  // Pas als de eerste stand er is (de soort van de vorige keer staat dan klaar); loopt er al iets,
  // dan vervalt het verzoek.
  $effect(() => {
    if (startGevraagd === null || !geladen) return;
    const gevraagd = startGevraagd;
    startGevraagd = null;
    untrack(() => {
      if (stand?.toestand.type !== 'klaar') return;
      // De soort die Rust koos (de vorige), niet een keuze die dit scherm lokaal nog had.
      soort = gevraagd;
      soortGezet = true;
      vraagStart();
    });
  });

  function vraagStart(): void {
    if (bezig) return;
    schermFout = null;
    // Bij Alleen ik is er niemand om het aan te vertellen: meteen starten.
    if (vraagtHerinnering(soort)) herinnering = true;
    else void start();
  }

  // De soort waarvoor de herinnering al bevestigd is; "Opnieuw proberen" vraagt hem dan niet nog eens.
  let bevestigdeSoort: Vergadersoort | null = null;

  /** Na een mislukte start: dezelfde start nog eens, zonder de herinnering opnieuw te vragen. */
  function startOpnieuw(): void {
    if (bezig) return;
    schermFout = null;
    if (bevestigdeSoort === soort) void start();
    else vraagStart();
  }

  async function start(): Promise<void> {
    herinnering = false;
    bevestigdeSoort = soort;
    // Eerst de meters uit; de app stopt ze bij de start ook zelf (voorrang).
    metersAan = false;
    await tick();
    const ok = await voerUit(() =>
      vergaderingStart(
        soort,
        microfoonWaarde === '' ? null : microfoonWaarde,
        metSysteemgeluid(soort) && gekozenAfspeel !== '' ? gekozenAfspeel : null
      )
    );
    // Gelukt: het startscherm is weg, dus ook de meters. Mislukt: ze komen terug.
    metersAan = true;
    if (!ok) await focusNaWissel('klaar');
  }

  /** Speelt het testgeluid op de gekozen uitgang; de meter van "Geluid van" moet dan bewegen. */
  async function speelTestgeluid(): Promise<void> {
    if (testBezig) return;
    testBezig = true;
    testFout = null;
    try {
      await testgeluidSpeel(gekozenAfspeel === '' ? null : gekozenAfspeel);
    } catch (e) {
      testFout = foutUitCommando(e, t().vergaderTestgeluidFout).melding;
    } finally {
      testBezig = false;
    }
  }

  async function annuleerStart(): Promise<void> {
    herinnering = false;
    await tick();
    startKnop?.focus();
  }

  // ---- Opname en uitschrijven ----

  async function stop(): Promise<void> {
    await voerUit(() => vergaderingStop(true));
  }

  async function later(): Promise<void> {
    if (await voerUit(vergaderingLater)) meld(t().vergaderLaterSchermlezer);
  }

  async function vraagWeggooien(id: string | null): Promise<void> {
    weggooi = { id };
    await tick();
    document.getElementById('weggooi-annuleer')?.focus();
  }

  async function annuleerWeggooien(): Promise<void> {
    const was = weggooi;
    weggooi = null;
    await tick();
    // Terug naar de knop van het menu waar de vraag vandaan kwam.
    const doel =
      was?.id != null
        ? document.getElementById(`weg-${was.id}`)
        : document.getElementById('meer-nu');
    doel?.focus();
  }

  async function bevestigWeggooien(): Promise<void> {
    const doel = weggooi;
    if (doel === null) return;
    weggooi = null;
    const ok = await voerUit(() => vergaderingWeggooien(doel.id));
    if (ok) meld(t().vergaderWeggegooidSchermlezer);
    await focusNaWissel('klaar');
  }

  /** Opent de opslagmap in de Verkenner (zoals op Bestanden); een fout komt bovenaan. */
  async function openMap(): Promise<void> {
    schermFout = null;
    try {
      await bewarenOpenMap();
    } catch (e) {
      schermFout = foutUitCommando(e, t().somsGingErIetsMis);
    }
  }

  async function uitOpnameLijst(opname: OpnameInfo): Promise<void> {
    await voerUit(() =>
      opname.afgesloten ? vergaderingUitschrijven(opname.id) : vergaderingHerstel(opname.id)
    );
  }

  // ---- Het verslag ----

  function wisMeldingen(): void {
    verslagMelding = null;
    verslagFout = null;
    opgeslagen = null;
    bestaatAl = null;
  }

  async function kopieer(): Promise<void> {
    // Een verbetering waar je net uit klikte, wordt eerst bewaard.
    await verslagLijst?.wachtOpBewaren();
    wisMeldingen();
    try {
      await vergaderingKopieer();
      verslagMelding = t().tekstGekopieerd;
    } catch {
      verslagFout = t().kopierenMislukt;
    }
  }

  async function verwerk(uitslag: BewaarUitslag): Promise<void> {
    if (uitslag.type === 'opgeslagen') {
      // Met "Opname ook bewaren als geluidsbestand" staat de WAV ernaast, of zegt Rust waarom
      // niet; het tekstbestand is er in beide gevallen.
      const audio = uitslag.audio ?? null;
      opgeslagen = { naam: uitslag.naam, map: uitslag.map, audio };
      if (uitslag.audioFout) verslagFout = t().vergaderAudioNaastFout(uitslag.audioFout);
      await tick();
      opslaanKnop?.focus();
    } else if (uitslag.type === 'bestaatAl') {
      bestaatAl = { vraag: uitslag.vraag, naam: uitslag.naam, voorstel: uitslag.voorstel };
      await tick();
      document.getElementById('bestaat-al-eerste')?.focus();
    }
  }

  async function opslaan(formaat: Formaat | null, anderePlek: boolean): Promise<void> {
    if (bezig) return;
    await verslagLijst?.wachtOpBewaren();
    wisMeldingen();
    bezig = true;
    opslaanActief = true;
    try {
      await verwerk(await vergaderingOpslaan(formaat, anderePlek));
    } catch (e) {
      verslagFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
      opslaanActief = false;
    }
  }

  async function beslis(keuze: BewaarKeuze): Promise<void> {
    if (!bestaatAl || bezig) return;
    const open = bestaatAl.vraag;
    bestaatAl = null;
    bezig = true;
    opslaanActief = true;
    try {
      await verwerk(await bestandBewaarBeslis(open, keuze));
    } catch (e) {
      verslagFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
      opslaanActief = false;
    }
    if (keuze === 'annuleren') {
      await tick();
      opslaanKnop?.focus();
    }
  }

  async function openen(): Promise<void> {
    if (toestand.type !== 'verslag') return;
    try {
      await bestandOpenOpgeslagen(toestand.taak);
    } catch (e) {
      verslagFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function toonMap(): Promise<void> {
    if (toestand.type !== 'verslag') return;
    try {
      await bestandToonOpgeslagen(toestand.taak);
    } catch (e) {
      verslagFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function bewaarAudio(): Promise<void> {
    if (audioBezig) return;
    wisMeldingen();
    audioBezig = true;
    try {
      const naam = await vergaderingAudioBewaren();
      if (naam !== null) verslagMelding = t().vergaderAudioBewaard(naam);
    } catch (e) {
      verslagFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      audioBezig = false;
    }
  }

  /**
   * Past het verslag aan (een naam, een andere spreker, samenvoegen) met een commando dat een
   * nieuwe stand geeft. Geeft de melding bij een fout, anders `null`.
   */
  async function wijzigVerslag(werk: () => Promise<VergaderingStand>): Promise<string | null> {
    // De vraag "bestaat al" houdt de oude namen vast: laat haar vervallen, dan moet het
    // verslag opnieuw worden opgeslagen.
    if (bestaatAl) {
      const open = bestaatAl.vraag;
      bestaatAl = null;
      await bestandBewaarBeslis(open, 'annuleren').catch(() => undefined);
    }
    const voor = verslagInhoud();
    try {
      neemOver(await werk());
    } catch (e) {
      return foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
    // Veranderde er niets (dezelfde tekst, een markering die al weg was), dan klopt wat er opgeslagen
    // is nog. Anders klopt de melding "Opgeslagen als..." niet meer met wat er nu staat.
    if (verslagInhoud() !== voor) opgeslagen = null;
    return null;
  }

  /** Alles wat in een opgeslagen of gekopieerd verslag terechtkomt, om een wijziging te herkennen. */
  function verslagInhoud(): string {
    if (toestand.type !== 'verslag') return '';
    return JSON.stringify([
      toestand.beurten,
      toestand.persoon.vervangen,
      toestand.persoon.eigen,
      toestand.naam,
      toestand.notulen.info
    ]);
  }

  /**
   * Geeft de spreker van een beurt een naam, voor al zijn beurten. Geeft de melding bij een fout.
   * De melding noemt wat er echt gebeurde: sprekers die samengingen, of na leegmaken de naam die er
   * nu staat ("Spreker 2", niet "Anderen").
   */
  async function geefNaam(beurt: BeurtInfo, naam: string): Promise<string | null> {
    const voor = stapNummer();
    const fout = await wijzigVerslag(() => vergaderingNaam(beurt.index, naam));
    const stap = nieuweStap(voor, huidigeStap());
    if (fout === null && stap) toonStap(naamMelding(stap, t()));
    return fout;
  }

  /** De stap aan de sprekers die Rust nu heeft, of `null`. */
  function huidigeStap(): Sprekerstap | null {
    return toestand.type === 'verslag' ? toestand.sprekerstap : null;
  }

  function stapNummer(): number | null {
    return huidigeStap()?.volgnummer ?? null;
  }

  function toonStap(zin: string): void {
    stapZin = zin;
    meld(zin);
  }

  async function zetStapTerug(): Promise<void> {
    const fout = await wijzigVerslag(() => vergaderingStapTerug());
    if (fout === null) {
      stapZin = null;
      meld(t().vergaderOngedaanGemaakt);
    } else {
      verslagFout = fout;
    }
  }

  /** Verbetert de tekst van een beurt. Geeft de melding bij een fout. */
  function zetBeurtTekst(index: number, tekst: string): Promise<string | null> {
    return wijzigVerslag(() => vergaderingZetTekst(index, tekst));
  }

  /** De sprekers van het verslag dat nu open staat. */
  function huidigeSprekers(): SprekerInfo[] {
    return toestand.type === 'verslag' ? sprekersVan(toestand.beurten, t()) : [];
  }

  /** Geeft een beurt aan een andere of nieuwe spreker (`null`). Geeft de melding bij een fout. */
  async function geefAanSpreker(index: number, nummer: number | null): Promise<string | null> {
    const naam = huidigeSprekers().find((s) => s.nummer === nummer)?.naam ?? null;
    const voor = stapNummer();
    const start =
      toestand.type === 'verslag' ? toestand.beurten.find((b) => b.index === index) : null;
    const fout = await wijzigVerslag(() => vergaderingGeefAanSpreker(index, nummer));
    if (fout === null && nieuweStap(voor, huidigeStap())) {
      toonStap(naam === null ? t().vergaderNieuweSprekerGegeven : t().vergaderBeurtGegeven(naam));
    }
    // De knop "Spreker aanwijzen" verdwijnt zodra de alinea een spreker heeft: de focus gaat naar
    // het nieuwe label van die alinea, niet naar boven in het scherm.
    if (fout === null && start) void focusLabelBij(start.startMs);
    return fout;
  }

  /** Zet de focus op het label van de alinea die `startMs` omvat (na aanwijzen kan ze samengaan). */
  async function focusLabelBij(startMs: number): Promise<void> {
    await tick();
    if (toestand.type !== 'verslag') return;
    const nu = toestand.beurten.find((b) => b.startMs <= startMs && startMs < b.eindMs);
    if (!nu) return;
    // Na het sluiten van de popover zet Bits de focus nog even terug; daarna pas de onze.
    window.setTimeout(() => {
      document.querySelector<HTMLElement>(`[data-label-beurt="${String(nu.index)}"]`)?.focus();
    }, 0);
  }

  /** Voegt twee sprekers samen: alle beurten van `van` worden van `naar`. */
  async function voegSamen(van: number, naar: number): Promise<string | null> {
    const sprekers = huidigeSprekers();
    const naam = (nummer: number): string =>
      sprekers.find((s) => s.nummer === nummer)?.naam ?? t().vergaderSpreker(nummer);
    const zin = t().vergaderSamengevoegd(naam(van), naam(naar));
    const voor = stapNummer();
    const fout = await wijzigVerslag(() => vergaderingVoegSprekersSamen(van, naar));
    if (fout === null && nieuweStap(voor, huidigeStap())) toonStap(zin);
    return fout;
  }

  /** Sluit het verslag, zonder vraag: er gaat niets verloren, het staat daarna onder Eerder opgenomen. */
  async function sluit(toch: boolean): Promise<void> {
    if (toestand.type !== 'verslag' || bezig) return;
    wisMeldingen();
    const ok = await voerUit(() => vergaderingSluiten(toch));
    if (ok) meld(t().vergaderGeslotenSchermlezer);
  }

  // ---- Opnieuw uitschrijven (N-C4, ADR 0023) ----

  /** Opent de dialoog en haalt de keuzes op: rollen, aanwezigheid en de tijd voor deze opname. */
  async function vraagOpnieuw(): Promise<void> {
    wisMeldingen();
    opnieuwKeuzes = null;
    opnieuwFout = null;
    opnieuwGekozen = null;
    opnieuwOpen = true;
    try {
      opnieuwKeuzes = await vergaderingOpnieuwKeuzes();
      opnieuwGekozen = eersteKeuze(opnieuwKeuzes.keuzes);
    } catch (e) {
      opnieuwFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function startHerschrijven(model: string): Promise<void> {
    if (bezig) return;
    bezig = true;
    opnieuwFout = null;
    try {
      neemOver(await vergaderingOpnieuwStart(model));
      opnieuwOpen = false;
    } catch (e) {
      opnieuwFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
    }
  }

  /** Stoppen: het verslag blijft zoals het was. */
  async function stopOpnieuw(): Promise<void> {
    const ok = await voerUit(() => vergaderingOpnieuwStoppen());
    if (ok) verslagMelding = t().opnieuwGestopt;
  }

  /**
   * De regel voor "Vorige versie terugzetten": ze blijft beschikbaar tot er opnieuw wordt
   * uitgeschreven of het verslag sluit, wat je ook wijzigt. Terugzetten zet het hele gesprek terug
   * zoals het was, dus ook je namen en verbeterde tekst van sindsdien; daarom eerst een vraag.
   */
  async function vraagVorigeTerug(): Promise<void> {
    wisMeldingen();
    vorigeVraag = true;
    await tick();
    document.getElementById('vorige-annuleer')?.focus();
  }

  async function zetVorigeTerug(): Promise<void> {
    vorigeVraag = false;
    const fout = await wijzigVerslag(() => vergaderingVorigeTerugzetten());
    if (fout === null) verslagMelding = t().opnieuwTeruggezet;
    else verslagFout = fout;
  }

  // ---- Notulen (T3) ----

  async function laadNotulenInstellingen(): Promise<void> {
    try {
      notulenIns = await notulenInstellingen();
    } catch {
      // Zonder instellingen blijft de knop weg; Instellingen zelf meldt wat er mis is.
    }
  }

  function kiesTab(nieuw: 'verslag' | 'notulen'): void {
    tab = nieuw;
    if (nieuw === 'notulen') void laadNotulenInstellingen();
  }

  /** Pijltjestoetsen wisselen van tabblad, zoals bij elke tabbalk. */
  function tabToets(event: KeyboardEvent): void {
    if (
      event.key !== 'ArrowLeft' &&
      event.key !== 'ArrowRight' &&
      event.key !== 'Home' &&
      event.key !== 'End'
    )
      return;
    event.preventDefault();
    const nieuw = event.key === 'ArrowLeft' || event.key === 'Home' ? 'verslag' : 'notulen';
    kiesTab(nieuw);
    void tick().then(() => document.getElementById(`verslag-tab-${nieuw}`)?.focus());
  }

  async function maakNotulen(overschrijf: boolean): Promise<void> {
    if (toestand.type !== 'verslag') return;
    const aanbieder = notulenIns?.aanbieder ?? null;
    if (aanbieder === null) return;
    const bevestiging = { aanbieder, vervangen: toestand.persoon.vervangen };
    const taak = toestand.taak;
    verslagMelding = null;
    // De bevestiging telt pas na een geslaagde start; Rust controleert ze nog eens.
    const ok = await voerUit(() => vergaderingNotulenMaken(taal, bevestiging, overschrijf));
    if (ok) notulenBevestigd = { ...bevestiging, taak };
    else await laadNotulenInstellingen();
  }

  async function stopNotulen(): Promise<void> {
    const ok = await voerUit(() => vergaderingNotulenStoppen());
    if (ok) verslagMelding = t().notulenGestopt;
  }

  async function kopieerNotulen(): Promise<void> {
    wisMeldingen();
    try {
      await vergaderingNotulenKopieer();
      verslagMelding = t().notulenGekopieerd;
    } catch {
      verslagFout = t().kopierenMislukt;
    }
  }

  function pasNotulenPuntAan(
    soort: PuntSoort,
    id: number,
    tekst: string,
    wie: string | null,
    wanneer: string | null
  ): Promise<string | null> {
    return wijzigVerslag(() => vergaderingNotulenPuntAanpassen(soort, id, tekst, wie, wanneer));
  }

  function haalNotulenPuntWeg(soort: PuntSoort, id: number): Promise<string | null> {
    return wijzigVerslag(() => vergaderingNotulenPuntWeghalen(soort, id));
  }

  /** De bronknop: naar het tabblad Verslag, die beurt in beeld en even gemarkeerd. */
  async function springNaarBeurt(beurt: number): Promise<void> {
    tab = 'verslag';
    await tick();
    const regel = document.getElementById(`verslag-regel-${String(beurt)}`);
    regel?.scrollIntoView({ block: 'center', inline: 'nearest' });
    regel?.focus({ preventScroll: true });
    oplicht = beurt;
    window.clearTimeout(oplichtTimer);
    oplichtTimer = window.setTimeout(() => {
      oplicht = null;
    }, OPLICHT_MS);
  }

  /** Vanuit Eerder opgenomen: eerst het verslag openen, dan de dialoog. */
  async function opnieuwUitLijst(verslag: VerslagInfo): Promise<void> {
    if (bezig) return;
    const ok = await voerUit(() => vergaderingVerslagOpenen(verslag.id));
    if (ok) await vraagOpnieuw();
  }

  // ---- Eerder opgenomen ----

  /** Opent een bewaard verslag uit de lijst; Rust weigert tijdens een opname of het uitschrijven. */
  async function openVerslag(verslag: VerslagInfo): Promise<void> {
    if (bezig) return;
    await voerUit(() => vergaderingVerslagOpenen(verslag.id));
  }

  /** Wist een verslag uit de lijst, na de vraag in de lijst zelf. */
  async function wisUitLijst(verslag: VerslagInfo): Promise<void> {
    const ok = await voerUit(() => vergaderingVerslagWissen(verslag.id));
    if (ok) meld(t().verslagGewistSchermlezer);
    await focusNaWissel('klaar');
  }

  // ---- Wissen en de naam van het verslag ----

  async function vraagVerslagWissen(): Promise<void> {
    wisMeldingen();
    verslagWisVraag = true;
    await tick();
    document.getElementById('verslag-wis-annuleer')?.focus();
  }

  async function annuleerVerslagWissen(): Promise<void> {
    verslagWisVraag = false;
    await tick();
    document.getElementById('verslag-menu')?.focus();
  }

  async function bevestigVerslagWissen(): Promise<void> {
    verslagWisVraag = false;
    const ok = await voerUit(() => vergaderingVerslagWissen(null));
    if (ok) meld(t().verslagGewistSchermlezer);
    await focusNaWissel('klaar');
  }

  /** De titel wordt een invoerveld met de huidige titel erin. */
  async function startNaamBewerken(): Promise<void> {
    if (toestand.type !== 'verslag') return;
    bewerkNaam = verslagTitel(toestand, t(), locale);
    await tick();
    const veld = document.getElementById('verslag-naam');
    veld?.focus();
    if (veld instanceof HTMLInputElement) veld.select();
  }

  /** Bewaart de naam uit het invoerveld. Dezelfde titel als Mondig zelf maakt, of leeg, zet terug. */
  async function bewaarNaam(): Promise<void> {
    if (toestand.type !== 'verslag' || bewerkNaam === null) return;
    const typed = bewerkNaam.trim();
    const standaard = standaardTitel(toestand.soort, toestand.beginMs, t(), locale);
    const huidig = toestand.naam ?? '';
    const gewenst = typed === standaard ? '' : typed;
    bewerkNaam = null;
    if (gewenst !== huidig) {
      const fout = await wijzigVerslag(() => vergaderingVerslagNaam(gewenst));
      if (fout !== null) verslagFout = fout;
      else meld(gewenst === '' ? t().verslagNaamTerug : t().verslagNaamGegeven(gewenst));
    }
    await tick();
    document.getElementById('verslag-naam-knop')?.focus();
  }

  async function annuleerNaam(): Promise<void> {
    bewerkNaam = null;
    await tick();
    document.getElementById('verslag-naam-knop')?.focus();
  }

  // ---- Teksten ----

  const locale = $derived(taal === 'nl' ? 'nl-NL' : 'en-GB');

  function tijdTekst(ms: number): string {
    return new Intl.DateTimeFormat(locale, {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false
    }).format(new Date(ms));
  }

  function datumTekst(ms: number): string {
    return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'long' }).format(new Date(ms));
  }

  /** "gisteren, 14:02", "vandaag, 09:15" of "12 oktober, 14:02". */
  function wanneer(ms: number): string {
    const dag = dagVan(ms, Date.now());
    const woord =
      dag === 'vandaag'
        ? t().vergaderDagVandaag
        : dag === 'gisteren'
          ? t().vergaderDagGisteren
          : datumTekst(ms);
    return t().vergaderWanneer(woord, tijdTekst(ms));
  }

  function duurZin(ms: number | null): string | null {
    return ms === null ? null : t().vergaderMinuten(duurMinuten(ms));
  }

  function soortNaam(s: Vergadersoort): string {
    if (s === 'online') return t().vergaderOnline;
    return s === 'ruimte' ? t().vergaderRuimte : t().vergaderAlleen;
  }

  function soortUitleg(s: Vergadersoort): string {
    if (s === 'online') return t().vergaderOnlineUitleg;
    return s === 'ruimte' ? t().vergaderRuimteUitleg : t().vergaderAlleenUitleg;
  }

  /** "Vandaag, 09:32": de regel in de lijst begint met een hoofdletter. */
  function hoofdletter(tekst: string): string {
    return tekst.charAt(0).toLocaleUpperCase(locale) + tekst.slice(1);
  }

  /** "Wordt automatisch gewist op 9 november, 14:02.", of `null` als dat niet bekend is. */
  function wistZin(wistMs: number | null): string | null {
    if (wistMs === null) return null;
    return t().vergaderWistOp(t().vergaderWanneer(datumTekst(wistMs), tijdTekst(wistMs)));
  }

  // ---- Opstarten ----

  // De melding "Wordt morgen automatisch gewist." hangt aan de klok: zolang er opnames in de
  // lijst staan, rekent het scherm elke minuut opnieuw, en stopt daarmee bij het verlaten.
  $effect(() => {
    if (wachtend.length === 0 && verslagen.length === 0 && scherm !== 'verslag') return;
    nuMs = Date.now();
    const id = window.setInterval(() => {
      nuMs = Date.now();
    }, 60_000);
    return () => {
      window.clearInterval(id);
    };
  });

  $effect(() => {
    if (scherm !== 'opname') return;
    tik = performance.now();
    const id = window.setInterval(() => {
      tik = performance.now();
    }, 500);
    return () => {
      window.clearInterval(id);
    };
  });

  // De luidsprekers pas ophalen als "online" gekozen is, en één keer.
  $effect(() => {
    if (metSysteemgeluid(soort) && geladen && !afspeelGeladen) {
      afspeelGeladen = true;
      void laadAfspeel();
    }
  });

  onMount(() => {
    let weg = false;
    const opruimers: (() => void)[] = [];
    const houdVast = (opruimen: () => void): void => {
      if (weg) opruimen();
      else opruimers.push(opruimen);
    };
    void bewarenInstellingen()
      .then((i) => {
        standaardFormaat = i.formaat;
      })
      .catch(() => undefined);
    void laadMicrofoons();
    void bewaartermijnInstelling()
      .then((dagen) => {
        bewaarDagen = dagen;
      })
      .catch(() => undefined);
    // Eerst alle luisteraars, dan pas de stand ophalen: zo gaat er niets verloren tussen die twee.
    void Promise.all([
      luisterNaarTaken(opTaak).then(houdVast),
      luisterNaarStand(neemOver).then(houdVast),
      luisterNaarStartGevraagd(() => void haalStartVraag()).then(houdVast),
      luisterNaarNiveau((n) => {
        if (scherm === 'opname') niveaus = n;
      }).then(houdVast)
    ])
      .then(laad)
      .then(() => {
        geladen = true;
        void haalStartVraag();
      });
    return () => {
      weg = true;
      opruimers.forEach((opruimen) => {
        opruimen();
      });
    };
  });

  // De naam links is kort: het keuzeveld krijgt de ruimte, zodat de volle tekst past.
  const labelKlasse = 'flex min-w-0 flex-[1_1_7rem] flex-col gap-0.5';
  const kopKlasse = `${paginaTitel} min-w-0 break-words`;
  const gevaarKnop = 'bg-fout-tekst text-achtergrond not-aria-disabled:hover:bg-fout-tekst/90';
  const kantBalk = 'rounded-groot bg-vlak';
  // Een tekstknop als link, zoals "Opname bewaren…" onder het verslag.
  const tekstKnop =
    'cursor-pointer text-gedempt underline underline-offset-[3px] hover:text-tekst aria-disabled:cursor-default aria-disabled:opacity-60';
  const menuInhoud =
    'z-50 min-w-[210px] rounded-groot border border-lijn bg-verhoogd p-1.5 shadow-[0_8px_24px_rgba(0,0,0,0.35)]';
  const menuItem =
    'flex h-8 cursor-pointer items-center rounded-klein px-2.5 text-basis text-tekst outline-none select-none data-highlighted:bg-hover';
</script>

<svelte:head>
  <title>{t().menuVergadering} - {t().appTitle}</title>
</svelte:head>

{#snippet foutMelding()}
  {#if fout}
    <div class={foutVak} role="alert">
      <p class="m-0 flex items-start gap-2.5">
        <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />{meldingTekst(
          fout,
          t().appNietBijgewerkt
        )}
      </p>
      {#if fout.vervolg === 'opnieuw_downloaden' || fout.vervolg === 'ander_model_kiezen'}
        <Button href={resolve('/modellen')} variant={scherm === 'klaar' ? 'tweede' : 'primair'}
          >{fout.vervolg === 'opnieuw_downloaden'
            ? t().spraakmodelDownloaden
            : t().anderModelKiezen}</Button
        >
      {:else if fout.vervolg === 'opnieuw_proberen' && scherm === 'klaar'}
        <!-- Microfoon bezet of reageert niet (Vervolg::OpnieuwProberen in mondig-core). Op dit
             scherm is "Opname starten" de enige groene knop, dus deze knop is een tweede. -->
        <Button variant="tweede" aria-disabled={bezig} onclick={startOpnieuw}
          >{t().opnieuwProberen}</Button
        >
      {:else if fout.vervolg === 'microfooninstelling_openen' && scherm === 'klaar'}
        <!-- Geen microfoon of geen toegang: eerst de Windows-instelling, daarna opnieuw
             proberen als rustige tweede keuze. -->
        <div class="flex flex-wrap items-center gap-2">
          <Button
            variant="tweede"
            onclick={() => {
              void microfooninstellingOpen();
            }}>{t().microfooninstellingOpenen}</Button
          >
          <Button variant="spook" aria-disabled={bezig} onclick={startOpnieuw}
            >{t().opnieuwProberen}</Button
          >
        </div>
      {:else if fout.vervolg === 'microfooninstelling_openen'}
        <Button
          onclick={() => {
            void microfooninstellingOpen();
          }}>{t().microfooninstellingOpenen}</Button
        >
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet weggooiVraag()}
  <div
    class="{vlak} flex flex-col gap-2.5 px-4 py-3.5"
    role="alertdialog"
    aria-labelledby="weggooi-kop"
    aria-describedby="weggooi-uitleg"
  >
    <b id="weggooi-kop" class="font-semibold"
      >{weggooi?.id != null ? t().vergaderLijstWeggooienVraag : t().vergaderWeggooienKop}</b
    >
    <span id="weggooi-uitleg" class="text-gedempt empty:hidden"
      >{weggooi?.id != null ? '' : t().vergaderWeggooienTekst}</span
    >
    <div class="flex flex-wrap gap-2">
      <Button class={gevaarKnop} aria-disabled={bezig} onclick={bevestigWeggooien}
        >{t().vergaderWeggooienBevestig}</Button
      >
      <Button id="weggooi-annuleer" variant="spook" onclick={annuleerWeggooien}
        >{t().annuleren}</Button
      >
    </div>
  </div>
{/snippet}

{#snippet meter(label: string, bron: string, niveau: number)}
  <!-- Eén rij van het raster hieronder: de naam, de bron en twintig streepjes. -->
  <b class="font-semibold">{label}</b>
  <span class="text-klein text-dim max-[639px]:hidden">{bron}</span>
  <Streepjes aan={streepjesAan(niveau, METER_STREEPJES_LANG)} aantal={METER_STREEPJES_LANG} />
{/snippet}

{#snippet soortIcoon(waarde: Vergadersoort, grootte: number)}
  {#if waarde === 'online'}
    <Monitor size={grootte} aria-hidden="true" />
  {:else if waarde === 'ruimte'}
    <Users size={grootte} aria-hidden="true" />
  {:else}
    <User size={grootte} aria-hidden="true" />
  {/if}
{/snippet}

{#snippet meterMelding(fout: Foutmelding | null, meter: { opnieuw: () => void } | undefined)}
  {#if fout}
    <p class="m-0 text-klein text-gedempt" role="status">{fout.melding}</p>
    <div class="flex flex-wrap gap-2">
      <Button variant="tweede" onclick={() => meter?.opnieuw()}>{t().opnieuwProberen}</Button>
      {#if fout.vervolg === 'microfooninstelling_openen'}
        <Button variant="tweede" onclick={() => void microfooninstellingOpen()}
          >{t().microfooninstellingOpenen}</Button
        >
      {/if}
    </div>
  {/if}
{/snippet}

<div class="flex flex-col gap-[18px]">
  {#if !geladen}
    <h1 class={paginaTitel}>{t().menuVergadering}</h1>
    <p class="m-0 text-gedempt" role="status">{t().gegevensOphalen}</p>
  {:else if scherm === 'klaar'}
    <!-- 1. Klaar om op te nemen (ontwerp UX-D): soort, apparaten, starten, en pas daarna wat
         nog wacht, zodat "Opname starten" op 900 bij 640 altijd in beeld staat. -->
    <h1 id="vergader-focus" class={kopKlasse} tabindex="-1">{t().menuVergadering}</h1>
    {@render foutMelding()}

    <!-- De soort als schakelaar met drie standen; pijltjes kiezen (bits-ui RadioGroup). -->
    <div class="flex flex-col gap-2">
      <RadioGroup.Root
        bind:value={soort}
        orientation="horizontal"
        aria-label={t().vergaderSoortLabel}
        aria-describedby="soort-uitleg"
        class="inline-flex max-w-full gap-0.5 self-start rounded-[8px] border border-lijn bg-vlak p-[3px]"
      >
        {#each SOORTEN as waarde (waarde)}
          <RadioGroup.Item
            value={waarde}
            class="inline-flex h-[30px] cursor-pointer items-center gap-[7px] rounded-[6px] px-3.5 font-medium whitespace-nowrap max-[639px]:gap-1.5 max-[639px]:px-2.5 text-gedempt transition-colors outline-none hover:text-tekst focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus data-[state=checked]:bg-hover data-[state=checked]:text-tekst"
          >
            {@render soortIcoon(waarde, 14)}{soortNaam(waarde)}
          </RadioGroup.Item>
        {/each}
      </RadioGroup.Root>
      <p id="soort-uitleg" class="m-0 text-gedempt">{soortUitleg(soort)}</p>
    </div>

    <!-- De apparaten met live meters; de microfoon gaat dicht bij een ander scherm. -->
    <div class="{vlak} flex flex-col">
      <div class={rij}>
        <div class={labelKlasse}>
          <h3 id="vergader-mic-kop" class={rijNaam}>{t().microfoonKop}</h3>
          {@render meterMelding(micMeterFout, micMeter)}
        </div>
        <div class={rijBediening}>
          <Keuze
            breedte="sm:w-[19rem]"
            aria-labelledby="vergader-mic-kop"
            value={microfoonWaarde}
            title={micLijst?.microfoons.find((m) => m.id === microfoonWaarde)?.naam}
            onchange={kiesMicrofoon}
            onfocus={laadMicrofoons}
          >
            {#if !heeftBewaardeMic}
              <option value="">{t().apparaatStandaard(micLijst?.standaardNaam ?? null)}</option>
            {/if}
            {#each micLijst?.microfoons ?? [] as mic (mic.id)}
              <option value={mic.id} title={mic.naam}>{mic.naam}</option>
            {/each}
          </Keuze>
          {#if metersAan && micKlaar}
            <Niveaumeter
              bind:this={micMeter}
              bron="microfoon"
              apparaat={microfoonWaarde === '' ? null : microfoonWaarde}
              meldingTonen={false}
              bind:fout={micMeterFout}
            />
          {/if}
        </div>
      </div>
      {#if metSysteemgeluid(soort)}
        <div class="{rij} border-t border-lijn">
          <div class={labelKlasse}>
            <h3 id="vergader-geluid-kop" class={rijNaam}>{t().vergaderGeluidVan}</h3>
            {#if afspeelFout}
              <span class="text-klein text-gedempt">{t().vergaderApparatenFout}</span>
            {/if}
            {@render meterMelding(sysMeterFout, sysMeter)}
          </div>
          <div class={rijBediening}>
            <Keuze
              breedte="sm:w-[19rem]"
              aria-labelledby="vergader-geluid-kop"
              value={gekozenAfspeel}
              onchange={kiesAfspeel}
              onfocus={laadAfspeel}
            >
              <option value=""
                >{t().apparaatStandaard(
                  afspeelLijst.find((a) => a.standaard)?.naam ?? null
                )}</option
              >
              {#each afspeelLijst as apparaat (apparaat.id)}
                <option value={apparaat.id} title={apparaat.naam}>{apparaat.naam}</option>
              {/each}
            </Keuze>
            {#if metersAan}
              <Niveaumeter
                bind:this={sysMeter}
                bron="systeem"
                apparaat={gekozenAfspeel === '' ? null : gekozenAfspeel}
                meldingTonen={false}
                bind:fout={sysMeterFout}
              />
            {/if}
          </div>
          <!-- De koptelefoontip, met rechts het testgeluid voor de gekozen uitgang. -->
          <div
            class="flex basis-full flex-wrap items-start gap-x-4 gap-y-1 text-klein leading-[1.45] text-gedempt"
          >
            <p class="m-0 flex min-w-0 flex-[1_1_16rem] items-start gap-2">
              <Headphones size={14} class="mt-0.5 shrink-0" aria-hidden="true" />{t()
                .vergaderTipOnline}
            </p>
            <Button
              variant="spook"
              class="ml-auto"
              aria-disabled={testBezig}
              onclick={speelTestgeluid}
              >{testBezig ? t().vergaderTestgeluidBezig : t().vergaderTestgeluid}</Button
            >
            <p class="m-0 basis-full text-klein text-fout-tekst empty:hidden" role="status">
              {testFout ?? ''}
            </p>
          </div>
        </div>
      {:else if soort === 'ruimte'}
        <p
          class="m-0 flex items-start gap-2 border-t border-lijn px-4 py-3 text-klein leading-[1.45] text-gedempt"
        >
          <Users size={14} class="mt-0.5 shrink-0" aria-hidden="true" />{t().vergaderTipRuimte}
        </p>
      {/if}
    </div>

    <!-- De startknop direct onder de apparaten, vóór de lijsten: daar kijk je als je wilt
         beginnen (gebruikerstest na N-C). Een gewone balk, niet zwevend. -->
    <StartBalk>
      <Button
        size="groot"
        class="max-[639px]:flex-1"
        bind:ref={startKnop}
        aria-disabled={bezig}
        onclick={vraagStart}><RingIcoon grootte={12} />{t().vergaderStarten}</Button
      >
      <span class="text-klein text-dim max-[639px]:hidden">{t().vergaderModelRegel}</span>
    </StartBalk>

    {#if wachtend.length > 0}
      <!-- De opnames die nog wachten, van de soort in de schakelaar: één regel per opname, met
           een prullenbak. Daaronder een regel per andere soort die ook nog opnames heeft. -->
      <section
        class="mt-1.5 flex flex-col gap-2"
        aria-labelledby={vanSoort.length > 0 ? 'wachtend-kop' : undefined}
        aria-label={vanSoort.length > 0 ? undefined : t().vergaderWachtendLabel}
      >
        {#if vanSoort.length > 0}
          <div class="flex flex-wrap items-center gap-2">
            <h2 id="wachtend-kop" class="m-0 font-tekst text-basis font-semibold text-tekst">
              {t().vergaderNogUitTeSchrijven}
            </h2>
            <Button variant="spook" class="ml-auto" onclick={openMap}
              ><FolderOpen size={14} aria-hidden="true" />{t().bestandenMapOpenen}</Button
            >
          </div>
          {#if bewaarDagen !== null}
            <p class="-mt-1.5 mb-0 text-klein text-dim">
              {t().vergaderWachtendWistRegel(bewaarDagen)}
            </p>
          {/if}
          <ul class="{vlak} m-0 flex list-none flex-col p-0" aria-label={t().vergaderWachtendLabel}>
            {#each zichtbaar as opname (opname.id)}
              {@const wanneerTekst = wanneer(opname.beginMs)}
              {@const wist = wistBinnenkort(opname.wistMs, nuMs)}
              {@const wistMoment = wistZin(opname.wistMs)}
              {@const uitleg = [
                opname.afgesloten
                  ? null
                  : t().vergaderAfgebroken(wanneerTekst, duurZin(opname.duurMs)),
                opname.onzeker ? t().vergaderOnzeker : null,
                wistMoment
              ]
                .filter((z) => z !== null)
                .join(' ')}
              <li class="flex flex-col gap-2.5 px-4 py-2.5 [&+&]:border-t [&+&]:border-lijn">
                <div class="flex items-center gap-3" title={uitleg === '' ? undefined : uitleg}>
                  <p class="m-0 min-w-0 flex-1">
                    <span>{hoofdletter(wanneerTekst)}</span>
                    {#if opname.duurMs !== null}
                      <span class="text-dim"
                        ><span aria-hidden="true">·</span>
                        {t().vergaderMinutenKort(duurMinuten(opname.duurMs))}</span
                      >
                    {/if}
                    {#if !opname.afgesloten}
                      <span class="text-dim" aria-hidden="true">·</span>
                      <span class="text-waarsch">{t().vergaderOnderbroken}</span>
                    {/if}
                    {#if wist}
                      <span class="text-dim" aria-hidden="true">·</span>
                      <span class="text-waarsch"
                        >{wist === 'vandaag'
                          ? t().vergaderWistKortVandaag
                          : t().vergaderWistKortMorgen}</span
                      >
                    {/if}
                    {#if uitleg !== ''}<span class="sr-only">. {uitleg}</span>{/if}
                  </p>
                  <Button
                    variant="tweede"
                    aria-disabled={bezig}
                    aria-label={t().vergaderLijstKnopAria(
                      opname.afgesloten
                        ? t().vergaderUitschrijvenKnop
                        : t().vergaderHerstelUitschrijven,
                      wanneerTekst
                    )}
                    onclick={() => uitOpnameLijst(opname)}
                    >{opname.afgesloten
                      ? t().vergaderUitschrijvenKnop
                      : t().vergaderHerstelUitschrijven}</Button
                  >
                  <button
                    type="button"
                    id="weg-{opname.id}"
                    class="inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-klein text-gedempt transition-colors hover:bg-hover hover:text-tekst aria-disabled:cursor-default aria-disabled:opacity-50"
                    aria-label={t().vergaderVerwijderAria(wanneerTekst)}
                    title={t().vergaderVerwijderen}
                    aria-disabled={bezig}
                    onclick={() => {
                      if (!bezig) void vraagWeggooien(opname.id);
                    }}
                  >
                    <Trash2 size={16} aria-hidden="true" />
                  </button>
                </div>
                {#if weggooi?.id === opname.id}
                  {@render weggooiVraag()}
                {/if}
              </li>
            {/each}
          </ul>
          {#if vanSoort.length > WACHTEND_ZICHTBAAR}
            <button
              type="button"
              class="{tekstKnop} self-start text-klein"
              aria-expanded={alleTonen}
              onclick={() => {
                alleTonen = !alleTonen;
              }}
              >{alleTonen
                ? t().vergaderMinderTonen
                : t().vergaderAlleTonen(vanSoort.length)}</button
            >
          {/if}
        {/if}
        {#each anderen as ander (ander.soort)}
          <button
            type="button"
            class="{tekstKnop} inline-flex items-center gap-1.5 self-start text-klein {ander.wist
              ? 'text-waarsch'
              : ''}"
            onclick={() => {
              soort = ander.soort;
            }}
          >
            {#if ander.wist}<CircleAlert size={14} aria-hidden="true" />{/if}
            {t().vergaderOokNog(ander.aantal, soortNaam(ander.soort), ander.wist, ander.aantalWist)}
          </button>
        {/each}
      </section>
    {/if}

    <!-- De bewaarde verslagen (N-C3): de laatste vijf, een klik opent het verslag. -->
    <EerderOpgenomen
      {verslagen}
      nu={nuMs}
      {bewaarDagen}
      {bezig}
      open={openVerslag}
      wis={wisUitLijst}
      opnieuw={opnieuwUitLijst}
    />
  {:else if scherm === 'starten'}
    <h1 class={kopKlasse}>{t().menuVergadering}</h1>
    {@render foutMelding()}
    <h2 id="vergader-focus" class="m-0 text-kop2" tabindex="-1" role="status">
      {t().vergaderStartBezig}
    </h2>
  {:else if toestand.type === 'voorbereiden'}
    <h1 class={kopKlasse}>{t().menuVergadering}</h1>
    <h2 id="vergader-focus" class="m-0 text-kop2" tabindex="-1" role="status">
      {toestand.herstellen ? t().vergaderHerstellen : t().vergaderAfronden}
    </h2>
  {:else if toestand.type === 'neemtOp'}
    <!-- 3. Opname loopt. -->
    <h1 class={kopKlasse}>{t().menuVergadering}</h1>
    {@render foutMelding()}
    <div class="flex flex-col gap-1">
      <div class="flex flex-wrap items-center gap-2.5 text-midden">
        <span
          class="size-2.5 shrink-0 rounded-full bg-opname motion-safe:animate-pulse"
          aria-hidden="true"
        ></span>
        <h2 id="vergader-focus" class="m-0 font-tekst text-midden font-semibold" tabindex="-1">
          {t().vergaderLoopt}
        </h2>
        <span class="text-dim"><span aria-hidden="true">·</span> {soortNaam(toestand.soort)}</span>
      </div>
      <div
        role="timer"
        aria-label={t().vergaderKlokAria}
        class="font-kop text-[60px] leading-[1.1] font-semibold tracking-[-0.02em] tabular-nums max-[639px]:text-[44px]"
      >
        {klokTijd(duurNu)}
      </div>
    </div>
    <div
      class="{vlak} grid grid-cols-[auto_1fr_auto] items-center gap-x-4 gap-y-3 px-4 py-3.5 max-[639px]:grid-cols-[auto_1fr]"
      aria-hidden="true"
    >
      {#if toestand.soort === 'alleen'}
        <!-- Alleen ik: geen Jij en Anderen, alleen de microfoon. -->
        {@render meter(t().vergaderBronMicrofoon, '', niveaus?.microfoon ?? 0)}
      {:else}
        {@render meter(t().sprekerJij, t().vergaderBronMicrofoon, niveaus?.microfoon ?? 0)}
      {/if}
      {#if metSysteemgeluid(toestand.soort)}
        {@render meter(t().sprekerAnderen, t().vergaderBronComputer, niveaus?.systeem ?? 0)}
      {/if}
    </div>
    <div class="flex flex-col gap-2 empty:hidden" role="status">
      {#if toestand.systeemStil}
        <div class="{kantBalk} flex items-start gap-3 px-3.5 py-3">
          <span class="mt-1.5 size-2 shrink-0 rounded-full bg-waarsch" aria-hidden="true"></span>
          <p class="m-0">{t().vergaderSysteemStil}</p>
        </div>
      {/if}
      {#each toestand.meldingen as melding (melding)}
        <p class="m-0 text-gedempt">{melding}</p>
      {/each}
    </div>
    {#if weggooi !== null && weggooi.id === null}
      {@render weggooiVraag()}
    {:else}
      <!-- Weggooien niet naast de stopknop (een misklik kost de hele opname), maar in een menu. -->
      <div class="flex flex-wrap items-center gap-2">
        <Button variant="rood" size="groot" aria-disabled={bezig} onclick={stop}
          ><Square size={14} fill="currentColor" aria-hidden="true" />{t()
            .vergaderStopUitschrijven}</Button
        >
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                id="meer-nu"
                variant="tweede"
                size="icoon-groot"
                aria-label={t().vergaderMeerMenuNu}
                title={t().vergaderMeerMenuNu}
              >
                <Ellipsis size={16} aria-hidden="true" />
              </Button>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              align="start"
              sideOffset={6}
              class={menuInhoud}
              onCloseAutoFocus={(e) => {
                if (weggooi !== null) e.preventDefault();
              }}
            >
              <DropdownMenu.Item class={menuItem} onSelect={() => vraagWeggooien(null)}
                >{t().vergaderOpnameWeggooien}</DropdownMenu.Item
              >
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      </div>
    {/if}
  {:else if toestand.type === 'uitschrijven'}
    <!-- 4. Uitschrijven. -->
    <h1 class={kopKlasse}>{t().menuVergadering}</h1>
    {@render foutMelding()}
    <div class="flex flex-col gap-2.5">
      <div class="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
        <h2 id="vergader-focus" class="m-0 text-kop2" tabindex="-1">
          {#if sprekersStap}
            {t().vergaderSprekersHerkennen}
          {:else if voortgang === null}
            {t().vergaderUitschrijvenKop}
          {:else}
            {t().vergaderUitschrijvenStand(t().bestandProcent(voortgang))}
          {/if}
        </h2>
        <!-- De resterende tijd naast het percentage: "nog ongeveer 3 minuten". -->
        {#if resterend !== null && voortgang !== null && !sprekersStap}
          <span class="text-klein text-dim"
            >{resterend.charAt(0).toLowerCase()}{resterend.slice(1)}</span
          >
        {/if}
      </div>
      <div class="h-1.5 w-full overflow-hidden rounded-full bg-hover" aria-hidden="true">
        <!-- Bij het herkennen van de sprekers is er geen percentage: de balk is vol en knippert rustig. -->
        <div
          class="h-full bg-actie transition-[width] {sprekersStap
            ? 'motion-safe:animate-pulse'
            : ''}"
          style:width="{sprekersStap ? 100 : (voortgang ?? 0)}%"
        ></div>
      </div>
      <p class="m-0 text-gedempt">{t().vergaderUitschrijvenHulp}</p>
    </div>
    <div class="flex flex-col items-start gap-1.5 border-t border-lijn pt-4">
      <Button variant="tweede" aria-disabled={bezig} onclick={later}>{t().vergaderLater}</Button>
      <span class="text-klein text-gedempt">{t().vergaderLaterHulp}</span>
    </div>
  {:else if toestand.type === 'verslag'}
    <!-- 5 en 6. Het verslag (N-C1): bovenaan terug en nieuw, dan de titel met zijn knoppen, één
         gegevensregel, en daaronder de werkbalk, de persoonsgegevens en de tekst zelf. -->
    {@const labels = toontLabels(toestand.soort, toestand.beurten)}
    {@const sprekers = sprekersVan(toestand.beurten, t())}
    {@const titel = verslagTitel({ ...toestand, naam: toestand.naamGetoond }, t(), locale)}
    {@const aantal = aantalSprekers(toestand.beurten)}
    {@const rol = toestand.modelRol ?? modelRol(toestand.model, modellenStand.overzicht)}
    {@const bewaarZin = bewaarTotTekst(toestand.wistMs, nuMs, t(), locale)}
    <div class="-mt-2 -mb-1.5 -ml-2 flex items-center gap-2">
      <Button
        variant="spook"
        aria-disabled={bezig}
        title={t().verslagSluitenTitel}
        onclick={() => sluit(false)}
        ><ArrowLeft size={16} aria-hidden="true" />{t().verslagSluiten}</Button
      >
      <Button
        variant="spook"
        class="ml-auto"
        aria-disabled={bezig}
        title={t().verslagSluitenTitel}
        onclick={() => sluit(false)}><RingIcoon grootte={11} />{t().verslagNieuweOpname}</Button
      >
    </div>
    {#if verslagWisVraag}
      <div
        class="{vlak} flex flex-col gap-2.5 px-4 py-3.5"
        role="alertdialog"
        aria-labelledby="verslag-wis-kop"
        aria-describedby="verslag-wis-uitleg"
      >
        <b id="verslag-wis-kop" class="font-semibold">{t().verslagWissenKop}</b>
        <span id="verslag-wis-uitleg" class="text-gedempt">{t().verslagWissenTekst}</span>
        <div class="flex flex-wrap gap-2">
          <Button class={gevaarKnop} aria-disabled={bezig} onclick={bevestigVerslagWissen}
            >{t().verslagWissenBevestig}</Button
          >
          <Button id="verslag-wis-annuleer" variant="spook" onclick={annuleerVerslagWissen}
            >{t().annuleren}</Button
          >
        </div>
      </div>
    {/if}
    {#if klaarMoment}
      <KlaarMoment
        zin={klaarZin(klaarMoment, t())}
        sluit={() => {
          klaarMoment = null;
        }}
      />
    {/if}
    <div class="flex items-center gap-2">
      <VerslagTitel
        {titel}
        bind:waarde={bewerkNaam}
        start={startNaamBewerken}
        bewaar={bewaarNaam}
        annuleer={annuleerNaam}
      />
      <Button
        variant="tweede"
        class="max-[639px]:w-8 max-[639px]:px-0"
        title={t().kopieren}
        onclick={kopieer}
        ><Copy size={16} aria-hidden="true" /><span class="max-[639px]:sr-only">{t().kopieren}</span
        ></Button
      >
      <OpslaanKnop
        standaard={standaardFormaat}
        {bezig}
        kort
        bind:ref={opslaanKnop}
        opslaan={(formaat: Formaat | null, anderePlek: boolean) => {
          void opslaan(formaat, anderePlek);
        }}
      />
      <!-- Opnieuw uitschrijven (N-C4), de opname als geluid, de naam en wissen. -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              id="verslag-menu"
              variant="tweede"
              size="icoon"
              aria-label={t().verslagMenu}
              title={t().verslagMenu}
            >
              <Ellipsis size={16} aria-hidden="true" />
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            sideOffset={6}
            class={menuInhoud}
            onCloseAutoFocus={(e) => {
              // De focus gaat naar het invoerveld, de vraag of de dialoog, niet terug naar de menuknop.
              if (bewerkNaam !== null || verslagWisVraag || opnieuwOpen) e.preventDefault();
            }}
          >
            {#if toestand.opnameAanwezig}
              <DropdownMenu.Item
                class="{menuItem} gap-2"
                disabled={toestand.opnieuw !== null}
                onSelect={() => {
                  void vraagOpnieuw();
                }}><RefreshCw size={14} aria-hidden="true" />{t().opnieuwMenu}</DropdownMenu.Item
              >
            {:else}
              <DropdownMenu.Item
                class="flex cursor-default items-start gap-2 rounded-klein px-2.5 py-1.5 text-basis text-dim outline-none select-none"
                disabled
                ><RefreshCw size={14} class="mt-1 shrink-0" aria-hidden="true" /><span
                  class="flex flex-col"
                  >{t().opnieuwGewistMenu}<small class="text-klein">{t().opnieuwGewistReden}</small
                  ></span
                ></DropdownMenu.Item
              >
            {/if}
            <DropdownMenu.Item class="{menuItem} gap-2" onSelect={startNaamBewerken}
              ><Pencil size={14} aria-hidden="true" />{t().verslagNaamWijzigen}</DropdownMenu.Item
            >
            {#if toestand.audioAanwezig}
              <DropdownMenu.Item class={menuItem} onSelect={bewaarAudio}
                >{t().vergaderAudioBewarenMenu}</DropdownMenu.Item
              >
            {/if}
            <DropdownMenu.Separator class="my-1 h-px bg-lijn" />
            <DropdownMenu.Item
              class="{menuItem} gap-2 text-fout-tekst"
              onSelect={vraagVerslagWissen}
              ><Trash2 size={14} aria-hidden="true" />{t().verslagWissenMenu}</DropdownMenu.Item
            >
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </div>

    <!-- Eén gegevensregel, als gedempte tekst met middenpunten. -->
    <p class="m-0 -mt-3 flex flex-wrap items-center gap-x-1.5 text-klein text-dim">
      <span>{hoofdletter(wanneer(toestand.beginMs))}</span><span aria-hidden="true">·</span>
      <span>{t().vergaderMinutenKort(duurMinuten(toestand.duurMs))}</span>
      {#if aantal !== null}<span aria-hidden="true">·</span><span
          >{t().verslagSprekers(aantal)}</span
        >{/if}
      {#if toestand.soort === 'ruimte'}<span aria-hidden="true">·</span><span
          >{t().vergaderInDeRuimte}</span
        >{/if}
      <span aria-hidden="true">·</span>
      <span title={t().uitgeschrevenMet(toestand.model)}
        >{rol === 'snel'
          ? t().verslagModelSnel
          : rol === 'nauwkeurig'
            ? t().verslagModelNauwkeurig
            : rol === 'extra_nauwkeurig'
              ? t().verslagModelExtraNauwkeurig
              : toestand.model}<span class="sr-only">, {t().uitgeschrevenMet(toestand.model)}</span
        ></span
      >
      <span aria-hidden="true">·</span>
      <span class="inline-flex items-center gap-1"
        ><ShieldCheck size={14} class="text-actie" aria-hidden="true" />{t().opDezeComputer}</span
      >
      {#if bewaarZin}
        <span class="inline-flex gap-x-1.5 whitespace-nowrap"
          ><span aria-hidden="true">·</span>{bewaarZin}</span
        >
      {/if}
    </p>

    <!-- De tabbladen Verslag en Notulen (T3): onder de gegevensregel, want twee tekstkolommen
         naast elkaar zijn op 900 breed niet leesbaar. -->
    <div
      class="-mt-1 flex gap-1 border-b border-lijn"
      role="tablist"
      aria-label={t().verslagTabsLabel}
      tabindex="-1"
      onkeydown={tabToets}
    >
      {#each [{ id: 'verslag', naam: t().verslagTabVerslag }, { id: 'notulen', naam: t().verslagTabNotulen }] as kaart (kaart.id)}
        {@const actief = tab === kaart.id}
        <button
          type="button"
          role="tab"
          id="verslag-tab-{kaart.id}"
          aria-selected={actief}
          aria-controls="verslag-paneel-{kaart.id}"
          tabindex={actief ? 0 : -1}
          class="-mb-px inline-flex min-h-9 cursor-pointer items-center gap-2 border-b-2 px-3.5 text-basis font-medium transition-colors {actief
            ? 'border-actie text-tekst'
            : 'border-transparent text-gedempt hover:text-tekst'}"
          onclick={() => {
            kiesTab(kaart.id === 'notulen' ? 'notulen' : 'verslag');
          }}
        >
          {kaart.naam}
          {#if kaart.id === 'notulen' && toestand.notulen.maken}
            <span
              class="size-3 shrink-0 rounded-full border-2 border-lijn border-t-actie motion-safe:animate-spin"
              aria-hidden="true"
            ></span>
          {/if}
        </button>
      {/each}
    </div>

    <!-- Opnieuw uitschrijven (N-C4): tijdens het lopen de balk met Stoppen, daarna de melding. -->
    {#if toestand.opnieuw}
      <OpnieuwVoortgang stand={toestand.opnieuw} {bezig} stop={stopOpnieuw} />
    {:else if toestand.naOpnieuw && toestand.vorigeVersie}
      <OpnieuwMelding na={toestand.naOpnieuw} {bezig} terug={vraagVorigeTerug} />
      {#if vorigeVraag}
        <div
          class="{vlak} flex flex-col gap-2.5 px-4 py-3.5"
          role="alertdialog"
          aria-labelledby="vorige-kop"
          aria-describedby="vorige-uitleg"
        >
          <b id="vorige-kop" class="font-semibold">{t().opnieuwTerugzettenKop}</b>
          <span id="vorige-uitleg" class="text-gedempt">{t().opnieuwTerugzettenTekst}</span>
          <div class="flex flex-wrap gap-2">
            <Button variant="tweede" aria-disabled={bezig} onclick={zetVorigeTerug}
              >{t().opnieuwTerugzettenBevestig}</Button
            >
            <Button id="vorige-annuleer" variant="spook" onclick={() => (vorigeVraag = false)}
              >{t().annuleren}</Button
            >
          </div>
        </div>
      {/if}
    {/if}

    <!-- Na een naam, samenvoegen of een andere spreker: wat er gebeurde, met Ongedaan maken. -->
    {#if toestand.sprekerstap && stapZin}
      <div
        class="flex flex-wrap items-center gap-2.5 rounded-groot border border-lijn bg-vlak py-2.5 pr-3 pl-3.5"
        role="status"
      >
        <span class="min-w-0 flex-1 text-klein" data-testid="sprekerstap">{stapZin}</span>
        <Button variant="spook" aria-disabled={bezig} onclick={() => void zetStapTerug()}
          >{t().vergaderOngedaanMaken}</Button
        >
      </div>
    {/if}

    <!-- Wat er net gebeurde: kopiëren, bezig, opgeslagen, de vraag, of een fout. -->
    <p class="m-0 text-klein text-gedempt empty:hidden" role="status">
      {opslaanActief
        ? t().opslaanBezig
        : audioBezig
          ? t().vergaderAudioBezig
          : (verslagMelding ?? '')}
    </p>
    {#if verslagFout}
      <div class={foutVak} role="alert">
        <p class="m-0 flex items-start gap-2.5">
          <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />{verslagFout}
        </p>
      </div>
    {/if}
    {@render foutMelding()}
    {#if toestand.bewarenMislukt}
      <!-- Het verslag is niet naast de opname bewaard: Sluiten weigert, maar de gebruiker kan toch sluiten. -->
      <div>
        <Button variant="tweede" aria-disabled={bezig} onclick={() => sluit(true)}
          >{t().verslagTochSluiten}</Button
        >
      </div>
    {/if}
    <div role="status" class="empty:hidden">
      {#if opgeslagen}
        <div class="flex flex-wrap items-center gap-2.5 rounded-groot bg-vlak px-3.5 py-2.5">
          <span class="size-2 shrink-0 rounded-full bg-gelukt" aria-hidden="true"></span>
          <span class="min-w-0 flex-1 break-words"
            ><b class="font-semibold">{t().opgeslagenKop}</b>
            {t().opgeslagenAlsIn(opgeslagen.naam, mapTekst(opgeslagen.map, t()))}
            {#if opgeslagen.audio}{t().vergaderAudioNaast(opgeslagen.audio)}{/if}</span
          >
          <span class="ml-auto flex gap-1.5">
            <Button variant="tweede" onclick={openen}>{t().opgeslagenOpenen}</Button>
            <Button variant="spook" onclick={toonMap}>{t().opgeslagenMapTonen}</Button>
          </span>
        </div>
      {/if}
    </div>
    {#if bestaatAl}
      <div
        class="{vlak} flex flex-col gap-2.5 px-4 py-3.5"
        role="alertdialog"
        aria-labelledby="bestaat-al-kop"
        aria-describedby="bestaat-al-naam"
      >
        <b id="bestaat-al-kop" class="font-semibold">{t().bestaatAlKop}</b>
        <span id="bestaat-al-naam" class="break-words text-gedempt">{bestaatAl.naam}</span>
        <div class="flex flex-wrap gap-2">
          <Button id="bestaat-al-eerste" onclick={() => beslis('nummer')}
            >{t().opslaanAlsNummer(volgnummer(bestaatAl.voorstel))}</Button
          >
          <Button variant="tweede" onclick={() => beslis('vervangen')}>{t().vervangen}</Button>
          <Button variant="spook" onclick={() => beslis('annuleren')}>{t().annuleren}</Button>
        </div>
      </div>
    {/if}

    <div
      id="verslag-paneel-verslag"
      role="tabpanel"
      aria-labelledby="verslag-tab-verslag"
      hidden={tab !== 'verslag'}
      class="flex flex-col gap-4"
    >
      {#if toestand.weggelaten > 0 && echoToon}
        <div class="{kantBalk} flex items-start gap-3 px-3.5 py-3" role="note">
          <Info size={16} class="mt-0.5 shrink-0 text-gedempt" aria-hidden="true" />
          <p class="m-0 flex-1">{t().vergaderEchoMelding}</p>
          <Button
            variant="spook"
            class="-my-1 -mr-1.5 px-1.5"
            title={t().vergaderMeldingSluiten}
            aria-label={t().vergaderMeldingSluiten}
            onclick={() => {
              echoToon = false;
            }}><X size={16} aria-hidden="true" /></Button
          >
        </div>
      {/if}
      {#each toestand.meldingen as melding (melding)}
        <p class="m-0 text-gedempt">{melding}</p>
      {/each}

      {#if toestand.beurten.length === 0}
        <p class="m-0 text-gedempt">{t().transcriptGeenTekst}</p>
      {:else}
        {@const beurten = toestand.beurten}
        {@const soort = toestand.soort}
        <VerslagLijst
          bind:this={verslagLijst}
          regels={beurten.map((b) => ({
            sleutel: b.index,
            startMs: b.startMs,
            eindMs: b.eindMs,
            tekst: b.tekst,
            markeringen: b.markeringen,
            veranderd: b.veranderd,
            // Een alinea zonder spreker heeft geen naam: zoeken vindt er geen "Jij" in.
            naam: labels && !zonderSpreker(soort, b) ? beurtNaamGetoond(b, t()) : undefined
          }))}
          {labels}
          lijstLabel={t().vergaderBeurtenLabel}
          zetTekst={zetBeurtTekst}
          persoon={toestand.persoon}
          zetVervangen={(aan: boolean) => wijzigVerslag(() => vergaderingPersoonVervangen(aan))}
          haalWeg={(tekst: string) => wijzigVerslag(() => vergaderingPersoonWeghalen(tekst))}
          markeer={(tekst: string) => wijzigVerslag(() => vergaderingPersoonMarkeer(tekst))}
          haalAudio={toestand.audioAanwezig ? vergaderingAudioStuk : null}
          geenGeluid={t().verslagGeenGeluidOpname}
          opgelicht={oplicht}
          inBeeld={tab === 'verslag'}
        >
          {#snippet label(regel: VerslagRegel)}
            {@const beurt = beurten.find((b) => b.index === regel.sleutel)}
            {#if beurt && zonderSpreker(soort, beurt)}
              <SprekerNaam
                {beurt}
                {sprekers}
                aanwijzen="kolom"
                kanAanSpreker
                geef={(naam: string) => geefNaam(beurt, naam)}
                wijsToe={(nummer: number | null) => geefAanSpreker(beurt.index, nummer)}
              />
            {:else if beurt}
              <SprekerNaam
                {beurt}
                {sprekers}
                kanAanSpreker={kanAanSpreker(soort, beurten, beurt)}
                geef={(naam: string) => geefNaam(beurt, naam)}
                wijsToe={(nummer: number | null) => geefAanSpreker(beurt.index, nummer)}
                voegSamen={(naar: number) => voegSamen(beurt.sprekernummer ?? naar, naar)}
              />
            {/if}
          {/snippet}
          {#snippet aanwijs(regel: VerslagRegel)}
            {@const beurt = beurten.find((b) => b.index === regel.sleutel)}
            {#if beurt && kanAanSpreker(soort, beurten, beurt)}
              <SprekerNaam
                {beurt}
                {sprekers}
                aanwijzen="rij"
                kanAanSpreker
                geef={(naam: string) => geefNaam(beurt, naam)}
                wijsToe={(nummer: number | null) => geefAanSpreker(beurt.index, nummer)}
              />
            {/if}
          {/snippet}
        </VerslagLijst>
      {/if}
    </div>
    <div
      id="verslag-paneel-notulen"
      role="tabpanel"
      aria-labelledby="verslag-tab-notulen"
      hidden={tab !== 'notulen'}
    >
      {#if tab === 'notulen'}
        <NotulenPaneel
          stand={toestand.notulen}
          instellingen={notulenIns}
          vervangen={toestand.persoon.vervangen}
          bevestigd={bevestigingGeldt(
            notulenBevestigd,
            toestand.taak,
            notulenIns?.aanbieder ?? null,
            toestand.persoon.vervangen
          )}
          {bezig}
          maak={(overschrijf: boolean) => void maakNotulen(overschrijf)}
          stop={() => void stopNotulen()}
          spring={(beurt: number) => void springNaarBeurt(beurt)}
          kopieer={() => void kopieerNotulen()}
          pasAan={pasNotulenPuntAan}
          haalWeg={haalNotulenPuntWeg}
        />
      {/if}
    </div>
  {/if}
</div>

<!-- 2. De herinnering vóór elke start: altijd, geen "niet meer vragen". -->
<Dialog.Root
  open={herinnering}
  onOpenChange={(open) => {
    if (!open && herinnering) void annuleerStart();
  }}
>
  <Dialog.Portal>
    <Dialog.Overlay class="fixed inset-0 z-50 bg-black/60" />
    <Dialog.Content
      class="fixed top-1/2 left-1/2 z-50 flex w-[440px] max-w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-[12px] border border-lijn bg-verhoogd p-6 shadow-[0_16px_48px_rgba(0,0,0,0.5)]"
      onOpenAutoFocus={(e) => {
        e.preventDefault();
        herinneringStart?.focus();
      }}
      onCloseAutoFocus={(e) => {
        e.preventDefault();
      }}
    >
      <Dialog.Title class="m-0 font-kop text-kop2 font-semibold"
        >{t().vergaderHerinneringKop}</Dialog.Title
      >
      <Dialog.Description class="m-0 text-midden leading-[1.55] text-gedempt"
        >{t().vergaderHerinneringTekst}</Dialog.Description
      >
      <div class="mt-2 flex flex-wrap justify-end gap-2">
        <Button variant="spook" onclick={annuleerStart}>{t().annuleren}</Button>
        <Button bind:ref={herinneringStart} onclick={start}>{t().vergaderHerinneringStart}</Button>
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<!-- Opnieuw uitschrijven (N-C4): de keuze van het model, wat er blijft, en starten. -->
<Dialog.Root bind:open={opnieuwOpen}>
  <Dialog.Portal>
    <Dialog.Overlay class="fixed inset-0 z-50 bg-black/60" />
    <Dialog.Content
      class="fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100vh-2rem)] w-[460px] max-w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-auto rounded-[12px] border border-lijn bg-verhoogd p-6 shadow-[0_16px_48px_rgba(0,0,0,0.5)] max-[639px]:p-4"
      aria-labelledby="opnieuw-kop"
    >
      <OpnieuwDialoog
        keuzes={opnieuwKeuzes}
        fout={opnieuwFout}
        verbeterd={toestand.type === 'verslag' ? toestand.verbeterd : 0}
        opnameMs={toestand.type === 'verslag' ? toestand.duurMs : 0}
        bind:gekozen={opnieuwGekozen}
        {bezig}
        annuleer={() => {
          opnieuwOpen = false;
        }}
        start={(model: string) => {
          void startHerschrijven(model);
        }}
      />
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<!-- Start, stop, voortgang en klaar voor de schermlezer, zonder de focus te verplaatsen (WCAG 4.1.3). -->
<div role="status" class="sr-only">{schermlezerZin}</div>
