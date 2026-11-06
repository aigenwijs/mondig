import { createRawSnippet } from 'svelte';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import MeerOpties from './MeerOpties.svelte';

const inhoud = createRawSnippet(() => ({ render: () => '<p>Een rij</p>' }));

function html(open: boolean): string {
  return render(MeerOpties, { props: { children: inhoud, open } }).body;
}

describe('Meer opties', () => {
  it('is standaard dicht: knop met aria-expanded false en verborgen inhoud', () => {
    const uit = render(MeerOpties, { props: { children: inhoud } }).body;
    expect(uit).toContain('aria-expanded="false"');
    expect(uit).toContain('Meer opties');
    expect(uit).toMatch(/<div[^>]*\shidden/);
  });

  it('verwijst met aria-controls naar het id van de inhoud', () => {
    const uit = html(false);
    const controls = /aria-controls="([^"]+)"/.exec(uit)?.[1];
    expect(controls).toBeTruthy();
    expect(uit).toContain(`id="${controls ?? ''}"`);
  });

  it('is een gewone knop, dus Enter en spatie werken met het toetsenbord', () => {
    expect(html(false)).toMatch(/<button[^>]*type="button"/);
  });

  it('open: aria-expanded true, tekst Minder opties en zichtbare inhoud', () => {
    const uit = html(true);
    expect(uit).toContain('aria-expanded="true"');
    expect(uit).toContain('Minder opties');
    expect(uit).not.toMatch(/<div[^>]*\shidden/);
    expect(uit).toContain('Een rij');
  });
});
