import { type ClassValue, clsx } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';
import { createTV } from 'tailwind-variants';

// De tekstmaten uit app.css (@theme, --text-*). Zonder deze lijst houdt tailwind-merge
// `text-klein` voor een kleur en gooit hij de tekstkleur ernaast weg.
const twMergeConfig = {
  extend: { theme: { text: ['sectie', 'klein', 'basis', 'midden', 'groot', 'kop2', 'kop'] } }
};
const twMerge = extendTailwindMerge(twMergeConfig);

/** Voegt klassenamen samen en laat Tailwind-conflicten door de laatste winnen. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/** tailwind-variants met dezelfde kennis van de eigen tekstmaten als `cn`. */
export const tv = createTV({ twMergeConfig });

// Hulptypen van shadcn-svelte voor componenten rond Bits UI.
export type WithoutChild<T> = T extends { child?: unknown } ? Omit<T, 'child'> : T;
export type WithoutChildren<T> = T extends { children?: unknown } ? Omit<T, 'children'> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
