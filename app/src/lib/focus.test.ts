import { describe, expect, it, vi } from 'vitest';
import { herstelFocus } from './focus';

// Een minimaal document: genoeg voor herstelFocus, zonder DOM-omgeving.
function document(actief: 'body' | 'knop' | 'plek' | null) {
  const body = { hasAttribute: () => false };
  const knop = { hasAttribute: () => false };
  const plek = { hasAttribute: (naam: string) => naam === 'data-focusplek' };
  const hoofdactie = { focus: vi.fn() };
  const zoek = vi.fn((doel: string) => (doel === '' ? null : hoofdactie));
  const doc = {
    body,
    activeElement:
      actief === 'body' ? body : actief === 'knop' ? knop : actief === 'plek' ? plek : null,
    querySelector: zoek
  } as unknown as Document;
  return { doc, hoofdactie, zoek };
}

describe('herstelFocus', () => {
  it('zet de focus op de hoofdactie als hij naar het begin viel', () => {
    const { doc, hoofdactie } = document('body');
    herstelFocus(doc);
    expect(hoofdactie.focus).toHaveBeenCalledOnce();
  });

  it('zet de focus op de hoofdactie als niets de focus heeft', () => {
    const { doc, hoofdactie } = document(null);
    herstelFocus(doc);
    expect(hoofdactie.focus).toHaveBeenCalledOnce();
  });

  it('laat de focus staan als een ander element hem heeft', () => {
    const { doc, hoofdactie } = document('knop');
    herstelFocus(doc);
    expect(hoofdactie.focus).not.toHaveBeenCalled();
  });
});

describe('herstelFocus met een eigen doel', () => {
  it('zoekt het opgegeven doel in plaats van de hoofdactie', () => {
    const { doc, hoofdactie, zoek } = document('body');
    herstelFocus(doc, '#dictaat-status');
    expect(zoek).toHaveBeenCalledWith('#dictaat-status');
    expect(hoofdactie.focus).toHaveBeenCalledOnce();
  });

  it('zoekt zonder doel de hoofdactie', () => {
    const { doc, zoek } = document(null);
    herstelFocus(doc);
    expect(zoek).toHaveBeenCalledWith('[data-hoofdactie]');
  });
});

describe('herstelFocus vanaf een vaste plek', () => {
  it('gaat van het paneel naar de hoofdactie, zoals Kopiëren na een klaar dictaat', () => {
    const { doc, hoofdactie, zoek } = document('plek');
    herstelFocus(doc);
    expect(zoek).toHaveBeenCalledWith('[data-hoofdactie]');
    expect(hoofdactie.focus).toHaveBeenCalledOnce();
  });
});
