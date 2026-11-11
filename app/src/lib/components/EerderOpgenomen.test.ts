import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import type { VerslagInfo } from '#lib/vergadering.js';
import EerderOpgenomen from './EerderOpgenomen.svelte';

// "Eerder opgenomen" (N-C3, ADR 0022): de laatste vijf bewaarde verslagen, "Alle N tonen", per regel
// de titel en de gegevens, een menu per regel, en onder de lijst hoe lang Mondig ze bewaart. De
// effecten van Svelte draaien hier niet (servermodus); de HTML is wat een schermlezer en het oog
// krijgen.

const om = (dag: number, uur: number, minuut = 0): number =>
  new Date(2026, 10, dag, uur, minuut).getTime();
const nu = om(4, 15);
const DAG = 86_400_000;

const verslag = (n: number, extra: Partial<VerslagInfo> = {}): VerslagInfo => ({
  id: `vergadering-${String(n)}`,
  klant: '',
  onderwerp: `Verslag nummer ${String(n)}`,
  soort: 'online',
  beginMs: om(4, 14) - (9 - n) * DAG,
  duurMs: 52 * 60_000,
  sprekers: null,
  wistMs: om(4, 14) + 7 * DAG,
  opnameAanwezig: true,
  opgeslagenBekend: false,
  ...extra
});

// Negen verslagen, nieuwste eerst zoals Rust ze geeft.
const negen = Array.from({ length: 9 }, (_, i) => verslag(9 - i));

const open = vi.fn();
const wis = vi.fn();

function html(
  props: Partial<{ verslagen: VerslagInfo[]; bewaarDagen: number | null }> = {}
): string {
  return render(EerderOpgenomen, {
    props: { verslagen: negen, nu, bewaarDagen: 7, open, wis, ...props }
  }).body;
}

describe('de lijst Eerder opgenomen', () => {
  it('toont zonder verslagen geen sectie', () => {
    expect(html({ verslagen: [] })).not.toContain('Eerder opgenomen');
    expect(html({ verslagen: [] }).trim()).not.toContain('<section');
  });

  it('toont de nieuwste vijf, nieuwste boven, met "Meer tonen" en de telling "5 van 9"', () => {
    const uit = html();
    expect(uit).toContain('Eerder opgenomen');
    for (const n of [9, 8, 7, 6, 5]) expect(uit).toContain(`Verslag nummer ${String(n)}`);
    for (const n of [4, 3, 2, 1]) expect(uit).not.toContain(`Verslag nummer ${String(n)}`);
    expect(uit.indexOf('Verslag nummer 9')).toBeLessThan(uit.indexOf('Verslag nummer 5'));
    expect(uit).toContain('Meer tonen');
    expect(uit).toContain('5 van 9');
    expect(uit).toContain('id="eerder-meer"');
    // Geen "Alle N tonen" en geen "Minder tonen" meer.
    expect(uit).not.toContain('Alle 9 tonen');
    expect(uit).not.toContain('Minder tonen');
  });

  it('telt bij zestien verslagen "5 van 16": het eerste stuk blijft vijf', () => {
    const zestien = Array.from({ length: 16 }, (_, i) => verslag(16 - i));
    const uit = html({ verslagen: zestien });
    expect(uit).toContain('5 van 16');
    expect((uit.match(/<span class="max-w-full truncate font-semibold">/g) ?? []).length).toBe(5);
  });

  it('geeft elk verslag een knop met een id, zodat de focus naar de eerste nieuwe rij kan', () => {
    const uit = html();
    for (const n of [9, 8, 7, 6, 5])
      expect(uit).toContain(`id="verslag-open-vergadering-${String(n)}"`);
    expect(uit).not.toContain('id="verslag-open-vergadering-4"');
  });

  it('heeft geen eigen statusregio: meldingen gaan naar die van de pagina', () => {
    expect(html()).not.toContain('role="status"');
  });

  it('toont bij vijf of minder alles en geen knop of telling', () => {
    const uit = html({ verslagen: negen.slice(0, 5) });
    expect((uit.match(/<span class="max-w-full truncate font-semibold">/g) ?? []).length).toBe(5);
    expect(uit).not.toContain('tonen');
    expect(uit).not.toMatch(/\d+ van \d+/);
  });

  it('toont per regel de titel en de gegevens, en een knop die het verslag opent', () => {
    const uit = html({ verslagen: [verslag(9, { sprekers: 3 })] });
    expect(uit).toContain('Verslag nummer 9');
    expect(uit).toContain('Vandaag, 14:00 · 52 min · 3 sprekers');
    expect(uit).toMatch(/<button[^>]*type="button"[^>]*>\s*<span[^>]*>Verslag nummer 9/);
  });

  it('toont zonder klant en onderwerp alleen het soortwoord als titel', () => {
    const uit = html({
      verslagen: [verslag(9, { onderwerp: null, soort: 'alleen', duurMs: 4 * 60_000 })]
    });
    expect(uit).toMatch(/<span[^>]*>Notitie<\/span>/);
    expect(uit).not.toContain('Notitie van');
    expect(uit).toContain('alleen ik');
  });

  it('toont klant en onderwerp met een middenpunt ertussen', () => {
    const uit = html({
      verslagen: [verslag(9, { klant: 'Gemeente Vlaardingen', onderwerp: 'Intake' })]
    });
    expect(uit).toContain('Gemeente Vlaardingen · Intake');
  });

  it('heeft per regel een menu met de titel in de naam', () => {
    const uit = html({ verslagen: [verslag(9)] });
    expect(uit).toContain('aria-label="Meer voor Verslag nummer 9"');
    expect(uit).toContain('id="verslag-meer-vergadering-9"');
  });

  it('meldt een dag vooraf dat een verslag wordt verwijderd', () => {
    const uit = html({
      verslagen: [verslag(9, { wistMs: om(5, 9) }), verslag(8, { wistMs: om(4, 22) }), verslag(7)]
    });
    expect(uit).toContain('wordt morgen verwijderd');
    expect(uit).toContain('wordt vandaag verwijderd');
    expect((uit.match(/wordt (morgen|vandaag) verwijderd/g) ?? []).length).toBe(2);
    // Met hetzelfde scheidingsteken als de rest van de regel.
    expect(uit).toMatch(/52 min · (<!--[^>]*-->)*<span class="text-waarsch"/);
  });

  it('zegt hoe lang Mondig ze bewaart, met een link naar de instelling', () => {
    const uit = html();
    expect(uit).toContain('Mondig verwijdert opnames na 7 dagen.');
    expect(uit).toMatch(/href="[^"]*\/instellingen#bewaartermijn"[^>]*>(<!---->)?Wijzigen/);
    expect(html({ bewaarDagen: 1 })).toContain('Mondig verwijdert opnames na 1 dag.');
    expect(html({ bewaarDagen: 30 })).toContain('Mondig verwijdert opnames na 30 dagen.');
  });

  it('laat de bewaarregel weg als de termijn nog niet bekend is', () => {
    expect(html({ bewaarDagen: null })).not.toContain('Mondig verwijdert opnames');
  });

  it('toont "Tonen in Verkenner" alleen bij een verslag met een opgeslagen bestand', () => {
    const uit = html({
      verslagen: [verslag(3, { opgeslagenBekend: true }), verslag(2), verslag(1)]
    });
    // Eén icoonknop, vóór de knop met het menu, met aria-label en tooltip gelijk.
    expect((uit.match(/aria-label="Tonen in Verkenner"/g) ?? []).length).toBe(1);
    expect((uit.match(/title="Tonen in Verkenner"/g) ?? []).length).toBe(1);
    expect(uit.indexOf('aria-label="Tonen in Verkenner"')).toBeLessThan(
      uit.indexOf('id="verslag-meer-vergadering-3"')
    );
    expect(html({ verslagen: [verslag(1), verslag(2)] })).not.toContain('Tonen in Verkenner');
  });

  it('toont de bevestigvraag niet vanzelf', () => {
    expect(html()).not.toContain('role="alertdialog"');
  });
});
