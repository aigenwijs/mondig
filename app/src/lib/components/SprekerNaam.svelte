<script lang="ts">
  import { Popover } from 'bits-ui';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    MAX_SPREKERS,
    beginNaam,
    beurtNaam,
    beurtNaamGetoond,
    standaardNaam,
    type BeurtInfo,
    type SprekerInfo
  } from '#lib/vergadering.js';
  import { tijdcode } from '#lib/bestanden.js';

  // Het label van een beurt in het verslag (ontwerp 4b-C, scherm 6) als knop: een klik opent een
  // popover. Daarin geef je de spreker een naam, altijd voor al zijn beurten ("Jij" en "Anderen"
  // zonder nummer net zo): een naam hoort bij een spreker, niet bij een regel (zoals in Vibe, MIT).
  // Eén regel die bij iemand anders hoort, geef je daaronder aan een andere spreker, en twee
  // sprekers die dezelfde persoon zijn, voeg je samen. Bits UI zorgt voor het toetsenbord: focus
  // in het veld, Esc sluit, en de focus gaat terug naar het label.
  //
  // Een alinea die in de ruimte niemand aanwees, heeft geen spreker en dus geen label. Met
  // `aanwijzen` is de knop dan "Spreker aanwijzen": 'rij' is een kleine knop naast de alinea
  // (zolang er geen labelkolom is, zichtbaar bij aanwijzen en bij toetsenbordfocus), 'kolom' het
  // gestippelde "+ Spreker" in de labelkolom. De popover heeft dan alleen "Deze beurt is van" met
  // de sprekers en "Nieuwe spreker": een naam geven kan daarna bij het label van die spreker.

  let {
    beurt,
    sprekers = [],
    kanAanSpreker = false,
    aanwijzen,
    geef,
    wijsToe,
    voegSamen
  }: {
    beurt: BeurtInfo;
    /** Alle sprekers in het verslag, oplopend; leeg zonder herkende sprekers. */
    sprekers?: SprekerInfo[];
    /** Mag deze beurt aan een (nieuwe) spreker gegeven worden? */
    kanAanSpreker?: boolean;
    /** De knop "Spreker aanwijzen" in plaats van het label, naast de alinea of in de kolom. */
    aanwijzen?: 'rij' | 'kolom';
    /** Geeft de naam door; de melding bij een fout, anders `null`. */
    geef: (naam: string) => Promise<string | null>;
    /** Geeft de beurt aan spreker `nummer`, of aan een nieuwe spreker bij `null`. */
    wijsToe?: (nummer: number | null) => Promise<string | null>;
    /** Voegt de spreker van deze beurt samen met spreker `naar`. */
    voegSamen?: (naar: number) => Promise<string | null>;
  } = $props();

  let open = $state(false);
  let naam = $state('');
  let bezig = $state(false);
  let fout = $state<string | null>(null);
  let veld = $state<HTMLInputElement | null>(null);

  // Met Vervangen aan toont het label een gemarkeerd gegeven in de naam zoals de export het geeft
  // ([GEGEVEN]), met dezelfde opvallende achtergrond als een vervangen stuk tekst in het verslag.
  const label = $derived(beurtNaamGetoond(beurt, t()));
  const vervangen = $derived(label !== beurtNaam(beurt, t()));
  const metSpreker = $derived(beurt.sprekernummer !== null);
  // Wat er na leegmaken staat: "Spreker 2", of "Jij" of "Anderen".
  const standaard = $derived(standaardNaam(beurt, t()));
  const id = $derived(`naam-${String(beurt.index)}`);
  const anderen = $derived(sprekers.filter((s) => s.nummer !== beurt.sprekernummer));
  const magNieuwe = $derived(sprekers.length < MAX_SPREKERS);
  // Een beurt zonder spreker toont bij aanwijzen een gestippeld "+ Spreker".
  const zonderSpreker = $derived(kanAanSpreker && !metSpreker);

  function bijWissel(nieuw: boolean): void {
    open = nieuw;
    if (nieuw) {
      naam = beginNaam(beurt);
      fout = null;
    }
  }

  async function doe(werk: () => Promise<string | null>): Promise<void> {
    if (bezig) return;
    bezig = true;
    fout = await werk();
    bezig = false;
    if (fout === null) open = false;
  }

  function bewaar(event: Event): void {
    event.preventDefault();
    void doe(() => geef(naam.trim()));
  }

  // Geen badge: de naam in vet, "Jij" in de actiekleur. Het blijft een knop (naam geven).
  const naamKlas =
    'group relative inline max-w-full cursor-pointer rounded-klein text-left text-basis font-semibold [overflow-wrap:anywhere] decoration-lijn underline-offset-4 hover:underline';
  const sprekerKnop =
    'inline-flex min-h-8 cursor-pointer items-center rounded-klein bg-hover px-2.5 text-basis text-tekst transition-colors outline-none hover:bg-lijn focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus aria-disabled:cursor-default aria-disabled:opacity-60 aria-pressed:text-actie aria-pressed:ring-1 aria-pressed:ring-actie aria-pressed:ring-inset';
  const koppen = 'm-0 text-klein font-medium text-gedempt';
  // De aanwijsknop is er voor het toetsenbord en de schermlezer altijd; zien doe je hem bij
  // aanwijzen van de rij (`group/rij` in de lijst), bij focus erin en op een scherm zonder muis.
  // In de kolom staat het gestippelde "+ Spreker" er altijd.
  const kolomKnop =
    'inline-flex min-h-6 cursor-pointer items-center rounded-klein border border-dashed border-dim bg-transparent px-1.5 text-klein font-normal whitespace-nowrap text-dim outline-none hover:text-tekst focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus';
  const aanwijsKnop =
    'inline-flex size-6 cursor-pointer items-center justify-center rounded-klein border border-dashed border-dim bg-achtergrond text-gedempt opacity-0 transition-opacity outline-none hover:text-tekst focus-visible:opacity-100 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus group-focus-within/rij:opacity-100 group-hover/rij:opacity-100 data-[state=open]:opacity-100 [@media(hover:none)]:opacity-100';
</script>

<Popover.Root {open} onOpenChange={bijWissel}>
  {#if aanwijzen === 'kolom'}
    <Popover.Trigger
      class={kolomKnop}
      aria-label={t().vergaderSprekerAanwijzenAria(tijdcode(beurt.startMs))}
      title={t().vergaderSprekerAanwijzen}
      data-aanwijs-beurt={beurt.index}>{t().vergaderPlusSpreker}</Popover.Trigger
    >
  {:else if aanwijzen}
    <Popover.Trigger
      class={aanwijsKnop}
      aria-label={t().vergaderSprekerAanwijzenAria(tijdcode(beurt.startMs))}
      title={t().vergaderSprekerAanwijzen}
      data-aanwijs-beurt={beurt.index}
    >
      <UserPlus size={14} aria-hidden="true" />
    </Popover.Trigger>
  {:else}
    <Popover.Trigger
      data-label-beurt={beurt.index}
      class="{naamKlas} {beurt.kant === 'jij' ? 'text-actie' : 'text-tekst'}"
      aria-label={t().vergaderNaamAria(label)}
      title={label}
    >
      <span class={vervangen ? 'rounded-[3px] bg-actie/15 px-0.5 text-actie' : undefined}
        >{label}</span
      >
      {#if zonderSpreker}
        <span
          class="pointer-events-none absolute top-0 left-full ml-1.5 rounded-klein border border-dashed border-dim bg-achtergrond px-1.5 py-px text-klein font-normal whitespace-nowrap text-dim no-underline opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100"
          aria-hidden="true">{t().vergaderPlusSpreker}</span
        >
      {/if}
    </Popover.Trigger>
  {/if}
  <Popover.Portal>
    <Popover.Content
      sideOffset={6}
      align="start"
      aria-label={aanwijzen ? t().vergaderBeurtVanKop : t().vergaderNaamGeven(label)}
      onCloseAutoFocus={(e) => {
        // De knop verdwijnt zodra de alinea een spreker heeft: de pagina zet de focus zelf op het
        // nieuwe label.
        if (aanwijzen) e.preventDefault();
      }}
      onOpenAutoFocus={(e) => {
        if (aanwijzen) return;
        e.preventDefault();
        veld?.focus();
        veld?.select();
      }}
      class="z-50 w-[300px] max-w-[calc(100vw-2rem)] rounded-groot border border-lijn bg-verhoogd p-3.5 text-basis shadow-[0_10px_30px_rgba(0,0,0,0.5)]"
    >
      {#if !aanwijzen}
        <form class="flex flex-col gap-3" onsubmit={bewaar}>
          <div class="flex flex-col gap-1.5">
            <label for="{id}-veld" class="m-0 text-klein font-medium text-gedempt"
              >{t().vergaderNaamSprekerKop}</label
            >
            <input
              id="{id}-veld"
              bind:this={veld}
              bind:value={naam}
              type="text"
              maxlength="60"
              autocomplete="off"
              spellcheck="false"
              class="h-[34px] w-full min-w-0 rounded-klein border border-rand-veld bg-achtergrond px-2.5 text-basis text-tekst placeholder:text-dim"
              placeholder={t().vergaderNaamVoorbeeld}
              aria-describedby="{id}-hulp"
            />
            <span id="{id}-hulp" class="text-klein text-dim"
              >{t().vergaderNaamSprekerHulp(standaard)}</span
            >
          </div>
          <div class="flex justify-end gap-2">
            <Popover.Close>
              {#snippet child({ props })}
                <Button {...props} variant="spook">{t().annuleren}</Button>
              {/snippet}
            </Popover.Close>
            <Button type="submit" aria-disabled={bezig}>{t().vergaderNaamOpslaan}</Button>
          </div>
        </form>
      {/if}

      {#if kanAanSpreker && wijsToe}
        <!-- Deze beurt aan een andere spreker geven, of aan een nieuwe. -->
        <div
          class="{aanwijzen ? '' : 'mt-3 border-t border-lijn pt-3'} flex flex-col gap-2"
          role="group"
          aria-labelledby="{id}-van"
        >
          <h4 id="{id}-van" class={koppen}>{t().vergaderBeurtVanKop}</h4>
          {#if aanwijzen}
            <p class="m-0 text-klein text-dim">{t().vergaderSprekerAanwijzenHulp}</p>
          {/if}
          <div class="flex flex-wrap gap-1.5">
            {#each sprekers as spreker (spreker.nummer)}
              <button
                type="button"
                class={sprekerKnop}
                aria-pressed={spreker.nummer === beurt.sprekernummer}
                aria-disabled={bezig}
                aria-label={t().vergaderBeurtVanAria(spreker.naam)}
                onclick={() => void doe(() => wijsToe(spreker.nummer))}>{spreker.naam}</button
              >
            {/each}
            {#if magNieuwe}
              <button
                type="button"
                class="{sprekerKnop} border border-dashed border-dim bg-transparent text-gedempt"
                aria-disabled={bezig}
                onclick={() => void doe(() => wijsToe(null))}>{t().vergaderNieuweSpreker}</button
              >
            {/if}
          </div>
        </div>
      {/if}

      {#if metSpreker && voegSamen && anderen.length > 0}
        <!-- Twee sprekers die dezelfde persoon zijn, samenvoegen. -->
        <div
          class="mt-3 flex flex-col gap-2 border-t border-lijn pt-3"
          role="group"
          aria-labelledby="{id}-samen"
        >
          <h4 id="{id}-samen" class={koppen}>{t().vergaderSamenvoegenKop}</h4>
          <p class="m-0 text-klein text-dim">{t().vergaderSamenvoegenHulp(label)}</p>
          <div class="flex flex-wrap gap-1.5">
            {#each anderen as spreker (spreker.nummer)}
              <button
                type="button"
                class={sprekerKnop}
                aria-disabled={bezig}
                onclick={() => void doe(() => voegSamen(spreker.nummer))}
                >{t().vergaderSamenvoegenMet(spreker.naam)}</button
              >
            {/each}
          </div>
        </div>
      {/if}

      {#if fout}
        <p class="m-0 mt-3 text-klein text-fout-tekst" role="alert">{fout}</p>
      {/if}
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>
