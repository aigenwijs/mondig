import type { ModelTekst } from './modeltekst';

// Nederlandse berichten: dit bestand is de bron. Een bericht is een tekst of een
// functie met getypte parameters (voor getallen en meervoud).
export const nl = {
  appTitle: 'Mondig',
  modelNodigKop: 'Eerst een spraakmodel downloaden',
  modelNodigTekst: (grootte: string): string =>
    `Eenmalig ${grootte}. Daarna werkt Mondig zonder internet.`,
  downloaden: 'Downloaden',
  homeDictate: 'Dicteren',
  terug: 'Terug',
  spraakmodelDownloaden: 'Spraakmodel downloaden',
  aantalBestanden: (n: number): string => {
    const regel = new Intl.PluralRules('nl').select(n);
    return regel === 'one' ? `${String(n)} bestand` : `${String(n)} bestanden`;
  },

  // Algemeen
  opnieuwControleren: 'Opnieuw controleren',
  opnieuwProberen: 'Opnieuw proberen',
  opnieuwDownloaden: 'Opnieuw downloaden',
  anderModelKiezen: 'Naar het spraakmodel',
  appNietBijgewerkt:
    'Mondig is niet volledig bijgewerkt. Sluit Mondig af en installeer de nieuwste versie opnieuw.',
  annuleren: 'Annuleren',
  kopierenMislukt: 'Kopiëren lukte niet. Selecteer de tekst en kopieer met Ctrl+C.',
  somsGingErIetsMis: 'Er ging iets mis. Probeer het opnieuw.',
  modelStatusFout:
    'Mondig kon niet nagaan of het spraakmodel er is. Probeer het opnieuw. Helpt dat niet, start Mondig dan opnieuw.',

  // Downloadscherm
  verderDownloaden: 'Verder downloaden',
  stoppen: 'Stoppen',
  downloadVoortgangNaam: 'Voortgang van de download',
  procent: (n: number): string => `${String(n)}%`,
  downloadVoortgang: (binnen: string, totaal: string): string => `${binnen} van ${totaal}`,
  downloadControleren: 'Bezig met controleren…',
  nogSeconden: (n: number): string => `Nog ongeveer ${String(n)} seconden`,
  nogMinuten: (n: number): string =>
    n === 1 ? 'Nog ongeveer 1 minuut' : `Nog ongeveer ${String(n)} minuten`,
  nogUren: (n: number): string => `Nog ongeveer ${String(n)} uur`,
  procentGedownload: (n: number): string => `${String(n)}% gedownload`,
  procentGedownloadControleren: (n: number): string =>
    `${String(n)}% gedownload. Het model wordt gecontroleerd.`,
  gegevensOphalen: 'Gegevens ophalen…',

  // Proef op het dicteerscherm
  bezigMetUitschrijven:
    'Bezig met uitschrijven… De eerste keer duurt dit wat langer, omdat het spraakmodel wordt geladen.',
  klaarIn: (seconden: number): string => {
    const tekst = new Intl.NumberFormat('nl', {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1
    }).format(seconden);
    return `Klaar in ${tekst} seconde${seconden >= 2 ? 'n' : ''}`;
  },
  lokaalVerwerkt: 'Lokaal verwerkt',
  uitgeschrevenMet: (naam: string): string => `Uitgeschreven met ${naam}`,
  tekstGekopieerd: 'De tekst is gekopieerd.',
  geenGeluid: 'Geen geluid gehoord.',
  geenTekst: 'Er is geen tekst herkend.',
  tekstVerdacht: 'Er is bijna geen spraak in de opname, dus de tekst klopt waarschijnlijk niet.',
  // Dicteren met de sneltoets
  sneltoetsUitleg: (toets: string): string =>
    `Houd ${toets} ingedrukt en praat. Kort tikken kan ook.`,
  sneltoetsTerugval: (gewenst: string, actief: string): string =>
    `${gewenst} wordt al door een ander programma gebruikt. Mondig gebruikt nu ${actief}.`,
  // De opgeslagen sneltoets kan op deze computer niet (bijvoorbeeld Ctrl+Spatie op de Mac, waar
  // het systeem die zelf gebruikt). Geen foutmelding: Mondig koos al een andere.
  sneltoetsOvergezet: (gewenst: string, actief: string): string =>
    `${gewenst} kan op deze computer niet als sneltoets. Mondig gebruikt nu ${actief}. Je kunt een andere toets kiezen onder Wijzigen.`,
  sneltoetsGeen: (toetsen: string): string =>
    `Mondig kon geen sneltoets instellen: ${toetsen} worden al door andere programma's gebruikt. Sluit zo'n programma en start Mondig opnieuw, of kies zelf een toets bij Instellingen, onder Geavanceerd.`,
  dictaatLuistert: 'Mondig luistert…',
  dictaatHoe: (toets: string): string =>
    `Klaar? Laat ${toets} los of tik hem nog eens. Esc annuleert.`,
  dictaatBijnaVol: 'Nog een minuut, dan stopt Mondig met luisteren en schrijft het uit.',
  dictaatGeannuleerd: 'Het dictaat is geannuleerd.',
  dictaatOpnieuw: (toets: string): string => `Probeer het opnieuw met ${toets}.`,
  laatsteDictaatKop: 'Laatste dictaat',
  // Dicteren: sneltoets, microfoon en proefveld op de pagina zelf (UX-D)
  proefveldKop: 'Probeer het hier',
  proefveldPlaats: (toets: string): string => `Klik hier, houd ${toets} ingedrukt en zeg iets.`,
  proefveldLabel: 'Proefveld om te dicteren',
  plakInProefveld: 'De tekst staat in het proefveld.',
  // Plakken (sprint 3c)
  kopieren: 'Kopiëren',
  plakGeplakt: 'Geplakt waar je typte.',
  plakWaarschijnlijk: 'Geplakt. Controleer even of de tekst er staat.',
  plakVorigKlembordKwijt: 'Je vorige klembord kon niet bewaard worden.',
  plakVorigKlembordOnvolledig: 'Je vorige klembord kon niet volledig bewaard worden.',
  plakOpKlembord: 'De tekst staat op het klembord. Plak hem met Ctrl+V.',
  plakGetypt: 'De tekst staat waar je typte.',
  plakTypenOnderbroken:
    'Mondig stopte halverwege met typen, omdat je van venster wisselde, een toets ingedrukt hield of in een wachtwoordveld kwam. Het begin staat er al; de rest, die nog niet getypt was, staat op het klembord, zodat je hem zelf kunt plakken.',
  plakGeenTekstveld:
    'Mondig vond geen tekstveld om in te typen. De tekst staat op het klembord, zodat je hem zelf kunt plakken; je vorige kopie is daarmee vervangen.',
  plakTypenMislukt:
    'Plakken en typen lukten niet in dit venster. De tekst staat op het klembord, zodat je hem zelf kunt plakken.',
  plakBeheerrechten:
    'Mondig kan niet plakken of typen in dit venster, omdat het met beheerrechten draait. De tekst staat op het klembord: plak hem met Ctrl+V. Je vorige kopie is daarmee vervangen.',
  plakToetsen:
    'Mondig plakte niet, omdat er nog een toets ingedrukt was. Kopieer de tekst en plak hem zelf.',
  plakGeenVenster: 'Er was geen venster om in te plakken. Kopieer de tekst en plak hem zelf.',
  plakNietGelezen:
    'Mondig weet niet zeker of de tekst is geplakt. Staat hij er niet, kopieer hem dan en plak hem zelf.',
  plakMislukt: 'Plakken lukte niet. Kopieer de tekst en plak hem zelf.',
  plakNietGeplakt:
    'Mondig kon niet plakken of typen in dit venster, misschien omdat het met beheerrechten draait. De tekst staat op het klembord, zodat je hem zelf kunt plakken; je vorige kopie is daarmee vervangen.',
  // Plakken op de Mac (mijlpaal M1)
  plakGeenToegang:
    'Mondig mag nog niet plakken, want macOS vraagt eerst je toestemming. De tekst staat op het klembord: plak hem met ⌘V.',
  plakWachtwoordveld:
    'Mondig kon niet plakken, waarschijnlijk omdat beveiligde invoer aanstond (bijvoorbeeld in een wachtwoordveld). De tekst staat op het klembord: plak hem met ⌘V.',
  toegankelijkheidOpenen: 'Toegankelijkheid openen',
  toegangKop: 'Geef Mondig toestemming om te plakken',
  toegangTekst:
    'Om de tekst te plakken waar je typt, moet Mondig toetsen mogen sturen. Zet Mondig aan in Systeeminstellingen, bij Privacy en beveiliging, Toegankelijkheid. Tot die tijd zet Mondig de tekst op het klembord.',
  toegangOpnieuw:
    'Staat Mondig daar al aan en plakt hij toch niet? Dat gebeurt na een nieuwe versie van Mondig. Haal Mondig dan uit de lijst met de minknop en zet hem er opnieuw in met de plusknop.',
  modelAanvullenKop: 'Het spraakmodel is nog niet compleet',
  modelAanvullenTekst: (grootte: string): string =>
    `Nog ${grootte} te downloaden. Daarna is Mondig klaar om te dicteren.`,
  // Instellingen
  instellingen: 'Instellingen',
  microfoonKop: 'Microfoon',
  // Eén plek voor de naam van een standaardapparaat (microfoon en luidspreker): de naam eerst.
  apparaatStandaard: (naam: string | null): string =>
    naam === null ? 'Standaard van Windows' : `${naam} (standaard)`,
  microfoonHulp: 'Voor dicteren. Bij Opnemen kies je per vergadering.',
  microfoonMeterAria: 'Niveau van de microfoon',
  systeemMeterAria: 'Niveau van het geluid van de computer',
  meterWaarde: (aan: number, totaal: number): string => `${String(aan)} van ${String(totaal)}`,
  meterVoorrang: 'De meter pauzeert zolang er een dictaat of opname loopt.',
  meterGestopt: 'De meter is gestopt.',
  micTestKnop: 'Microfoon testen',
  micTestInstellingen: 'Testen',
  micTestStoppen: 'Klik om te stoppen',
  micTestKlaar: 'Klaar',
  micTestHint: 'Zeg iets. Beweegt de meter, dan hoort Mondig je.',
  micTestStilte: 'Mondig hoorde niets en heeft de microfoon weer uitgezet.',
  proefveldOpenen: 'Probeer het hier',
  meerOpties: 'Meer opties',
  minderOpties: 'Minder opties',
  groepDezeComputer: 'Deze computer',
  groepDicteren: 'Dicteren',
  groepOpslaan: 'Opslaan',
  groepBewaakteMap: 'Map in de gaten houden',
  groepOpstarten: 'Opstarten',
  microfoonWeg:
    'Je gekozen microfoon is niet aangesloten. Mondig gebruikt nu de standaard van Windows.',
  sneltoetsKop: 'Sneltoets',
  sneltoetsWijzigen: 'Wijzigen',
  sneltoetsHoudTik:
    'Ingedrukt houden: opnemen tot je loslaat. Kort tikken: opnemen tot je weer tikt.',
  sneltoetsVang: 'Druk de toetsen die je wilt gebruiken…',
  sneltoetsVangHulp:
    'Wat je indrukt verschijnt hier; loslaten legt hem vast. Esc stopt zonder te wijzigen.',
  sneltoetsVangAfgebroken: 'Het wijzigen is gestopt. De sneltoets is ongewijzigd.',
  sneltoetsHerstellen: (standaard: string): string => `Standaard herstellen (${standaard})`,
  sneltoetsHerstellenKort: 'Standaard herstellen',
  sneltoetsOngewijzigd: '(ongewijzigd)',
  sneltoetsIngesteld: (toets: string): string => `De sneltoets is nu ${toets}.`,
  sneltoetsBezetKop: 'Deze toets is al in gebruik',
  sneltoetsBezetTekst: (toets: string): string =>
    `${toets} wordt door een ander programma gebruikt. Kies een andere combinatie.`,
  sneltoetsFoutWindows:
    'Combinaties met de Windows-toets kunnen niet. Probeer het met Ctrl of Alt.',
  sneltoetsFoutLosseToets: (mac: boolean): string =>
    mac
      ? 'Deze toets heb je nodig om te typen of te navigeren. Kies een functietoets, of voeg Control, Option of Command toe.'
      : 'Deze toets heb je nodig om te typen of te navigeren. Kies Insert, Scroll Lock of een functietoets, of voeg Ctrl of Alt toe.',
  sneltoetsFoutShiftAlleen: (mac: boolean): string =>
    mac
      ? 'Shift alleen is niet genoeg. Voeg Control, Option of Command toe.'
      : 'Shift alleen is niet genoeg. Voeg Ctrl of Alt toe.',
  sneltoetsFoutWerktNiet: (toets: string, mac: boolean): string => {
    if (mac) return 'Pause werkt niet als sneltoets. Kies een andere toets.';
    return toets.includes('+')
      ? 'Pause werkt niet als sneltoets, ook niet met Ctrl of Alt. Kies een andere toets.'
      : 'Pause werkt niet als sneltoets: Windows meldt niet wanneer je loslaat. Kies een andere toets.';
  },
  sneltoetsFoutHoofdtoets: (voorbeeld: string): string =>
    `Druk ook een gewone toets in, bijvoorbeeld ${voorbeeld} of F9.`,
  sneltoetsFoutMeerdere: 'Gebruik maar één gewone toets.',
  sneltoetsFoutOnbekend: 'Deze toets kan niet als sneltoets. Kies een andere.',
  sneltoetsFoutGereserveerd: "Deze combinatie heb je nodig in andere programma's. Kies een andere.",
  sneltoetsFunctieKop: (toets: string): string => `${toets} gebruiken als sneltoets?`,
  sneltoetsFunctieTekst: (toets: string): string => {
    switch (toets) {
      case 'F1':
        return "F1 opent in de meeste programma's de hulp. Dat werkt dan niet meer.";
      case 'F2':
        return 'F2 hernoemt een bestand of cel. Dat werkt dan niet meer.';
      case 'F5':
        return 'F5 vernieuwt een pagina in je browser. Dat werkt dan niet meer.';
      case 'F11':
        return 'F11 zet je browser op volledig scherm. Dat werkt dan niet meer.';
      case 'F12':
        return 'F12 is "Opslaan als" in Word. Dat werkt dan niet meer.';
      default:
        return "Deze toets doet dan in andere programma's niets meer.";
    }
  },
  sneltoetsToch: 'Toch gebruiken',
  sneltoetsAndere: 'Andere toets kiezen',
  geheugenKop: 'Model in het geheugen houden',
  geheugenHulp: 'Langer is sneller bij het volgende dictaat, maar houdt geheugen bezet.',
  rusttijd5m: '5 minuten (standaard)',
  rusttijd15m: '15 minuten',
  rusttijd1u: '1 uur',
  rusttijdNooit: 'Altijd',
  videokaartKop: 'Videokaart gebruiken',
  videokaartHulp:
    'Sneller uitschrijven met een losse videokaart. Lukt het niet, dan gebruikt Mondig vanzelf de processor.',
  videokaartZoeken: 'Mondig zoekt een videokaart…',
  videokaartGevonden: (naam: string): string => `Gevonden: ${naam}.`,
  videokaartIngebouwd: (naam: string): string =>
    `Gevonden: ${naam} (ingebouwd). Een ingebouwde videochip is vaak niet sneller; test eerst de snelheid.`,
  videokaartGeen: 'Geen geschikte videokaart gevonden.',
  videokaartOnbekend: 'Mondig kon niet nagaan of er een videokaart is.',
  inGebruikVideokaart: 'In gebruik: videokaart',
  inGebruikProcessor: 'In gebruik: processor',
  inGebruikTerugval: 'De videokaart gaf een fout; Mondig gebruikt nu de processor',
  snelheidTesten: 'Snelheid testen',
  snelheidBezig: 'Mondig test de snelheid, dat duurt ongeveer een halve minuut…',
  snelheidLoont: (keer: string): string =>
    `De videokaart is ${keer} keer sneller. Aanzetten loont.`,
  snelheidLoontNiet: 'Op deze computer is de processor even snel of sneller; laat dit uit.',
  snelheidFaalde:
    'De videokaart gaf een fout tijdens de test. Laat dit uit; Mondig gebruikt de processor.',
  // Eerste start (fase 5, S-1): de wizard en de hardwarecheck
  computerGeheugen: (grootte: string): string => `${grootte} werkgeheugen`,
  computerKernen: (n: number): string => `${String(n)} processorkernen`,
  computerKaart: (naam: string): string => `videokaart ${naam}`,
  computerKaartIngebouwd: (naam: string): string => `ingebouwde videochip ${naam}`,
  computerGeenKaart: 'geen losse videokaart',
  adviesKaartGeen: 'Mondig rekent op de processor.',
  adviesKaartIngebouwd:
    'Alleen een ingebouwde videochip: die is vaak niet sneller dan de processor, dus Mondig rekent op de processor.',
  adviesKaartTesten: 'Een proef laat zien of je videokaart sneller is.',
  adviesKaartAanzetten: (keer: string): string =>
    `De videokaart is ${keer} keer sneller. Gebruik hem.`,
  adviesKaartLaten: 'De videokaart is hier niet sneller. Mondig rekent op de processor.',
  adviesKaartFaalde: 'De videokaart gaf een fout in de proef. Mondig rekent op de processor.',
  wizStap: (n: number, van: number): string => `Stap ${String(n)} van ${String(van)}`,
  wizNuNiet: 'Nu niet',
  wizVerder: 'Verder',
  wizWaaromKop: 'Mondig schrijft op je eigen computer',
  wizWaaromIntro:
    'Daarvoor heeft Mondig een spraakmodel nodig. Dat download je één keer; daarna werkt alles zonder internet.',
  wizPuntLokaalKop: 'Je geluid blijft bij jou',
  wizPuntLokaalTekst:
    'Het uitschrijven gebeurt op deze computer. Er gaat geen geluid het internet op.',
  wizPuntDownloadKop: 'Eén keer downloaden',
  wizPuntDownloadTekst: (grootte: string, duur: string): string =>
    `Een bestand van ongeveer ${grootte}. Bij een gewone verbinding duurt dat ${duur}.`,
  wizDuur: (snel: number, traag: number): string =>
    snel === traag
      ? `ongeveer ${String(snel)} ${snel === 1 ? 'minuut' : 'minuten'}`
      : `${String(snel)} tot ${String(traag)} minuten`,
  wizPuntRuimteKop: 'Schijfruimte',
  wizPuntRuimteTekst: (vrij: string): string => `Er is ${vrij} vrij op je schijf.`,
  wizPuntRuimteOnbekend: 'Mondig kon niet nagaan hoeveel ruimte er vrij is.',
  wizAdviesKop: 'Dit past bij deze computer',
  wizAdviesLeest: 'Mondig bekijkt je computer…',
  wizAdviesFout: 'Mondig kon je computer niet goed uitlezen. Het standaardmodel past bijna altijd.',
  wizAdviesGelezen: (zin: string): string => `We keken naar je computer: ${zin}.`,
  wizAdviesDicteren: 'Om te dicteren',
  wizAdviesBestanden: 'Voor opnames en bestanden',
  wizBestandenZelfde:
    'Hetzelfde model. Op een computer met weinig werkgeheugen gebruikt Mondig één model voor alles.',
  wizBestandenKrap:
    'Dit past, maar Mondig laadt dit model alleen tijdens het uitschrijven, omdat je werkgeheugen niet ruim is.',
  wizBestandenGoed:
    'Nauwkeuriger voor opnames en video\u2019s. Mondig laadt het alleen tijdens het uitschrijven.',
  wizTweedeModel: (grootte: string): string => `Ook het model voor opnames downloaden (${grootte})`,
  wizTweedeUitleg:
    'Overslaan kan: dan gebruikt Mondig het eerste model ook voor bestanden. Je kunt het later onder Modellen alsnog downloaden.',
  wizGeheugenWeinig: 'Deze computer heeft weinig werkgeheugen. Mondig werkt, maar kan trager zijn.',
  wizRuimteTekort: (tekort: string): string =>
    `Er is te weinig schijfruimte. Maak ongeveer ${tekort} vrij en controleer opnieuw.`,
  wizDownloadKnop: (grootte: string): string => `Downloaden (${grootte})`,
  wizDownloadKop: (aantal: number): string =>
    aantal > 1 ? 'De spraakmodellen worden gedownload' : 'Het spraakmodel wordt gedownload',
  wizDownloadVan: (binnen: string, totaal: string): string => `${binnen} van ${totaal}`,
  wizDownloadTip:
    'Je kunt dit venster open laten staan. Valt je verbinding weg, dan gaat Mondig daarna verder waar het bleef.',
  wizDownloadGestopt: 'De download is gestopt. Wat al binnen is, blijft bewaard.',
  wizDoorgaanStandaard: 'Doorgaan met het standaardmodel',
  wizDownloadMislukt: 'Het downloaden is niet gelukt',
  wizDownloadGestart: 'De download begint…',
  wizTestKop: 'Mondig test je videokaart',
  wizTestUitleg:
    'Een korte proef van ongeveer een minuut laat zien of je videokaart sneller is dan de processor. Dicteren kan intussen niet.',
  wizTestMislukt: 'De proef lukte niet. Mondig rekent op de processor.',
  wizKlaarKop: 'Je kunt dicteren',
  wizKlaarSneltoets: (toets: string): string =>
    `Houd ${toets} ingedrukt en praat. De tekst verschijnt waar je typt. Kort tikken kan ook.`,
  wizKlaarSneltoetsGeen:
    'De sneltoets kun je instellen onder Instellingen. Houd hem ingedrukt en praat.',
  wizKlaarTweedeOvergeslagen:
    'Voor bestanden gebruikt Mondig hetzelfde model. Een nauwkeuriger model downloaden kan altijd onder Modellen.',
  wizKlaarKaart: 'Videokaart gebruiken',
  wizKlaarKaartHulp:
    'De proef liet zien dat dit sneller is. Je kunt het later uitzetten in Instellingen.',
  wizAanDeSlag: 'Aan de slag',
  computerCheckKop: 'Mondig opnieuw afstellen',
  computerCheckHulp:
    'Na een nieuwe computer, videokaart of meer geheugen. Mondig kijkt wat nu het beste past en zet het met één klik goed.',
  computerCheckKnop: 'Afstellen',
  computerCheckKaartAan: 'De videokaart staat aan.',
  // Mondig opnieuw afstellen (N-C5): bezig, advies met vinkjes, overgenomen, alles goed
  afstelBezigKop: 'Mondig bekijkt je computer',
  afstelBezigHulp: 'Dat duurt ongeveer een minuut. Je kunt gewoon verder werken.',
  afstelStapComputer: 'Processor en geheugen',
  afstelStapKaart: 'Videokaart testen',
  afstelStapModellen: 'Modellen vergelijken',
  afstelStapKlaarAria: 'Klaar',
  afstelStapBezigAria: 'Bezig',
  afstelStoppen: 'Stoppen',
  afstelGestopt: 'Gestopt.',
  afstelGestoptTest:
    'Gestopt. De snelheidstest van de videokaart kan niet halverwege stoppen: Mondig rondt hem op de achtergrond af (hooguit een halve minuut) en gebruikt de uitkomst niet.',
  afstelAdviesKop: 'Dit past beter bij deze computer',
  afstelAdviesLijstAria: 'Wat Mondig voorstelt. Zet een vinkje weg om het over te slaan.',
  afstelOvernemen: 'Advies overnemen',
  afstelNietNu: 'Niet nu',
  afstelNietsAangevinkt: 'Vink minstens één punt aan om over te nemen.',
  afstelKaartTitel: 'Videokaart gebruiken',
  afstelKaartUitleg: (keer: string): string => `Uitschrijven gaat ongeveer ${keer} keer zo snel.`,
  afstelKaartGoedTitel: 'De videokaart staat al aan',
  afstelKaartGoedUitleg: 'Dat is op deze computer sneller.',
  afstelKaartUitTitel: 'De videokaart blijft uit',
  afstelKaartUitUitleg: 'Hij is hier niet sneller dan de processor.',
  afstelDicterenTitel: 'Het snelle model om te dicteren',
  afstelDicterenUitleg: 'Het snelste model: je tekst staat er na een halve seconde.',
  afstelDicterenGoedTitel: 'Dicteren blijft zoals het is',
  afstelBestandenTitel: 'Nauwkeurig model voor opnames en bestanden',
  afstelBestandenUitleg: 'Minder fouten in verslagen.',
  afstelBestandenGoedTitel: 'Opnames en bestanden blijven zoals ze zijn',
  afstelDicterenGoedUitleg: 'Het snelle model past al het beste.',
  afstelBestandenGoedUitleg: 'Het nauwkeurige model past al het beste.',
  afstelBestandenGoedUitlegEen: 'Het model voor dicteren past hier ook het beste voor bestanden.',
  afstelKaartNietGetestTitel: 'De videokaart is niet getest',
  afstelKaartNietGetestUitleg: (reden: string): string =>
    `Mondig kon de videokaart niet testen: ${reden}. Probeer het later opnieuw.`,
  afstelKaartFaaldeUitleg:
    'De videokaart gaf een fout tijdens de test. Mondig rekent op de processor. Probeer het later opnieuw.',
  afstelGeenRuimteBeide:
    'Er is niet genoeg ruimte voor beide modellen; maak ruimte vrij of kies er één.',
  afstelGeenRuimteEen: 'Er is niet genoeg ruimte voor dit model; maak ruimte vrij.',
  afstelTestRondtAf:
    'De vorige snelheidstest rondt Mondig nog af. Probeer het over een halve minuut opnieuw.',
  afstelRolSnel: 'Het snelle model',
  afstelRolNauwkeurig: 'Het nauwkeurige model',
  afstelRolModel: 'Het model',
  afstelDownloaden: (grootte: string): string => `Downloaden: ${grootte}.`,
  afstelStaatEr: 'Staat al op je computer.',
  afstelLoopt: 'Wordt al gedownload.',
  afstelOvergenomenKop: 'Overgenomen',
  afstelOvergenomenDownload: (welk: string): string =>
    `${welk} wordt gedownload; tot het klaar is, gebruikt Mondig het huidige model.`,
  afstelOvergenomenDicteren: 'Mondig dicteert nu met het snelle model.',
  afstelOvergenomenBestanden:
    'Opnames en bestanden schrijft Mondig nu uit met het nauwkeurige model.',
  afstelOvergenomenNiets: 'Er is niets veranderd.',
  afstelGoedKop: 'Alles staat al goed',
  afstelGoedHulp: (datum: string): string =>
    `Mondig keek naar je computer en vond niets dat beter kan. Gecontroleerd op ${datum}.`,
  afstelSluiten: 'Sluiten',
  groepProblemen: 'Problemen oplossen',
  logboekKop: 'Logboek',
  logboekHulp:
    'Mondig bewaart op deze computer een logboek met gebeurtenissen en fouten, zonder wat je dicteert of uitschrijft. Er gaat niets vanzelf weg: jij stuurt het zelf mee als je hulp vraagt.',
  logboekKopieren: 'Logboek kopiëren',
  logboekGekopieerd: 'Het logboek staat op je klembord.',
  logboekKopieerMislukt: 'Kopiëren lukte niet. Open de logmap en stuur het bestand mee.',
  logmapOpenen: 'Logmap openen',
  terugzetKop: 'Mondig terugzetten',
  terugzetHulp: 'Zet alle instellingen terug naar de standaard. Je modellen blijven staan.',
  terugzetKnop: 'Terugzetten',
  terugzetVraag: 'Alle instellingen terugzetten naar de standaard?',
  terugzetBevestig: 'Ja, terugzetten',
  terugzetKlaar: 'Alles staat weer op de standaard.',
  terugzetSneltoetsBezet:
    'De standaardsneltoets wordt al door een ander programma gebruikt; je oude sneltoets blijft werken.',
  klembordKop: 'Alleen op het klembord zetten',
  klembordHulp:
    'Niet zelf plakken. Handig als plakken in een programma niet lukt; je plakt dan met Ctrl+V.',
  stopwoordenKop: 'Stopwoorden weghalen',
  stopwoordenHulp:
    "Haalt klanken als 'ehm' en 'uh' weg. Geldt ook voor uitschriften van bestanden en opnames.",
  opgeslagen: 'Opgeslagen.',

  // Dictaat op andere schermen
  stripVerwerkt: 'Mondig schrijft uit…',
  stripKlaar: 'Klaar, de tekst staat op het startscherm',
  stripNaarStart: 'Naar het startscherm',
  annulerenMetEsc: 'Annuleren (Esc)',

  // Zijbalk en startscherm (ADR 0014)
  hoofdmenu: 'Hoofdmenu',
  menuBestanden: 'Bestanden',
  menuVergadering: 'Opnemen',
  lokaalRegel: 'Alles blijft op deze pc. Alleen het downloaden van een model gaat via internet.',
  dicterenRustUitleg:
    'Houd ingedrukt en praat, of tik kort om te starten en te stoppen. De tekst verschijnt waar je typt.',
  dicterenGeenToets: 'Zodra er een sneltoets is, dicteer je met die toets in elk programma.',
  pilStoppen: 'Stoppen en uitschrijven',
  pilLokaal: 'Lokaal',
  pilUitschrijven: 'Bezig met uitschrijven…',
  pilGeplakt: 'Geplakt',
  pilDuur: (seconden: number): string =>
    `${new Intl.NumberFormat('nl', { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(seconden)} s`,
  pilWaarschijnlijk: 'Geplakt, controleer even of de tekst er staat',
  pilOpKlembord: 'Op het klembord, plak met Ctrl+V',
  pilGeenTekst: 'Geen tekst herkend',
  pilGeenGeluid: 'Geen geluid gehoord',
  pilVerdacht: 'Bijna geen spraak gehoord, controleer de tekst',
  pilRest: (seconden: number): string => `Nog ${String(seconden)} s, dan stopt het dicteren`,
  pilPlakMislukt: 'Plakken lukte niet in dit venster',
  pilPlakBeheer: 'Dit venster draait met beheerrechten, de tekst staat op het klembord',
  pilTypenOnderbroken: 'Gestopt met typen, de rest staat op het klembord',
  pilGeenTekstveld: 'Geen tekstveld gevonden, de tekst staat op het klembord',
  pilTypenMislukt: 'Plakken en typen lukten niet, de tekst staat op het klembord',
  pilPlakOnzeker: 'Niet zeker of de tekst is geplakt',
  pilPlakGeenToegang: 'Mondig mag nog niet plakken, de tekst staat op het klembord',
  pilGekopieerd: 'Gekopieerd, plak met Ctrl+V',
  plakInMondig: 'De tekst staat hieronder, want Mondig stond vooraan toen hij klaar was.',
  pilInMondig: 'De tekst staat in Mondig',
  pilSluiten: 'Sluiten',
  instellingOpenen: 'Instelling openen',
  mondigOpenen: 'Mondig openen',
  microfooninstellingOpenen: 'Microfooninstelling openen',

  // Instellingen: melding, geluid en starten (sprint 3d)
  positieKop: 'Plek van de dicteerbalk',
  positieHulp: 'Waar de dicteerbalk verschijnt terwijl je praat.',
  positieOnder: 'Onderaan het scherm (standaard)',
  positieBoven: 'Bovenaan het scherm',
  geluidKop: 'Geluid bij starten en stoppen',
  geluidHulp: 'Een kort, zacht geluidje als Mondig begint en stopt met luisteren.',
  geluidVolumeKop: 'Volume van dat geluid',
  autostartKop: 'Starten bij aanmelden',
  autostartHulp: 'Mondig start mee als je je aanmeldt bij Windows, klaar om te dicteren.',
  verborgenKop: 'Geminimaliseerd starten',
  verborgenHulp: 'In het systeemvak, zonder venster.',
  // Over Mondig (ADR 0016)
  overMondig: 'Over Mondig',
  overUitleg: 'Door Aigenwijs. Wat is er nieuw, licenties en privacy.',
  openen: 'Openen',
  sluiten: 'Sluiten',
  doorAigenwijs: 'door Aigenwijs',
  voetAria: (versie: string): string => `Over Mondig, versie ${versie}`,
  versieRegelTekst: (versie: string): string => `Versie ${versie}`,
  bouwRegelTekst: (bouw: string, datum: string): string => `Bouw ${bouw}, ${datum}`,
  versieKopieren: 'Versiegegevens kopiëren',
  versieGekopieerd: 'Versiegegevens gekopieerd',
  versieKopierenUitleg: 'Zonder persoonsgegevens: versie, bouw, Windows, processor en model.',
  versieLaden: 'Versiegegevens ophalen…',
  versieFout: 'Mondig kon de versiegegevens niet ophalen. Start Mondig opnieuw.',
  nieuwKop: 'Wat is er nieuw',
  nieuwNogNiet: 'Testversie: dit is alles wat sinds de vorige uitgave is gebouwd.',
  nieuwVersie: (versie: string): string => `Wijzigingen in versie ${versie}.`,
  nieuwGeen: 'Voor deze versie staan geen wijzigingen in de changelog.',
  alleWijzigingenTonen: 'Alle wijzigingen tonen',
  minderTonen: 'Minder tonen',
  eerdereUitgaven: 'Eerdere uitgaven',
  nieuwRegelKop: (versie: string): string => `Nieuw in ${versie}:`,
  nieuwRegelLezen: 'Lees meer',
  modellenKop: 'Spraakmodellen',
  modellenUitleg:
    'Mondig bevat de spraakmodellen niet zelf. De app laat je ze downloaden, in een omgezette vorm van handy-computer. Hier staat van wie ze oorspronkelijk zijn, onder welke licentie, en wat er is gewijzigd.',
  modellenFout: 'Mondig kon de gegevens van de spraakmodellen niet ophalen.',
  hulpmodel: 'hulpmodel',
  vermMaker: 'Maker',
  vermCopyright: 'Copyright',
  vermGeenCopyright: 'De bron geeft geen copyrightregel',
  vermOorspronkelijk: 'Oorspronkelijk model',
  vermLicentie: 'Licentie',
  vermDisclaimer: 'Garantiedisclaimer',
  vermDisclaimerTekst: 'Wat de licentie zegt over garantie en aansprakelijkheid',
  vermGewijzigd: 'Gewijzigd',
  vermOmgezet: 'Omgezette versie',
  adresOpenenMislukt: 'Het adres openen lukte niet. Kopieer het en open het zelf in je browser.',
  onderdelenKop: 'Onderdelen van anderen',
  onderdelenUitleg:
    'Mondig is gebouwd met open-sourceonderdelen, waaronder de lettertypen Sora en Geist (zoek op "font"). Per onderdeel staat de licentie; klap het open voor de volledige tekst.',
  onderdelenZoeken: 'Zoek op naam of licentie',
  onderdelenAantal: (getoond: number, totaal: number): string =>
    getoond === totaal
      ? `${String(totaal)} onderdelen`
      : `${String(getoond)} van ${String(totaal)} onderdelen`,
  onderdelenGeen: 'Geen onderdeel gevonden.',
  onderdelenMeer: (n: number): string => `Toon nog ${String(n)}`,
  onderdelenNietAanwezig:
    'Deze bouw bevat de lijst met onderdelen niet. Ze wordt gemaakt als de installer wordt gebouwd (npm run bundel).',
  onderdelenLaden: 'Onderdelen ophalen…',
  licentieTekstKop: 'Licentietekst',
  noticeKop: 'NOTICE van het onderdeel',
  licentieTekstOntbreekt: (licentie: string): string =>
    `De licentietekst zit niet in deze lijst. Licentie: ${licentie}.`,
  privacyKop: 'Privacy',
  privacyTekst: 'Mondig verzamelt niets en stuurt niets door.',
  copyrightRegel: '© 2026 Aigenwijs',
  // Bestanden (sprint 4-B)
  bestandenSleepKop: 'Sleep een opname of video hierheen',
  bestandenSleepUitleg: 'Een gesprek, een Teams-opname, een spraakbericht. Ook meerdere tegelijk.',
  bestandenLoslaten: 'Laat los, dan begint Mondig',
  bestandenLoslatenUitleg: 'Ze worden op deze computer uitgeschreven, na elkaar.',
  bestandenNietGepakt:
    'Mondig kon dit bestand niet pakken. Sleep het eerst naar het bureaublad en dan hierheen.',
  bestandenKiezen: 'Bestanden kiezen',
  bestandenMapOpenen: 'Map openen',
  bestandenUitschriftenOpenen: 'Uitschriften openen',
  bestandenKomtIn: (map: string): string => `Het uitschrift komt in ${map}.`,
  bestandenOpslaanIn: (map: string): string => `Opslaan gaat naar ${map}.`,
  bestandenMeerToevoegen: 'Meer toevoegen',
  bestandenLijstLabel: 'Bestanden in de rij',
  bestandenDicterenGaatVoor:
    'Dicteren blijft gewoon werken terwijl dit loopt; een dictaat gaat voor.',
  bestandWacht: 'wacht',
  bestandBezig: 'bezig',
  bestandProcent: (p: number): string => `${String(p)}%`,
  bestandProcentRest: (p: number, rest: string): string =>
    `${String(p)}%, ${rest.charAt(0).toLowerCase()}${rest.slice(1)}`,
  bestandKlaar: 'klaar',
  bestandKlaarIn: (duur: string): string => `uitgeschreven in ${duur}`,
  bestandGeannuleerd: 'geannuleerd',
  duurSeconden: (s: number): string => `${String(s)} s`,
  duurMinutenSeconden: (m: number, s: number): string => `${String(m)} min ${String(s)} s`,
  bestandOpenen: 'Openen',
  bestandOpenenAria: (naam: string): string => `${naam} openen`,
  bestandAnnulerenKort: 'Annuleren',
  bestandAnnuleren: (naam: string): string => `${naam} annuleren`,
  bestandUitRijKort: 'Uit de rij halen',
  bestandUitRij: (naam: string): string => `${naam} uit de rij halen`,
  bestandWeghalenKort: 'Weghalen',
  bestandWeghalen: (naam: string): string => `${naam} weghalen`,
  bestandProcentSchermlezer: (naam: string, p: number): string => `${naam}: ${String(p)} procent`,
  bestandKlaarSchermlezer: (naam: string): string => `${naam} is uitgeschreven.`,
  bestandFoutSchermlezer: (naam: string, melding: string): string => `${naam}: ${melding}`,
  terugNaarBestanden: 'Terug naar Bestanden',
  // Bewaren en exporteren (sprint 4-C)
  formaatDocx: 'Word',
  formaatPdf: 'PDF voor archief',
  formaatOdt: 'LibreOffice',
  formaatTxt: 'Platte tekst',
  formaatMd: 'Markdown',
  formaatHtml: 'Webpagina',
  formaatSrt: 'Ondertitels',
  formaatVtt: 'Ondertitels voor het web',
  bouwsteenDatum: 'Datum',
  bouwsteenNaam: 'Naam van de opname',
  bouwsteenLabel: 'Label',
  bouwsteenTijd: 'Tijd',
  bouwsteenModel: 'Model',
  mapDocumenten: 'Documenten',
  mapGeen: 'Nog geen map gekozen',
  opslaanInKop: 'Map',
  opslaanInHulp: 'Voor uitschriften en verslagen.',
  mapKiezenAria: (map: string): string => `Opslaan in ${map}. Andere map kiezen`,
  bestandsnaamKop: 'Bestandsnaam',
  bestandsnaamHulp:
    'Zet onderdelen aan of uit. Sleep ze, of gebruik Alt + pijl links of rechts, voor de volgorde.',
  bestandsnaamPlek: (naam: string, plek: number, totaal: number): string =>
    `${naam} staat nu op plek ${String(plek)} van ${String(totaal)}`,
  bestandsnaamVoorbeeld: (naam: string): string => `Voorbeeld: ${naam}`,
  standaardformaatKop: 'Standaardformaat',
  standaardformaatHulp: 'Wat "Opslaan" maakt. Andere formaten via het pijltje.',
  tijdenKop: 'Tijden in het document',
  tijdenHulp: 'Zet 00:07 voor elke zin. Ondertitels hebben altijd tijden.',
  automatischKop: 'Automatisch opslaan',
  automatischHulp: 'Elk uitschrift meteen in de map, zonder te vragen.',

  // Bewaakte map (B-1)
  bewaakAanKop: 'Nieuwe opnames vanzelf uitschrijven',
  bewaakAanHulp:
    'Mondig kijkt in deze map en schrijft nieuwe opnames uit, zolang Mondig draait. De opname zelf blijft zoals hij was. Wat er nu al in de map staat, blijft liggen.',
  bewaakMapKop: 'Map',
  bewaakMapHulp: 'Bijvoorbeeld de map waar Teams of OneDrive je opnames neerzet.',
  bewaakMapGeen: 'Kies een map',
  bewaakMapKiezenAria: (map: string): string => `Map in de gaten houden: ${map}. Andere map kiezen`,
  bewaakSubmappenKop: 'Ook submappen',
  bewaakSubmappenHulp: 'Mondig kijkt dan ook in de mappen binnen deze map.',
  bewaakOverslaanKop: 'Overslaan als er al een transcript is',
  bewaakOverslaanHulp:
    'Mondig zoekt in je transcriptmap en naast de opname naar een transcript met dezelfde naam.',
  bewaakNaastKop: 'Transcript naast de opname zetten',
  bewaakNaastHulp:
    'In plaats van in je transcriptmap komt het transcript in dezelfde map als de opname.',
  bewaakProbleemMapOntbreekt:
    'Deze map is niet te vinden. Mondig kijkt weer mee zodra hij terug is.',
  bewaakProbleemZelfdeAlsOpslag:
    'Dit is de map waarin Mondig je transcripten bewaart. Kies een andere map om in de gaten te houden.',
  bewaakKiezenEerst: 'Kies eerst een map om in de gaten te houden.',
  bestandUitMap: 'uit de bewaakte map',
  bestandOvergeslagen: 'overgeslagen, er is al een transcript',
  bewaakProbleemTeVeel:
    'In deze map staan meer bestanden dan Mondig kan volgen. Kies een kleinere map, of zet de submappen uit.',
  wachtrijRest: (tijd: string, aantal: number): string =>
    `${tijd} voor ${String(aantal)} ${aantal === 1 ? 'bestand' : 'bestanden'}`,
  opslaanAls: (formaat: string): string => `Opslaan als ${formaat}`,
  opslaanKort: 'Opslaan',
  opslaanTitel: (formaat: string, extensie: string): string =>
    `Opslaan als ${formaat} (${extensie}); andere vormen onder het pijltje`,
  opslaanMenu: 'Ander formaat of andere plek',
  opslaanAnderePlek: 'Opslaan op een andere plek…',
  opslaanBezig: 'Bezig met opslaan…',
  labelKop: 'Label',
  labelHulp: 'Komt in de bestandsnaam en in de gegevens van het document.',
  labelVoorbeeld: 'Bijvoorbeeld een zaaknummer',
  opgeslagenKop: 'Opgeslagen',
  opgeslagenAlsIn: (naam: string, map: string): string => `als ${naam} in ${map}`,
  opgeslagenOpenen: 'Openen',
  opgeslagenMapTonen: 'Map tonen',
  bestaatAlKop: 'Er staat al een bestand met deze naam',
  opslaanAlsNummer: (nummer: string): string => `Opslaan als ${nummer}`,
  vervangen: 'Vervangen',
  verwijderenKnop: 'Verwijderen…',
  verwijderKop: 'Transcript verwijderen?',
  verwijderMetBestanden: (namen: string): string =>
    `Mondig haalt het transcript weg en verwijdert de bestanden die het opsloeg: ${namen}. Dit kun je niet ongedaan maken.`,
  verwijderZonderBestanden:
    'Mondig haalt het transcript weg. Er zijn geen bestanden van opgeslagen.',
  verwijderBevestig: 'Verwijderen',
  bestandInMap: (naam: string, map: string): string => `${naam} in ${map}`,
  verwijderInGebruik: (namen: string): string =>
    `Niet verwijderd, want open in een ander programma: ${namen}. Sluit het en probeer het opnieuw.`,
  verwijderMislukt: (namen: string): string => `Niet verwijderd: ${namen}. Probeer het opnieuw.`,
  verwijderdMelding: (aantal: number): string =>
    aantal === 1
      ? 'Transcript en 1 bestand verwijderd.'
      : `Transcript en ${String(aantal)} bestanden verwijderd.`,
  verwijderdZonderBestanden: 'Transcript weggehaald.',
  verwijderGebleven: (namen: string): string =>
    `Bleef staan, want gewijzigd of verplaatst na het opslaan: ${namen}.`,
  bestandOpgeslagenAls: (naam: string): string => `opgeslagen als ${naam}`,
  bestandOpgeslagenVerouderd: (naam: string): string =>
    `opgeslagen als ${naam}, zonder je laatste wijzigingen`,
  bestandBewaarFout: (melding: string): string => `Automatisch opslaan lukte niet: ${melding}`,
  transcriptRegelsLabel: 'Het transcript',
  transcriptGeenTekst: 'Er is geen tekst herkend.',
  transcriptNietGevonden: 'Dit transcript is er niet meer.',
  // Vergadering (sprint 4b-C)
  sprekerJij: 'Jij',
  sprekerAnderen: 'Anderen',
  vergaderSoortLabel: 'Soort opname',
  vergaderOnline: 'Online',
  vergaderOnlineUitleg:
    'Teams, Zoom of Meet. Jouw stem en die van de anderen worden apart opgenomen.',
  vergaderRuimte: 'In de ruimte',
  vergaderRuimteUitleg: 'Iedereen in één ruimte, met de microfoon van deze computer.',
  vergaderGeluidVan: 'Geluid van',
  vergaderApparatenFout:
    'Mondig kon de luidsprekers niet ophalen. De standaard luidspreker wordt gebruikt.',
  vergaderTipOnline:
    'Draag een koptelefoon. Dan komt wat de anderen zeggen niet dubbel in het verslag.',
  vergaderTipRuimte: 'Leg de laptop midden op tafel.',
  vergaderStarten: 'Opname starten',
  vergaderModelRegel: 'Wordt uitgeschreven op deze computer.',
  vergaderHerinneringKop: 'Vertel het de anderen',
  vergaderHerinneringTekst:
    'Laat iedereen weten dat je deze vergadering opneemt en uitschrijft. Dat hoort zo, en vaak is het ook verplicht.',
  vergaderHerinneringStart: 'Ik heb het gemeld, start',
  vergaderStartBezig: 'Opname wordt gestart…',
  vergaderLoopt: 'Opname loopt',
  vergaderKlokAria: 'Opgenomen tijd',
  vergaderBronMicrofoon: 'Microfoon',
  vergaderBronComputer: 'Geluid van de computer',
  vergaderMetersLabel: 'Geluid dat binnenkomt',
  vergaderSysteemStil:
    'Er komt al een minuut geen geluid van de computer binnen. Speelt de vergadering via een ander apparaat?',
  vergaderStopUitschrijven: 'Stoppen en uitschrijven',
  vergaderWeggooienKop: 'Opname weggooien?',
  vergaderWeggooienTekst:
    'Mondig wist de opname met alles wat er al is opgenomen. Dit kun je niet ongedaan maken.',
  vergaderWeggooienBevestig: 'Ja, weggooien',
  vergaderAfronden: 'Mondig rondt de opname af…',
  vergaderHerstellen: 'Mondig herstelt de opname…',
  vergaderUitschrijvenKop: 'Uitschrijven…',
  vergaderUitschrijvenStand: (stand: string): string => `Uitschrijven… ${stand}`,
  vergaderUitschrijvenHulp:
    'Je kunt Mondig gewoon gebruiken. Het verslag staat hier klaar zodra het is uitgeschreven.',
  vergaderLater: 'Later uitschrijven',
  vergaderLaterHulp: 'De opname blijft dan bewaard tot je hem uitschrijft of weggooit.',
  vergaderingVan: (dag: string, tijd: string): string => `Vergadering van ${dag}, ${tijd}`,
  vergaderMinuten: (n: number): string => (n === 1 ? '1 minuut' : `${String(n)} minuten`),
  vergaderInDeRuimte: 'Fysieke vergadering',
  vergaderEchoMelding:
    'Een deel van wat de anderen zeiden kwam ook via je microfoon binnen. Die dubbele tekst is weggelaten. Met een koptelefoon wordt het verslag het best.',
  vergaderMeldingSluiten: 'Melding sluiten',
  vergaderBeurtenLabel: 'Het verslag',
  vergaderNaamAria: (naam: string): string => `${naam}, naam wijzigen`,
  vergaderNaamGeven: (kant: string): string => `Naam geven aan ${kant}`,
  vergaderNaamVoorbeeld: 'Bijvoorbeeld Wethouder De Vries',
  vergaderSpreker: (nummer: number): string => `Spreker ${String(nummer)}`,
  vergaderNaamOpslaan: 'Opslaan',
  vergaderNaamGegeven: (naam: string): string => `Naam gegeven: ${naam}.`,
  vergaderNaamTerug: (kant: string): string => `Terug naar ${kant}.`,
  // Sprekers herkennen (4b-D, fase D-2)
  vergaderNaamSprekerKop: 'Naam van deze spreker',
  vergaderNaamSprekerHulp: (standaard: string): string =>
    `Geldt voor alle beurten van deze spreker. Heeft een andere spreker dezelfde naam, dan worden ze samengevoegd. Laat het veld leeg om terug te gaan naar ${standaard}.`,
  vergaderBeurtVanKop: 'Deze beurt is van',
  vergaderBeurtVanAria: (naam: string): string => `Geef deze beurt aan ${naam}`,
  vergaderNieuweSpreker: 'Nieuwe spreker',
  vergaderPlusSpreker: '+ Spreker',
  vergaderSprekerAanwijzen: 'Spreker aanwijzen',
  vergaderSprekerAanwijzenAria: (tijd: string): string =>
    `Spreker aanwijzen bij de alinea van ${tijd}`,
  vergaderSprekerAanwijzenHulp:
    'Kies van wie deze alinea is, of maak een nieuwe spreker. Een naam geven kan daarna bij de spreker.',
  vergaderSamenvoegenKop: 'Is dit dezelfde persoon?',
  vergaderSamenvoegenHulp: (naam: string): string =>
    `Voeg ${naam} samen met een andere spreker. Alle beurten van ${naam} worden dan van die spreker.`,
  vergaderSamenvoegenMet: (naam: string): string => `Samenvoegen met ${naam}`,
  vergaderBeurtGegeven: (naam: string): string => `De beurt is nu van ${naam}.`,
  vergaderNieuweSprekerGegeven: 'De beurt is nu van een nieuwe spreker.',
  vergaderSamengevoegd: (van: string, naar: string): string =>
    `${van} en ${naar} zijn samengevoegd.`,
  vergaderZijnNu: (namen: string[], naam: string): string => {
    const laatste = namen.at(-1) ?? '';
    const lijst = namen.length > 1 ? `${namen.slice(0, -1).join(', ')} en ${laatste}` : laatste;
    return `${lijst} zijn nu ${naam}.`;
  },
  vergaderOngedaanMaken: 'Ongedaan maken',
  vergaderOngedaanGemaakt: 'Ongedaan gemaakt.',
  vergaderSprekersHerkennen: 'Sprekers herkennen…',
  vergaderSprekersSchermlezer: 'Mondig herkent nu de sprekers.',
  sprekersKop: 'Sprekers herkennen',
  sprekersHulp: (grootte: string): string =>
    `Laat zien wie er spreekt. Heeft een apart model nodig (${grootte}). Herkent tot 4 sprekers.`,
  sprekersHulpAanwezig: 'Laat zien wie er spreekt. Herkent tot 4 sprekers.',
  sprekersCompactHulp: 'Wie zei wat, tot 4 sprekers.',
  sprekersCompactDownload: (grootte: string): string =>
    `Wie zei wat, tot 4 sprekers. Eerst ${grootte} downloaden.`,
  sprekersVraagKop: 'Sprekermodel downloaden?',
  sprekersVraagTekst: (grootte: string): string =>
    `Om sprekers te herkennen heeft Mondig een apart model nodig (${grootte}). Mondig haalt het eenmalig op via internet; daarna werkt het zonder internet. Je kunt het later weer verwijderen op de pagina Modellen.`,
  sprekersDownloaden: (grootte: string): string => `Downloaden (${grootte})`,
  sprekersBezig: 'Sprekermodel wordt opgehaald…',
  sprekersBezigStand: (stand: string): string => `Sprekermodel wordt opgehaald… ${stand}`,
  sprekersOpslaanFout: 'De instelling kon niet worden bewaard.',
  vergaderAudioBezig: 'Bezig met bewaren…',
  vergaderAudioBewaard: (naam: string): string => `Opname bewaard als ${naam}.`,
  vergaderAudioBewarenMenu: 'Opname bewaren als geluidsbestand…',
  verslagSluiten: 'Sluiten',
  verslagSluitenTitel: 'Terug naar Opnemen. Het verslag blijft bewaard.',
  verslagNieuweOpname: 'Nieuwe opname',
  verslagMenu: 'Meer',
  verslagSprekers: (aantal: number): string =>
    aantal === 1 ? '1 spreker' : `${String(aantal)} sprekers`,
  verslagModelSnel: 'snel model',
  verslagModelNauwkeurig: 'nauwkeurig model',
  verslagModelExtraNauwkeurig: 'extra nauwkeurig model',
  opDezeComputer: 'op deze computer',
  klaarKop: 'Je verslag staat klaar',
  klaarGesprek: (minuten: number): string =>
    `${String(minuten)} ${minuten === 1 ? 'minuut' : 'minuten'} gesprek`,
  klaarUitgeschreven: (minuten: number): string =>
    `in ${String(minuten)} ${minuten === 1 ? 'minuut' : 'minuten'} uitgeschreven`,
  klaarUitgeschrevenKort: 'in minder dan een minuut uitgeschreven',
  klaarLokaal: 'Alles bleef op deze computer.',
  vergaderWachtendLabel: 'Opnames die nog wachten',
  vergaderWanneer: (dag: string, tijd: string): string => `${dag}, ${tijd}`,
  vergaderDagVandaag: 'vandaag',
  vergaderDagGisteren: 'gisteren',
  vergaderAfgebroken: (wanneer: string, duur: string | null): string =>
    `Er is een opname van ${wanneer}${duur === null ? '' : ` (${duur})`} die niet goed is afgesloten. Mondig kan hem herstellen tot vlak voor het moment dat het misging.`,
  // Dictaatgeschiedenis (ADR 0021): de lijst op Dicteren en de instelling
  dictatenKop: 'Je dictaten',
  dictatenBewaard: (termijn: string): string => `${termijn} bewaard`,
  dictatenTermijnNiet: 'Niet bewaren',
  dictatenTermijn24u: '24 uur',
  dictatenTermijn7d: '7 dagen',
  dictatenMenu: 'Meer opties voor je dictaten',
  dictatenHoeLang: 'Hoe lang bewaren…',
  dictatenAllesWissen: 'Alles wissen…',
  dictatenAllesWissenKop: 'Alle dictaten wissen?',
  dictatenWisVraag: (aantal: number): string =>
    aantal === 1
      ? 'Je bewaarde dictaat wordt gewist.'
      : `Je ${String(aantal)} bewaarde dictaten worden gewist.`,
  dictatenWisBevestig: 'Alles wissen',
  dictatenNietBewarenKop: 'Niet meer bewaren?',
  dictatenNietBewarenBevestig: 'Wissen en niet meer bewaren',
  dictatenDagVandaag: 'Vandaag',
  dictatenDagGisteren: 'Gisteren',
  dictatenKopieerAria: (tijd: string): string => `Dictaat van ${tijd} kopiëren`,
  dictatenVerwijderAria: (tijd: string): string => `Dictaat van ${tijd} verwijderen`,
  dictatenGekopieerd: 'Gekopieerd',
  dictatenVerwijderd: 'Dictaat verwijderd',
  dictatenAlleGewist: 'Alle dictaten zijn gewist',
  dictatenLeegKop: 'Hier komt wat je dicteert',
  dictatenLeegTekst: (termijn: string): string =>
    `Dicteer in Word, Outlook of je browser. Wat je zegt, blijft hier ${termijn} staan, zodat je het terugvindt als het plakken niet lukte.`,
  dictatenNietBewaard: 'Mondig bewaart je dictaten niet; dit verdwijnt als je Mondig sluit.',
  dictatenWijzigen: 'Wijzigen',
  dictatenBewarenKop: 'Dictaten bewaren',
  dictatenBewarenHulp: 'Zodat je ze op Dicteren terugvindt. Alleen op deze computer.',

  // Dictaten opschonen (ADR 0027): Instellingen, de tweede sneltoets, de pil en de geschiedenis
  opschonenKop: 'Dictaten opschonen',
  opschonenKeuzeUit: 'Uit',
  opschonenKeuzeSneltoets: 'Met een eigen sneltoets',
  opschonenKeuzeElk: 'Bij elk dictaat',
  opschonenUitleg: 'Een taalmodel maakt van je dictaat lopende zinnen met leestekens.',
  opschonenUitlegNaar: (naam: string, label: string): string =>
    `Een taalmodel maakt van je dictaat lopende zinnen met leestekens. Je tekst gaat naar ${naam} (${label}). Namen en andere persoonsgegevens worden niet vervangen. De letterlijke tekst blijft op deze computer.`,
  opschonenUitlegLokaal:
    'Een taalmodel maakt van je dictaat lopende zinnen met leestekens. Je tekst blijft op deze computer (Lokaal). Werkt alleen vlot met een goede videokaart.',
  opschonenGeenAanbieder: 'Kies eerst een taalmodel onder Notulen.',
  opschonenNaarNotulen: 'Naar Notulen',
  opschonenVraagTekst: (naam: string, label: string): string =>
    `Je dictaten gaan naar ${naam} (${label}) om op te schonen. Namen en andere persoonsgegevens worden niet vervangen. De letterlijke tekst blijft op deze computer.`,
  opschonenAanzetten: 'Aanzetten',
  opschonenLabel: (label: string): string =>
    ({ lokaal: 'Lokaal', europees: 'Europees', buiten_europa: 'Buiten Europa' })[label] ?? label,
  opschoonToetsKop: 'Sneltoets voor opschonen',
  opschoonToetsHulp:
    'Met deze toets wordt je dictaat opgeschoond. De gewone sneltoets geeft de letterlijke tekst.',
  opschoonToetsIngesteld: (toets: string): string => `De sneltoets voor opschonen is nu ${toets}.`,
  opschoonToetsDubbel: 'Dit is al je sneltoets voor dicteren. Kies een andere toets.',
  opschoonToetsBezet: (toets: string): string =>
    `${toets} wordt al door een ander programma gebruikt. Kies een andere toets.`,
  opschoonToetsOpDicteren: 'opgeschoond',
  opschoonToetsOpDicterenAria: (toets: string): string => `${toets}: opgeschoond`,
  sneltoetsDubbelMetOpschonen: 'Dit is al je sneltoets voor opschonen. Kies een andere toets.',
  pilOpschonen: 'Opschonen',
  pilOpschonenStatus: (label: string): string => `Opschonen (${label})`,
  pilOpschonenStoppen: 'Stoppen met opschonen, letterlijke tekst plakken',
  pilNietOpgeschoond: (soort: string): string =>
    ({
      geplakt: 'Niet opgeschoond, letterlijk geplakt',
      waarschijnlijk: 'Niet opgeschoond, letterlijk geplakt. Controleer even of de tekst er staat',
      in_mondig: 'Niet opgeschoond, de letterlijke tekst staat in Mondig'
    })[soort] ?? 'Niet opgeschoond, letterlijk op het klembord. Plak met Ctrl+V',
  dictatenOpgeschoondDoor: (model: string, label: string): string =>
    `Opgeschoond door ${model} (${label})`,
  dictatenOpgeschoondDoorZonderLabel: (model: string): string => `Opgeschoond door ${model}`,
  dictatenLetterlijkKopieren: 'Letterlijke tekst kopiëren',
  dictatenLetterlijkKopierenAria: (tijd: string): string =>
    `Letterlijke tekst van het dictaat van ${tijd} kopiëren`,
  dictatenLetterlijkGekopieerd: 'Letterlijke tekst gekopieerd',
  bewaarKop: 'Opnames en verslagen wissen na',
  bewaarHulp: 'Wat je opslaat als bestand, blijft in je map.',
  bewaar1: '1 dag',
  bewaar7: '7 dagen',
  bewaar30: '30 dagen',
  vergaderOnzeker:
    'Mondig kan niet nagaan of deze opname nog loopt. Staat Mondig ergens anders open, sluit dat dan eerst.',
  vergaderHerstelUitschrijven: 'Herstellen',
  vergaderUitschrijvenKnop: 'Uitschrijven',
  vergaderLijstWeggooienVraag: 'Deze opname weggooien? Dit kun je niet ongedaan maken.',
  vergaderLijstKnopAria: (knop: string, wanneer: string): string =>
    `${knop}, opname van ${wanneer}`,
  vergaderGestartSchermlezer: 'De opname is gestart.',
  vergaderUitschrijvenSchermlezer: 'De opname is gestopt. Mondig schrijft nu uit.',
  vergaderProcentSchermlezer: (p: number): string => `Uitschrijven: ${String(p)} procent`,
  vergaderKlaarSchermlezer: 'Het verslag is klaar.',
  vergaderFoutSchermlezer: (melding: string): string => `Fout: ${melding}`,
  vergaderWeggegooidSchermlezer: 'De opname is weggegooid.',
  vergaderLaterSchermlezer: 'Het uitschrijven is gestopt. De opname blijft bewaard.',
  vergaderGeslotenSchermlezer:
    'Het verslag is gesloten. Je vindt het terug onder Eerder opgenomen.',
  // Opnemen (fase UX-D): de soort Alleen ik, het startscherm en de lijst die nog wacht
  vergaderAlleen: 'Alleen ik',
  vergaderAlleenUitleg: 'Je gedachten inspreken. Mondig schrijft het na afloop uit.',
  notitieVan: (dag: string, tijd: string): string => `Notitie van ${dag}, ${tijd}`,
  vergaderTestgeluid: 'Testgeluid afspelen',
  vergaderTestgeluidBezig: 'Testgeluid speelt…',
  vergaderTestgeluidFout: 'Het testgeluid speelde niet af.',
  vergaderNogUitTeSchrijven: 'Nog uit te schrijven',
  vergaderWachtendWistRegel: (dagen: number): string =>
    `Mondig wist ze na ${String(dagen)} ${dagen === 1 ? 'dag' : 'dagen'}.`,
  // Vanaf een uur: "1 uur 4 min" (ontwerp N-0), anders "52 min".
  vergaderMinutenKort: (n: number): string => {
    if (n < 60) return `${String(n)} min`;
    const rest = n % 60;
    return rest === 0
      ? `${String(n / 60)} uur`
      : `${String(Math.floor(n / 60))} uur ${String(rest)} min`;
  },
  vergaderOnderbroken: 'onderbroken',
  vergaderWistKortVandaag: 'wordt vandaag gewist',
  vergaderWistKortMorgen: 'wordt morgen gewist',
  vergaderWistOp: (moment: string): string => `Wordt automatisch gewist op ${moment}.`,
  vergaderAlleTonen: (n: number): string => `Alle ${String(n)} tonen`,
  vergaderMinderTonen: 'Minder tonen',
  // Verslagen bewaren en Eerder opgenomen (N-C3, ADR 0022)
  vergaderEerderOpgenomen: 'Eerder opgenomen',
  vergaderEerderLabel: 'Eerder opgenomen vergaderingen en notities',
  verslagAlleenIk: 'alleen ik',
  vergaderDagMorgen: 'morgen',
  vergaderBewaartRegel: (dagen: number): string =>
    `Mondig bewaart ze ${String(dagen)} ${dagen === 1 ? 'dag' : 'dagen'}.`,
  vergaderBewaartWijzigen: 'Wijzigen',
  verslagMeerAria: (titel: string): string => `Meer voor ${titel}`,
  verslagMeer: 'Meer',
  verslagWissenMenu: 'Verslag wissen…',
  verslagWissenKop: 'Verslag wissen?',
  verslagWissenTekst:
    'Mondig wist het verslag en de opname. Wat je als bestand hebt opgeslagen, blijft in je map. Dit kun je niet ongedaan maken.',
  verslagWissenBevestig: 'Ja, wissen',
  verslagGewistSchermlezer: 'Het verslag is gewist.',
  verslagTochSluiten: 'Toch sluiten',
  verslagNaamWijzigen: 'Naam wijzigen',
  verslagNaamLabel: 'Naam van het verslag',
  verslagNaamWijzigenAria: (titel: string): string => `Naam wijzigen, ${titel}`,
  verslagNaamGegeven: (naam: string): string => `Het verslag heet nu ${naam}.`,
  verslagNaamTerug: 'Het verslag heeft weer de titel die Mondig maakt.',
  verslagBewaardTot: (dag: string): string => `Mondig bewaart dit verslag tot ${dag}`,
  vergaderMeerMenuNu: 'Meer voor deze opname',
  vergaderVerwijderen: 'Verwijderen',
  vergaderVerwijderAria: (wanneer: string): string => `Opname van ${wanneer} verwijderen`,
  vergaderOokNog: (
    aantal: number,
    soort: string,
    wist: 'vandaag' | 'morgen' | null,
    aantalWist: number
  ): string => {
    const basis =
      aantal === 1
        ? `Ook nog 1 opname bij ${soort}`
        : `Ook nog ${String(aantal)} opnames bij ${soort}`;
    if (wist === null) return basis;
    if (aantal === 1) return `${basis}, wordt ${wist} gewist`;
    return `${basis}, ${String(aantalWist)} ${aantalWist === 1 ? 'wordt' : 'worden'} ${wist} gewist`;
  },
  vergaderAudioNaast: (naam: string): string => `De opname staat ernaast als ${naam}.`,
  vergaderAudioNaastFout: (melding: string): string =>
    `Het verslag is opgeslagen, maar de opname niet: ${melding}`,
  audioNaastKop: 'Opname ook bewaren als geluidsbestand',
  audioNaastHulp:
    'Bij het opslaan van een verslag komt de opname als WAV ernaast, met dezelfde naam.',
  mapOpenenKort: 'Openen',
  mapOpenenAria: (map: string): string => `${map} openen in de Verkenner`,
  vergaderOpnameWeggooien: 'Opname weggooien…',

  // Modellen (sprint 5-A)
  menuModellen: 'Modellen',
  modellenTitel: 'Modellen',
  modellenPaginaUitleg:
    'Het model is het deel van Mondig dat spraak omzet in tekst, op deze computer. Bovenaan zie je wat Mondig nu gebruikt, daaronder beheer je de modellen.',
  modellenLaadFout: 'Mondig kon de modellen niet ophalen. Probeer het opnieuw.',
  taakDicteren: 'Dicteren',
  taakBestanden: 'Opnames en bestanden',
  wijzigen: 'Wijzigen',
  watMondigGebruiktKop: 'Wat Mondig gebruikt',
  functieDicterenWaarom: 'Hier telt snelheid.',
  functieBestandenWaarom: 'Hier telt nauwkeurigheid.',
  functieSprekersWaarom: 'Wie zei wat in een vergadering.',
  modelRolNaam: (rol: 'snel' | 'nauwkeurig' | 'extra_nauwkeurig' | null): string =>
    rol === 'snel'
      ? 'Snel model'
      : rol === 'nauwkeurig'
        ? 'Nauwkeurig model'
        : rol === 'extra_nauwkeurig'
          ? 'Extra nauwkeurig model'
          : 'Model',
  sprekermodelNaam: 'Sprekermodel',
  pastBijComputer: 'Past bij deze computer',
  gecontroleerdOp: (datum: string): string => `Gecontroleerd op ${datum}`,
  opnieuwControlerenComputer: 'Opnieuw controleren',
  laatComputerBekijken: 'Laat Mondig je computer bekijken',
  functieWijzigenAria: (functie: string): string => `Model voor ${functie} wijzigen`,
  functieHuidig: 'In gebruik',
  functieEerstDownloaden: (grootte: string): string => `Eerst ${grootte} downloaden.`,
  functieWordtGedownload: 'Wordt al gedownload.',
  functieTeWeinigRuimte: (grootte: string): string => `Te weinig ruimte: nog ${grootte} nodig.`,
  kaartVolgt: (rol: string): string => `${rol} volgt zodra de download klaar is.`,
  sprekersModelAanwezig: 'Staat op deze computer. Herkent tot 4 sprekers.',
  sprekersModelNiet: (grootte: string): string =>
    `Nog niet gedownload (${grootte}). Herkent tot 4 sprekers.`,
  naHervattenGebruik: (dicteren: boolean, opnames: boolean): string =>
    dicteren && opnames
      ? 'Wordt gebruikt voor dicteren en opnames zodra de download klaar is.'
      : dicteren
        ? 'Wordt gebruikt voor dicteren zodra de download klaar is.'
        : 'Wordt gebruikt voor opnames zodra de download klaar is.',
  kaartDicterenSub: (tijd: string): string => `Je tekst staat er ${tijd}.`,
  kaartBestandenSub: (tijd: string): string => `Een uur opname duurt ${tijd}.`,
  kaartOnbekend: 'Dit model staat nog niet op je computer.',
  groepOpDezeComputer: 'Op deze computer',
  groepTeDownloaden: 'Te downloaden',
  balkNauwkeurig: 'Nauwkeurig',
  balkSnel: 'Snel',
  balkStappen: (naam: string, stap: number): string => `${naam}: ${String(stap)} van 5`,
  balkUitlegKnop: 'Uitleg over de balkjes',
  balkUitleg:
    'Gemeten op voorgelezen Nederlandse tekst, op de processor. Op jouw computer kan het anders zijn.',
  talenAantal: (aantal: number, turksArabisch: boolean): string =>
    turksArabisch ? `${String(aantal)} talen, ook Turks en Arabisch` : `${String(aantal)} talen`,
  labelAanbevolen: 'Aanbevolen',
  labelSneller: 'Sneller',
  labelNauwkeuriger: 'Nauwkeuriger',
  labelTeTraag: 'Te traag',
  labelNieuw: 'Nieuw',
  labelLokaal: 'Lokaal',
  inGebruikVoorDicteren: 'in gebruik voor dicteren',
  inGebruikVoorBestanden: 'in gebruik voor opnames',
  inGebruikVoorBeide: 'in gebruik voor dicteren en opnames',
  gedeeltelijkGedownload: (binnen: string, totaal: string): string =>
    `${binnen} van ${totaal} is al binnen`,
  verderDownloadenGrootte: (rest: string): string => `Verder downloaden (nog ${rest})`,
  stoppenMetDownloaden: 'Stoppen met downloaden',
  verwijderen: 'Verwijderen',
  verwijderenBevestigen: (grootte: string): string => `Verwijderen? Dit maakt ${grootte} vrij.`,
  nietWeg:
    'Je gebruikt dit model nog. Kies eerst een ander model; anders kan Mondig niets uitschrijven.',
  eenDownloadTegelijk: 'Er loopt al een download. Start dit model als die klaar is.',
  ruimteTekortKop: 'Er is te weinig ruimte voor dit model',
  ruimteTekortTekst: (nodig: string, vrij: string): string =>
    `Mondig heeft ${nodig} nodig en er is ${vrij} vrij. Maak ruimte vrij op deze computer, of verwijder een model dat je niet gebruikt, en controleer opnieuw.`,
  ruimteTekortOnbekend:
    'Er is te weinig ruimte op deze computer. Maak ruimte vrij, of verwijder een model dat je niet gebruikt, en controleer opnieuw.',
  downloadMisluktKop: 'Het downloaden is niet gelukt',
  modellenTotaalVoor: 'Modellen nemen samen ',
  modellenTotaalNa: ' in beslag op deze computer.',
  modelBezigOphalen: 'Mondig haalt het model op…',
  zijbalkDownload: (naam: string): string => `${naam} downloaden`,
  zijbalkProcent: (n: number): string => `${String(n)}%`,
  zijbalkDownloadAria: (naam: string, n: number): string =>
    `${naam} downloaden, ${String(n)} procent. Open Modellen.`,
  wordtGestart: 'Wordt gestart…',
  hervatten: 'Hervatten',
  nietHervatten: 'Niet hervatten',
  downloadWacht: 'Wacht tot de download ervoor klaar is.',
  downloadNieuwePoging: (poging: number, van: number): string =>
    `De verbinding viel weg. Mondig probeert het zelf opnieuw (poging ${String(poging)} van ${String(van)})…`,
  downloadOnderbroken: 'Onderbroken toen Mondig werd afgesloten. Wat al binnen was, staat er nog.',
  zijbalkDownloadsKop: 'Downloads',
  zijbalkWacht: 'Wacht',
  zijbalkOpnieuw: (poging: number, van: number): string =>
    `Opnieuw (${String(poging)} van ${String(van)})`,
  zijbalkKlaar: 'Klaar',
  zijbalkMislukt: 'Mislukt',
  zijbalkGestopt: 'Gestopt',
  zijbalkOnderbroken: 'Onderbroken',
  zijbalkItemAria: (naam: string, stand: string): string => `${naam}: ${stand}. Open Modellen.`,
  zijbalkIngeklaptAria: (n: number): string =>
    n === 1 ? '1 download. Open Modellen.' : `${String(n)} downloads. Open Modellen.`,
  sprekersWordtGestart: 'Het downloaden van het sprekermodel wordt gestart…',
  sprekersWacht: 'Het sprekermodel staat in de rij en komt na de download ervoor.',
  sprekersAangezet: 'Het sprekermodel staat op je computer. Sprekers herkennen staat aan.',
  instellingModellenKop: 'Modellen',
  instellingModellenHulp: (verschillend: boolean): string =>
    verschillend
      ? 'Dicteren met het snelle model, opnames en bestanden met het nauwkeurige.'
      : 'Dicteren, opnames en bestanden met hetzelfde model.',
  instellingModellenKies: 'Kiezen, downloaden en verwijderen.',
  instellingModellenKnop: 'Naar Modellen',
  modelTekst: (id: string): ModelTekst | null => {
    switch (id) {
      case 'parakeet-ultra-q8':
        return {
          naam: 'Parakeet Ultra',
          zin: 'Snel en betrouwbaar. De beste keuze om te dicteren: je tekst staat er na een halve seconde.',
          dicterenTijd: 'na een halve seconde',
          uurOpname: 'ongeveer 4 minuten'
        };
      case 'canary-1b-v2-q8':
        return {
          naam: 'Canary',
          zin: 'Schrijft nauwkeuriger dan Parakeet Ultra, in ongeveer drie keer zoveel tijd. Een goede balans voor bestanden.',
          dicterenTijd: 'na ongeveer 2 seconden',
          uurOpname: 'ongeveer 10 minuten'
        };
      case 'whisper-large-v3-turbo-q8':
        return {
          naam: 'Whisper turbo',
          zin: 'De nauwkeurigste, en de enige voor Turks en Arabisch. Te traag om mee te dicteren, en traag zonder snelle videokaart.',
          dicterenTijd: 'na ongeveer 12 seconden',
          uurOpname: 'ongeveer 26 minuten',
          extra: 'Voor Turks, Arabisch en talen buiten de 25. Alleen te kiezen voor bestanden.'
        };
      case 'sortformer-4spk-v2.1-q8':
        return {
          naam: 'Sortformer',
          zin: 'Herkent wie er spreekt, tot vier mensen per opname. Heeft een apart model nodig, naast het spraakmodel.',
          extra:
            'Van NVIDIA, onder de NVIDIA Open Model License. De licentie staat bij Over Mondig, onder Spraakmodellen.'
        };
      default:
        return null;
    }
  },
  // Het verslag verbeteren, beluisteren en doorzoeken (stroom V, V-1)
  verslagHint: 'Klik op een regel om de tekst te verbeteren.',
  verslagHintAfspelen:
    'Klik op een regel om de tekst te verbeteren, of op de tijd om te luisteren.',
  verslagRegelVeld: (tijd: string): string => `Tekst van de regel van ${tijd}`,
  verslagVeldHulp: 'Enter bewaart, Shift+Enter begint een nieuwe regel, Esc maakt ongedaan.',
  verslagRegelBewaard: 'De regel is aangepast.',
  verslagAfspelenVanaf: (tijd: string): string => `Afspelen vanaf ${tijd}`,
  verslagPauzeren: (tijd: string): string => `Pauzeren, regel van ${tijd}`,
  verslagAfspelenMislukt: 'Het afspelen lukte niet. Probeer het opnieuw.',
  verslagLaden: 'Het geluid wordt klaargezet.',
  verslagGeenGeluidOpname: 'Afspelen kan niet: de opname is gewist.',
  verslagGeenGeluidBestand: 'Afspelen kan niet: het bronbestand is verplaatst of verwijderd.',
  verslagSnelheidKop: 'Snelheid',
  verslagSnelheidTitel: 'Snelheid van afspelen',
  verslagWerkbalk: 'Hulpmiddelen voor het verslag',
  verslagSnelheid: (tienden: number): string =>
    tienden === 15 ? '1,5×' : `${String(tienden / 10)}×`,
  verslagSnelheidAria: (tienden: number): string =>
    tienden === 15 ? 'Snelheid 1,5 keer' : `Snelheid ${String(tienden / 10)} keer`,
  verslagZoeken: 'Zoeken',
  verslagZoekenTitel: 'Zoeken in het verslag (Ctrl+F)',
  verslagZoekVeld: 'Zoeken in het verslag',
  verslagZoekTeller: (nu: number, totaal: number): string => `${String(nu)} van ${String(totaal)}`,
  verslagZoekGeen: 'Geen treffers',
  verslagZoekVolgende: 'Volgende treffer',
  verslagZoekVorige: 'Vorige treffer',
  verslagZoekSluiten: 'Zoeken sluiten',

  // Persoonsgegevens markeren en vervangen (stroom V, V-2)
  persoonSwitch: 'Persoonsgegevens vervangen bij Kopiëren en Opslaan',
  persoonGeen: 'Er is niets gemarkeerd.',
  persoonWeghalen: (tekst: string): string => `Markering van ${tekst} weghalen`,
  persoonWeghalenTitel: 'Markering weghalen',
  persoonTeller: (aantal: number): string =>
    aantal === 0
      ? 'Geen persoonsgegevens'
      : `${String(aantal)} ${aantal === 1 ? 'persoonsgegeven' : 'persoonsgegevens'}`,
  persoonStandUit: 'Niet vervangen',
  persoonStandUitRest: 'bij kopiëren en opslaan',
  persoonStandAan: 'Vervangen',
  persoonStandAanRest: 'door [BSN] en zo, bij kopiëren en opslaan',
  persoonUitlegKnop: 'Hoe werkt dit?',
  persoonUitleg1:
    'Mondig markeert gegevens met een vaste vorm: BSN, IBAN, telefoonnummer, e-mailadres, adres, datum en kenteken. Namen markeer je zelf.',
  persoonUitleg2:
    'Staat de schakelaar aan, dan komt er bij Kopiëren en Opslaan [BSN] of [TELEFOON] op die plek. De opname gaat dan niet mee.',
  persoonUitleg3:
    'Getallen in woorden (zoals "nul zes") en uitgesproken e-mailadressen (zoals "apenstaartje") herkent Mondig niet. Lees het verslag dus altijd zelf na. Wat Mondig automatisch opslaat, bevat de oorspronkelijke tekst.',
  persoonGevondenKop: (aantal: number): string =>
    aantal === 0 ? 'Niets gevonden' : `${String(aantal)} gevonden`,
  persoonGevondenHulp:
    'Mondig vindt niet alles. Lees het verslag zelf na en markeer wat ontbreekt.',
  persoonKeerVoor: (aantal: number): string => `${String(aantal)} keer`,
  // Opnieuw uitschrijven (N-C4, ADR 0023)
  opnieuwMenu: 'Opnieuw uitschrijven…',
  opnieuwGewistMenu: 'Opnieuw uitschrijven',
  opnieuwGewistReden: 'Kan niet meer: de opname is gewist.',
  opnieuwKop: 'Opnieuw uitschrijven',
  opnieuwUitleg: (rol: string | null): string =>
    rol === null
      ? 'Kies een model; het nieuwe verslag komt in de plaats van dit verslag.'
      : `Dit verslag is gemaakt met het ${rol} model. Kies een model; het nieuwe verslag komt in de plaats van dit verslag.`,
  // De rol als bijvoeglijk naamwoord: "het nauwkeurige model".
  opnieuwRol: (rol: string): string =>
    ({ snel: 'snelle', nauwkeurig: 'nauwkeurige', extra_nauwkeurig: 'extra nauwkeurige' })[rol] ??
    rol,
  opnieuwKeuzeNaam: (rol: string): string =>
    rol === 'extra_nauwkeurig' ? 'Extra nauwkeurig model' : 'Nauwkeurig model',
  opnieuwVoordeel: (rol: string): string =>
    rol === 'extra_nauwkeurig' ? 'De minste fouten.' : 'Minder fouten.',
  opnieuwDuur: (minuten: number, opname: number): string =>
    `Duurt ongeveer ${String(minuten)} ${minuten === 1 ? 'minuut' : 'minuten'} voor deze ${String(opname)} ${opname === 1 ? 'minuut' : 'minuten'}.`,
  opnieuwDuurOnbekend: (rol: string | null): string =>
    rol === null ? 'Duurt langer dan het snelle model.' : `Duurt langer dan het ${rol} model.`,
  opnieuwOntbreekt: (grootte: string): string =>
    `Nog niet op deze computer. Eerst downloaden: ${grootte}.`,
  opnieuwNaarModellen: 'Naar Modellen',
  opnieuwLaden: 'Mondig kijkt welke modellen er zijn…',
  opnieuwGeenKeuze: 'Er is geen nauwkeuriger model om dit verslag mee uit te schrijven.',
  opnieuwBlijftSprekers: 'Sprekers en de namen die je gaf, blijven.',
  opnieuwBlijftMarkeringen: 'Wat je zelf markeerde, blijft gemarkeerd.',
  opnieuwVerbeterd: (aantal: number): string =>
    aantal === 1
      ? 'Je verbeterde 1 regel. Die wordt opnieuw uitgeschreven; daarna kun je terug naar deze versie.'
      : `Je verbeterde ${String(aantal)} regels. Die worden opnieuw uitgeschreven; daarna kun je terug naar deze versie.`,
  opnieuwStart: 'Opnieuw uitschrijven',
  opnieuwBezig: (rol: string | null): string =>
    rol === null ? 'Opnieuw uitschrijven…' : `Opnieuw uitschrijven met het ${rol} model…`,
  opnieuwBezigHulp: 'Verbeteren kan weer als het klaar is.',
  opnieuwStoppen: 'Stoppen',
  opnieuwGestopt: 'Opnieuw uitschrijven is gestopt. Het verslag is niet veranderd.',
  opnieuwKlaar: (rol: string | null, veranderd: number): string => {
    const met =
      rol === null ? 'Opnieuw uitgeschreven.' : `Opnieuw uitgeschreven met het ${rol} model.`;
    if (veranderd === 0) return `${met} Er is geen regel veranderd.`;
    return veranderd === 1
      ? `${met} 1 regel is veranderd.`
      : `${met} ${String(veranderd)} regels zijn veranderd.`;
  },
  opnieuwVerdwenen: (aantal: number): string =>
    aantal === 1
      ? '1 markering staat niet meer in de tekst; lees het verslag na.'
      : `${String(aantal)} markeringen staan niet meer in de tekst; lees het verslag na.`,
  opnieuwTerugzetten: 'Vorige versie terugzetten',
  opnieuwTerugzettenKop: 'Vorige versie terugzetten?',
  opnieuwTerugzettenTekst:
    'Het verslag wordt weer zoals het was voor het opnieuw uitschrijven. Namen en verbeterde tekst die je sindsdien aanpaste, gaan daarbij ook terug. Je markeringen van persoonsgegevens blijven.',
  opnieuwTerugzettenBevestig: 'Ja, terugzetten',
  opnieuwTeruggezet: 'De vorige versie staat weer terug.',
  opnieuwVeranderdAria: 'veranderd bij opnieuw uitschrijven',
  persoonZelfKop: 'Zelf iets markeren',
  persoonZelfHulp:
    'Typ of selecteer een stuk tekst, bijvoorbeeld een naam. Het wordt overal in het verslag gemarkeerd.',
  persoonZelfVeld: 'Stuk tekst om te markeren',
  persoonZelfKnop: 'Markeren',
  persoonWeggehaald: 'De markering is weggehaald.',
  persoonGemarkeerd: 'Het stuk is gemarkeerd.',
  persoonSoort: (soort: string): string =>
    ({
      bsn: 'BSN',
      iban: 'IBAN',
      telefoon: 'Telefoon',
      email: 'E-mail',
      adres: 'Adres',
      datum: 'Datum',
      kenteken: 'Kenteken',
      handmatig: 'Zelf gemarkeerd'
    })[soort] ?? soort,
  persoonSoortAria: (soort: string): string => `gemarkeerd als ${soort}`,

  // Notulen (T3, ADR 0024 tot en met 0026): Instellingen en het tabblad Notulen van het verslag
  groepNotulen: 'Notulen',
  notulenKop: 'Notulen maken met',
  notulenHulp:
    'Kies waar de tekst van een verslag heen gaat. Niets ingesteld: dan maakt Mondig geen notulen.',
  notulenKeuzeGeen: 'Niet ingesteld',
  notulenKeuzeMistral: 'Mistral (Europees)',
  notulenKeuzeOllama: 'Ollama op deze computer',
  notulenSleutelKop: 'Sleutel van Mistral',
  notulenSleutelHulp:
    'Mondig bewaart de sleutel in de sleutelbos van je computer, niet in een bestand. Een sleutel maak je aan in de Mistral-console: console.mistral.ai.',
  notulenSleutelVeld: 'Sleutel van Mistral invullen',
  notulenSleutelPlaats: 'Plak hier je sleutel',
  notulenSleutelOpgeslagen: 'Opgeslagen',
  notulenSleutelBewaren: 'Bewaren',
  notulenSleutelTesten: 'Testen',
  notulenSleutelTestBezig: 'Aan het testen...',
  notulenSleutelTestOk: 'De sleutel werkt.',
  notulenSleutelWissen: 'Wissen',
  notulenSleutelGewist: 'De sleutel is gewist.',
  notulenSleutelVervangen: 'Een nieuwe sleutel invullen',
  notulenTrainingVinkje: 'Training staat uit voor deze sleutel.',
  notulenTrainingWaarom: 'Waarom vraagt Mondig dit?',
  notulenTrainingUitleg:
    'Volgens de voorwaarden van Mistral mag Mistral de gegevens die je via de API stuurt gebruiken om zijn modellen te trainen, tenzij je dat uitzet (verwerkersovereenkomst, artikel 2.3). Mondig kan niet controleren of dat bij jouw sleutel is gedaan. Zet het daarom zelf uit in de Mistral-console (console.mistral.ai) en vink dit daarna pas aan. Zonder vinkje maakt Mondig geen notulen via Mistral.',
  notulenModelKop: 'Model',
  notulenModelNauwkeurig: 'Nauwkeurig (Mistral Large)',
  notulenModelSnel: 'Sneller (Mistral Small)',
  notulenOllamaModelKop: 'Model van Ollama',
  notulenOllamaDraait: 'Ollama draait op deze computer.',
  notulenOllamaDraaitNiet: 'Ollama draait niet. Start Ollama en kijk daarna opnieuw.',
  notulenOllamaGeenModellen: 'Ollama heeft nog geen model. Haal er eerst een op met Ollama.',
  notulenOllamaKies: 'Kies een model',
  notulenOllamaOpnieuwKijken: 'Opnieuw kijken',
  notulenOllamaKijkt: 'Mondig kijkt of Ollama draait...',
  notulenOllamaDuur: 'Zonder snelle videokaart duurt dit 10 tot 20 minuten.',
  verslagTabsLabel: 'Verslag of notulen',
  verslagTabVerslag: 'Verslag',
  verslagTabNotulen: 'Notulen',
  notulenLeegKop: 'Nog geen notulen',
  notulenLeegTekst:
    'Mondig maakt van dit verslag een samenvatting, de besluiten en de actiepunten. Bij elk punt staat waar het gezegd is.',
  notulenMaken: 'Notulen maken',
  notulenNietIngesteld: 'Notulen maken is nog niet ingesteld.',
  notulenOnvolledig: 'Notulen maken is nog niet klaar voor gebruik.',
  notulenNaarInstellingen: 'Naar Instellingen',
  notulenVraagKop: 'Waar gaat de tekst heen?',
  notulenVraagTekst: (aanbieder: string, vervangen: boolean): string =>
    `De tekst van dit verslag gaat naar ${aanbieder}. ${
      vervangen ? 'Persoonsgegevens worden vervangen.' : 'Persoonsgegevens worden niet vervangen.'
    }`,
  notulenBezigKop: 'Mondig maakt de notulen',
  notulenBezigLeest: 'Mondig leest het verslag.',
  notulenBezigDeel: (deel: number, totaal: number): string =>
    `Deel ${String(deel)} van ${String(totaal)}`,
  notulenStoppen: 'Stoppen',
  notulenGestopt: 'Het maken van de notulen is gestopt. Het verslag is niet veranderd.',
  notulenSamenvatting: 'Samenvatting',
  notulenSamenvattingNota: 'Gemaakt door een taalmodel, lees na.',
  notulenBesluiten: 'Besluiten',
  notulenGeenBesluiten: 'Geen besluiten.',
  notulenActiepunten: 'Actiepunten',
  notulenGeenActiepunten: 'Geen actiepunten.',
  notulenWie: 'wie',
  notulenWanneer: 'wanneer',
  notulenNietGenoemd: 'niet genoemd',
  notulenBronAria: (tijd: string): string => `Ga naar ${tijd} in het verslag`,
  notulenBronCitaat: (citaat: string): string => `Het model citeerde: ${citaat}`,
  notulenNietTeruggevonden: 'Niet teruggevonden in het verslag',
  notulenVeldNietGevonden: 'niet in het verslag gevonden',
  notulenAanpassen: 'Aanpassen',
  notulenWeghalen: 'Weghalen',
  notulenPuntAanpassenAria: (soort: string): string => `${soort} aanpassen`,
  notulenPuntWeghalenAria: (soort: string): string => `${soort} weghalen`,
  notulenSoortBesluit: 'Besluit',
  notulenSoortActiepunt: 'Actiepunt',
  notulenVeldWat: 'Wat',
  notulenVeldWie: 'Wie',
  notulenVeldWanneer: 'Wanneer',
  notulenVeldTekst: 'Tekst van het besluit',
  notulenPuntBewaren: 'Bewaren',
  notulenPuntGewijzigd: 'Het punt is aangepast.',
  notulenPuntWeg: 'Het punt is weggehaald.',
  notulenPlekEuropees: (model: string): string => `${model} (Europees)`,
  notulenPlekLokaal: (model: string): string => `${model} op deze computer (Lokaal)`,
  notulenOverschrijfVraag: 'Er staan notulen van een nieuwere versie van Mondig. Overschrijven?',
  notulenOverschrijven: 'Overschrijven en maken',
  notulenGemaaktMet: (model: string, moment: string): string => `Gemaakt met ${model} · ${moment}`,
  notulenKopieren: 'Kopiëren',
  notulenGekopieerd: 'De notulen staan op het klembord.',
  notulenOpnieuwMaken: 'Opnieuw maken',
  notulenVorigeVersie:
    'Gemaakt bij de vorige versie van het verslag. De bronnen kloppen mogelijk niet meer en zijn niet te openen.',
  notulenOnleesbaar:
    'Dit verslag heeft notulen die deze versie van Mondig niet kan lezen. Ze blijven bewaard.',
  notulenMarkeringTelling: (aantal: number): string =>
    aantal === 1
      ? '1 punt is niet teruggevonden in het verslag. Controleer dat punt.'
      : `${String(aantal)} punten zijn niet teruggevonden in het verslag. Controleer die punten.`
} as const;
