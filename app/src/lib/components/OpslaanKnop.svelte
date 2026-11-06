<script lang="ts">
  import { DropdownMenu } from 'bits-ui';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Download from '@lucide/svelte/icons/download';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { FORMATEN, formaatExtensie, formaatNaam, type Formaat } from '#lib/bewaren.js';

  // De splitknop van 4-C (ontwerp 4-C, scherm 2): links het standaardformaat, rechts het menu met
  // de andere formaten en "Opslaan op een andere plek". Gedeeld door het transcript van een
  // bestand en het verslag van een vergadering.

  let {
    standaard,
    bezig,
    opslaan,
    kort = false,
    ref = $bindable(null)
  }: {
    standaard: Formaat;
    bezig: boolean;
    opslaan: (formaat: Formaat | null, anderePlek: boolean) => void;
    /** Op een smal venster alleen "Opslaan"; het formaat staat dan in de tooltip. */
    kort?: boolean;
    ref?: HTMLElement | null;
  } = $props();

  const menuItem =
    'flex h-8 cursor-pointer items-center justify-between gap-4 rounded-klein px-2.5 text-basis text-tekst outline-none select-none data-highlighted:bg-hover';
</script>

<div class="inline-flex">
  <Button
    class="rounded-r-none"
    bind:ref
    aria-disabled={bezig}
    title={t().opslaanTitel(formaatNaam(standaard, t()), formaatExtensie(standaard))}
    onclick={() => {
      opslaan(null, false);
    }}
    ><Download size={16} aria-hidden="true" />{#if kort}{t().opslaanKort}{:else}{t().opslaanAls(
        formaatNaam(standaard, t())
      )}{/if}</Button
  >
  <DropdownMenu.Root>
    <DropdownMenu.Trigger
      class="inline-flex min-h-8 cursor-pointer items-center rounded-r-klein border-l border-black/25 bg-actie px-2 text-op-actie transition-colors hover:bg-actie-hover aria-disabled:cursor-default aria-disabled:opacity-50"
      aria-label={t().opslaanMenu}
      disabled={bezig}
    >
      <ChevronDown size={16} aria-hidden="true" />
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content
        align="end"
        sideOffset={6}
        class="z-50 w-[250px] rounded-groot border border-lijn bg-verhoogd p-1.5 shadow-[0_8px_24px_rgba(0,0,0,0.35)]"
      >
        {#each FORMATEN as formaat (formaat)}
          <DropdownMenu.Item
            class={menuItem}
            textValue={formaatNaam(formaat, t())}
            onSelect={() => {
              opslaan(formaat, false);
            }}
          >
            {formaatNaam(formaat, t())}<span class="text-sectie text-dim"
              >{formaatExtensie(formaat)}</span
            >
          </DropdownMenu.Item>
        {/each}
        <DropdownMenu.Separator class="mx-1 my-[5px] h-px bg-lijn" />
        <DropdownMenu.Item
          class={menuItem}
          onSelect={() => {
            opslaan(null, true);
          }}
        >
          {t().opslaanAnderePlek}
        </DropdownMenu.Item>
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
</div>
