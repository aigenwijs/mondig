<script lang="ts">
  import Pencil from '@lucide/svelte/icons/pencil';
  import { t } from '#lib';

  // De titel van het verslag (N-C3, ontwerp ux-naar-9 sectie 3): één regel, afgekapt met "…", met
  // een potlood dat bij aanwijzen en bij focus verschijnt. Een klik op het potlood (of "Naam
  // wijzigen" in het menu) maakt er een invoerveld van: Enter bewaart, Escape annuleert. Het veld
  // sluit ook bij het verlaten en bewaart dan.

  interface Props {
    /** De titel zoals het verslag hem nu toont. */
    titel: string;
    /** De tekst in het invoerveld; `null` als de titel niet wordt bewerkt. */
    waarde?: string | null;
    /** De naam is het invoerveld in. */
    start: () => void;
    /** Enter of het veld verlaten: bewaar wat er staat. */
    bewaar: () => void;
    /** Escape: laat de naam zoals hij was. */
    annuleer: () => void;
  }

  let { titel, waarde = $bindable(null), start, bewaar, annuleer }: Props = $props();

  function bijToets(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      bewaar();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      annuleer();
    }
  }
</script>

{#if waarde !== null}
  <input
    id="verslag-naam"
    type="text"
    class="h-9 min-w-0 flex-1 rounded-klein border border-rand-veld bg-verhoogd px-3 text-kop2 tracking-[-0.015em] text-tekst"
    maxlength={100}
    autocomplete="off"
    spellcheck="false"
    aria-label={t().verslagNaamLabel}
    bind:value={waarde}
    onkeydown={bijToets}
    onblur={bewaar}
  />
{:else}
  <div class="group flex min-w-0 flex-1 items-center gap-1.5">
    <h1
      id="vergader-focus"
      class="m-0 min-w-0 truncate text-kop2 tracking-[-0.015em] outline-none"
      title={titel}
      tabindex="-1"
    >
      {titel}
    </h1>
    <button
      type="button"
      id="verslag-naam-knop"
      class="inline-flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-klein text-gedempt opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100 hover:bg-hover hover:text-tekst focus-visible:opacity-100"
      aria-label={t().verslagNaamWijzigenAria(titel)}
      title={t().verslagNaamWijzigen}
      onclick={start}
    >
      <Pencil size={14} aria-hidden="true" />
    </button>
  </div>
{/if}
