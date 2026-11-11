<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import KlantVeld from '#lib/components/KlantVeld.svelte';
  import { t } from '#lib';
  import { klantenLijst } from '#lib/klanten.js';
  import {
    gelijkeNaamvelden,
    maakNaamveldenBeheer,
    type NaamveldenBeheer,
    type Naamvelden
  } from '#lib/naamvelden.js';
  import { vlak } from '#lib/stijl.js';

  // De kaart "Naam van het bestand" (UI-E, ontwerpsysteem §5): Datum, Klant (met suggesties) en
  // Onderwerp, en daaronder live "Wordt 2026-11-02 Gemeente Vlaardingen Intake.docx". Boven het
  // uitschrift (Bestanden) en in het verslag (Opnemen) is het dezelfde kaart.
  //
  // - Het voorbeeld rekent Rust uit (`haalVoorbeeld`), zodat het altijd gelijk is aan het echte
  //   bestand, met een korte vertraging bij het typen.
  // - De velden worden bewaard bij blur, Enter of een keuze uit de suggesties, niet bij elke
  //   toets (`bewaar`; bij een verslag schrijft dat naar schijf). De ouder roept `bewaarNu` aan
  //   vlak voor Opslaan, zodat wat er staat ook in het bestand komt.
  // - "Opgeslagen" met "Tonen in Verkenner" staat niet in deze kaart: de pagina toont die melding
  //   na elk opslaan al, en twee keer dezelfde melding is ruis.
  // - Onderwerp en Datum staan ingevuld met de standaard als gewone tekst (geen placeholder); een
  //   leeg veld valt weg uit de naam.

  interface Props {
    /** Het voorvoegsel voor de ids ("transcript", "verslag"), zodat de pagina en de tests ze vinden. */
    id: string;
    /** De bewaarde waarden uit Rust. Een nieuwe stand vervangt wat de gebruiker niet aanpaste. */
    velden: Naamvelden;
    /** De bestandsnaam bij deze velden, uit Rust. */
    haalVoorbeeld: (velden: Naamvelden) => Promise<string>;
    /** Bewaart de velden. Geeft de melding bij een fout, anders `null`. */
    bewaar: (velden: Naamvelden) => Promise<string | null>;
    /** Het voorbeeld dat er al is, voor het eerste antwoord van Rust (en voor de tests). */
    voorbeeldStart?: string;
  }
  let { id, velden, haalVoorbeeld, bewaar, voorbeeldStart = '' }: Props = $props();

  // Wat in de velden staat; begint bij de bewaarde waarden.
  let datum = $state(untrack(() => velden.datum));
  let klant = $state(untrack(() => velden.klant));
  let onderwerp = $state(untrack(() => velden.onderwerp));
  let voorbeeld = $state(untrack(() => voorbeeldStart));
  let fout = $state<string | null>(null);
  let suggesties = $state<string[]>([]);
  // De waarden die de ouder het laatst gaf, om te zien welk veld de gebruiker aanraakte.
  let basis: Naamvelden = untrack(() => ({ ...velden }));

  const beheer: NaamveldenBeheer = maakNaamveldenBeheer(
    {
      haalVoorbeeld: (v) => haalVoorbeeld(v),
      bewaar: (v) => bewaar(v),
      zetVoorbeeld: (naam) => {
        voorbeeld = naam;
      },
      zetFout: (melding) => {
        fout = melding;
      }
    },
    untrack(() => ({ ...velden }))
  );

  const huidig = (): Naamvelden => ({ klant, onderwerp, datum });

  // Nieuwe waarden van buiten (een nieuwe stand van het verslag): een veld dat de gebruiker niet
  // aanraakte, neemt ze over; wat hij typte blijft staan.
  $effect(() => {
    const nieuw: Naamvelden = {
      klant: velden.klant,
      onderwerp: velden.onderwerp,
      datum: velden.datum
    };
    untrack(() => {
      if (
        nieuw.klant === basis.klant &&
        nieuw.onderwerp === basis.onderwerp &&
        nieuw.datum === basis.datum
      )
        return;
      if (klant === basis.klant) klant = nieuw.klant;
      if (onderwerp === basis.onderwerp) onderwerp = nieuw.onderwerp;
      if (datum === basis.datum) datum = nieuw.datum;
      basis = nieuw;
      beheer.bewaard(nieuw);
      // Staat er nog niet bewaarde invoer tussen, dan volgt het voorbeeld die en niet de stand.
      if (!gelijkeNaamvelden(huidig(), nieuw)) beheer.invoer(huidig());
    });
  });

  onMount(() => {
    // Het eerste voorbeeld; daarna volgt het de invoer.
    beheer.bewaard(huidig());
    void ophalen();
  });
  onDestroy(() => {
    beheer.stop();
  });

  /** Haalt de klantnamen op; mislukt dat, dan zijn er geen suggesties en werkt het veld gewoon. */
  async function ophalen(): Promise<void> {
    try {
      suggesties = await klantenLijst();
    } catch {
      suggesties = [];
    }
  }

  /** Bewaart wat er staat als het anders is dan wat bewaard is. Geeft `false` bij een fout. */
  export function bewaarNu(): Promise<boolean> {
    return beheer.bewaarNu();
  }

  function bijInvoer(): void {
    beheer.invoer(huidig());
  }

  function bijKlaar(): void {
    void beheer.bewaarNu();
  }

  function bijToets(event: KeyboardEvent): void {
    if (event.key === 'Enter') bijKlaar();
  }

  const veld =
    'h-9 w-full min-w-0 rounded-klein border border-rand-veld bg-verhoogd px-3 text-basis text-tekst';
  const labelKlasse = 'm-0 text-klein font-medium text-gedempt';
</script>

<div class="{vlak} flex flex-col gap-3 px-4 py-3.5" role="group" aria-labelledby="{id}-naam-kop">
  <h2 id="{id}-naam-kop" class="m-0 font-tekst text-basis font-semibold text-tekst">
    {t().naamkaartKop}
  </h2>
  <div class="grid grid-cols-[repeat(auto-fit,minmax(11rem,1fr))] gap-3">
    <div class="flex flex-col gap-1">
      <label for="{id}-datum" class={labelKlasse}>{t().naamveldDatum}</label>
      <input
        id="{id}-datum"
        type="text"
        maxlength="80"
        autocomplete="off"
        spellcheck="false"
        class={veld}
        aria-describedby="{id}-voorbeeld"
        value={datum}
        oninput={(event) => {
          datum = event.currentTarget.value;
          bijInvoer();
        }}
        onkeydown={bijToets}
        onblur={bijKlaar}
      />
    </div>
    <div class="flex flex-col gap-1">
      <label for="{id}-klant" class={labelKlasse}>{t().naamveldKlant}</label>
      <KlantVeld
        id="{id}-klant"
        waarde={klant}
        {suggesties}
        beschrijving="{id}-voorbeeld"
        bijWijzig={(waarde: string) => {
          klant = waarde;
          bijInvoer();
        }}
        {bijKlaar}
        bijFocus={() => void ophalen()}
      />
    </div>
    <div class="flex flex-col gap-1">
      <label for="{id}-onderwerp" class={labelKlasse}>{t().naamveldOnderwerp}</label>
      <input
        id="{id}-onderwerp"
        type="text"
        maxlength="80"
        autocomplete="off"
        spellcheck="false"
        class={veld}
        aria-describedby="{id}-voorbeeld"
        value={onderwerp}
        oninput={(event) => {
          onderwerp = event.currentTarget.value;
          bijInvoer();
        }}
        onkeydown={bijToets}
        onblur={bijKlaar}
      />
    </div>
  </div>
  <p id="{id}-voorbeeld" class="m-0 text-klein break-words text-gedempt">
    {t().naamkaartWordt}
    <code class="rounded-klein bg-verhoogd px-1.5 py-0.5 font-mono text-tekst">{voorbeeld}</code>
  </p>
  {#if fout}
    <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
  {/if}
</div>
