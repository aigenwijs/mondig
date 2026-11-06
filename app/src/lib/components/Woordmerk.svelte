<script lang="ts">
  // Het woordmerk "mondig." in Sora 600, met de punt in de actiekleur (groen in donker, paars
  // in licht). Voorlopig als tekst; de definitieve versie met paden komt alleen hier.
  let { klein = false }: { klein?: boolean } = $props();
</script>

<span
  class="font-kop leading-none font-semibold tracking-[-0.045em] text-tekst {klein
    ? 'text-kop2'
    : 'text-[20px]'}"
  aria-label="Mondig"
  role="img">mondig<span class="text-actie">.</span></span
>
