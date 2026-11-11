/**
 * Maakt van een async actie een versie die een tweede aanroep negeert zolang de eerste loopt
 * (dubbelklik). `bijWissel` meldt wanneer de actie begint en eindigt, voor aria-disabled.
 */
export function eenmalig(
  actie: () => Promise<void>,
  bijWissel?: (bezig: boolean) => void
): () => Promise<void> {
  let bezig = false;
  return async () => {
    if (bezig) return;
    bezig = true;
    bijWissel?.(true);
    try {
      await actie();
    } finally {
      bezig = false;
      bijWissel?.(false);
    }
  };
}
