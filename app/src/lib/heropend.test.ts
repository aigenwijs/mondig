import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

// Een heropend verslag toont "Tonen in Verkenner" naar het bestand dat eerder is opgeslagen. Is de
// tekst daarna veranderd, dan staat in dat bestand de oude: de knop moet dan weg. De logica staat
// in de pagina zelf (effecten draaien niet in een servertest), dus controleren we de bron.
const bron = readFileSync(join(__dirname, '../routes/vergadering/+page.svelte'), 'utf-8');

describe('de mapknop van een heropend verslag', () => {
  it('vervalt samen met "Opgeslagen als" bij elke inhoudelijke wijziging', () => {
    const functie =
      /function vergeetOpgeslagen\(\): void \{([\s\S]*?)\n {2}\}/.exec(bron)?.[1] ?? '';
    expect(functie).toContain('opgeslagen = null');
    expect(functie).toContain('heropendId = null');
    // Opnieuw uitgeschreven, nieuwe notulen en een gewijzigde tekst gaan alle drie erdoor.
    expect(bron).toContain('if (opnieuwGeslaagd(vorig?.toestand, nu)) vergeetOpgeslagen();');
    expect(bron).toContain('if (verslagInhoud() !== voor) vergeetOpgeslagen();');
    const notulen =
      /momentMs !== nu\.notulen\.info\?\.momentMs\n?\s*\) \{\s*vergeetOpgeslagen\(\);/.test(bron);
    expect(notulen).toBe(true);
  });
});
