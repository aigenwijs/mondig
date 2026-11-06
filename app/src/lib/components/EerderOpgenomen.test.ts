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
  naam: `Verslag nummer ${String(n)}`,
  soort: 'online',
  beginMs: om(4, 14) - (9 - n) * DAG,
  duurMs: 52 * 60_000,
  sprekers: null,
  wistMs: om(4, 14) + 7 * DAG,
  opnameAanwezig: true,
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

  it('toont de nieuwste vijf, nieuwste boven, met "Alle 9 tonen"', () => {
    const uit = html();
    expect(uit).toContain('Eerder opgenomen');
    for (const n of [9, 8, 7, 6, 5]) expect(uit).toContain(`Verslag nummer ${String(n)}`);
    for (const n of [4, 3, 2, 1]) expect(uit).not.toContain(`Verslag nummer ${String(n)}`);
    expect(uit.indexOf('Verslag nummer 9')).toBeLessThan(uit.indexOf('Verslag nummer 5'));
    expect(uit).toContain('Alle 9 tonen');
    expect(uit).toContain('aria-expanded="false"');
  });

  it('toont bij vijf of minder alles en geen knop "Alle N tonen"', () => {
    const uit = html({ verslagen: negen.slice(0, 5) });
    expect((uit.match(/<span class="max-w-full truncate font-semibold">/g) ?? []).length).toBe(5);
    expect(uit).not.toContain('tonen');
  });

  it('toont per regel de titel en de gegevens, en een knop die het verslag opent', () => {
    const uit = html({ verslagen: [verslag(9, { sprekers: 3 })] });
    expect(uit).toContain('Verslag nummer 9');
    expect(uit).toContain('Vandaag, 14:00 · 52 min · 3 sprekers');
    expect(uit).toMatch(/<button[^>]*type="button"[^>]*>\s*<span[^>]*>Verslag nummer 9/);
  });

  it('maakt zonder eigen naam zelf de titel', () => {
    const uit = html({
      verslagen: [verslag(9, { naam: null, soort: 'alleen', duurMs: 4 * 60_000 })]
    });
    expect(uit).toContain('Notitie van 4 november, 14:00');
    expect(uit).toContain('alleen ik');
  });

  it('heeft per regel een menu met de titel in de naam', () => {
    const uit = html({ verslagen: [verslag(9)] });
    expect(uit).toContain('aria-label="Meer voor Verslag nummer 9"');
    expect(uit).toContain('id="verslag-meer-vergadering-9"');
  });

  it('meldt een dag vooraf dat een verslag wordt gewist', () => {
    const uit = html({
      verslagen: [verslag(9, { wistMs: om(5, 9) }), verslag(8, { wistMs: om(4, 22) }), verslag(7)]
    });
    expect(uit).toContain('wordt morgen gewist');
    expect(uit).toContain('wordt vandaag gewist');
    expect((uit.match(/wordt (morgen|vandaag) gewist/g) ?? []).length).toBe(2);
    // Met hetzelfde scheidingsteken als de rest van de regel.
    expect(uit).toMatch(/52 min · (<!--[^>]*-->)*<span class="text-waarsch"/);
  });

  it('zegt hoe lang Mondig ze bewaart, met een link naar de instelling', () => {
    const uit = html();
    expect(uit).toContain('Mondig bewaart ze 7 dagen.');
    expect(uit).toMatch(/href="[^"]*\/instellingen#bewaartermijn"[^>]*>(<!---->)?Wijzigen/);
    expect(html({ bewaarDagen: 1 })).toContain('Mondig bewaart ze 1 dag.');
    expect(html({ bewaarDagen: 30 })).toContain('Mondig bewaart ze 30 dagen.');
  });

  it('laat de bewaarregel weg als de termijn nog niet bekend is', () => {
    expect(html({ bewaarDagen: null })).not.toContain('Mondig bewaart ze');
  });

  it('toont de bevestigvraag niet vanzelf', () => {
    expect(html()).not.toContain('role="alertdialog"');
  });
});
