import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';

// De meter mag niet vanzelf starten: dat is wat "Testen" in Instellingen belooft (besluit 3).
const startMeter = vi.hoisted(() => vi.fn());
vi.mock('../meter', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../meter')>()),
  startMeter
}));

import InstellingMicrofoon from './InstellingMicrofoon.svelte';

describe('Microfoon in Instellingen', () => {
  it('zet de microfoon niet aan bij het openen en toont alleen de knop Testen', () => {
    const { body } = render(InstellingMicrofoon);
    expect(startMeter).not.toHaveBeenCalled();
    expect(body).toContain('>Testen<');
    // Geen meter en geen "klik om te stoppen" zolang er niet getest wordt.
    expect(body).not.toContain('Klik om te stoppen');
    expect(body).not.toContain('role="meter"');
  });

  it('legt uit dat dit voor dicteren is en dat Opnemen per vergadering kiest', () => {
    const { body } = render(InstellingMicrofoon);
    expect(body).toContain('Voor dicteren. Bij Opnemen kies je per vergadering.');
  });
});
