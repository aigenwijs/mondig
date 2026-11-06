<script lang="ts">
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import type { Snippet } from 'svelte';
  import { t } from '#lib';

  // De uitklapper "Meer opties" onder een groep instellingen: één niveau diep, niet te
  // nesten. Een gewone knop met aria-expanded en aria-controls, dus Enter en spatie werken
  // vanzelf. De inhoud blijft in de pagina staan (alleen verborgen), zodat de rijen hun
  // eigen stand al hebben geladen als je ze opent. Standaard dicht; de stand wordt niet
  // bewaard.

  interface Props {
    /** De rijen die achter de uitklapper zitten. */
    children: Snippet;
    /** Staat hij bij het begin open? Alleen voor tests en schermafbeeldingen. */
    open?: boolean;
  }
  let { children, open = $bindable(false) }: Props = $props();

  const id = $props.id();
  const inhoudId = `meer-${id}`;
</script>

<div>
  <button
    type="button"
    class="flex w-full cursor-pointer items-center gap-2 px-4 py-2.5 text-left text-basis text-gedempt transition-colors hover:text-tekst"
    aria-expanded={open}
    aria-controls={inhoudId}
    onclick={() => {
      open = !open;
    }}
  >
    {#if open}
      <ChevronDown size={16} class="shrink-0" aria-hidden="true" />
    {:else}
      <ChevronRight size={16} class="shrink-0" aria-hidden="true" />
    {/if}
    {open ? t().minderOpties : t().meerOpties}
  </button>
  <!-- Ingesprongen met een lichte tint, zoals `.kind` in het ontwerp. -->
  <div
    id={inhoudId}
    class="divide-y divide-lijn border-t border-lijn bg-white/[.015] pl-4"
    hidden={!open}
  >
    {@render children()}
  </div>
</div>
