import type { Berichten } from '#lib';
import type { Platform } from './sneltoets';

/** Het label van "Tonen in Verkenner" per platform; op de Mac "Toon in Finder". */
export function toonInMapLabel(platform: Platform, berichten: Berichten): string {
  return platform === 'mac' ? berichten.toonInFinder : berichten.toonInVerkenner;
}
