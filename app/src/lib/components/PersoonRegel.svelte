<script lang="ts">
  import { tick } from 'svelte';
  import { Popover } from 'bits-ui';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import CircleQuestionMark from '@lucide/svelte/icons/circle-question-mark';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Schakelaar from '#lib/components/Schakelaar.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { gemarkeerdeStukken, type PersoonStand, type VerslagRegel } from '#lib/verslag.js';

  // Persoonsgegevens als één regel boven het verslag (N-C1, ontwerp 3): de teller die de lijst in
  // een popover opent, de schakelaar met een tekst die zegt wat er per stand gebeurt, en een
  // vraagteken met "Hoe werkt dit?". Escape en een klik erbuiten sluiten de popover; de focus
  // gaat terug naar de knop die hem opende (bits-ui regelt dat). De regels zelf tonen in de
  // stand Vervangen al de plaatsvervangers; dat doet de lijst (zie `vervangenRegel`).

  let {
    regels,
    persoon,
    zetVervangen,
    haalWeg,
    markeer,
    zelfVeld = $bindable(''),
    meldFout,
    meldStatus
  }: {
    /** De echte regels (met de gegevens zelf), voor de lijst in de popover. */
    regels: VerslagRegel[];
    persoon: PersoonStand;
    /** Zet het vervangen aan of uit; geeft de melding bij een fout, anders `null`. */
    zetVervangen: (aan: boolean) => Promise<string | null>;
    /** Haalt de markering van een stuk tekst weg; geeft de melding bij een fout, anders `null`. */
    haalWeg: (tekst: string) => Promise<string | null>;
    /** Markeert zelf een stuk tekst; geeft de melding bij een fout, anders `null`. */
    markeer: (tekst: string) => Promise<string | null>;
    /** Het veld "Zelf iets markeren"; de lijst vult het met een selectie in het verslag. */
    zelfVeld?: string;
    /** Een fout (of `null` om te wissen) voor het deel van het scherm dat meldingen toont. */
    meldFout: (melding: string | null) => void;
    /** Een korte zin voor de schermlezer na een gelukte actie. */
    meldStatus: (zin: string) => void;
  } = $props();

  let lijstOpen = $state(false);
  let uitlegOpen = $state(false);
  let rijEl = $state<HTMLElement | null>(null);
  let lijstInhoud = $state<HTMLElement | null>(null);
  // Zet de focus niet terug op de teller als de popover alleen wisselt naar de uitleg.
  let naarUitleg = false;

  // De gemarkeerde stukken, met de zelf gemarkeerde die (nog) nergens voorkomen erbij.
  const gemarkeerd = $derived.by(() => {
    const lijst = gemarkeerdeStukken(regels);
    for (const eigen of persoon.eigen) {
      if (!lijst.some((g) => g.tekst.toLowerCase() === eigen)) {
        lijst.push({ tekst: eigen, soort: 'handmatig', aantal: 0 });
      }
    }
    return lijst;
  });

  async function actie(werk: () => Promise<string | null>, gelukt: string): Promise<boolean> {
    meldFout(null);
    const melding = await werk();
    if (melding === null) {
      if (gelukt !== '') meldStatus(gelukt);
      return true;
    }
    meldFout(melding);
    return false;
  }

  async function weghalen(tekst: string): Promise<void> {
    if (await actie(() => haalWeg(tekst), t().persoonWeggehaald)) {
      // Het knopje dat de focus had is weg; geef hem aan de popover zelf.
      await tick();
      lijstInhoud?.focus();
    }
  }

  async function markeerZelf(): Promise<void> {
    const waarde = zelfVeld.trim();
    if (waarde === '') return;
    if (await actie(() => markeer(waarde), t().persoonGemarkeerd)) zelfVeld = '';
  }

  function naarUitlegPopover(): void {
    naarUitleg = true;
    lijstOpen = false;
    uitlegOpen = true;
  }

  const inhoudKlasse =
    'z-50 flex w-[380px] max-w-[calc(100vw-1.5rem)] flex-col gap-2 rounded-groot border border-lijn bg-verhoogd p-4 shadow-[0_8px_24px_rgba(0,0,0,0.35)] outline-none';
</script>

<div class="flex flex-wrap items-center gap-x-2.5 gap-y-1 pl-0.5" bind:this={rijEl}>
  <Popover.Root bind:open={lijstOpen}>
    <Popover.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="spook" class="-ml-2 text-tekst">
          {t().persoonTeller(gemarkeerd.length)}<ChevronDown size={14} aria-hidden="true" />
        </Button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Portal>
      <Popover.Content
        customAnchor={rijEl}
        align="end"
        sideOffset={6}
        collisionPadding={12}
        class={inhoudKlasse}
        bind:ref={lijstInhoud}
        tabindex={-1}
        onCloseAutoFocus={(e) => {
          if (naarUitleg) {
            e.preventDefault();
            naarUitleg = false;
          }
        }}
      >
        <h4 class="m-0 font-tekst text-basis font-semibold">
          {t().persoonGevondenKop(gemarkeerd.length)}
        </h4>
        <p class="m-0 text-klein leading-normal text-gedempt">
          {t().persoonGevondenHulp}
          <button
            type="button"
            class="cursor-pointer text-actie underline underline-offset-2"
            onclick={naarUitlegPopover}>{t().persoonUitlegKnop}</button
          >
        </p>
        {#if gemarkeerd.length > 0}
          <ul
            class="m-0 flex max-h-56 list-none flex-col gap-0.5 overflow-y-auto rounded-klein bg-vlak p-1"
          >
            {#each gemarkeerd as stuk (stuk.tekst.toLowerCase())}
              <li class="flex items-center gap-2 px-2 py-1">
                <span class="flex min-w-0 flex-1 flex-col">
                  <b class="font-medium break-words">{stuk.tekst}</b>
                  <small class="text-klein text-dim"
                    >{t().persoonSoort(stuk.soort)}{#if stuk.aantal > 1}
                      · {t().persoonKeerVoor(stuk.aantal)}{/if}</small
                  >
                </span>
                <Button
                  variant="spook"
                  size="icoon"
                  title={t().persoonWeghalenTitel}
                  aria-label={t().persoonWeghalen(stuk.tekst)}
                  onclick={() => weghalen(stuk.tekst)}
                  ><Trash2 size={16} aria-hidden="true" /></Button
                >
              </li>
            {/each}
          </ul>
        {/if}
        <form
          class="flex flex-col gap-1.5"
          onsubmit={(e) => {
            e.preventDefault();
            void markeerZelf();
          }}
        >
          <label for="persoon-zelf" class="text-klein font-medium">{t().persoonZelfKop}</label>
          <span id="persoon-zelf-hulp" class="text-klein text-gedempt">{t().persoonZelfHulp}</span>
          <div class="flex items-center gap-2">
            <input
              id="persoon-zelf"
              type="text"
              maxlength="100"
              autocomplete="off"
              spellcheck="false"
              aria-describedby="persoon-zelf-hulp"
              bind:value={zelfVeld}
              class="h-8 min-w-0 flex-1 rounded-klein border border-rand-veld bg-verhoogd px-3 text-basis text-tekst placeholder:text-dim"
            />
            <Button type="submit" variant="tweede">{t().persoonZelfKnop}</Button>
          </div>
        </form>
      </Popover.Content>
    </Popover.Portal>
  </Popover.Root>

  <Schakelaar
    aan={persoon.vervangen}
    wijzig={(aan: boolean) => {
      void actie(() => zetVervangen(aan), '');
    }}
    labelledby="persoon-switch"
    describedby="persoon-stand"
  />
  <span id="persoon-switch" class="sr-only">{t().persoonSwitch}</span>
  <span id="persoon-stand" class="min-w-[9.5rem] flex-1 text-klein text-gedempt"
    ><b class="font-medium text-tekst"
      >{persoon.vervangen ? t().persoonStandAan : t().persoonStandUit}</b
    >
    {persoon.vervangen ? t().persoonStandAanRest : t().persoonStandUitRest}</span
  >

  <Popover.Root bind:open={uitlegOpen}>
    <Popover.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="spook"
          size="icoon"
          title={t().persoonUitlegKnop}
          aria-label={t().persoonUitlegKnop}
        >
          <CircleQuestionMark size={16} aria-hidden="true" />
        </Button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Portal>
      <Popover.Content
        customAnchor={rijEl}
        align="end"
        sideOffset={6}
        collisionPadding={12}
        class={inhoudKlasse}
        tabindex={-1}
      >
        <h4 class="m-0 font-tekst text-basis font-semibold">{t().persoonUitlegKnop}</h4>
        <p class="m-0 text-klein leading-normal text-gedempt">{t().persoonUitleg1}</p>
        <p class="m-0 text-klein leading-normal text-gedempt">{t().persoonUitleg2}</p>
        <p class="m-0 text-klein leading-normal text-gedempt">{t().persoonUitleg3}</p>
      </Popover.Content>
    </Popover.Portal>
  </Popover.Root>
</div>
