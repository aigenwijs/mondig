<script lang="ts">
  import { tick } from 'svelte';
  import BewaarMelding from '#lib/components/BewaarMelding.svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import { t } from '#lib';
  import {
    bouwsteenNaam,
    plekVanBouwsteen,
    schuifBouwsteen,
    steenVolgorde,
    verplaatsBouwsteen,
    wisselBouwsteen,
    type Bouwsteen
  } from '#lib/bewaren.js';
  import { bewaarStand } from '#lib/bewarenstand.svelte.js';

  // De bestandsnaam uit blokjes: aan- en uitzetten, en de volgorde bepalen. Slepen met de
  // muis (pointer-events, want HTML5 drag-and-drop botst met het slepen van bestanden in
  // Tauri), en zonder slepen (WCAG 2.2, 2.5.7): met focus op een blokje verschuift Alt + pijl
  // links of rechts het blokje, met een aankondiging voor schermlezers. De volgorde van de
  // gekozen blokjes is de volgorde in de naam; Rust gebruikt de bewaarde lijst zoals hij is.

  const gekozen = $derived(bewaarStand.stand?.bouwstenen ?? []);
  // Tijdens het slepen de voorlopige volgorde; pas bij loslaten wordt hij bewaard.
  let sleepLijst = $state<Bouwsteen[] | null>(null);
  let sleepSteen = $state<Bouwsteen | null>(null);
  // Wat een schermlezer hoort na een verschuiving of wissel.
  let aankondiging = $state('');

  const getoond = $derived(sleepLijst ?? gekozen);
  const knoppen: Partial<Record<Bouwsteen, HTMLButtonElement>> = {};
  let groep: HTMLDivElement | undefined;
  // Een begonnen sleepbeweging; pas na een paar pixels is het slepen en geen klik.
  let aanzet: { steen: Bouwsteen; x: number; y: number; id: number } | null = null;
  let slaClickOver = false;

  const steen =
    'inline-flex h-8 items-center rounded-klein border px-[9px] text-klein transition-colors select-none';

  function meld(steenNaam: Bouwsteen, lijst: readonly Bouwsteen[]): void {
    const plek = plekVanBouwsteen(lijst, steenNaam);
    if (plek === null) return;
    aankondiging = t().bestandsnaamPlek(bouwsteenNaam(steenNaam, t()), plek, lijst.length);
  }

  async function bewaarVolgorde(lijst: Bouwsteen[], metFocus: Bouwsteen | null): Promise<void> {
    await bewaarStand.zetBouwstenen(lijst, t().somsGingErIetsMis);
    if (metFocus !== null) {
      // Het blokje is in de pagina verplaatst; daarbij raakt het zijn focus kwijt.
      await tick();
      knoppen[metFocus]?.focus();
    }
  }

  async function wissel(s: Bouwsteen): Promise<void> {
    if (slaClickOver) return;
    await bewaarStand.zetBouwstenen(wisselBouwsteen(gekozen, s), t().somsGingErIetsMis);
    aankondiging = t().bestandsnaamVoorbeeld(bewaarStand.stand?.voorbeeld ?? '');
  }

  async function opToets(event: KeyboardEvent, s: Bouwsteen): Promise<void> {
    if (!event.altKey || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return;
    event.preventDefault();
    if (!gekozen.includes(s)) return;
    const nieuw = schuifBouwsteen(gekozen, s, event.key === 'ArrowLeft' ? -1 : 1);
    if (nieuw.join() === gekozen.join()) {
      // Staat al aan het begin of eind: toch zeggen waar hij staat.
      meld(s, gekozen);
      return;
    }
    meld(s, nieuw);
    await bewaarVolgorde(nieuw, s);
  }

  function pointerDown(event: PointerEvent, s: Bouwsteen): void {
    if (event.button !== 0 || !gekozen.includes(s)) return;
    aanzet = { steen: s, x: event.clientX, y: event.clientY, id: event.pointerId };
  }

  function pointerMove(event: PointerEvent): void {
    if (aanzet?.id !== event.pointerId) return;
    if (sleepSteen === null) {
      if (Math.hypot(event.clientX - aanzet.x, event.clientY - aanzet.y) < 5) return;
      sleepSteen = aanzet.steen;
      sleepLijst = [...gekozen];
      // Vanaf nu volgt de container de aanwijzer, ook buiten de blokjes en na het verplaatsen.
      groep?.setPointerCapture(aanzet.id);
    }
    const lijst = sleepLijst ?? gekozen;
    // Het blokje waar de aanwijzer het dichtst bij zit bepaalt de nieuwe plek.
    let beste = -1;
    let afstand = Infinity;
    lijst.forEach((s, i) => {
      const rechthoek = knoppen[s]?.getBoundingClientRect();
      if (!rechthoek) return;
      const d = Math.hypot(
        event.clientX - (rechthoek.left + rechthoek.width / 2),
        event.clientY - (rechthoek.top + rechthoek.height / 2)
      );
      if (d < afstand) {
        afstand = d;
        beste = i;
      }
    });
    if (beste !== -1 && beste !== lijst.indexOf(sleepSteen)) {
      sleepLijst = verplaatsBouwsteen(lijst, sleepSteen, beste);
    }
  }

  function pointerUp(event: PointerEvent): void {
    if (aanzet?.id !== event.pointerId) return;
    const klaar = sleepSteen;
    const lijst = sleepLijst;
    aanzet = null;
    sleepSteen = null;
    sleepLijst = null;
    if (klaar === null || lijst === null) return;
    // Na het slepen volgt er nog een click op het blokje; die mag niet wisselen.
    slaClickOver = true;
    setTimeout(() => {
      slaClickOver = false;
    }, 0);
    if (lijst.join() !== gekozen.join()) {
      meld(klaar, lijst);
      void bewaarVolgorde(lijst, null);
    }
  }

  function pointerAnnuleer(): void {
    aanzet = null;
    sleepSteen = null;
    sleepLijst = null;
  }
</script>

<div class="flex flex-col gap-2.5 px-4 py-3.5">
  <!-- Geen rijLabel: die heeft een flex-basis voor naast elkaar, hier staat alles onder elkaar. -->
  <div class="flex min-w-0 flex-col gap-0.5">
    <RijKop
      kopId="naam-kop"
      label={t().bestandsnaamKop}
      hulpId="naam-hulp"
      hulp={t().bestandsnaamHulp}
    />
    <BewaarMelding rij="naam" />
  </div>
  <div
    bind:this={groep}
    class="flex flex-wrap gap-1.5"
    role="group"
    aria-labelledby="naam-kop"
    aria-describedby="naam-hulp"
    onpointermove={pointerMove}
    onpointerup={pointerUp}
    onpointercancel={pointerAnnuleer}
  >
    {#each steenVolgorde(getoond) as s (s)}
      {@const aan = getoond.includes(s)}
      <button
        bind:this={knoppen[s]}
        type="button"
        class="{steen} {aan
          ? 'cursor-grab touch-none border-actie bg-verhoogd text-tekst'
          : 'cursor-pointer border-dashed border-lijn bg-verhoogd text-dim hover:text-tekst'} {sleepSteen ===
        s
          ? 'cursor-grabbing ring-2 ring-actie'
          : ''}"
        aria-pressed={aan}
        aria-keyshortcuts={aan ? 'Alt+ArrowLeft Alt+ArrowRight' : undefined}
        onclick={() => void wissel(s)}
        onkeydown={(event) => void opToets(event, s)}
        onpointerdown={(event) => {
          pointerDown(event, s);
        }}>{bouwsteenNaam(s, t())}</button
      >
    {/each}
  </div>
  <p
    class="m-0 self-start rounded-klein bg-verhoogd px-2.5 py-1.5 font-mono text-klein break-words text-gedempt"
  >
    {bewaarStand.stand ? t().bestandsnaamVoorbeeld(bewaarStand.stand.voorbeeld) : ''}
  </p>
  <!-- De aankondiging na een verschuiving of wissel; verborgen, alleen voor schermlezers. -->
  <p class="sr-only" aria-live="polite">{aankondiging}</p>
</div>
