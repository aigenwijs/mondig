<script lang="ts">
  // De ring met een punt erin: "opnemen". Neemt de tekstkleur van de knop over.
  let { grootte = 12 }: { grootte?: number } = $props();
</script>

<span
  class="inline-flex shrink-0 items-center justify-center rounded-full border-2 border-current after:size-1 after:rounded-full after:bg-current"
  style:width="{grootte}px"
  style:height="{grootte}px"
  aria-hidden="true"
></span>
