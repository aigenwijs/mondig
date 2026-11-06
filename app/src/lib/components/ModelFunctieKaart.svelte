<script lang="ts">
  import { DropdownMenu } from 'bits-ui';
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import FileText from '@lucide/svelte/icons/file-text';
  import Mic from '@lucide/svelte/icons/mic';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import { controleStand } from '#lib/controlestand.svelte.js';
  import { leesbareGrootte } from '#lib/grootte.js';
  import {
    pastBijComputer,
    passendVoor,
    zoekModel,
    type Gebruik,
    type ModelRegel
  } from '#lib/modellen.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { chipAanbevolen, vlak } from '#lib/stijl.js';
  import type { Foutmelding } from '#lib/taak.js';

  // Een kaart onder "Wat Mondig gebruikt" (gebruikerstest na N-C): per functie het gekozen model met
  // zijn rol als hoofdtekst en de naam klein erbij, één zin over wat je merkt, en Wijzigen.
  // Wijzigen toont alleen de modellen die bij deze functie passen; een model dat er nog niet
  // staat, wordt gedownload en daarna gebruikt. "Past bij deze computer" komt uit de laatste
  // controle van "Mondig opnieuw afstellen". Een model dat al binnenkomt, krijgt deze functie
  // erbij; een model dat niet op de schijf past, staat uit met de reden. De nieuwste keuze wint:
  // een andere download die deze functie zou overnemen, verliest hem (Rust, `model_kiezen`).

  interface Props {
    gebruik: Gebruik;
  }
  let { gebruik }: Props = $props();

  const taal = $derived(huidigeTaal());
  const overzicht = $derived(modellenStand.overzicht);
  const model = $derived(
    overzicht
      ? zoekModel(
          overzicht.modellen,
          gebruik === 'dicteren' ? overzicht.dicteren : overzicht.bestanden
        )
      : undefined
  );
  const keuzes = $derived(overzicht ? passendVoor(overzicht.modellen, gebruik) : []);
  const functie = $derived(gebruik === 'dicteren' ? t().taakDicteren : t().taakBestanden);

  let fout = $state<Foutmelding | null>(null);
  let kaart = $state<HTMLElement | null>(null);

  /** De naam zoals Modellen hem toont. */
  function naamVan(regel: ModelRegel): string {
    return t().modelTekst(regel.id)?.naam ?? regel.naam;
  }

  /** Eén zin over wat je merkt van dit model bij deze functie. */
  function zinVan(regel: ModelRegel): string {
    const tekst = t().modelTekst(regel.id);
    const tijd = gebruik === 'dicteren' ? tekst?.dicterenTijd : tekst?.uurOpname;
    if (tijd === undefined) return '';
    return gebruik === 'dicteren' ? t().kaartDicterenSub(tijd) : t().kaartBestandenSub(tijd);
  }

  const sub = $derived.by(() => {
    if (!model) return '';
    if (model.status !== 'compleet') return t().kaartOnbekend;
    return zinVan(model);
  });

  // Komt er een ander model voor deze functie binnen, dan zeggen we dat.
  const volgtId = $derived(modellenStand.volgtVoor(gebruik));
  const volgt = $derived.by(() => {
    if (volgtId === null || volgtId === model?.id) return null;
    const regel = overzicht ? zoekModel(overzicht.modellen, volgtId) : undefined;
    return regel ? t().kaartVolgt(t().modelRolNaam(regel.rol)) : null;
  });

  /** Wat er onder een keuze in het menu staat, en of hij te kiezen is. */
  function keuzeStand(regel: ModelRegel): { zin: string; uit: boolean } {
    const zin = zinVan(regel);
    if (regel.status === 'compleet') return { zin, uit: false };
    if (modellenStand.isActief(regel.id))
      return { zin: `${zin} ${t().functieWordtGedownload}`, uit: false };
    if (regel.tekortBytes > 0) {
      return {
        zin: t().functieTeWeinigRuimte(leesbareGrootte(regel.tekortBytes, taal)),
        uit: true
      };
    }
    const nog = leesbareGrootte(Math.max(0, regel.grootteBytes - regel.aanwezigBytes), taal);
    return { zin: `${zin} ${t().functieEerstDownloaden(nog)}`, uit: false };
  }

  async function kies(regel: ModelRegel): Promise<void> {
    fout = null;
    // Het huidige model nog eens kiezen doet alleen iets als er een download klaarstaat om
    // deze functie over te nemen: dan blijft het huidige.
    if (regel.id === model?.id && volgt === null) return;
    fout = await modellenStand.kies(gebruik, regel);
  }

  const menuInhoud =
    'z-50 w-[max(var(--bits-floating-anchor-width),280px)] max-w-[calc(100vw-32px)] rounded-groot border border-lijn bg-verhoogd p-1.5 shadow-[0_8px_24px_rgba(0,0,0,0.35)]';
  const menuItem =
    'flex cursor-pointer items-start gap-2.5 rounded-klein px-2.5 py-2 text-basis text-tekst outline-none select-none data-highlighted:bg-hover data-disabled:cursor-default';
</script>

<div bind:this={kaart} class="{vlak} flex flex-col gap-1.5 px-4 py-3.5" data-functie={gebruik}>
  <span class="flex items-center gap-1.5 text-klein font-medium whitespace-nowrap text-gedempt">
    {#if gebruik === 'dicteren'}
      <Mic size={14} class="shrink-0" aria-hidden="true" />
    {:else}
      <FileText size={14} class="shrink-0" aria-hidden="true" />
    {/if}
    {functie}
  </span>
  <span class="text-klein text-dim"
    >{gebruik === 'dicteren' ? t().functieDicterenWaarom : t().functieBestandenWaarom}</span
  >
  {#if model}
    <span class="mt-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
      <span class="font-kop text-midden font-semibold">{t().modelRolNaam(model.rol)}</span>
      <span class="text-klein text-dim">{naamVan(model)}</span>
    </span>
    <span class="text-klein leading-snug text-gedempt">{volgt ?? sub}</span>
    {#if pastBijComputer(controleStand.controle, gebruik, model.id)}
      <span class="{chipAanbevolen} self-start">{t().pastBijComputer}</span>
    {/if}
  {/if}
  {#if fout}
    <span class="text-klein text-fout-tekst" role="alert">{fout.melding}</span>
  {/if}
  <div class="mt-auto flex items-center pt-1.5">
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="tweede"
            class="ml-auto text-klein"
            aria-label={t().functieWijzigenAria(functie)}
            >{t().wijzigen}<ChevronDown size={14} aria-hidden="true" /></Button
          >
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <!-- Uitgelijnd op de linkerrand van de kaart, zodat het menu in de inhoudskolom blijft en
             niet over de zijbalk valt. -->
        <DropdownMenu.Content
          customAnchor={kaart}
          side="bottom"
          align="start"
          sideOffset={6}
          collisionPadding={16}
          class={menuInhoud}
        >
          {#each keuzes as regel (regel.id)}
            {@const huidig = regel.id === model?.id}
            {@const stand = keuzeStand(regel)}
            <DropdownMenu.Item
              class={menuItem}
              disabled={stand.uit}
              onSelect={() => void kies(regel)}
            >
              <span class="mt-0.5 flex size-4 shrink-0 items-center justify-center text-actie">
                {#if huidig}<Check size={14} aria-label={t().functieHuidig} />{/if}
              </span>
              <span class="flex min-w-0 flex-col gap-0.5">
                <span class="flex flex-wrap items-baseline gap-x-2">
                  <b class="font-semibold {stand.uit ? 'text-gedempt' : ''}"
                    >{t().modelRolNaam(regel.rol)}</b
                  >
                  <span class="text-klein text-dim">{naamVan(regel)}</span>
                </span>
                <span class="text-klein leading-snug {stand.uit ? 'text-waarsch' : 'text-gedempt'}"
                  >{stand.zin}</span
                >
                {#if pastBijComputer(controleStand.controle, gebruik, regel.id)}
                  <span class="{chipAanbevolen} mt-0.5 self-start">{t().pastBijComputer}</span>
                {/if}
              </span>
            </DropdownMenu.Item>
          {/each}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  </div>
</div>
