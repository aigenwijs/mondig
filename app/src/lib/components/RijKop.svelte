<script lang="ts">
  import { rijNaam } from '#lib/stijl.js';

  // De kop van een instellingenrij: het label met de uitleg als korte beschrijving in grijs
  // eronder. Zonder `hulp` staat er alleen het label.

  interface Props {
    /** Het id van de kop; de bediening verwijst ernaar met aria-labelledby. */
    kopId: string;
    label: string;
    /** Het id van de beschrijving; de bediening verwijst ernaar met aria-describedby. */
    hulpId?: string;
    hulp?: string;
  }
  let { kopId, label, hulpId, hulp }: Props = $props();
</script>

<div class="flex flex-col gap-0.5">
  <h3 id={kopId} class={rijNaam}>{label}</h3>
  {#if hulp && hulpId}
    <p id={hulpId} class="m-0 text-klein text-dim">{hulp}</p>
  {/if}
</div>
