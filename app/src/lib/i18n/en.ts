import type { ModelTekst } from './modeltekst';
import type { Berichten } from './typen';

// Engelse berichten; moeten exact dezelfde sleutels en signaturen hebben als nl.ts.
export const en = {
  appTitle: 'Mondig',
  modelNodigKop: 'Download a speech model first',
  modelNodigTekst: (grootte: string): string =>
    `One-time download of ${grootte}. After that, Mondig works without internet.`,
  downloaden: 'Download',
  homeDictate: 'Dictate',
  terug: 'Back',
  spraakmodelDownloaden: 'Download speech model',
  aantalBestanden: (n: number): string => {
    const regel = new Intl.PluralRules('en').select(n);
    return regel === 'one' ? `${String(n)} file` : `${String(n)} files`;
  },

  // Algemeen
  opnieuwControleren: 'Check again',
  opnieuwProberen: 'Try again',
  opnieuwDownloaden: 'Download again',
  anderModelKiezen: 'Go to the speech model',
  appNietBijgewerkt:
    'Mondig was not fully updated. Close Mondig and install the latest version again.',
  annuleren: 'Cancel',
  kopierenMislukt: 'Copying did not work. Select the text and copy it with Ctrl+C.',
  somsGingErIetsMis: 'Something went wrong. Try again.',
  modelStatusFout:
    'Mondig could not check whether the speech model is there. Try again. If that does not help, restart Mondig.',

  // Downloadscherm
  verderDownloaden: 'Continue downloading',
  stoppen: 'Stop',
  downloadVoortgangNaam: 'Download progress',
  procent: (n: number): string => `${String(n)}%`,
  downloadVoortgang: (binnen: string, totaal: string): string => `${binnen} of ${totaal}`,
  downloadControleren: 'Checking the download…',
  nogSeconden: (n: number): string => `About ${String(n)} seconds left`,
  nogMinuten: (n: number): string =>
    n === 1 ? 'About 1 minute left' : `About ${String(n)} minutes left`,
  nogUren: (n: number): string => (n === 1 ? 'About 1 hour left' : `About ${String(n)} hours left`),
  procentGedownload: (n: number): string => `${String(n)}% downloaded`,
  procentGedownloadControleren: (n: number): string =>
    `${String(n)}% downloaded. The model is being checked.`,
  gegevensOphalen: 'Getting details…',

  // Proef op het dicteerscherm
  bezigMetUitschrijven:
    'Transcribing… The first time takes a little longer, because the speech model is being loaded.',
  klaarIn: (seconden: number): string => {
    const tekst = new Intl.NumberFormat('en', {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1
    }).format(seconden);
    return `Done in ${tekst} seconds`;
  },
  lokaalVerwerkt: 'Processed locally',
  uitgeschrevenMet: (naam: string): string => `Written out with ${naam}`,
  tekstGekopieerd: 'The text has been copied.',
  geenGeluid: 'No sound heard.',
  geenTekst: 'No text was recognised.',
  tekstVerdacht: 'There is almost no speech in the recording, so the text is probably wrong.',
  // Dictating with the shortcut
  sneltoetsUitleg: (toets: string): string => `Hold ${toets} and speak. A short tap works too.`,
  sneltoetsTerugval: (gewenst: string, actief: string): string =>
    `${gewenst} is already used by another program. Mondig now uses ${actief}.`,
  sneltoetsGeen: (toetsen: string): string =>
    `Mondig could not set up a shortcut: ${toetsen} are already used by other programs. Close one of them and restart Mondig, or choose your own key under Settings, Advanced.`,
  dictaatLuistert: 'Mondig is listening…',
  dictaatHoe: (toets: string): string => `Done? Release ${toets} or tap it again. Esc cancels.`,
  dictaatBijnaVol: 'One minute left, then Mondig stops listening and transcribes.',
  dictaatGeannuleerd: 'The dictation was cancelled.',
  dictaatOpnieuw: (toets: string): string => `Try again with ${toets}.`,
  laatsteDictaatKop: 'Last dictation',
  // Dictation: shortcut, microphone and practice box on the page itself (UX-D)
  proefveldKop: 'Try it here',
  proefveldPlaats: (toets: string): string => `Click here, hold ${toets} and say something.`,
  proefveldLabel: 'Practice box for dictation',
  plakInProefveld: 'The text is in the practice box.',
  kopieren: 'Copy',
  plakGeplakt: 'Pasted where you were typing.',
  plakWaarschijnlijk: 'Pasted. Check that the text is there.',
  plakVorigKlembordKwijt: 'Your previous clipboard could not be saved.',
  plakVorigKlembordOnvolledig: 'Your previous clipboard could not be saved completely.',
  plakOpKlembord: 'The text is on the clipboard. Paste it with Ctrl+V.',
  plakBeheerrechten:
    'Mondig cannot paste into this window, because it runs as administrator. Copy the text and paste it yourself.',
  plakToetsen:
    'Mondig did not paste, because a key was still held down. Copy the text and paste it yourself.',
  plakGeenVenster: 'There was no window to paste into. Copy the text and paste it yourself.',
  plakNietGelezen:
    "Mondig is not sure the text was pasted. If it isn't there, copy it and paste it yourself.",
  plakMislukt: 'Pasting did not work. Copy the text and paste it yourself.',
  plakNietGeplakt: 'Mondig could not paste into this window. The text is ready under Copy.',
  modelAanvullenKop: 'The speech model is not complete yet',
  modelAanvullenTekst: (grootte: string): string =>
    `${grootte} still to download. After that, Mondig is ready to dictate.`,
  // Settings
  instellingen: 'Settings',
  microfoonKop: 'Microphone',
  // One place for the name of a default device (microphone and speaker): the name first.
  apparaatStandaard: (naam: string | null): string =>
    naam === null ? 'Windows default' : `${naam} (default)`,
  microfoonHulp: 'For dictation. For Record you choose per meeting.',
  microfoonMeterAria: 'Microphone level',
  systeemMeterAria: 'Level of the sound the computer plays',
  meterWaarde: (aan: number, totaal: number): string => `${String(aan)} of ${String(totaal)}`,
  meterVoorrang: 'The meter pauses while a dictation or recording is running.',
  meterGestopt: 'The meter has stopped.',
  micTestKnop: 'Test microphone',
  micTestInstellingen: 'Test',
  micTestStoppen: 'Click to stop',
  micTestKlaar: 'Done',
  micTestHint: 'Say something. If the meter moves, Mondig can hear you.',
  micTestStilte: 'Mondig heard nothing and turned the microphone off again.',
  proefveldOpenen: 'Try it here',
  meerOpties: 'More options',
  minderOpties: 'Fewer options',
  groepDezeComputer: 'This computer',
  groepDicteren: 'Dictation',
  groepOpslaan: 'Saving',
  groepBewaakteMap: 'Watch a folder',
  groepOpstarten: 'Startup',
  microfoonWeg: 'Your chosen microphone is not connected. Mondig now uses the Windows default.',
  sneltoetsKop: 'Shortcut',
  sneltoetsWijzigen: 'Change',
  sneltoetsHoudTik: 'Hold: record until you let go. Tap briefly: record until you tap again.',
  sneltoetsVang: 'Press the keys you want to use…',
  sneltoetsVangHulp:
    'What you press appears here; letting go saves it. Esc stops without changing.',
  sneltoetsVangAfgebroken: 'Changing stopped. The shortcut is unchanged.',
  sneltoetsHerstellen: (standaard: string): string => `Restore default (${standaard})`,
  sneltoetsHerstellenKort: 'Restore default',
  sneltoetsOngewijzigd: '(unchanged)',
  sneltoetsIngesteld: (toets: string): string => `The shortcut is now ${toets}.`,
  sneltoetsBezetKop: 'This key is already in use',
  sneltoetsBezetTekst: (toets: string): string =>
    `${toets} is used by another program. Choose a different combination.`,
  sneltoetsFoutWindows: 'Combinations with the Windows key are not possible. Try Ctrl or Alt.',
  sneltoetsFoutLosseToets:
    'You need this key for typing or navigating. Choose Insert, Scroll Lock or a function key, or add Ctrl or Alt.',
  sneltoetsFoutShiftAlleen: 'Shift alone is not enough. Add Ctrl or Alt.',
  sneltoetsFoutWerktNiet: (toets: string): string =>
    toets.includes('+')
      ? 'Pause does not work as a shortcut, not even with Ctrl or Alt. Choose another key.'
      : 'Pause does not work as a shortcut: Windows does not report when you let go. Choose another key.',
  sneltoetsFoutHoofdtoets: 'Also press a regular key, for example Ctrl+Space or F9.',
  sneltoetsFoutMeerdere: 'Use only one regular key.',
  sneltoetsFoutOnbekend: 'This key cannot be a shortcut. Choose another.',
  sneltoetsFoutGereserveerd: 'You need this combination in other programs. Choose another.',
  sneltoetsFunctieKop: (toets: string): string => `Use ${toets} as the shortcut?`,
  sneltoetsFunctieTekst: (toets: string): string => {
    switch (toets) {
      case 'F1':
        return 'F1 opens help in most programs. That will no longer work.';
      case 'F2':
        return 'F2 renames a file or cell. That will no longer work.';
      case 'F5':
        return 'F5 refreshes a page in your browser. That will no longer work.';
      case 'F11':
        return 'F11 makes your browser full screen. That will no longer work.';
      case 'F12':
        return 'F12 is "Save as" in Word. That will no longer work.';
      default:
        return 'This key will no longer do anything in other programs.';
    }
  },
  sneltoetsToch: 'Use it anyway',
  sneltoetsAndere: 'Choose another key',
  geheugenKop: 'Keep the model in memory',
  geheugenHulp: 'Longer is faster for the next dictation, but keeps memory in use.',
  rusttijd5m: '5 minutes (default)',
  rusttijd15m: '15 minutes',
  rusttijd1u: '1 hour',
  rusttijdNooit: 'Always',
  videokaartKop: 'Use graphics card',
  videokaartHulp:
    'Faster transcription with a dedicated graphics card. If it fails, Mondig switches to the processor by itself.',
  videokaartZoeken: 'Mondig is looking for a graphics card…',
  videokaartGevonden: (naam: string): string => `Found: ${naam}.`,
  videokaartIngebouwd: (naam: string): string =>
    `Found: ${naam} (built in). A built-in graphics chip is often not faster; test the speed first.`,
  videokaartGeen: 'No suitable graphics card found.',
  videokaartOnbekend: 'Mondig could not check for a graphics card.',
  inGebruikVideokaart: 'In use: graphics card',
  inGebruikProcessor: 'In use: processor',
  inGebruikTerugval: 'The graphics card gave an error; Mondig now uses the processor',
  snelheidTesten: 'Test speed',
  snelheidBezig: 'Mondig is testing the speed, this takes about half a minute…',
  snelheidLoont: (keer: string): string =>
    `The graphics card is ${keer} times faster. Worth turning on.`,
  snelheidLoontNiet: 'On this computer the processor is as fast or faster; leave this off.',
  snelheidFaalde:
    'The graphics card gave an error during the test. Leave this off; Mondig uses the processor.',
  // First start (phase 5, S-1): the wizard and the hardware check
  computerGeheugen: (grootte: string): string => `${grootte} memory`,
  computerKernen: (n: number): string => `${String(n)} processor cores`,
  computerKaart: (naam: string): string => `graphics card ${naam}`,
  computerKaartIngebouwd: (naam: string): string => `built-in graphics chip ${naam}`,
  computerGeenKaart: 'no separate graphics card',
  adviesKaartGeen: 'Mondig uses the processor.',
  adviesKaartIngebouwd:
    'Only a built-in graphics chip: it is often no faster than the processor, so Mondig uses the processor.',
  adviesKaartTesten: 'A test shows whether your graphics card is faster.',
  adviesKaartAanzetten: (keer: string): string =>
    `The graphics card is ${keer} times faster. Use it.`,
  adviesKaartLaten: 'The graphics card is not faster here. Mondig uses the processor.',
  adviesKaartFaalde: 'The graphics card gave an error in the test. Mondig uses the processor.',
  wizStap: (n: number, van: number): string => `Step ${String(n)} of ${String(van)}`,
  wizNuNiet: 'Not now',
  wizVerder: 'Continue',
  wizWaaromKop: 'Mondig writes on your own computer',
  wizWaaromIntro:
    'For that, Mondig needs a speech model. You download it once; after that everything works without internet.',
  wizPuntLokaalKop: 'Your audio stays with you',
  wizPuntLokaalTekst: 'Transcribing happens on this computer. No audio goes onto the internet.',
  wizPuntDownloadKop: 'Download once',
  wizPuntDownloadTekst: (grootte: string, duur: string): string =>
    `A file of about ${grootte}. On an ordinary connection that takes ${duur}.`,
  wizDuur: (snel: number, traag: number): string =>
    snel === traag
      ? `about ${String(snel)} ${snel === 1 ? 'minute' : 'minutes'}`
      : `${String(snel)} to ${String(traag)} minutes`,
  wizPuntRuimteKop: 'Disk space',
  wizPuntRuimteTekst: (vrij: string): string => `${vrij} is free on your disk.`,
  wizPuntRuimteOnbekend: 'Mondig could not check how much space is free.',
  wizAdviesKop: 'This suits this computer',
  wizAdviesLeest: 'Mondig is looking at your computer…',
  wizAdviesFout:
    'Mondig could not read your computer properly. The standard model almost always fits.',
  wizAdviesGelezen: (zin: string): string => `We looked at your computer: ${zin}.`,
  wizAdviesDicteren: 'For dictating',
  wizAdviesBestanden: 'For recordings and files',
  wizBestandenZelfde:
    'The same model. On a computer with little memory Mondig uses one model for everything.',
  wizBestandenKrap:
    'This fits, but Mondig only loads this model while transcribing, because your memory is not generous.',
  wizBestandenGoed:
    'More accurate for recordings and videos. Mondig only loads it while transcribing.',
  wizTweedeModel: (grootte: string): string =>
    `Also download the model for recordings (${grootte})`,
  wizTweedeUitleg:
    'You can skip it: Mondig then uses the first model for files too. You can still download it later under Models.',
  wizGeheugenWeinig: 'This computer has little memory. Mondig works, but may be slower.',
  wizRuimteTekort: (tekort: string): string =>
    `There is too little disk space. Free up about ${tekort} and check again.`,
  wizDownloadKnop: (grootte: string): string => `Download (${grootte})`,
  wizDownloadKop: (aantal: number): string =>
    aantal > 1 ? 'The speech models are downloading' : 'The speech model is downloading',
  wizDownloadVan: (binnen: string, totaal: string): string => `${binnen} of ${totaal}`,
  wizDownloadTip:
    'You can leave this window open. If your connection drops, Mondig carries on where it left off afterwards.',
  wizDownloadGestopt: 'The download has stopped. What is already in stays saved.',
  wizDoorgaanStandaard: 'Continue with the standard model',
  wizDownloadMislukt: 'The download did not work',
  wizDownloadGestart: 'The download is starting…',
  wizTestKop: 'Mondig is testing your graphics card',
  wizTestUitleg:
    'A short test of about a minute shows whether your graphics card is faster than the processor. Dictating is not possible meanwhile.',
  wizTestMislukt: 'The test did not work. Mondig uses the processor.',
  wizKlaarKop: 'You can dictate',
  wizKlaarSneltoets: (toets: string): string =>
    `Hold ${toets} and speak. The text appears where you type. A short tap works too.`,
  wizKlaarSneltoetsGeen: 'You can set the shortcut under Settings. Hold it and speak.',
  wizKlaarTweedeOvergeslagen:
    'For files Mondig uses the same model. You can always download a more accurate model under Models.',
  wizKlaarKaart: 'Use graphics card',
  wizKlaarKaartHulp: 'The test showed this is faster. You can turn it off later in Settings.',
  wizAanDeSlag: 'Get started',
  computerCheckKop: 'Set up Mondig again',
  computerCheckHulp:
    'After a new computer, graphics card or more memory. Mondig looks at what fits best now and applies it with one click.',
  computerCheckKnop: 'Set up',
  computerCheckKaartAan: 'The graphics card is on.',
  afstelBezigKop: 'Mondig is looking at your computer',
  afstelBezigHulp: 'This takes about a minute. You can keep working.',
  afstelStapComputer: 'Processor and memory',
  afstelStapKaart: 'Testing the graphics card',
  afstelStapModellen: 'Comparing models',
  afstelStapKlaarAria: 'Done',
  afstelStapBezigAria: 'In progress',
  afstelStoppen: 'Stop',
  afstelGestopt: 'Stopped.',
  afstelGestoptTest:
    'Stopped. The graphics card speed test cannot stop halfway: Mondig finishes it in the background (half a minute at most) and does not use the result.',
  afstelAdviesKop: 'This fits this computer better',
  afstelAdviesLijstAria: 'What Mondig suggests. Clear a tick to skip it.',
  afstelOvernemen: 'Apply advice',
  afstelNietNu: 'Not now',
  afstelNietsAangevinkt: 'Tick at least one item to apply.',
  afstelKaartTitel: 'Use the graphics card',
  afstelKaartUitleg: (keer: string): string => `Transcribing is about ${keer} times as fast.`,
  afstelKaartGoedTitel: 'The graphics card is already on',
  afstelKaartGoedUitleg: 'That is faster on this computer.',
  afstelKaartUitTitel: 'The graphics card stays off',
  afstelKaartUitUitleg: 'It is not faster than the processor here.',
  afstelDicterenTitel: 'The fast model for dictating',
  afstelDicterenUitleg: 'The fastest model: your text appears after half a second.',
  afstelDicterenGoedTitel: 'Dictating stays as it is',
  afstelBestandenTitel: 'Accurate model for recordings and files',
  afstelBestandenUitleg: 'Fewer mistakes in reports.',
  afstelBestandenGoedTitel: 'Recordings and files stay as they are',
  afstelDicterenGoedUitleg: 'The fast model already fits best.',
  afstelBestandenGoedUitleg: 'The accurate model already fits best.',
  afstelBestandenGoedUitlegEen: 'The dictating model also fits best for files here.',
  afstelKaartNietGetestTitel: 'The graphics card was not tested',
  afstelKaartNietGetestUitleg: (reden: string): string =>
    `Mondig could not test the graphics card: ${reden}. Try again later.`,
  afstelKaartFaaldeUitleg:
    'The graphics card gave an error during the test. Mondig uses the processor. Try again later.',
  afstelGeenRuimteBeide: 'Not enough space for both models; free up space or choose one.',
  afstelGeenRuimteEen: 'Not enough space for this model; free up space.',
  afstelTestRondtAf:
    'Mondig is still finishing the previous speed test. Try again in half a minute.',
  afstelRolSnel: 'The fast model',
  afstelRolNauwkeurig: 'The accurate model',
  afstelRolModel: 'The model',
  afstelDownloaden: (grootte: string): string => `Download: ${grootte}.`,
  afstelStaatEr: 'Already on your computer.',
  afstelLoopt: 'Already downloading.',
  afstelOvergenomenKop: 'Applied',
  afstelOvergenomenDownload: (welk: string): string =>
    `${welk} is downloading; until it is done, Mondig keeps using the current model.`,
  afstelOvergenomenDicteren: 'Mondig now dictates with the fast model.',
  afstelOvergenomenBestanden:
    'Mondig now transcribes recordings and files with the accurate model.',
  afstelOvergenomenNiets: 'Nothing changed.',
  afstelGoedKop: 'Everything is already fine',
  afstelGoedHulp: (datum: string): string =>
    `Mondig looked at your computer and found nothing that could be better. Checked on ${datum}.`,
  afstelSluiten: 'Close',
  groepProblemen: 'Troubleshooting',
  logboekKop: 'Log',
  logboekHulp:
    'Mondig keeps a log of events and errors on this computer, without what you dictate or transcribe. Nothing is sent automatically: you include it yourself when you ask for help.',
  logboekKopieren: 'Copy log',
  logboekGekopieerd: 'The log is on your clipboard.',
  logboekKopieerMislukt: 'Copying did not work. Open the log folder and send the file.',
  logmapOpenen: 'Open log folder',
  terugzetKop: 'Reset Mondig',
  terugzetHulp: 'Puts all settings back to the default. Your models stay.',
  terugzetKnop: 'Reset',
  terugzetVraag: 'Reset all settings to the default?',
  terugzetBevestig: 'Yes, reset',
  terugzetKlaar: 'Everything is back to the default.',
  terugzetSneltoetsBezet:
    'The default shortcut is already used by another program; your old shortcut keeps working.',
  klembordKop: 'Only put it on the clipboard',
  klembordHulp:
    'Do not paste for you. Handy when pasting into a program fails; you paste with Ctrl+V.',
  stopwoordenKop: 'Remove filler words',
  stopwoordenHulp:
    "Removes sounds like 'um' and 'uh'. Also applies to transcripts of files and recordings.",
  opgeslagen: 'Saved.',

  // Dictation on other screens
  stripVerwerkt: 'Mondig is transcribing…',
  stripKlaar: 'Done, the text is on the home screen',
  stripNaarStart: 'Go to the home screen',
  annulerenMetEsc: 'Cancel (Esc)',

  // Sidebar and start screen (ADR 0014)
  hoofdmenu: 'Main menu',
  menuBestanden: 'Files',
  menuVergadering: 'Record',
  lokaalRegel: 'Everything stays on this PC. Only downloading a model goes over the internet.',
  dicterenRustUitleg:
    'Hold and speak, or tap briefly to start and stop. The text appears where you type.',
  dicterenGeenToets: 'Once there is a shortcut, you can dictate with it in any program.',
  pilStoppen: 'Stop and transcribe',
  pilLokaal: 'Local',
  pilUitschrijven: 'Transcribing…',
  pilGeplakt: 'Pasted',
  pilDuur: (seconden: number): string =>
    `${new Intl.NumberFormat('en', { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(seconden)} s`,
  pilWaarschijnlijk: 'Pasted, check that the text is there',
  pilOpKlembord: 'On the clipboard, paste with Ctrl+V',
  pilGeenTekst: 'No text recognised',
  pilGeenGeluid: 'No sound heard',
  pilVerdacht: 'Hardly any speech heard, check the text',
  pilRest: (seconden: number): string => `${String(seconden)} s left, then dictation stops`,
  pilPlakMislukt: 'Pasting did not work in this window',
  pilPlakBeheer: 'This window runs as administrator, paste the text yourself',
  pilPlakOnzeker: 'Not sure the text was pasted',
  pilGekopieerd: 'Copied, paste with Ctrl+V',
  plakInMondig: 'The text is below, because Mondig was in front when it was ready.',
  pilInMondig: 'The text is in Mondig',
  pilSluiten: 'Close',
  instellingOpenen: 'Open setting',
  mondigOpenen: 'Open Mondig',
  microfooninstellingOpenen: 'Open microphone settings',

  // Settings: notice, sound and startup (sprint 3d)
  positieKop: 'Place of the dictation bar',
  positieHulp: 'Where the dictation bar appears while you speak.',
  positieOnder: 'Bottom of the screen (default)',
  positieBoven: 'Top of the screen',
  geluidKop: 'Sound when starting and stopping',
  geluidHulp: 'A short, soft sound when Mondig starts and stops listening.',
  geluidVolumeKop: 'Volume of that sound',
  autostartKop: 'Start when signing in',
  autostartHulp: 'Mondig starts when you sign in to Windows, ready to dictate.',
  verborgenKop: 'Start minimized',
  verborgenHulp: 'In the system tray, without a window.',
  // About Mondig (ADR 0016)
  overMondig: 'About Mondig',
  overUitleg: 'By Aigenwijs. What is new, licences and privacy.',
  openen: 'Open',
  sluiten: 'Close',
  doorAigenwijs: 'by Aigenwijs',
  voetAria: (versie: string): string => `About Mondig, version ${versie}`,
  versieRegelTekst: (versie: string): string => `Version ${versie}`,
  bouwRegelTekst: (bouw: string, datum: string): string => `Build ${bouw}, ${datum}`,
  versieKopieren: 'Copy version details',
  versieGekopieerd: 'Version details copied',
  versieKopierenUitleg: 'No personal data: version, build, Windows, processor and model.',
  versieLaden: 'Loading version details…',
  versieFout: 'Mondig could not read its version details. Restart Mondig.',
  nieuwKop: 'What is new',
  nieuwNogNiet: 'Test version: this is everything built since the previous release.',
  nieuwVersie: (versie: string): string => `Changes in version ${versie}.`,
  nieuwGeen: 'The changelog has no changes for this version.',
  alleWijzigingenTonen: 'Show all changes',
  minderTonen: 'Show fewer',
  eerdereUitgaven: 'Earlier releases',
  nieuwRegelKop: (versie: string): string => `New in ${versie}:`,
  nieuwRegelLezen: 'Read more',
  modellenKop: 'Speech models',
  modellenUitleg:
    'Mondig does not contain the speech models. The app lets you download them, in a converted form by handy-computer. Here is who made them originally, under which licence, and what was changed.',
  modellenFout: 'Mondig could not read the details of the speech models.',
  hulpmodel: 'helper model',
  vermMaker: 'Maker',
  vermCopyright: 'Copyright',
  vermGeenCopyright: 'The source gives no copyright line',
  vermOorspronkelijk: 'Original model',
  vermLicentie: 'Licence',
  vermDisclaimer: 'Warranty disclaimer',
  vermDisclaimerTekst: 'What the licence says about warranty and liability',
  vermGewijzigd: 'Changed',
  vermOmgezet: 'Converted version',
  adresOpenenMislukt: 'Opening the address failed. Copy it and open it in your browser.',
  onderdelenKop: 'Components by others',
  onderdelenUitleg:
    'Mondig is built with open-source components, including the fonts Sora and Geist (search for "font"). Each component shows its licence; open it for the full text.',
  onderdelenZoeken: 'Search by name or licence',
  onderdelenAantal: (getoond: number, totaal: number): string =>
    getoond === totaal
      ? `${String(totaal)} components`
      : `${String(getoond)} of ${String(totaal)} components`,
  onderdelenGeen: 'No component found.',
  onderdelenMeer: (n: number): string => `Show ${String(n)} more`,
  onderdelenNietAanwezig:
    'This build does not contain the list of components. It is made when the installer is built (npm run bundel).',
  onderdelenLaden: 'Loading components…',
  licentieTekstKop: 'Licence text',
  noticeKop: 'NOTICE of the component',
  licentieTekstOntbreekt: (licentie: string): string =>
    `The licence text is not in this list. Licence: ${licentie}.`,
  privacyKop: 'Privacy',
  privacyTekst: 'Mondig collects nothing and sends nothing on.',
  copyrightRegel: '© 2026 Aigenwijs',
  // Files (sprint 4-B)
  bestandenSleepKop: 'Drag a recording or video here',
  bestandenSleepUitleg:
    'A conversation, a Teams recording, a voice message. Several at once is fine.',
  bestandenLoslaten: 'Release, and Mondig starts',
  bestandenLoslatenUitleg: 'They are transcribed on this computer, one after the other.',
  bestandenKiezen: 'Choose files',
  bestandenMapOpenen: 'Open folder',
  bestandenUitschriftenOpenen: 'Open transcripts',
  bestandenKomtIn: (map: string): string => `The transcript goes to ${map}.`,
  bestandenOpslaanIn: (map: string): string => `Saving goes to ${map}.`,
  bestandenMeerToevoegen: 'Add more',
  bestandenLijstLabel: 'Files in the queue',
  bestandenDicterenGaatVoor: 'Dictation keeps working while this runs; a dictation goes first.',
  bestandWacht: 'waiting',
  bestandBezig: 'working',
  bestandProcent: (p: number): string => `${String(p)}%`,
  bestandProcentRest: (p: number, rest: string): string =>
    `${String(p)}%, ${rest.charAt(0).toLowerCase()}${rest.slice(1)}`,
  bestandKlaar: 'done',
  bestandKlaarIn: (duur: string): string => `written out in ${duur}`,
  bestandGeannuleerd: 'cancelled',
  duurSeconden: (s: number): string => `${String(s)} s`,
  duurMinutenSeconden: (m: number, s: number): string => `${String(m)} min ${String(s)} s`,
  bestandOpenen: 'Open',
  bestandOpenenAria: (naam: string): string => `Open ${naam}`,
  bestandAnnulerenKort: 'Cancel',
  bestandAnnuleren: (naam: string): string => `Cancel ${naam}`,
  bestandUitRijKort: 'Remove from queue',
  bestandUitRij: (naam: string): string => `Remove ${naam} from the queue`,
  bestandWeghalenKort: 'Remove',
  bestandWeghalen: (naam: string): string => `Remove ${naam}`,
  bestandProcentSchermlezer: (naam: string, p: number): string => `${naam}: ${String(p)} percent`,
  bestandKlaarSchermlezer: (naam: string): string => `${naam} has been transcribed.`,
  bestandFoutSchermlezer: (naam: string, melding: string): string => `${naam}: ${melding}`,
  terugNaarBestanden: 'Back to Files',
  formaatDocx: 'Word',
  formaatPdf: 'PDF for archiving',
  formaatOdt: 'LibreOffice',
  formaatTxt: 'Plain text',
  formaatMd: 'Markdown',
  formaatHtml: 'Web page',
  formaatSrt: 'Subtitles',
  formaatVtt: 'Subtitles for the web',
  bouwsteenDatum: 'Date',
  bouwsteenNaam: 'Name of the recording',
  bouwsteenLabel: 'Label',
  bouwsteenTijd: 'Time',
  bouwsteenModel: 'Model',
  mapDocumenten: 'Documents',
  mapGeen: 'No folder chosen yet',
  opslaanInKop: 'Folder',
  opslaanInHulp: 'For transcripts and reports.',
  mapKiezenAria: (map: string): string => `Save in ${map}. Choose another folder`,
  bestandsnaamKop: 'File name',
  bestandsnaamHulp:
    'Turn parts on or off. Drag them, or use Alt + left or right arrow, to set the order.',
  bestandsnaamPlek: (naam: string, plek: number, totaal: number): string =>
    `${naam} is now at position ${String(plek)} of ${String(totaal)}`,
  bestandsnaamVoorbeeld: (naam: string): string => `Example: ${naam}`,
  standaardformaatKop: 'Default format',
  standaardformaatHulp: 'What "Save" makes. Other formats via the arrow.',
  tijdenKop: 'Times in the document',
  tijdenHulp: 'Puts 00:07 before each sentence. Subtitles always have times.',
  automatischKop: 'Save automatically',
  automatischHulp: 'Every transcript goes straight into the folder, without asking.',

  // Watched folder (B-1)
  bewaakAanKop: 'Transcribe new recordings automatically',
  bewaakAanHulp:
    'Mondig watches this folder and transcribes new recordings, as long as Mondig is running. The recording itself stays as it was. Whatever is in the folder now stays untouched.',
  bewaakMapKop: 'Folder',
  bewaakMapHulp: 'For example the folder where Teams or OneDrive puts your recordings.',
  bewaakMapGeen: 'Choose a folder',
  bewaakMapKiezenAria: (map: string): string => `Watched folder: ${map}. Choose another folder`,
  bewaakSubmappenKop: 'Include subfolders',
  bewaakSubmappenHulp: 'Mondig then also looks in the folders inside this folder.',
  bewaakOverslaanKop: 'Skip if a transcript already exists',
  bewaakOverslaanHulp:
    'Mondig looks in your transcript folder and next to the recording for a transcript with the same name.',
  bewaakNaastKop: 'Put the transcript next to the recording',
  bewaakNaastHulp:
    'Instead of your transcript folder, the transcript goes in the same folder as the recording.',
  bewaakProbleemMapOntbreekt:
    'This folder cannot be found. Mondig watches it again as soon as it is back.',
  bewaakProbleemZelfdeAlsOpslag:
    'This is the folder where Mondig keeps your transcripts. Choose another folder to watch.',
  bewaakKiezenEerst: 'Choose a folder to watch first.',
  bestandUitMap: 'from the watched folder',
  bestandOvergeslagen: 'skipped, a transcript already exists',
  bewaakProbleemTeVeel:
    'This folder holds more files than Mondig can follow. Choose a smaller folder, or turn off subfolders.',
  wachtrijRest: (tijd: string, aantal: number): string =>
    `${tijd} for ${String(aantal)} ${aantal === 1 ? 'file' : 'files'}`,
  opslaanAls: (formaat: string): string => `Save as ${formaat}`,
  opslaanKort: 'Save',
  opslaanTitel: (formaat: string, extensie: string): string =>
    `Save as ${formaat} (${extensie}); other forms under the arrow`,
  opslaanMenu: 'Other format or place',
  opslaanAnderePlek: 'Save somewhere else…',
  opslaanBezig: 'Saving…',
  labelKop: 'Label',
  labelHulp: 'Goes into the file name and the document details.',
  labelVoorbeeld: 'For example a case number',
  opgeslagenKop: 'Saved',
  opgeslagenAlsIn: (naam: string, map: string): string => `as ${naam} in ${map}`,
  opgeslagenOpenen: 'Open',
  opgeslagenMapTonen: 'Show folder',
  bestaatAlKop: 'A file with this name already exists',
  opslaanAlsNummer: (nummer: string): string => `Save as ${nummer}`,
  vervangen: 'Replace',
  verwijderenKnop: 'Delete…',
  verwijderKop: 'Delete transcript?',
  verwijderMetBestanden: (namen: string): string =>
    `Mondig removes the transcript and deletes the files it saved: ${namen}. This cannot be undone.`,
  verwijderZonderBestanden: 'Mondig removes the transcript. No files were saved from it.',
  verwijderBevestig: 'Delete',
  bestandInMap: (naam: string, map: string): string => `${naam} in ${map}`,
  verwijderInGebruik: (namen: string): string =>
    `Not deleted, because it is open in another program: ${namen}. Close it and try again.`,
  verwijderMislukt: (namen: string): string => `Not deleted: ${namen}. Please try again.`,
  verwijderdMelding: (aantal: number): string =>
    aantal === 1
      ? 'Transcript and 1 file deleted.'
      : `Transcript and ${String(aantal)} files deleted.`,
  verwijderdZonderBestanden: 'Transcript removed.',
  verwijderGebleven: (namen: string): string =>
    `Kept, because changed or moved after saving: ${namen}.`,
  bestandOpgeslagenAls: (naam: string): string => `saved as ${naam}`,
  bestandOpgeslagenVerouderd: (naam: string): string =>
    `saved as ${naam}, without your latest changes`,
  bestandBewaarFout: (melding: string): string => `Saving automatically failed: ${melding}`,
  transcriptRegelsLabel: 'The transcript',
  transcriptGeenTekst: 'No text was recognised.',
  transcriptNietGevonden: 'This transcript is no longer available.',
  // Meeting (sprint 4b-C)
  sprekerJij: 'You',
  sprekerAnderen: 'Others',
  vergaderSoortLabel: 'Type of recording',
  vergaderOnline: 'Online',
  vergaderOnlineUitleg: 'Teams, Zoom or Meet. Your voice and the others are recorded separately.',
  vergaderRuimte: 'In the room',
  vergaderRuimteUitleg: 'Everyone in one room, with the microphone of this computer.',
  vergaderGeluidVan: 'Sound from',
  vergaderApparatenFout: 'Mondig could not get the speakers. The default speaker is used.',
  vergaderTipOnline:
    'Wear headphones. Then what the others say does not end up twice in the report.',
  vergaderTipRuimte: 'Put the laptop in the middle of the table.',
  vergaderStarten: 'Start recording',
  vergaderModelRegel: 'Will be written out on this computer.',
  vergaderHerinneringKop: 'Tell the others',
  vergaderHerinneringTekst:
    'Let everyone know you are recording and transcribing this meeting. That is the right thing to do, and often it is also required.',
  vergaderHerinneringStart: 'I have told them, start',
  vergaderStartBezig: 'Starting the recording…',
  vergaderLoopt: 'Recording',
  vergaderKlokAria: 'Recorded time',
  vergaderBronMicrofoon: 'Microphone',
  vergaderBronComputer: 'Sound from the computer',
  vergaderMetersLabel: 'Sound coming in',
  vergaderSysteemStil:
    'No sound has come in from the computer for a minute. Is the meeting playing through another device?',
  vergaderStopUitschrijven: 'Stop and write out',
  vergaderWeggooienKop: 'Discard the recording?',
  vergaderWeggooienTekst:
    'Mondig deletes the recording and everything recorded so far. You cannot undo this.',
  vergaderWeggooienBevestig: 'Yes, discard',
  vergaderAfronden: 'Mondig is finishing the recording…',
  vergaderHerstellen: 'Mondig is recovering the recording…',
  vergaderUitschrijvenKop: 'Writing out…',
  vergaderUitschrijvenStand: (stand: string): string => `Writing out… ${stand}`,
  vergaderUitschrijvenHulp:
    'You can keep using Mondig. The report appears here as soon as it is written out.',
  vergaderLater: 'Write out later',
  vergaderLaterHulp: 'The recording is then kept until you write it out or discard it.',
  vergaderingVan: (dag: string, tijd: string): string => `Meeting of ${dag}, ${tijd}`,
  vergaderMinuten: (n: number): string => (n === 1 ? '1 minute' : `${String(n)} minutes`),
  vergaderInDeRuimte: 'In-person meeting',
  vergaderEchoMelding:
    'Part of what the others said also came in through your microphone. That double text was left out. With headphones the report comes out best.',
  vergaderMeldingSluiten: 'Close message',
  vergaderBeurtenLabel: 'The report',
  vergaderNaamAria: (naam: string): string => `${naam}, change name`,
  vergaderNaamGeven: (kant: string): string => `Name for ${kant}`,
  vergaderNaamKop: 'Name',
  vergaderNaamVoorbeeld: 'For example Councillor De Vries',
  vergaderNaamLeeg: (kant: string): string => `Leave the field empty to go back to ${kant}.`,
  vergaderNaamBereikLabel: 'For which turns',
  vergaderNaamBeurt: 'Only this turn',
  vergaderNaamKant: (kant: string): string => `All turns of ${kant}`,
  vergaderSpreker: (nummer: number): string => `Speaker ${String(nummer)}`,
  vergaderNaamOpslaan: 'Save',
  vergaderNaamGegeven: (naam: string): string => `Name set: ${naam}.`,
  vergaderNaamTerug: (kant: string): string => `Back to ${kant}.`,
  // Recognising speakers (4b-D, phase D-2)
  vergaderNaamSprekerKop: 'Name of this speaker',
  vergaderNaamSprekerHulp: (standaard: string): string =>
    `Applies to all turns of this speaker. Leave the field empty to go back to ${standaard}.`,
  vergaderBeurtVanKop: 'This turn is by',
  vergaderBeurtVanAria: (naam: string): string => `Give this turn to ${naam}`,
  vergaderNieuweSpreker: 'New speaker',
  vergaderPlusSpreker: '+ Speaker',
  vergaderSamenvoegenKop: 'Is this the same person?',
  vergaderSamenvoegenHulp: (naam: string): string =>
    `Merge ${naam} with another speaker. All turns of ${naam} then become that speaker's.`,
  vergaderSamenvoegenMet: (naam: string): string => `Merge with ${naam}`,
  vergaderBeurtGegeven: (naam: string): string => `The turn now belongs to ${naam}.`,
  vergaderNieuweSprekerGegeven: 'The turn now belongs to a new speaker.',
  vergaderSamengevoegd: (van: string, naar: string): string =>
    `${van} and ${naar} have been merged.`,
  vergaderSprekersHerkennen: 'Recognising speakers…',
  vergaderSprekersSchermlezer: 'Mondig is now recognising the speakers.',
  sprekersKop: 'Recognise speakers',
  sprekersHulp: (grootte: string): string =>
    `Shows who is speaking. Needs a separate model (${grootte}). Recognises up to 4 speakers.`,
  sprekersHulpAanwezig: 'Shows who is speaking. Recognises up to 4 speakers.',
  sprekersCompactHulp: 'Who said what, up to 4 speakers.',
  sprekersCompactDownload: (grootte: string): string =>
    `Who said what, up to 4 speakers. Download ${grootte} first.`,
  sprekersVraagKop: 'Download the speaker model?',
  sprekersVraagTekst: (grootte: string): string =>
    `To recognise speakers Mondig needs a separate model (${grootte}). Mondig fetches it once over the internet; after that it works offline. You can delete it again on the Models page.`,
  sprekersDownloaden: (grootte: string): string => `Download (${grootte})`,
  sprekersBezig: 'Fetching the speaker model…',
  sprekersBezigStand: (stand: string): string => `Fetching the speaker model… ${stand}`,
  sprekersOpslaanFout: 'The setting could not be saved.',
  vergaderAudioBezig: 'Saving the recording…',
  vergaderAudioBewaard: (naam: string): string => `Recording saved as ${naam}.`,
  vergaderAudioBewarenMenu: 'Keep the recording as a sound file…',
  verslagSluiten: 'Close',
  verslagSluitenTitel: 'Back to Record. The report stays saved.',
  verslagNieuweOpname: 'New recording',
  verslagMenu: 'More',
  verslagSprekers: (aantal: number): string =>
    aantal === 1 ? '1 speaker' : `${String(aantal)} speakers`,
  verslagModelSnel: 'fast model',
  verslagModelNauwkeurig: 'accurate model',
  verslagModelExtraNauwkeurig: 'extra accurate model',
  opDezeComputer: 'on this computer',
  klaarKop: 'Your report is ready',
  klaarGesprek: (minuten: number): string =>
    `${String(minuten)} ${minuten === 1 ? 'minute' : 'minutes'} of conversation`,
  klaarUitgeschreven: (minuten: number): string =>
    `written out in ${String(minuten)} ${minuten === 1 ? 'minute' : 'minutes'}`,
  klaarUitgeschrevenKort: 'written out in under a minute',
  klaarLokaal: 'Everything stayed on this computer.',
  vergaderWachtendLabel: 'Recordings still waiting',
  vergaderWanneer: (dag: string, tijd: string): string => `${dag}, ${tijd}`,
  vergaderDagVandaag: 'today',
  vergaderDagGisteren: 'yesterday',
  vergaderAfgebroken: (wanneer: string, duur: string | null): string =>
    `There is a recording from ${wanneer}${duur === null ? '' : ` (${duur})`} that was not closed properly. Mondig can recover it up to just before things went wrong.`,
  // Dictation history (ADR 0021): the list on Dictate and the setting
  dictatenKop: 'Your dictations',
  dictatenBewaard: (termijn: string): string => `Kept for ${termijn}`,
  dictatenTermijnNiet: "Don't keep",
  dictatenTermijn24u: '24 hours',
  dictatenTermijn7d: '7 days',
  dictatenMenu: 'More options for your dictations',
  dictatenHoeLang: 'How long to keep…',
  dictatenAllesWissen: 'Delete all…',
  dictatenAllesWissenKop: 'Delete all dictations?',
  dictatenWisVraag: (aantal: number): string =>
    aantal === 1
      ? 'Your saved dictation will be deleted.'
      : `Your ${String(aantal)} saved dictations will be deleted.`,
  dictatenWisBevestig: 'Delete all',
  dictatenNietBewarenKop: 'Stop keeping dictations?',
  dictatenNietBewarenBevestig: 'Delete and stop keeping',
  dictatenDagVandaag: 'Today',
  dictatenDagGisteren: 'Yesterday',
  dictatenKopieerAria: (tijd: string): string => `Copy dictation from ${tijd}`,
  dictatenVerwijderAria: (tijd: string): string => `Delete dictation from ${tijd}`,
  dictatenGekopieerd: 'Copied',
  dictatenVerwijderd: 'Dictation deleted',
  dictatenAlleGewist: 'All dictations deleted',
  dictatenLeegKop: 'What you dictate will appear here',
  dictatenLeegTekst: (termijn: string): string =>
    `Dictate in Word, Outlook or your browser. What you say stays here for ${termijn}, so you can find it if pasting did not work.`,
  dictatenNietBewaard:
    'Mondig does not keep your dictations; this disappears when you close Mondig.',
  dictatenWijzigen: 'Change',
  dictatenBewarenKop: 'Keep dictations',
  dictatenBewarenHulp: 'So you can find them on Dictate. Only on this computer.',

  // Dictaten opschonen (ADR 0027): Instellingen, de tweede sneltoets, de pil en de geschiedenis
  opschonenKop: 'Clean up dictations',
  opschonenKeuzeUit: 'Off',
  opschonenKeuzeSneltoets: 'With its own shortcut',
  opschonenKeuzeElk: 'For every dictation',
  opschonenUitleg: 'A language model turns your dictation into flowing sentences with punctuation.',
  opschonenUitlegNaar: (naam: string, label: string): string =>
    `A language model turns your dictation into flowing sentences with punctuation. Your text goes to ${naam} (${label}). Names and other personal data are not replaced. The word-for-word text stays on this computer.`,
  opschonenUitlegLokaal:
    'A language model turns your dictation into flowing sentences with punctuation. Your text stays on this computer (Local). Only quick with a good graphics card.',
  opschonenGeenAanbieder: 'First choose a language model under Minutes.',
  opschonenNaarNotulen: 'Go to Minutes',
  opschonenVraagTekst: (naam: string, label: string): string =>
    `Your dictations go to ${naam} (${label}) to be cleaned up. Names and other personal data are not replaced. The word-for-word text stays on this computer.`,
  opschonenAanzetten: 'Turn on',
  opschonenLabel: (label: string): string =>
    ({ lokaal: 'Local', europees: 'European', buiten_europa: 'Outside Europe' })[label] ?? label,
  opschoonToetsKop: 'Shortcut for cleaning up',
  opschoonToetsHulp:
    'This key gives you a cleaned-up dictation. The normal shortcut gives the word-for-word text.',
  opschoonToetsIngesteld: (toets: string): string =>
    `The shortcut for cleaning up is now ${toets}.`,
  opschoonToetsDubbel: 'This is already your shortcut for dictating. Choose another key.',
  opschoonToetsBezet: (toets: string): string =>
    `${toets} is already used by another program. Choose another key.`,
  opschoonToetsOpDicteren: 'cleaned up',
  opschoonToetsOpDicterenAria: (toets: string): string => `${toets}: cleaned up`,
  sneltoetsDubbelMetOpschonen: 'This is already your shortcut for cleaning up. Choose another key.',
  pilOpschonen: 'Cleaning up',
  pilOpschonenStatus: (label: string): string => `Cleaning up (${label})`,
  pilOpschonenStoppen: 'Stop cleaning up, paste the word-for-word text',
  pilNietOpgeschoond: (soort: string): string =>
    ({
      geplakt: 'Not cleaned up, pasted word for word',
      waarschijnlijk: 'Not cleaned up, pasted word for word. Check that the text is there',
      in_mondig: 'Not cleaned up, the word-for-word text is in Mondig'
    })[soort] ?? 'Not cleaned up, word for word on the clipboard. Paste with Ctrl+V',
  dictatenOpgeschoondDoor: (model: string, label: string): string =>
    `Cleaned up by ${model} (${label})`,
  dictatenOpgeschoondDoorZonderLabel: (model: string): string => `Cleaned up by ${model}`,
  dictatenLetterlijkKopieren: 'Copy word-for-word text',
  dictatenLetterlijkKopierenAria: (tijd: string): string =>
    `Copy the word-for-word text of the dictation from ${tijd}`,
  dictatenLetterlijkGekopieerd: 'Word-for-word text copied',
  bewaarKop: 'Delete recordings and reports after',
  bewaarHulp: 'What you save as a file stays in your folder.',
  bewaar1: '1 day',
  bewaar7: '7 days',
  bewaar30: '30 days',
  vergaderOnzeker:
    'Mondig cannot tell whether this recording is still running. If Mondig is open somewhere else, close that first.',
  vergaderHerstelUitschrijven: 'Recover',
  vergaderUitschrijvenKnop: 'Write out',
  vergaderLijstWeggooienVraag: 'Discard this recording? You cannot undo this.',
  vergaderLijstKnopAria: (knop: string, wanneer: string): string =>
    `${knop}, recording from ${wanneer}`,
  vergaderGestartSchermlezer: 'The recording has started.',
  vergaderUitschrijvenSchermlezer: 'The recording has stopped. Mondig is writing it out now.',
  vergaderProcentSchermlezer: (p: number): string => `Writing out: ${String(p)} percent`,
  vergaderKlaarSchermlezer: 'The report is ready.',
  vergaderFoutSchermlezer: (melding: string): string => `Error: ${melding}`,
  vergaderWeggegooidSchermlezer: 'The recording has been discarded.',
  vergaderLaterSchermlezer: 'Writing out has stopped. The recording is kept.',
  vergaderGeslotenSchermlezer: 'The report is closed. You will find it under Recorded earlier.',
  // Record (phase UX-D): the type Just me, the start screen and the list still waiting
  vergaderAlleen: 'Just me',
  vergaderAlleenUitleg: 'Speak your thoughts. Mondig writes it out afterwards.',
  notitieVan: (dag: string, tijd: string): string => `Note of ${dag}, ${tijd}`,
  vergaderTestgeluid: 'Play test sound',
  vergaderTestgeluidBezig: 'Playing test sound…',
  vergaderTestgeluidFout: 'The test sound did not play.',
  vergaderNogUitTeSchrijven: 'Still to write out',
  vergaderWachtendWistRegel: (dagen: number): string =>
    `Mondig deletes them after ${String(dagen)} ${dagen === 1 ? 'day' : 'days'}.`,
  // From one hour on: "1 h 4 min", otherwise "52 min".
  vergaderMinutenKort: (n: number): string => {
    if (n < 60) return `${String(n)} min`;
    const rest = n % 60;
    return rest === 0
      ? `${String(n / 60)} h`
      : `${String(Math.floor(n / 60))} h ${String(rest)} min`;
  },
  vergaderOnderbroken: 'interrupted',
  vergaderWistKortVandaag: 'deleted today',
  vergaderWistKortMorgen: 'deleted tomorrow',
  vergaderWistOp: (moment: string): string => `Will be deleted automatically on ${moment}.`,
  vergaderAlleTonen: (n: number): string => `Show all ${String(n)}`,
  vergaderMinderTonen: 'Show fewer',
  // Saving reports and Recorded earlier (N-C3, ADR 0022)
  vergaderEerderOpgenomen: 'Recorded earlier',
  vergaderEerderLabel: 'Meetings and notes recorded earlier',
  verslagAlleenIk: 'just me',
  vergaderDagMorgen: 'tomorrow',
  vergaderBewaartRegel: (dagen: number): string =>
    `Mondig keeps them for ${String(dagen)} ${dagen === 1 ? 'day' : 'days'}.`,
  vergaderBewaartWijzigen: 'Change',
  verslagMeerAria: (titel: string): string => `More for ${titel}`,
  verslagMeer: 'More',
  verslagWissenMenu: 'Delete report…',
  verslagWissenKop: 'Delete the report?',
  verslagWissenTekst:
    'Mondig deletes the report and the recording. What you saved as a file stays in your folder. You cannot undo this.',
  verslagWissenBevestig: 'Yes, delete',
  verslagGewistSchermlezer: 'The report is deleted.',
  verslagTochSluiten: 'Close anyway',
  verslagNaamWijzigen: 'Rename',
  verslagNaamLabel: 'Name of the report',
  verslagNaamWijzigenAria: (titel: string): string => `Rename, ${titel}`,
  verslagNaamGegeven: (naam: string): string => `The report is now called ${naam}.`,
  verslagNaamTerug: 'The report has the title Mondig makes again.',
  verslagBewaardTot: (dag: string): string => `Mondig keeps this report until ${dag}`,
  vergaderMeerMenuNu: 'More for this recording',
  vergaderVerwijderen: 'Delete',
  vergaderVerwijderAria: (wanneer: string): string => `Delete recording from ${wanneer}`,
  vergaderOokNog: (
    aantal: number,
    soort: string,
    wist: 'vandaag' | 'morgen' | null,
    aantalWist: number
  ): string => {
    const basis =
      aantal === 1
        ? `1 more recording under ${soort}`
        : `${String(aantal)} more recordings under ${soort}`;
    if (wist === null) return basis;
    const dag = wist === 'vandaag' ? 'today' : 'tomorrow';
    if (aantal === 1) return `${basis}, deleted ${dag}`;
    return `${basis}, ${String(aantalWist)} deleted ${dag}`;
  },
  vergaderAudioNaast: (naam: string): string => `The recording is next to it as ${naam}.`,
  vergaderAudioNaastFout: (melding: string): string =>
    `The report was saved, but the recording was not: ${melding}`,
  audioNaastKop: 'Also save the recording as an audio file',
  audioNaastHulp:
    'When you save a report, the recording goes next to it as a WAV, with the same name.',
  mapOpenenKort: 'Open',
  mapOpenenAria: (map: string): string => `Open ${map} in File Explorer`,
  vergaderOpnameWeggooien: 'Discard recording…',

  // Models (sprint 5-A)
  menuModellen: 'Models',
  modellenTitel: 'Models',
  modellenPaginaUitleg:
    'The model is the part of Mondig that turns speech into text, on this computer. At the top you see what Mondig uses now; below you manage the models.',
  modellenLaadFout: 'Mondig could not load the models. Try again.',
  taakDicteren: 'Dictating',
  taakBestanden: 'Recordings and files',
  wijzigen: 'Change',
  watMondigGebruiktKop: 'What Mondig uses',
  functieDicterenWaarom: 'Speed matters here.',
  functieBestandenWaarom: 'Accuracy matters here.',
  functieSprekersWaarom: 'Who said what in a meeting.',
  modelRolNaam: (rol: 'snel' | 'nauwkeurig' | 'extra_nauwkeurig' | null): string =>
    rol === 'snel'
      ? 'Fast model'
      : rol === 'nauwkeurig'
        ? 'Accurate model'
        : rol === 'extra_nauwkeurig'
          ? 'Extra accurate model'
          : 'Model',
  sprekermodelNaam: 'Speaker model',
  pastBijComputer: 'Suits this computer',
  gecontroleerdOp: (datum: string): string => `Checked on ${datum}`,
  opnieuwControlerenComputer: 'Check again',
  laatComputerBekijken: 'Let Mondig look at your computer',
  functieWijzigenAria: (functie: string): string => `Change model for ${functie}`,
  functieHuidig: 'In use',
  functieEerstDownloaden: (grootte: string): string => `Download ${grootte} first.`,
  functieWordtGedownload: 'Already downloading.',
  functieTeWeinigRuimte: (grootte: string): string => `Not enough space: ${grootte} more needed.`,
  kaartVolgt: (rol: string): string => `${rol} follows when the download is done.`,
  sprekersModelAanwezig: 'On this computer. Recognises up to 4 speakers.',
  sprekersModelNiet: (grootte: string): string =>
    `Not downloaded yet (${grootte}). Recognises up to 4 speakers.`,
  naHervattenGebruik: (dicteren: boolean, opnames: boolean): string =>
    dicteren && opnames
      ? 'Will be used for dictation and recordings when the download is done.'
      : dicteren
        ? 'Will be used for dictation when the download is done.'
        : 'Will be used for recordings when the download is done.',
  kaartDicterenSub: (tijd: string): string => `Your text appears ${tijd}.`,
  kaartBestandenSub: (tijd: string): string => `An hour of recording takes ${tijd}.`,
  kaartOnbekend: 'This model is not on your computer yet.',
  groepOpDezeComputer: 'On this computer',
  groepTeDownloaden: 'Available to download',
  balkNauwkeurig: 'Accurate',
  balkSnel: 'Fast',
  balkStappen: (naam: string, stap: number): string => `${naam}: ${String(stap)} of 5`,
  balkUitlegKnop: 'About the bars',
  balkUitleg:
    'Measured on read-aloud Dutch text, on the processor. On your computer it can be different.',
  talenAantal: (aantal: number, turksArabisch: boolean): string =>
    turksArabisch
      ? `${String(aantal)} languages, also Turkish and Arabic`
      : `${String(aantal)} languages`,
  labelAanbevolen: 'Recommended',
  labelSneller: 'Faster',
  labelNauwkeuriger: 'More accurate',
  labelTeTraag: 'Too slow',
  labelNieuw: 'New',
  labelLokaal: 'Local',
  inGebruikVoorDicteren: 'in use for dictation',
  inGebruikVoorBestanden: 'in use for recordings',
  inGebruikVoorBeide: 'in use for dictation and recordings',
  gedeeltelijkGedownload: (binnen: string, totaal: string): string =>
    `${binnen} of ${totaal} is already downloaded`,
  verderDownloadenGrootte: (rest: string): string => `Continue downloading (${rest} left)`,
  stoppenMetDownloaden: 'Stop downloading',
  verwijderen: 'Delete',
  verwijderenBevestigen: (grootte: string): string => `Delete? This frees ${grootte}.`,
  nietWeg:
    'You are still using this model. Choose another model first; otherwise Mondig cannot transcribe anything.',
  eenDownloadTegelijk: 'A download is already running. Start this model when it is finished.',
  ruimteTekortKop: 'There is not enough space for this model',
  ruimteTekortTekst: (nodig: string, vrij: string): string =>
    `Mondig needs ${nodig} and ${vrij} is free. Free up space on this computer, or delete a model you do not use, and check again.`,
  ruimteTekortOnbekend:
    'There is not enough space on this computer. Free up space, or delete a model you do not use, and check again.',
  downloadMisluktKop: 'The download did not work',
  modellenTotaalVoor: 'Models take up ',
  modellenTotaalNa: ' together on this computer.',
  modelBezigOphalen: 'Mondig is fetching the model…',
  zijbalkDownload: (naam: string): string => `Downloading ${naam}`,
  zijbalkProcent: (n: number): string => `${String(n)}%`,
  zijbalkDownloadAria: (naam: string, n: number): string =>
    `Downloading ${naam}, ${String(n)} percent. Open Models.`,
  wordtGestart: 'Starting…',
  hervatten: 'Resume',
  nietHervatten: 'Do not resume',
  downloadWacht: 'Waiting for the download before it to finish.',
  downloadNieuwePoging: (poging: number, van: number): string =>
    `The connection dropped. Mondig is trying again by itself (attempt ${String(poging)} of ${String(van)})…`,
  downloadOnderbroken: 'Interrupted when Mondig was closed. What came in is still there.',
  zijbalkDownloadsKop: 'Downloads',
  zijbalkWacht: 'Waiting',
  zijbalkOpnieuw: (poging: number, van: number): string =>
    `Retrying (${String(poging)} of ${String(van)})`,
  zijbalkKlaar: 'Done',
  zijbalkMislukt: 'Failed',
  zijbalkGestopt: 'Stopped',
  zijbalkOnderbroken: 'Interrupted',
  zijbalkItemAria: (naam: string, stand: string): string => `${naam}: ${stand}. Open Models.`,
  zijbalkIngeklaptAria: (n: number): string =>
    n === 1 ? '1 download. Open Models.' : `${String(n)} downloads. Open Models.`,
  sprekersWordtGestart: 'Starting the download of the speaker model…',
  sprekersWacht: 'The speaker model is in the queue and comes after the download before it.',
  sprekersAangezet: 'The speaker model is on your computer. Recognise speakers is on.',
  instellingModellenKop: 'Models',
  instellingModellenHulp: (verschillend: boolean): string =>
    verschillend
      ? 'Dictation with the fast model, recordings and files with the accurate one.'
      : 'Dictation, recordings and files with the same model.',
  instellingModellenKies: 'Choose, download and delete.',
  instellingModellenKnop: 'Go to Models',
  modelTekst: (id: string): ModelTekst | null => {
    switch (id) {
      case 'parakeet-ultra-q8':
        return {
          naam: 'Parakeet Ultra',
          zin: 'Fast and reliable. The best choice for dictating: your text appears after half a second.',
          dicterenTijd: 'after half a second',
          uurOpname: 'about 4 minutes'
        };
      case 'canary-1b-v2-q8':
        return {
          naam: 'Canary',
          zin: 'Transcribes more accurately than Parakeet Ultra, in about three times as long. A good balance for files.',
          dicterenTijd: 'after about 2 seconds',
          uurOpname: 'about 10 minutes'
        };
      case 'whisper-large-v3-turbo-q8':
        return {
          naam: 'Whisper turbo',
          zin: 'The most accurate, and the only one for Turkish and Arabic. Too slow to dictate with, and slow without a fast graphics card.',
          dicterenTijd: 'after about 12 seconds',
          uurOpname: 'about 26 minutes',
          extra: 'For Turkish, Arabic and languages outside the 25. Can only be chosen for files.'
        };
      case 'sortformer-4spk-v2.1-q8':
        return {
          naam: 'Sortformer',
          zin: 'Recognises who is speaking, up to four people per recording. Needs a separate model, next to the speech model.',
          extra:
            'By NVIDIA, under the NVIDIA Open Model License. The licence is under About Mondig, under Speech models.'
        };
      default:
        return null;
    }
  },
  // Editing, listening to and searching the report (stream V, V-1)
  verslagHint: 'Click a line to correct its text.',
  verslagHintAfspelen: 'Click a line to correct its text, or the time to listen.',
  verslagRegelVeld: (tijd: string): string => `Text of the line at ${tijd}`,
  verslagVeldHulp: 'Enter saves, Shift+Enter starts a new line, Esc undoes.',
  verslagRegelBewaard: 'The line has been changed.',
  verslagAfspelenVanaf: (tijd: string): string => `Play from ${tijd}`,
  verslagPauzeren: (tijd: string): string => `Pause, line at ${tijd}`,
  verslagAfspelenMislukt: 'Playback did not work. Try again.',
  verslagLaden: 'Getting the sound ready.',
  verslagGeenGeluidOpname: 'Playback is not possible: the recording has been deleted.',
  verslagGeenGeluidBestand: 'Playback is not possible: the source file was moved or deleted.',
  verslagSnelheidKop: 'Speed',
  verslagSnelheidTitel: 'Playback speed',
  verslagWerkbalk: 'Report tools',
  verslagSnelheid: (tienden: number): string =>
    tienden === 15 ? '1.5×' : `${String(tienden / 10)}×`,
  verslagSnelheidAria: (tienden: number): string =>
    tienden === 15 ? 'Speed 1.5 times' : `Speed ${String(tienden / 10)} times`,
  verslagZoeken: 'Search',
  verslagZoekenTitel: 'Search the report (Ctrl+F)',
  verslagZoekVeld: 'Search the report',
  verslagZoekTeller: (nu: number, totaal: number): string => `${String(nu)} of ${String(totaal)}`,
  verslagZoekGeen: 'No matches',
  verslagZoekVolgende: 'Next match',
  verslagZoekVorige: 'Previous match',
  verslagZoekSluiten: 'Close search',

  // Marking and replacing personal data (stream V, V-2)
  persoonSwitch: 'Replace personal data when copying and saving',
  persoonGeen: 'Nothing has been marked.',
  persoonWeghalen: (tekst: string): string => `Remove the mark from ${tekst}`,
  persoonWeghalenTitel: 'Remove the mark',
  persoonTeller: (aantal: number): string =>
    aantal === 0
      ? 'No personal data'
      : `${String(aantal)} ${aantal === 1 ? 'piece of personal data' : 'pieces of personal data'}`,
  persoonStandUit: 'Not replaced',
  persoonStandUitRest: 'when copying and saving',
  persoonStandAan: 'Replaced',
  persoonStandAanRest: 'by [BSN] and so on, when copying and saving',
  persoonUitlegKnop: 'How does this work?',
  persoonUitleg1:
    'Mondig marks data with a fixed form: BSN, IBAN, phone number, email address, address, date and licence plate. You mark names yourself.',
  persoonUitleg2:
    'When the switch is on, copying and saving put [BSN] or [TELEFOON] in that place. The recording is then not saved with it.',
  persoonUitleg3:
    'Numbers written as words (such as "zero six") and spoken email addresses (such as "at sign") are not recognised. So always read the report yourself. What Mondig saves automatically contains the original text.',
  persoonGevondenKop: (aantal: number): string =>
    aantal === 0 ? 'Nothing found' : `${String(aantal)} found`,
  persoonGevondenHulp:
    'Mondig does not find everything. Read the report yourself and mark what is missing.',
  persoonKeerVoor: (aantal: number): string => `${String(aantal)} times`,
  // Opnieuw uitschrijven (N-C4, ADR 0023)
  opnieuwMenu: 'Write out again…',
  opnieuwGewistMenu: 'Write out again',
  opnieuwGewistReden: 'No longer possible: the recording was deleted.',
  opnieuwKop: 'Write out again',
  opnieuwUitleg: (rol: string | null): string =>
    rol === null
      ? 'Choose a model; the new report replaces this one.'
      : `This report was made with the ${rol} model. Choose a model; the new report replaces this one.`,
  opnieuwRol: (rol: string): string =>
    ({ snel: 'fast', nauwkeurig: 'accurate', extra_nauwkeurig: 'extra accurate' })[rol] ?? rol,
  opnieuwKeuzeNaam: (rol: string): string =>
    rol === 'extra_nauwkeurig' ? 'Extra accurate model' : 'Accurate model',
  opnieuwVoordeel: (rol: string): string =>
    rol === 'extra_nauwkeurig' ? 'The fewest mistakes.' : 'Fewer mistakes.',
  opnieuwDuur: (minuten: number, opname: number): string =>
    `Takes about ${String(minuten)} ${minuten === 1 ? 'minute' : 'minutes'} for these ${String(opname)} ${opname === 1 ? 'minute' : 'minutes'}.`,
  opnieuwDuurOnbekend: (rol: string | null): string =>
    rol === null ? 'Takes longer than the fast model.' : `Takes longer than the ${rol} model.`,
  opnieuwOntbreekt: (grootte: string): string =>
    `Not on this computer yet. Download first: ${grootte}.`,
  opnieuwNaarModellen: 'Go to Models',
  opnieuwLaden: 'Mondig is checking which models are available…',
  opnieuwGeenKeuze: 'There is no more accurate model to write this report out with.',
  opnieuwBlijftSprekers: 'Speakers and the names you gave stay.',
  opnieuwBlijftMarkeringen: 'What you marked yourself stays marked.',
  opnieuwVerbeterd: (aantal: number): string =>
    aantal === 1
      ? 'You corrected 1 line. It will be written out again; afterwards you can go back to this version.'
      : `You corrected ${String(aantal)} lines. They will be written out again; afterwards you can go back to this version.`,
  opnieuwStart: 'Write out again',
  opnieuwBezig: (rol: string | null): string =>
    rol === null ? 'Writing out again…' : `Writing out again with the ${rol} model…`,
  opnieuwBezigHulp: 'You can make corrections again when it is done.',
  opnieuwStoppen: 'Stop',
  opnieuwGestopt: 'Writing out again has stopped. The report has not changed.',
  opnieuwKlaar: (rol: string | null, veranderd: number): string => {
    const met = rol === null ? 'Written out again.' : `Written out again with the ${rol} model.`;
    if (veranderd === 0) return `${met} No lines changed.`;
    return veranderd === 1
      ? `${met} 1 line changed.`
      : `${met} ${String(veranderd)} lines changed.`;
  },
  opnieuwVerdwenen: (aantal: number): string =>
    aantal === 1
      ? '1 marking is no longer in the text; check the report.'
      : `${String(aantal)} markings are no longer in the text; check the report.`,
  opnieuwTerugzetten: 'Restore previous version',
  opnieuwTeruggezet: 'The previous version is back.',
  opnieuwVeranderdAria: 'changed when written out again',
  persoonZelfKop: 'Mark something yourself',
  persoonZelfHulp:
    'Type or select a piece of text, for example a name. It is marked everywhere in the report.',
  persoonZelfVeld: 'Text to mark',
  persoonZelfKnop: 'Mark',
  persoonWeggehaald: 'The mark has been removed.',
  persoonGemarkeerd: 'The text has been marked.',
  persoonSoort: (soort: string): string =>
    ({
      bsn: 'BSN',
      iban: 'IBAN',
      telefoon: 'Phone',
      email: 'Email',
      adres: 'Address',
      datum: 'Date',
      kenteken: 'Licence plate',
      handmatig: 'Marked by you'
    })[soort] ?? soort,
  persoonSoortAria: (soort: string): string => `marked as ${soort}`,

  // Minutes (T3, ADR 0024 to 0026): Settings and the Minutes tab of the transcript
  groepNotulen: 'Minutes',
  notulenKop: 'Make minutes with',
  notulenHulp:
    'Choose where the text of a transcript goes. Nothing set: then Mondig does not make minutes.',
  notulenKeuzeGeen: 'Not set',
  notulenKeuzeMistral: 'Mistral (European)',
  notulenKeuzeOllama: 'Ollama on this computer',
  notulenSleutelKop: 'Mistral key',
  notulenSleutelHulp:
    'Mondig keeps the key in your computer’s key store, not in a file. You make a key in the Mistral console: console.mistral.ai.',
  notulenSleutelVeld: 'Enter the Mistral key',
  notulenSleutelPlaats: 'Paste your key here',
  notulenSleutelOpgeslagen: 'Saved',
  notulenSleutelBewaren: 'Save',
  notulenSleutelTesten: 'Test',
  notulenSleutelTestBezig: 'Testing...',
  notulenSleutelTestOk: 'The key works.',
  notulenSleutelWissen: 'Delete',
  notulenSleutelGewist: 'The key has been deleted.',
  notulenSleutelVervangen: 'Enter a new key',
  notulenTrainingVinkje: 'Training is switched off for this key.',
  notulenTrainingWaarom: 'Why does Mondig ask this?',
  notulenTrainingUitleg:
    'Under Mistral’s terms, Mistral may use the data you send through the API to train its models unless you switch that off (data processing addendum, article 2.3). Mondig cannot check whether that has been done for your key. So switch it off yourself in the Mistral console (console.mistral.ai) and only then tick this box. Without the tick, Mondig does not make minutes through Mistral.',
  notulenModelKop: 'Model',
  notulenModelNauwkeurig: 'Accurate (Mistral Large)',
  notulenModelSnel: 'Faster (Mistral Small)',
  notulenOllamaModelKop: 'Ollama model',
  notulenOllamaDraait: 'Ollama is running on this computer.',
  notulenOllamaDraaitNiet: 'Ollama is not running. Start Ollama and look again.',
  notulenOllamaGeenModellen: 'Ollama has no model yet. Pull one with Ollama first.',
  notulenOllamaKies: 'Choose a model',
  notulenOllamaOpnieuwKijken: 'Look again',
  notulenOllamaKijkt: 'Mondig is checking whether Ollama is running...',
  notulenOllamaDuur: 'Without a fast graphics card this takes 10 to 20 minutes.',
  verslagTabsLabel: 'Transcript or minutes',
  verslagTabVerslag: 'Transcript',
  verslagTabNotulen: 'Minutes',
  notulenLeegKop: 'No minutes yet',
  notulenLeegTekst:
    'Mondig makes a summary, the decisions and the action items from this transcript. Each point shows where it was said.',
  notulenMaken: 'Make minutes',
  notulenNietIngesteld: 'Making minutes is not set up yet.',
  notulenOnvolledig: 'Making minutes is not ready to use yet.',
  notulenNaarInstellingen: 'Go to Settings',
  notulenVraagKop: 'Where does the text go?',
  notulenVraagTekst: (aanbieder: string, vervangen: boolean): string =>
    `The text of this transcript goes to ${aanbieder}. ${
      vervangen ? 'Personal data is replaced.' : 'Personal data is not replaced.'
    }`,
  notulenBezigKop: 'Mondig is making the minutes',
  notulenBezigLeest: 'Mondig is reading the transcript.',
  notulenBezigDeel: (deel: number, totaal: number): string =>
    `Part ${String(deel)} of ${String(totaal)}`,
  notulenStoppen: 'Stop',
  notulenGestopt: 'Making the minutes has stopped. The transcript has not changed.',
  notulenSamenvatting: 'Summary',
  notulenSamenvattingNota: 'Made by a language model, check it.',
  notulenBesluiten: 'Decisions',
  notulenGeenBesluiten: 'No decisions.',
  notulenActiepunten: 'Action items',
  notulenGeenActiepunten: 'No action items.',
  notulenWie: 'who',
  notulenWanneer: 'when',
  notulenNietGenoemd: 'not mentioned',
  notulenBronAria: (tijd: string): string => `Go to ${tijd} in the transcript`,
  notulenBronCitaat: (citaat: string): string => `The model quoted: ${citaat}`,
  notulenNietTeruggevonden: 'Not found in the transcript',
  notulenVeldNietGevonden: 'not found in the transcript',
  notulenAanpassen: 'Edit',
  notulenWeghalen: 'Remove',
  notulenPuntAanpassenAria: (soort: string): string => `Edit ${soort}`,
  notulenPuntWeghalenAria: (soort: string): string => `Remove ${soort}`,
  notulenSoortBesluit: 'decision',
  notulenSoortActiepunt: 'action item',
  notulenVeldWat: 'What',
  notulenVeldWie: 'Who',
  notulenVeldWanneer: 'When',
  notulenVeldTekst: 'Text of the decision',
  notulenPuntBewaren: 'Save',
  notulenPuntGewijzigd: 'The point has been changed.',
  notulenPuntWeg: 'The point has been removed.',
  notulenPlekEuropees: (model: string): string => `${model} (European)`,
  notulenPlekLokaal: (model: string): string => `${model} on this computer (Local)`,
  notulenOverschrijfVraag: 'There are minutes from a newer version of Mondig. Overwrite them?',
  notulenOverschrijven: 'Overwrite and make',
  notulenGemaaktMet: (model: string, moment: string): string => `Made with ${model} · ${moment}`,
  notulenKopieren: 'Copy',
  notulenGekopieerd: 'The minutes are on the clipboard.',
  notulenOpnieuwMaken: 'Make again',
  notulenVorigeVersie:
    'Made from the previous version of the transcript. The sources may no longer match and cannot be opened.',
  notulenOnleesbaar:
    'This transcript has minutes that this version of Mondig cannot read. They stay saved.',
  notulenMarkeringTelling: (aantal: number): string =>
    aantal === 1
      ? '1 point was not found in the transcript. Check that point.'
      : `${String(aantal)} points were not found in the transcript. Check those points.`
} satisfies Berichten;
