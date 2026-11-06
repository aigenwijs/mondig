<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Checkbox } from 'bits-ui';
  import Check from '@lucide/svelte/icons/check';

  // Het eigen vinkvak (ADR 0014): vak van 20 px met een rand in het veldtoken, aangevinkt gevuld
  // in de actiekleur. De omringende label maakt het klikgebied minstens 32 px hoog en laat een
  // klik op de tekst het vak omzetten. De focusring komt uit app.css (:focus-visible).
  let {
    checked = $bindable(false),
    disabled = false,
    children: inhoud
  }: {
    checked?: boolean;
    disabled?: boolean;
    /** De tekst naast het vak; ook de naam voor een schermlezer. */
    children: Snippet;
  } = $props();
</script>

<label
  class="flex min-h-8 cursor-pointer items-start gap-2.5 py-1 text-basis has-[[data-disabled]]:cursor-not-allowed"
>
  <Checkbox.Root
    bind:checked
    {disabled}
    class="mt-0.5 flex size-5 shrink-0 cursor-pointer items-center justify-center rounded-[4px] border border-rand-veld bg-verhoogd text-op-actie transition-colors duration-150 data-[disabled]:cursor-not-allowed data-[disabled]:opacity-50 data-[state=checked]:border-actie data-[state=checked]:bg-actie"
  >
    {#snippet children({ checked: aan })}
      {#if aan}<Check size={14} strokeWidth={3} aria-hidden="true" />{/if}
    {/snippet}
  </Checkbox.Root>
  <span class="min-w-0">{@render inhoud()}</span>
</label>
