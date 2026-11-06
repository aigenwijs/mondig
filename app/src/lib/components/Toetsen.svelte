<script lang="ts">
  // Een sneltoets als losse toetsjes ("Ctrl" + "Spatie"). Een eindigende plus (Ctrl+Alt+,
  // tijdens het vangen) blijft staan.
  let { label, groot = false }: { label: string; groot?: boolean } = $props();

  const delen = $derived(label.split('+'));
  const toets = $derived(
    groot
      ? 'h-[34px] px-3 font-kop text-midden font-semibold'
      : 'h-6 px-2 font-tekst text-klein font-medium'
  );
</script>

<span class="inline-flex flex-wrap items-center gap-1.5">
  {#each delen as deel, i (i)}
    {#if i > 0}<span class="text-gedempt">+</span>{/if}
    {#if deel !== ''}
      <kbd
        class="inline-flex items-center rounded-klein border border-b-2 border-lijn bg-verhoogd text-tekst {toets}"
        >{deel}</kbd
      >
    {/if}
  {/each}
</span>
