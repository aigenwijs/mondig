import { describe, expect, it, vi } from 'vitest';
import { herstelFocus } from './focus';

// Een minimaal document: genoeg voor herstelFocus, zonder DOM-omgeving.
function document(actief: 'body' | 'knop' | null) {
  const body = {};
  const knop = {};
  const hoofdactie = { focus: vi.fn() };
  const doc = {
    body,
    activeElement: actief === 'body' ? body : actief === 'knop' ? knop : null,
    querySelector: vi.fn(() => hoofdactie)
  } as unknown as Document;
  return { doc, hoofdactie };
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
