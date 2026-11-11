<script lang="ts">
  import { tick } from 'svelte';
  import { DropdownMenu } from 'bits-ui';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { Button } from '#lib/components/ui/button/index.js';
  import BewaartermijnRegel from '#lib/components/BewaartermijnRegel.svelte';
  import LijstMeer from '#lib/components/LijstMeer.svelte';
  import ToonInMap from '#lib/components/ToonInMap.svelte';
  import { huidigeTaal, t } from '#lib';
  import { sectieKop, vlak } from '#lib/stijl.js';
  import {
    VERSLAGEN_ERBIJ,
    VERSLAGEN_ZICHTBAAR,
    vergaderingVerslagToonOpgeslagen,
    wistBinnenkort,
    zichtbareVerslagen,
    type VerslagInfo
  } from '#lib/vergadering.js';
  import { gegevensDelen, verslagTitel } from '#lib/verslagtitel.js';

  // De lijst "Eerder opgenomen" (N-C3, ADR 0022, ontwerp ux-naar-9 sectie 2): de laatste vijf
  // bewaarde verslagen, nieuwste boven, met "Meer tonen" (tien erbij) en de telling "5 van 9"
  // (UI-C, lijst met maat). Een klik opent het verslag, het menu
  // (⋯) heeft "Verslag wissen…" met een bevestigvraag. Onder de lijst staat hoe lang Mondig ze
  // bewaart, met een link naar de instelling. Daarboven in het menu "Opnieuw uitschrijven…" (N-C4):
  // dat opent het verslag en dan de dialoog, of staat grijs met de reden als de opname weg is. Zonder verslagen is er geen sectie. De lijst staat
  // op het startscherm, dus alleen als er niets loopt: tijdens opnemen of uitschrijven is er
  // niets te openen en dus ook niets te zien.

  interface Props {
    verslagen: readonly VerslagInfo[];
    /** De tijd van nu in milliseconden, voor "Vandaag" en de melding vooraf. */
    nu: number;
    /** De bewaartermijn in dagen; `null` als die nog niet bekend is. */
    bewaarDagen: number | null;
    /** Is er een opdracht onderweg? Dan is de lijst even niet te bedienen. */
    bezig?: boolean;
    open: (verslag: VerslagInfo) => void;
    wis: (verslag: VerslagInfo) => void;
    /** Opent het verslag en daarna de dialoog "Opnieuw uitschrijven" (N-C4). */
    opnieuw?: (verslag: VerslagInfo) => void;
    /** Een mislukte "Tonen in Verkenner": de melding in gewone taal. */
    bijToonFout?: (melding: string) => void;
    /** Een zin voor de schermlezer, in de statusregio van de pagina ("10 verslagen erbij getoond"). */
    meld?: (zin: string) => void;
  }

  let {
    verslagen,
    nu,
    bewaarDagen,
    bezig = false,
    open,
    wis,
    opnieuw,
    bijToonFout,
    meld
  }: Props = $props();

  // Hoeveel verslagen er in beeld staan: eerst vijf, bij elke klik op "Meer tonen" tien erbij.
  let aantal = $state(VERSLAGEN_ZICHTBAAR);
  // De rij waar de vraag "Wissen?" bij openstaat.
  let wisId = $state<string | null>(null);

  const locale = $derived(huidigeTaal() === 'nl' ? 'nl-NL' : 'en-GB');
  const zichtbaar = $derived(zichtbareVerslagen(verslagen, aantal));

  async function vraagWissen(verslag: VerslagInfo): Promise<void> {
    wisId = verslag.id;
    await tick();
    document.getElementById('wis-annuleer')?.focus();
  }

  async function annuleer(verslag: VerslagInfo): Promise<void> {
    wisId = null;
    await tick();
    document.getElementById(`verslag-meer-${verslag.id}`)?.focus();
  }

  function bevestig(verslag: VerslagInfo): void {
    wisId = null;
    wis(verslag);
  }

  const menuInhoud =
    'z-50 min-w-[210px] rounded-groot border border-lijn bg-verhoogd p-1.5 shadow-[0_8px_24px_rgba(0,0,0,0.35)]';
  const menuItem =
    'flex h-8 cursor-pointer items-center gap-2 rounded-klein px-2.5 text-basis text-fout-tekst outline-none select-none data-highlighted:bg-hover';
  const menuGewoon =
    'flex h-8 cursor-pointer items-center gap-2 rounded-klein px-2.5 text-basis text-tekst outline-none select-none data-highlighted:bg-hover';
  const menuUit =
    'flex cursor-default items-start gap-2 rounded-klein px-2.5 py-1.5 text-basis text-dim outline-none select-none';
</script>

{#if verslagen.length > 0}
  <section class="mt-1.5 flex flex-col gap-2" aria-labelledby="eerder-kop">
    <h2 id="eerder-kop" class="{sectieKop} mb-0">{t().vergaderEerderOpgenomen}</h2>
    <div class="{vlak} flex flex-col">
      <ul class="m-0 flex list-none flex-col p-0" aria-label={t().vergaderEerderLabel}>
        {#each zichtbaar as verslag (verslag.id)}
          {@const titel = verslagTitel(verslag, t())}
          {@const delen = gegevensDelen(verslag, nu, t(), locale)}
          {@const wist = wistBinnenkort(verslag.wistMs, nu)}
          <li class="flex flex-col [&+&]:border-t [&+&]:border-lijn">
            <div class="flex items-center gap-1 pr-2">
              <button
                type="button"
                id="verslag-open-{verslag.id}"
                class="flex min-w-0 flex-1 cursor-pointer flex-col items-start gap-0.5 rounded-groot px-4 py-2.5 text-left transition-colors outline-none hover:bg-hover focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus aria-disabled:cursor-default aria-disabled:opacity-60"
                aria-disabled={bezig}
                title={titel}
                onclick={() => {
                  if (!bezig) open(verslag);
                }}
              >
                <span class="max-w-full truncate font-semibold">{titel}</span>
                <span class="max-w-full truncate text-klein text-dim"
                  >{wist ? `${delen.join(' · ')} · ` : delen.join(' · ')}{#if wist}<span
                      class="text-waarsch"
                      >{wist === 'vandaag'
                        ? t().vergaderWistKortVandaag
                        : t().vergaderWistKortMorgen}</span
                    >{/if}</span
                >
              </button>
              <!-- Alleen bij een verslag waarvan het opgeslagen bestand er nog staat. -->
              {#if verslag.opgeslagenBekend}
                <ToonInMap
                  toon={() => vergaderingVerslagToonOpgeslagen(verslag.id)}
                  bijFout={bijToonFout}
                />
              {/if}
              <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  {#snippet child({ props })}
                    <Button
                      {...props}
                      id="verslag-meer-{verslag.id}"
                      variant="spook"
                      size="icoon"
                      aria-disabled={bezig}
                      aria-label={t().verslagMeerAria(titel)}
                      title={t().verslagMeer}
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
                      if (wisId !== null) e.preventDefault();
                    }}
                  >
                    {#if verslag.opgeslagenBekend}
                      <ToonInMap
                        variant="menu"
                        toon={() => vergaderingVerslagToonOpgeslagen(verslag.id)}
                        bijFout={bijToonFout}
                      />
                    {/if}
                    {#if opnieuw}
                      {#if verslag.opnameAanwezig}
                        <DropdownMenu.Item
                          class={menuGewoon}
                          onSelect={() => {
                            opnieuw(verslag);
                          }}
                          ><RefreshCw size={14} aria-hidden="true" />{t()
                            .opnieuwMenu}</DropdownMenu.Item
                        >
                      {:else}
                        <DropdownMenu.Item class={menuUit} disabled
                          ><RefreshCw size={14} class="mt-1 shrink-0" aria-hidden="true" /><span
                            class="flex flex-col"
                            >{t().opnieuwGewistMenu}<small class="text-klein"
                              >{t().opnieuwGewistReden}</small
                            ></span
                          ></DropdownMenu.Item
                        >
                      {/if}
                      <DropdownMenu.Separator class="my-1 h-px bg-lijn" />
                    {/if}
                    <DropdownMenu.Item
                      class={menuItem}
                      onSelect={() => {
                        void vraagWissen(verslag);
                      }}
                      ><Trash2 size={14} aria-hidden="true" />{t()
                        .verslagWissenMenu}</DropdownMenu.Item
                    >
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            </div>
            {#if wisId === verslag.id}
              <div
                class="m-2 mt-0 flex flex-col gap-2.5 rounded-groot bg-verhoogd px-4 py-3.5"
                role="alertdialog"
                aria-labelledby="wis-kop"
                aria-describedby="wis-uitleg"
              >
                <b id="wis-kop" class="font-semibold">{t().verslagWissenKop}</b>
                <span id="wis-uitleg" class="text-gedempt">{t().verslagWissenTekst}</span>
                <div class="flex flex-wrap gap-2">
                  <Button
                    class="bg-fout-tekst text-achtergrond not-aria-disabled:hover:bg-fout-tekst/90"
                    aria-disabled={bezig}
                    onclick={() => {
                      if (!bezig) bevestig(verslag);
                    }}>{t().verslagWissenBevestig}</Button
                  >
                  <Button
                    id="wis-annuleer"
                    variant="spook"
                    onclick={() => {
                      void annuleer(verslag);
                    }}>{t().annuleren}</Button
                  >
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      <!-- Het volgende stuk van de lijst: tien erbij, met de telling ernaast. -->
      <LijstMeer
        id="eerder-meer"
        getoond={zichtbaar.length}
        totaal={verslagen.length}
        meer={() => {
          aantal += VERSLAGEN_ERBIJ;
          return Promise.resolve();
        }}
        rij={(i: number) => {
          const verslag = zichtbaar[i];
          return verslag ? document.getElementById(`verslag-open-${verslag.id}`) : null;
        }}
        bijErbij={(erbij: number) => {
          meld?.(t().vergaderVerslagenErbij(erbij));
        }}
      />
      {#if bewaarDagen !== null}
        <div
          class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-lijn px-4 py-2 text-klein"
        >
          <span class="ml-auto text-dim"><BewaartermijnRegel dagen={bewaarDagen} /></span>
        </div>
      {/if}
    </div>
  </section>
{/if}
