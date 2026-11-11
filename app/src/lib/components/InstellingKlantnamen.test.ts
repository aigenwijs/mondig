import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import InstellingKlantnamen from './InstellingKlantnamen.svelte';

// Klantnamen onder Instellingen (UI-E): de rij met uitleg, een knop "Verwijderen…" als er namen
// zijn, en een vaste plek (`role="status"`) voor de focus na het verwijderen. De effecten van
// Svelte draaien hier niet (servermodus); het verwijderen zelf staat in klanten.test.ts en in
// de mock.

function html(startAantal: number | null): string {
  return render(InstellingKlantnamen, { props: { startAantal } }).body;
}

describe('de rij Klantnamen', () => {
  it('legt uit wat Mondig onthoudt en waar', () => {
    const uit = html(12);
    expect(uit).toContain('Klantnamen');
    expect(uit).toContain('Mondig onthoudt ze als suggestie, alleen op deze computer.');
  });

  it('toont met namen het aantal en een knop Verwijderen… met de naam van de rij erbij', () => {
    const uit = html(12);
    expect(uit).toContain('12 klantnamen');
    expect(uit).toMatch(/<button[^>]*aria-label="Verwijderen… Klantnamen"[^>]*>/);
    expect(uit).toContain('Verwijderen…');
  });

  it('noemt één naam in het enkelvoud', () => {
    expect(html(1)).toContain('1 klantnaam');
    expect(html(1)).not.toContain('1 klantnamen');
  });

  it('heeft bij nul namen geen knop maar een uitleg dat er geen zijn', () => {
    const uit = html(0);
    expect(uit).not.toContain('<button');
    expect(uit).toContain('Mondig onthoudt nog geen klantnamen.');
  });

  it('heeft een vaste plek voor de focus na het verwijderen: een status die blijft bestaan', () => {
    for (const aantal of [0, 12]) {
      expect(html(aantal)).toMatch(/<p[^>]*id="klantnamen-status"[^>]*role="status"/);
      expect(html(aantal)).toMatch(/<p[^>]*id="klantnamen-status"[^>]*tabindex="-1"/);
    }
  });

  it('toont niets zolang de lijst niet is opgehaald, behalve de vaste plek', () => {
    const uit = html(null);
    expect(uit).not.toContain('<button');
    expect(uit).not.toContain('klantnamen<');
  });
});
