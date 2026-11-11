<script lang="ts">
  import { onMount } from 'svelte';
  import { DropdownMenu } from 'bits-ui';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { bestandToonOpgeslagen } from '#lib/bewaren.js';
  import { haalPlatform } from '#lib/platform.js';
  import type { Platform } from '#lib/sneltoets.js';
  import { toonInMapLabel } from '#lib/toonInMap.js';
  import { foutUitCommando } from '#lib/taak.js';

  // "Tonen in Verkenner" (ontwerpsysteem §5): opent Verkenner of Finder met het opgeslagen
  // bestand geselecteerd. Een icoonknop met map-icoon (aria-label en tooltip gelijk), of een
  // menu-item met icoon en tekst. Alleen tekenen bij een bestand dat bestaat; dat bepaalt de
  // ouder. Het label hangt aan het platform, dat Rust meldt (`sneltoets_info`); tot het
  // antwoord er is, of buiten de app, geldt Windows.

  // Wat er getoond wordt, geeft de ouder op één van twee manieren: een `taak` (een uitschrift
  // of net opgeslagen verslag, `bestand_toon_opgeslagen`) of een eigen `toon` die Rust aanroept
  // (een bewaard verslag in Eerder opgenomen, dat zijn pad zelf bewaart).
  type Bron =
    | {
        /** De taak waarvan Mondig het bestand opsloeg (`bestand_toon_opgeslagen`). */
        taak: number;
        toon?: never;
      }
    | {
        /** Toont zelf het bestand; een fout werpt de aanroep, in de vorm van een Rust-melding. */
        toon: () => Promise<void>;
        taak?: never;
      };
  type Props = Bron & {
    /** Een icoonknop, of een item in een `DropdownMenu.Content`. */
    variant?: 'icoon' | 'menu';
    /** Het platform; zonder haalt het onderdeel het zelf op. */
    platform?: Platform;
    /** Een mislukte aanroep: de melding in gewone taal. */
    bijFout?: ((melding: string) => void) | undefined;
    /** Extra klassen voor de knop of het item. */
    class?: string;
  };
  let {
    taak,
    toon: eigenToon,
    variant = 'icoon',
    platform,
    bijFout,
    class: klasse
  }: Props = $props();

  let opgehaald = $state<Platform | null>(null);
  const label = $derived(toonInMapLabel(platform ?? opgehaald ?? 'windows', t()));

  onMount(() => {
    if (platform !== undefined) return;
    void haalPlatform().then((p) => {
      opgehaald = p;
    });
  });

  async function toon(): Promise<void> {
    try {
      if (eigenToon) await eigenToon();
      else if (taak !== undefined) await bestandToonOpgeslagen(taak);
    } catch (e) {
      bijFout?.(foutUitCommando(e, t().somsGingErIetsMis).melding);
    }
  }

  const menuItem =
    'flex h-8 cursor-pointer items-center gap-2 rounded-klein px-2.5 text-basis text-tekst outline-none select-none data-highlighted:bg-hover';
</script>

{#if variant === 'menu'}
  <DropdownMenu.Item class="{menuItem} {klasse ?? ''}" onSelect={toon}>
    <FolderSearch size={16} aria-hidden="true" />{label}
  </DropdownMenu.Item>
{:else}
  <Button
    variant="spook"
    size="icoon"
    class={klasse}
    aria-label={label}
    title={label}
    onclick={toon}
  >
    <FolderSearch size={16} aria-hidden="true" />
  </Button>
{/if}
