<script lang="ts" module>
  import type { VariantProps } from 'tailwind-variants';
  import { cn, tv, type WithElementRef } from '#lib/utils.js';
  import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';

  // Knop van 32 px, radius 6, 14 px (ADR 0014, aanvulling 4-11-2026). Drie maten: default (32 px),
  // groot (44 px, voor de hoofdknoppen) en icoon (32 x 32). De focusring komt uit app.css (:focus-visible).
  // Een knop met aria-disabled blijft met Tab bereikbaar, maar reageert niet op hover.
  // Uitgeschakeld is nooit doorzichtig: elke variant krijgt de tekstkleur `dim` (minstens 3:1
  // tegen elk vlak, bewaakt in stijl.test.ts), zodat de tekst leesbaar blijft ("Verwijderen" op
  // Modellen). Dit geldt voor aria-disabled en voor het echte disabled-attribuut.
  export const buttonVariants = tv({
    base: 'inline-flex min-h-8 shrink-0 cursor-pointer items-center justify-center gap-[7px] rounded-klein border border-transparent px-3 py-1.5 text-center text-basis leading-tight font-medium transition-colors select-none aria-disabled:cursor-default disabled:cursor-default [&_svg]:pointer-events-none [&_svg]:shrink-0',
    variants: {
      variant: {
        // De hoofdactie: één per scherm, in de actiekleur.
        // Uitgeschakeld zonder actiekleur: een vlak in de hover-tint met gedimde tekst, zodat
        // hij in donker niet fel groen blijft en toch leesbaar is.
        primair:
          'bg-actie font-semibold text-op-actie not-aria-disabled:not-disabled:hover:bg-actie-hover aria-disabled:bg-hover aria-disabled:text-dim disabled:bg-hover disabled:text-dim',
        // Een tweede knop krijgt als enige een rand, zodat hij als knop herkenbaar blijft.
        tweede:
          'border-lijn bg-verhoogd text-tekst not-aria-disabled:not-disabled:hover:bg-hover aria-disabled:text-dim disabled:text-dim',
        // Rustig, zonder vlak tot je erover gaat.
        spook:
          'px-2 text-gedempt not-aria-disabled:not-disabled:hover:bg-hover not-aria-disabled:not-disabled:hover:text-tekst aria-disabled:text-dim disabled:text-dim',
        // Rood is alleen voor opnemen (besluit 9): de stopknop tijdens een opname.
        rood: 'bg-opname font-semibold text-op-opname not-aria-disabled:not-disabled:hover:brightness-110 aria-disabled:bg-hover aria-disabled:text-dim disabled:bg-hover disabled:text-dim'
      },
      size: {
        default: '',
        groot: 'h-11 min-h-11 px-[22px] text-midden',
        icoon: 'size-8 px-0 py-0',
        // Naast een knop van 44 px, zoals het menu bij Stoppen en uitschrijven.
        'icoon-groot': 'size-11 px-0 py-0'
      }
    },
    defaultVariants: {
      variant: 'primair',
      size: 'default'
    }
  });

  export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
  export type ButtonSize = VariantProps<typeof buttonVariants>['size'];

  export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
    WithElementRef<HTMLAnchorAttributes> & {
      variant?: ButtonVariant;
      size?: ButtonSize;
    };
</script>

<script lang="ts">
  let {
    class: className,
    variant = 'primair',
    size = 'default',
    ref = $bindable(null),
    href,
    type = 'button',
    disabled,
    children,
    ...restProps
  }: ButtonProps = $props();
</script>

{#if href}
  <!-- Het href is al met resolve() opgelost door de aanroeper. -->
  <!-- eslint-disable svelte/no-navigation-without-resolve -->
  <a
    bind:this={ref}
    data-slot="button"
    class={cn(buttonVariants({ variant, size }), className)}
    href={disabled ? undefined : href}
    aria-disabled={disabled}
    role={disabled ? 'link' : undefined}
    tabindex={disabled ? -1 : undefined}
    {...restProps}
  >
    {@render children?.()}
  </a>
  <!-- eslint-enable svelte/no-navigation-without-resolve -->
{:else}
  <button
    bind:this={ref}
    data-slot="button"
    class={cn(buttonVariants({ variant, size }), className)}
    {type}
    {disabled}
    {...restProps}
  >
    {@render children?.()}
  </button>
{/if}
