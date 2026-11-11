<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '#lib';
  import { comboToets, filterKlanten } from '#lib/klanten.js';

  // Het veld Klant met suggesties: een combobox volgens het WAI-ARIA-patroon "combobox with list
  // autocomplete" (APG). De focus blijft in het invoerveld; de pijltjes kiezen een suggestie
  // (`aria-activedescendant`), Enter neemt de gekozen suggestie over, Escape sluit de lijst, en
  // vrije tekst mag altijd: zonder gekozen suggestie bewaart Enter wat er staat. Er wordt niets
  // vanzelf gekozen: typen "Gem" en Enter geeft "Gem", niet de eerste suggestie.
  //
  // Bewust een eigen onderdeel en niet de Combobox van bits-ui: die kiest bij typen vanzelf de
  // eerste suggestie (Enter neemt die dan over, ook als je eigen tekst bedoelde) en zet de tekst
  // terug naar de gekozen waarde bij het sluiten, en dat past slecht bij vrije tekst.

  interface Props {
    /** De id van het invoerveld; het label wijst ernaar met `for`. */
    id: string;
    /** De tekst in het veld. */
    waarde: string;
    /** Alle klantnamen die Mondig onthoudt, nieuwste eerst. */
    suggesties: readonly string[];
    /** Er is getypt of een suggestie gekozen: de tekst is anders. */
    bijWijzig: (waarde: string) => void;
    /** Enter, het veld verlaten of een suggestie gekozen: tijd om te bewaren. */
    bijKlaar: () => void;
    /** Een veld met meer uitleg, zoals het voorbeeld van de bestandsnaam. */
    beschrijving?: string;
    /** De lijst met klantnamen opnieuw ophalen: de gebruiker kwam in het veld. */
    bijFocus?: () => void;
    /** Alleen voor de tests: begin met de lijst open en deze suggestie actief. */
    startOpen?: boolean;
    startActief?: number;
  }
  let {
    id,
    waarde,
    suggesties,
    bijWijzig,
    bijKlaar,
    beschrijving,
    bijFocus,
    startOpen = false,
    startActief = -1
  }: Props = $props();

  let open = $state(untrack(() => startOpen));
  // De suggestie die de pijltjes aanwijzen; -1: geen, dan geldt de getypte tekst.
  let actief = $state(untrack(() => startActief));

  const lijst = $derived(filterKlanten(suggesties, waarde));
  const toon = $derived(open && lijst.length > 0);
  const lijstId = $derived(`${id}-lijst`);
  const optieId = (i: number): string => `${id}-optie-${String(i)}`;

  function sluit(): void {
    open = false;
    actief = -1;
  }

  function kies(i: number): void {
    const gekozen = lijst[i];
    if (!gekozen) return;
    sluit();
    bijWijzig(gekozen.naam);
    bijKlaar();
  }

  function bijInvoer(event: Event & { currentTarget: HTMLInputElement }): void {
    open = true;
    actief = -1;
    bijWijzig(event.currentTarget.value);
  }

  function bijToets(event: KeyboardEvent): void {
    const uit = comboToets({ open, actief }, event.key, lijst.length);
    if (uit.blokkeer) event.preventDefault();
    if (uit.stopVerder) event.stopPropagation();
    open = uit.stand.open;
    actief = uit.stand.actief;
    if (uit.kies !== null) kies(uit.kies);
    else if (uit.klaar) bijKlaar();
  }

  function bijVerlaten(): void {
    sluit();
    bijKlaar();
  }
</script>

<div class="relative">
  <input
    {id}
    type="text"
    role="combobox"
    aria-autocomplete="list"
    aria-expanded={toon}
    aria-controls={lijstId}
    aria-activedescendant={toon && actief >= 0 ? optieId(actief) : undefined}
    aria-describedby={beschrijving}
    maxlength="80"
    autocomplete="off"
    spellcheck="false"
    class="h-9 w-full min-w-0 rounded-klein border border-rand-veld bg-verhoogd px-3 text-basis text-tekst"
    value={waarde}
    oninput={bijInvoer}
    onkeydown={bijToets}
    onfocus={() => bijFocus?.()}
    onblur={bijVerlaten}
  />
  {#if toon}
    <ul
      id={lijstId}
      role="listbox"
      aria-label={t().klantSuggestiesLabel}
      class="absolute inset-x-0 top-full z-30 m-0 mt-1 list-none rounded-groot border border-lijn bg-verhoogd p-1.5 shadow-[0_8px_24px_rgba(0,0,0,0.35)]"
    >
      {#each lijst as suggestie, i (suggestie.naam)}
        <!-- De focus blijft in het veld: een klik mag het veld niet laten blurren. De toetsen
             werken in het veld zelf (pijltjes en Enter), dus hier is geen toetsenbordhandler nodig. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id={optieId(i)}
          role="option"
          aria-selected={i === actief}
          class="flex min-h-8 cursor-pointer items-center rounded-klein px-2.5 text-basis text-gedempt select-none {i ===
          actief
            ? 'bg-hover text-tekst'
            : 'hover:bg-hover'}"
          onmousedown={(event) => {
            event.preventDefault();
          }}
          onclick={() => {
            kies(i);
          }}
        >
          <span class="min-w-0 truncate"
            >{suggestie.voor}{#if suggestie.gevonden}<mark
                class="bg-transparent font-semibold text-tekst">{suggestie.gevonden}</mark
              >{/if}{suggestie.na}</span
          >
        </li>
      {/each}
    </ul>
  {/if}
  <!-- Voor de schermlezer: hoeveel suggesties er zijn, zonder de focus te verplaatsen. -->
  <span class="sr-only" role="status"
    >{open ? (lijst.length > 0 ? t().klantSuggestiesAantal(lijst.length) : '') : ''}</span
  >
</div>
