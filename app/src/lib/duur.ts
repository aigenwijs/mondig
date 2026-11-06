/**
 * Eén notatie voor een duur in beeld (pil, opnameteller, tijdcodes in het verslag): "m:ss" onder
 * het uur en "u:mm:ss" vanaf een uur, dus 0:09, 12:36 en 1:02:40. Afgerond naar beneden op hele
 * seconden. De kloktijd van een dag (14:00) valt hier niet onder.
 */
export function formatDuur(ms: number): string {
  const totaal = Math.max(0, Math.floor(ms / 1000));
  const uren = Math.floor(totaal / 3600);
  const minuten = Math.floor((totaal % 3600) / 60);
  const seconden = String(totaal % 60).padStart(2, '0');
  if (uren > 0) return `${String(uren)}:${String(minuten).padStart(2, '0')}:${seconden}`;
  return `${String(minuten)}:${seconden}`;
}
