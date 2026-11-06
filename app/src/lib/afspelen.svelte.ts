import { bewaarSnelheid, snelheidFactor, UITLOOP_MS, type Snelheid } from './verslag';

// Het stuk geluid bij een regel van het verslag afspelen (stroom V, V-1). Rust decodeert het
// stuk en geeft WAV-bytes; hier wordt er een blob van gemaakt voor een audio-element. De
// webview krijgt dus nooit een pad of een bestandsrecht (CSP: alleen `media-src blob:`).
// Patroon van Vibe (MIT): afspelen vanaf de tijd van een regel en een snelheid van 1, 1,5 of 2.

/** Wat de afspeler van een audio-element nodig heeft; een nep-element kan dit ook zijn. */
export interface AudioElement {
  src: string;
  playbackRate: number;
  defaultPlaybackRate: number;
  preservesPitch: boolean;
  play(): Promise<void>;
  pause(): void;
  removeAttribute(naam: string): void;
  addEventListener(soort: 'ended' | 'pause' | 'play', bij: () => void): void;
  currentTime: number;
  ended: boolean;
}

/** Wat de afspeler van de omgeving nodig heeft; in een test zijn het nepjes. */
export interface AfspeelOmgeving {
  maakAudio: () => AudioElement;
  maakUrl: (bytes: ArrayBuffer) => string;
  geefUrlVrij: (url: string) => void;
}

const WEBVIEW: AfspeelOmgeving = {
  maakAudio: () => new Audio(),
  maakUrl: (bytes) => URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })),
  geefUrlVrij: (url) => {
    URL.revokeObjectURL(url);
  }
};

/** Is dit de AbortError van een `play()` die door een nieuwe opdracht onderbroken werd? */
function isAfgebroken(fout: unknown): boolean {
  return typeof fout === 'object' && fout !== null && 'name' in fout && fout.name === 'AbortError';
}

/**
 * De afspeler van één scherm. Er speelt hooguit één regel tegelijk. Een klik op de tijd van een
 * regel speelt die regel (tot het eind ervan); een tweede klik pauzeert, en nog een klik gaat
 * door. `haal` vraagt Rust om het stuk tussen twee tijden in milliseconden.
 */
export class Afspeler {
  /** Klinkt er nu geluid? */
  speelt = $state(false);
  /** De sleutel van de regel die geladen is of speelt. */
  sleutel = $state<number | null>(null);
  /** Wacht de afspeler op Rust? */
  laden = $state(false);
  /** Waarom het afspelen niet lukte, in gewone taal. */
  fout = $state<string | null>(null);
  snelheid = $state<Snelheid>(10);

  #audio: AudioElement | null = null;
  #url: string | null = null;
  /** Het stuk dat nu in het element staat, om hetzelfde stuk niet opnieuw te vragen. */
  #geladen = '';
  /** Telt op bij elke nieuwe opdracht, zodat een laat antwoord een nieuwere opdracht niet breekt. */
  #opdracht = 0;
  #haal: (beginMs: number, eindMs: number) => Promise<ArrayBuffer>;
  #omgeving: AfspeelOmgeving;
  #foutTekst: (e: unknown) => string;

  constructor(
    haal: (beginMs: number, eindMs: number) => Promise<ArrayBuffer>,
    foutTekst: (e: unknown) => string,
    omgeving: AfspeelOmgeving = WEBVIEW
  ) {
    this.#haal = haal;
    this.#foutTekst = foutTekst;
    this.#omgeving = omgeving;
  }

  #element(): AudioElement {
    if (this.#audio) return this.#audio;
    const audio = this.#omgeving.maakAudio();
    // Een snelheid van 1,5 of 2 keer blijft zo goed te verstaan.
    audio.preservesPitch = true;
    audio.addEventListener('ended', () => {
      this.speelt = false;
    });
    audio.addEventListener('pause', () => {
      this.speelt = false;
    });
    audio.addEventListener('play', () => {
      this.speelt = true;
    });
    this.#audio = audio;
    this.#pasSnelheidToe();
    return audio;
  }

  #pasSnelheidToe(): void {
    if (!this.#audio) return;
    const factor = snelheidFactor(this.snelheid);
    // Een nieuw bronbestand zet de snelheid terug naar de standaard, dus die zetten we ook.
    this.#audio.defaultPlaybackRate = factor;
    this.#audio.playbackRate = factor;
  }

  /** Zet de snelheid, voor nu en voor de volgende keer. */
  zetSnelheid(snelheid: Snelheid): void {
    this.snelheid = snelheid;
    this.#pasSnelheidToe();
    void bewaarSnelheid(snelheid);
  }

  /** Neemt een bewaarde snelheid over zonder hem opnieuw te bewaren. */
  neemSnelheidOver(snelheid: Snelheid): void {
    this.snelheid = snelheid;
    this.#pasSnelheidToe();
  }

  /**
   * Speelt de regel `sleutel` van `beginMs` tot `eindMs`. Speelt die regel al, dan pauzeert dit;
   * staat hij gepauzeerd, dan gaat hij door; is hij afgelopen, dan begint hij opnieuw.
   */
  async speel(sleutel: number, beginMs: number, eindMs: number): Promise<void> {
    const stuk = `${String(sleutel)}:${String(beginMs)}:${String(eindMs)}`;
    this.fout = null;
    const audio = this.#element();
    if (this.#geladen === stuk && this.#url !== null) {
      this.#opdracht += 1;
      if (this.speelt) {
        audio.pause();
        return;
      }
      if (audio.ended) audio.currentTime = 0;
      await this.#start(audio);
      return;
    }
    const opdracht = ++this.#opdracht;
    audio.pause();
    // Het oude stuk gaat weg zodra een nieuw stuk wordt gevraagd: spatie mag het niet terugbrengen.
    this.#maakVrij();
    this.sleutel = sleutel;
    this.laden = true;
    try {
      const bytes = await this.#haal(beginMs, eindMs + UITLOOP_MS);
      if (opdracht !== this.#opdracht) return;
      this.#maakVrij();
      this.#url = this.#omgeving.maakUrl(bytes);
      this.#geladen = stuk;
      audio.src = this.#url;
      this.#pasSnelheidToe();
      await this.#start(audio);
    } catch (e) {
      if (opdracht !== this.#opdracht) return;
      this.#maakVrij();
      this.sleutel = null;
      this.fout = this.#foutTekst(e);
    } finally {
      if (opdracht === this.#opdracht) this.laden = false;
    }
  }

  async #start(audio: AudioElement): Promise<void> {
    try {
      await audio.play();
    } catch (e) {
      this.speelt = false;
      // Een nieuwe opdracht onderbreekt play() met een AbortError: dat is geen fout.
      if (isAfgebroken(e)) return;
      this.fout = this.#foutTekst(e);
    }
  }

  /**
   * Spatie: pauzeert wat speelt, zet een gepauzeerde regel voort. Geeft `false` als er niets is.
   * Tijdens het klaarzetten van een nieuwe regel doet spatie niets (er is dan nog niets te spelen).
   */
  schakel(): boolean {
    if (this.laden) return true;
    const audio = this.#audio;
    if (this.sleutel === null || this.#url === null || !audio) return false;
    if (this.speelt) {
      audio.pause();
    } else {
      if (audio.ended) audio.currentTime = 0;
      void this.#start(audio);
    }
    return true;
  }

  /** Stopt en laat het geluid los (bijvoorbeeld als het verslag sluit). */
  stop(): void {
    this.#opdracht += 1;
    this.#audio?.pause();
    this.#maakVrij();
    this.sleutel = null;
    this.speelt = false;
    this.laden = false;
  }

  #maakVrij(): void {
    if (this.#url !== null) {
      this.#audio?.removeAttribute('src');
      this.#omgeving.geefUrlVrij(this.#url);
      this.#url = null;
    }
    this.#geladen = '';
  }
}
