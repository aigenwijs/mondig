<script lang="ts">
  import { Popover } from 'bits-ui';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    MAX_SPREKERS,
    beginNaam,
    beurtNaam,
    kantNaam,
    type BeurtInfo,
    type Naambereik,
    type SprekerInfo
  } from '#lib/vergadering.js';

  // Het label van een beurt in het verslag (ontwerp 4b-C, scherm 6) als knop: een klik opent een
  // popover. Daarin kun je "Jij" of "Anderen" een naam geven, voor alleen deze beurt of voor alle
  // beurten met dezelfde naam. Zijn de sprekers herkend (4b-D), dan geldt de naam voor de hele
  // spreker, en kun je daaronder de beurt aan een andere spreker geven of twee sprekers
  // samenvoegen (patroon van het sprekerlabel in Vibe, MIT). Bits UI zorgt voor het toetsenbord:
  // focus in het veld, Esc sluit, en de focus gaat terug naar het label.

  let {
    beurt,
    sprekers = [],
    kanAanSpreker = false,
    geef,
    wijsToe,
    voegSamen
  }: {
    beurt: BeurtInfo;
    /** Alle sprekers in het verslag, oplopend; leeg zonder herkende sprekers. */
    sprekers?: SprekerInfo[];
    /** Mag deze beurt aan een (nieuwe) spreker gegeven worden? */
    kanAanSpreker?: boolean;
    /** Geeft de naam door; de melding bij een fout, anders `null`. */
    geef: (naam: string, bereik: Naambereik) => Promise<string | null>;
    /** Geeft de beurt aan spreker `nummer`, of aan een nieuwe spreker bij `null`. */
    wijsToe?: (nummer: number | null) => Promise<string | null>;
    /** Voegt de spreker van deze beurt samen met spreker `naar`. */
    voegSamen?: (naar: number) => Promise<string | null>;
  } = $props();

  let open = $state(false);
  let naam = $state('');
  let bereik = $state<Naambereik>('beurt');
  let bezig = $state(false);
  let fout = $state<string | null>(null);
  let veld = $state<HTMLInputElement | null>(null);

  const label = $derived(beurtNaam(beurt, t()));
  const kant = $derived(kantNaam(beurt.kant, t()));
  // Met een sprekernummer gaat de naam altijd naar de hele spreker; zonder blijft de keuze.
  const metSpreker = $derived(beurt.sprekernummer !== null);
  const standaard = $derived(metSpreker ? t().vergaderSpreker(beurt.sprekernummer ?? 0) : kant);
  const id = $derived(`naam-${String(beurt.index)}`);
  const anderen = $derived(sprekers.filter((s) => s.nummer !== beurt.sprekernummer));
  const magNieuwe = $derived(sprekers.length < MAX_SPREKERS);
  // Een beurt zonder spreker toont bij aanwijzen een gestippeld "+ Spreker".
  const zonderSpreker = $derived(kanAanSpreker && !metSpreker);

  function bijWissel(nieuw: boolean): void {
    open = nieuw;
    if (nieuw) {
      naam = beginNaam(beurt);
      bereik = 'beurt';
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
    void doe(() => geef(naam.trim(), metSpreker ? 'spreker' : bereik));
  }

  // Geen badge: de naam in vet, "Jij" in de actiekleur. Het blijft een knop (naam geven).
  const naamKlas =
    'group relative inline max-w-full cursor-pointer rounded-klein text-left text-basis font-semibold [overflow-wrap:anywhere] decoration-lijn underline-offset-4 hover:underline';
  const sprekerKnop =
    'inline-flex min-h-8 cursor-pointer items-center rounded-klein bg-hover px-2.5 text-basis text-tekst transition-colors outline-none hover:bg-lijn focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus aria-disabled:cursor-default aria-disabled:opacity-60 aria-pressed:text-actie aria-pressed:ring-1 aria-pressed:ring-actie aria-pressed:ring-inset';
  const koppen = 'm-0 text-klein font-medium text-gedempt';
</script>

<Popover.Root {open} onOpenChange={bijWissel}>
  <Popover.Trigger
    class="{naamKlas} {beurt.kant === 'jij' ? 'text-actie' : 'text-tekst'}"
    aria-label={t().vergaderNaamAria(label)}
    title={label}
  >
    <span>{label}</span>
    {#if zonderSpreker}
      <span
        class="pointer-events-none absolute top-0 left-full ml-1.5 rounded-klein border border-dashed border-dim bg-achtergrond px-1.5 py-px text-klein font-normal whitespace-nowrap text-dim no-underline opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100"
        aria-hidden="true">{t().vergaderPlusSpreker}</span
      >
    {/if}
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content
      sideOffset={6}
      align="start"
      aria-label={t().vergaderNaamGeven(metSpreker ? label : kant)}
      onOpenAutoFocus={(e) => {
        e.preventDefault();
        veld?.focus();
        veld?.select();
      }}
      class="z-50 w-[300px] max-w-[calc(100vw-2rem)] rounded-groot border border-lijn bg-verhoogd p-3.5 text-basis shadow-[0_10px_30px_rgba(0,0,0,0.5)]"
    >
      <form class="flex flex-col gap-3" onsubmit={bewaar}>
        <div class="flex flex-col gap-1.5">
          <label for="{id}-veld" class="m-0 text-klein font-medium text-gedempt"
            >{metSpreker ? t().vergaderNaamSprekerKop : t().vergaderNaamKop}</label
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
            >{metSpreker
              ? t().vergaderNaamSprekerHulp(standaard)
              : t().vergaderNaamLeeg(kant)}</span
          >
        </div>
        {#if !metSpreker}
          <fieldset class="m-0 flex flex-col gap-0.5 border-0 p-0">
            <legend class="sr-only">{t().vergaderNaamBereikLabel}</legend>
            <label class="flex h-8 cursor-pointer items-center gap-2.5 text-basis">
              <input
                type="radio"
                name="{id}-bereik"
                value="beurt"
                bind:group={bereik}
                class="size-4 accent-actie"
              />{t().vergaderNaamBeurt}
            </label>
            <label class="flex h-8 cursor-pointer items-center gap-2.5 text-basis">
              <input
                type="radio"
                name="{id}-bereik"
                value="kant"
                bind:group={bereik}
                class="size-4 accent-actie"
              />{t().vergaderNaamKant(label)}
            </label>
          </fieldset>
        {/if}
        <div class="flex justify-end gap-2">
          <Popover.Close>
            {#snippet child({ props })}
              <Button {...props} variant="spook">{t().annuleren}</Button>
            {/snippet}
          </Popover.Close>
          <Button type="submit" aria-disabled={bezig}>{t().vergaderNaamOpslaan}</Button>
        </div>
      </form>

      {#if kanAanSpreker && wijsToe}
        <!-- Deze beurt aan een andere spreker geven, of aan een nieuwe. -->
        <div
          class="mt-3 flex flex-col gap-2 border-t border-lijn pt-3"
          role="group"
          aria-labelledby="{id}-van"
        >
          <h4 id="{id}-van" class={koppen}>{t().vergaderBeurtVanKop}</h4>
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
