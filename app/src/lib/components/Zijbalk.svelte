<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import type { Component } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import CircleDot from '@lucide/svelte/icons/circle-dot';
  import Download from '@lucide/svelte/icons/download';
  import FileText from '@lucide/svelte/icons/file-text';
  import Info from '@lucide/svelte/icons/info';
  import Mic from '@lucide/svelte/icons/mic';
  import Package from '@lucide/svelte/icons/package';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import { Progress } from '#lib/components/ui/progress/index.js';
  import Woordmerk from '#lib/components/Woordmerk.svelte';
  import { t } from '#lib';
  import { isActief, isBezig, procentVan, type DownloadItem } from '#lib/downloadrij.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { chipNieuw } from '#lib/stijl.js';
  import { versieStand } from '#lib/versie.svelte.js';

  // De vaste navigatie (ADR 0014): Dicteren, Vergadering en Bestanden bovenaan, Modellen en
  // Instellingen onderaan. Boven Modellen staan alle downloads uit de rij, op elke pagina:
  // naam, voortgang of stand, klikbaar naar Modellen. Klaar verdwijnt na een paar seconden, een
  // fout blijft tot de pagina Modellen gezien is. Onder 640 CSS-px (smal venster of 200 procent
  // zoom) blijven alleen de iconen over, met een tooltip; de downloads worden dan één icoon met
  // een ring voor de voortgang en een teller.

  type Pad = '/' | '/vergadering' | '/bestanden' | '/modellen' | '/instellingen' | '/over';

  const pad = $derived(page.url.pathname);
  const actief = $derived<Pad | null>(
    pad === '/'
      ? '/'
      : pad.startsWith('/vergadering')
        ? '/vergadering'
        : pad.startsWith('/bestanden')
          ? '/bestanden'
          : pad.startsWith('/modellen')
            ? '/modellen'
            : pad.startsWith('/instellingen')
              ? '/instellingen'
              : pad.startsWith('/over')
                ? '/over'
                : null
  );

  // Een tooltip naast het icoon als de zijbalk is ingeklapt. Vast gepositioneerd, zodat de
  // zijbalk hem niet afsnijdt.
  let ingeklapt = $state(false);
  let tip = $state<{ tekst: string; top: number; left: number } | null>(null);

  function toonTip(el: HTMLElement, tekst: string): void {
    const vak = el.getBoundingClientRect();
    tip = { tekst, top: vak.top + vak.height / 2, left: vak.right + 8 };
  }

  function tipBijHover(event: Event, tekst: string): void {
    if (!ingeklapt) return;
    toonTip(event.currentTarget as HTMLElement, tekst);
  }

  function verbergTip(): void {
    tip = null;
  }

  onMount(() => {
    void versieStand.laad();
    const smal = window.matchMedia('(max-width: 639px)');
    const opWissel = (): void => {
      ingeklapt = smal.matches;
      tip = null;
    };
    opWissel();
    smal.addEventListener('change', opWissel);
    return () => {
      smal.removeEventListener('change', opWissel);
    };
  });

  const downloads = $derived(modellenStand.zijbalk);
  const actieveDownloads = $derived(downloads.filter((d) => isActief(d.toestand)).length);
  const heeftFout = $derived(downloads.some((d) => d.toestand.type === 'fout'));
  const ringProcent = $derived(modellenStand.procent);
  // De ring om het icoon in de ingeklapte zijbalk: omtrek van een cirkel met straal 9.
  const OMTREK = 2 * Math.PI * 9;

  function naamVan(model: string): string {
    const regel = modellenStand.overzicht?.modellen.find((m) => m.id === model);
    return t().modelTekst(model)?.naam ?? regel?.naam ?? model;
  }

  /** De stand van een download in een paar woorden, voor naast de naam. */
  function standKort(d: DownloadItem): string {
    switch (d.toestand.type) {
      case 'wacht':
        return t().zijbalkWacht;
      case 'bezig':
        return `${String(procentVan(d.voortgang))}%`;
      case 'nieuwe_poging':
        return t().zijbalkOpnieuw(d.toestand.poging, d.toestand.van);
      case 'klaar':
        return t().zijbalkKlaar;
      case 'fout':
        return t().zijbalkMislukt;
      case 'gestopt':
        return t().zijbalkGestopt;
      case 'onderbroken':
        return t().zijbalkOnderbroken;
    }
  }

  /** Hetzelfde voor een schermlezer: procenten voluit. */
  function standAria(d: DownloadItem): string {
    return d.toestand.type === 'bezig' ? t().zijbalkProcent(procentVan(d.voortgang)) : standKort(d);
  }

  const item =
    'relative flex h-[34px] w-full shrink-0 items-center gap-2.5 rounded-klein px-2.5 text-left text-basis font-medium transition-colors max-[639px]:justify-center max-[639px]:px-0';
  const naam = 'truncate max-[639px]:hidden';
</script>

{#snippet link(
  doel: Pad,
  Icoon: Component<{ size?: number; 'aria-hidden'?: 'true' }>,
  label: string,
  badge: string | null = null
)}
  <a
    href={resolve(doel)}
    class="{item} {actief === doel
      ? 'bg-hover text-tekst'
      : 'text-gedempt hover:bg-hover hover:text-tekst'}"
    aria-current={actief === doel ? 'page' : undefined}
    aria-label={ingeklapt ? label : undefined}
    onmouseenter={(e) => {
      tipBijHover(e, label);
    }}
    onfocus={(e) => {
      tipBijHover(e, label);
    }}
    onmouseleave={verbergTip}
    onblur={verbergTip}
  >
    <Icoon size={16} aria-hidden="true" />
    <span class={naam}>{label}</span>
    {#if badge}<span class="{chipNieuw} ml-auto max-[639px]:hidden">{badge}</span>{/if}
  </a>
{/snippet}

<aside
  class="flex h-dvh w-52 shrink-0 flex-col gap-0.5 overflow-y-auto bg-vlak px-2.5 pt-[18px] pb-3 max-[639px]:w-[60px] max-[639px]:px-2"
>
  <div
    class="flex shrink-0 items-center px-2 pt-0.5 pb-5 max-[639px]:justify-center max-[639px]:px-0"
  >
    <span class="flex flex-col gap-1 max-[639px]:hidden">
      <Woordmerk />
      <small class="text-sectie leading-none text-dim">{t().doorAigenwijs}</small>
    </span>
    <span
      class="hidden font-kop text-[20px] leading-none font-semibold tracking-[-0.045em] max-[639px]:inline"
      aria-hidden="true">m<span class="text-actie">.</span></span
    >
  </div>

  <nav aria-label={t().hoofdmenu} class="flex flex-1 flex-col gap-0.5">
    {@render link('/', Mic, t().homeDictate)}
    {@render link('/vergadering', CircleDot, t().menuVergadering)}
    {@render link('/bestanden', FileText, t().menuBestanden)}
    <div class="mt-auto"></div>
    {#if downloads.length > 0}
      <ul
        aria-label={t().zijbalkDownloadsKop}
        class="m-0 mt-1 mb-1 flex list-none flex-col gap-0.5 rounded-klein bg-verhoogd p-1 max-[639px]:hidden"
        data-zijbalk-downloads
      >
        {#each downloads as d (d.id)}
          {@const naam = naamVan(d.model)}
          <li>
            <a
              href={resolve('/modellen')}
              class="flex flex-col gap-1 rounded-klein px-1.5 py-1.5 text-sectie text-gedempt transition-colors hover:bg-hover"
              aria-label={t().zijbalkItemAria(naam, standAria(d))}
              data-download-stand={d.toestand.type}
            >
              <span class="flex items-center gap-1.5" aria-hidden="true">
                {#if d.toestand.type === 'fout'}
                  <CircleAlert size={12} class="shrink-0 text-fout-tekst" />
                {:else if d.toestand.type === 'klaar'}
                  <Check size={12} class="shrink-0 text-tekst" />
                {/if}
                <b class="min-w-0 flex-1 truncate font-medium text-tekst">{naam}</b>
                <span class="shrink-0 {d.toestand.type === 'fout' ? 'text-fout-tekst' : ''}"
                  >{standKort(d)}</span
                >
              </span>
              {#if isBezig(d.toestand)}
                <Progress
                  value={procentVan(d.voortgang)}
                  aria-hidden="true"
                  class="h-1.5 bg-lijn"
                />
              {/if}
            </a>
          </li>
        {/each}
      </ul>
      <!-- Ingeklapt: één icoon met een ring voor wat nu binnenkomt en een teller. -->
      <a
        href={resolve('/modellen')}
        class="relative hidden h-[34px] w-full shrink-0 items-center justify-center rounded-klein text-gedempt transition-colors hover:bg-hover hover:text-tekst max-[639px]:flex"
        aria-label={t().zijbalkIngeklaptAria(actieveDownloads || downloads.length)}
        onmouseenter={(e) => {
          tipBijHover(e, t().zijbalkIngeklaptAria(actieveDownloads || downloads.length));
        }}
        onfocus={(e) => {
          tipBijHover(e, t().zijbalkIngeklaptAria(actieveDownloads || downloads.length));
        }}
        onmouseleave={verbergTip}
        onblur={verbergTip}
        data-zijbalk-ring
      >
        <span
          class="relative inline-flex size-[22px] items-center justify-center"
          aria-hidden="true"
        >
          <svg viewBox="0 0 22 22" class="absolute inset-0 -rotate-90">
            <circle
              cx="11"
              cy="11"
              r="9"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              class="text-lijn"
            />
            <circle
              cx="11"
              cy="11"
              r="9"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              class="text-actie"
              stroke-dasharray={OMTREK}
              stroke-dashoffset={OMTREK * (1 - ringProcent / 100)}
            />
          </svg>
          <Download size={11} />
          {#if actieveDownloads > 1}
            <span
              class="absolute -top-1.5 -right-2 min-w-4 rounded-full bg-tekst px-1 text-center text-sectie leading-4 font-semibold text-achtergrond"
              >{actieveDownloads}</span
            >
          {/if}
          {#if heeftFout}
            <span class="absolute -bottom-0.5 -right-0.5 size-2 rounded-full bg-fout-tekst"></span>
          {/if}
        </span>
      </a>
    {/if}
    {@render link(
      '/modellen',
      Package,
      t().menuModellen,
      modellenStand.heeftNieuw && actief !== '/modellen' ? t().labelNieuw : null
    )}
    {@render link('/instellingen', SlidersHorizontal, t().instellingen)}
  </nav>

  <!-- De voet (ADR 0016, aanvulling 2-11-2026): één regel met links de lokaal-status en rechts
       de versie, klikbaar naar Over Mondig. Ingeklapt alleen de twee iconen onder elkaar. -->
  <div
    class="mt-2 flex shrink-0 items-center gap-2 border-t border-lijn px-2.5 pt-2.5 pb-0.5 text-sectie text-gedempt max-[639px]:flex-col max-[639px]:gap-2.5 max-[639px]:px-0"
  >
    <span
      class="inline-flex items-center gap-1.5 max-[639px]:justify-center"
      title={t().lokaalRegel}
      role="img"
      aria-label={t().lokaalRegel}
      onmouseenter={(e) => {
        tipBijHover(e, t().lokaalRegel);
      }}
      onmouseleave={verbergTip}
    >
      <ShieldCheck size={14} class="text-actie" aria-hidden="true" />
      <span class="max-[639px]:hidden" aria-hidden="true">{t().labelLokaal}</span>
    </span>
    <a
      href={resolve('/over')}
      class="ml-auto inline-flex items-center gap-1.5 rounded-klein p-1 transition-colors hover:bg-hover hover:text-tekst max-[639px]:m-0 {actief ===
      '/over'
        ? 'bg-hover text-tekst'
        : ''}"
      aria-current={actief === '/over' ? 'page' : undefined}
      aria-label={versieStand.info ? t().voetAria(versieStand.info.versie) : t().overMondig}
      onmouseenter={(e) => {
        tipBijHover(e, t().overMondig);
      }}
      onfocus={(e) => {
        tipBijHover(e, t().overMondig);
      }}
      onmouseleave={verbergTip}
      onblur={verbergTip}
    >
      <Info size={14} aria-hidden="true" />
      {#if versieStand.info}
        <span class="max-[639px]:hidden" aria-hidden="true"
          >{versieStand.info.versie.replace(/-dev$/, '')}</span
        >
      {/if}
    </a>
  </div>
</aside>

{#if tip}
  <div
    class="pointer-events-none fixed z-50 -translate-y-1/2 rounded-klein bg-tekst px-2 py-1 text-klein whitespace-nowrap text-achtergrond shadow-sm"
    style:top="{tip.top}px"
    style:left="{tip.left}px"
    aria-hidden="true"
  >
    {tip.tekst}
  </div>
{/if}
