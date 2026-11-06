import { createRawSnippet } from 'svelte';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import Vinkvak from './Vinkvak.svelte';

const tekst = createRawSnippet(() => ({ render: () => '<span>Ook het tweede model</span>' }));

function html(checked: boolean, disabled = false): string {
  return render(Vinkvak, { props: { children: tekst, checked, disabled } }).body;
}

describe('Vinkvak', () => {
  it('is een echt selectievakje met de tekst als naam, binnen een label', () => {
    const uit = html(false);
    expect(uit).toMatch(/<label[^>]*>/);
    expect(uit).toContain('role="checkbox"');
    expect(uit).toContain('aria-checked="false"');
    expect(uit).toContain('Ook het tweede model');
  });

  it('heeft een vak van 20 px en een label van minstens 32 px', () => {
    const uit = html(false);
    expect(uit).toContain('size-5');
    expect(uit).toContain('min-h-8');
  });

  it('gebruikt de tokens voor rand en actiekleur, zonder vaste kleuren', () => {
    const uit = html(false);
    expect(uit).toContain('border-rand-veld');
    expect(uit).toContain('data-[state=checked]:bg-actie');
    expect(uit).toContain('text-op-actie');
    expect(uit).not.toMatch(/#[0-9a-fA-F]{3,6}\b/);
  });

  it('aangevinkt: aria-checked true en een vinkje', () => {
    const uit = html(true);
    expect(uit).toContain('aria-checked="true"');
    expect(uit).toContain('<svg');
  });

  it('uitgeschakeld meldt dat aan de schermlezer', () => {
    expect(html(false, true)).toContain('disabled');
  });
});
