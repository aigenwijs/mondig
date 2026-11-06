<script lang="ts">
  import { t } from '#lib';
  import { openVermeldingsadres } from '#lib/versie.js';

  // Een link naar een pagina buiten de app. Mondig opent hem zelf in de standaardbrowser:
  // een gewone link zou het venster van Mondig vervangen. Lukt dat niet, dan staat het
  // adres er nog, om te kopiëren.
  let { href, tekst = href }: { href: string; tekst?: string } = $props();

  let mislukt = $state(false);

  async function open(event: MouseEvent): Promise<void> {
    event.preventDefault();
    mislukt = false;
    try {
      await openVermeldingsadres(href);
    } catch {
      mislukt = true;
    }
  }
</script>

<!-- Het adres is een externe pagina, geen route van de app. -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<a
  {href}
  class="break-all text-tekst underline decoration-lijn underline-offset-2 hover:decoration-tekst"
  onclick={open}>{tekst}</a
>
{#if mislukt}
  <span class="mt-1 block text-klein text-fout-tekst" role="alert">{t().adresOpenenMislukt}</span>
{/if}
