// De tekst bij een bewaartermijn van de dictaatgeschiedenis, op één plek: de keuzelijst in
// Instellingen en het menu boven de lijst tonen dezelfde woorden.

import type { DictatenTermijn } from './dictaten';
import { t } from './i18n/index.svelte';

/** De naam van een termijn in de keuzelijst: "Laatste 10", "24 uur", "30 dagen". */
export function termijnLabel(termijn: DictatenTermijn): string {
  switch (termijn) {
    case 'niet':
      return t().dictatenTermijnNiet;
    case 'laatste10':
      return t().dictatenTermijnLaatste10;
    case '24u':
      return t().dictatenTermijn24u;
    case '7d':
      return t().dictatenTermijn7d;
    case '30d':
      return t().dictatenTermijn30d;
  }
}
