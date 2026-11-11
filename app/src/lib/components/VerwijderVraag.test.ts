import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import VerwijderVraag from './VerwijderVraag.svelte';

// "Verwijderen met bevestiging" (ontwerpsysteem §5). De focus (op Annuleren bij het openen, terug
// op de knop erna) doet effecten en een DOM; dat is in de mock met de hand gecontroleerd.

const stil = () => Promise.resolve();

function html(props: Record<string, unknown> = {}): string {
  return render(VerwijderVraag, {
    props: {
      vraag: 'Uitschrift verwijderen?',
      uitleg: 'Wat je als bestand hebt opgeslagen, blijft in je map.',
      verwijder: stil,
      ...props
    }
  }).body;
}

describe('VerwijderVraag', () => {
  it('toont eerst alleen de knop Verwijderen… en nog geen vraag', () => {
    const uit = html();
    expect(uit).toContain('Verwijderen…');
    expect(uit).not.toContain('role="alertdialog"');
    expect(uit).not.toContain('Uitschrift verwijderen?');
  });

  it('is een gewone knop, geen rode of groene (ontwerpsysteem §2)', () => {
    for (const uit of [html(), html({ open: true })]) {
      expect(uit).not.toContain('bg-opname');
      expect(uit).not.toContain('bg-actie');
      expect(uit).not.toContain('bg-fout-tekst');
    }
  });

  it('toont open de vraag, de zin over wat blijft en de knoppen Verwijderen en Annuleren', () => {
    const uit = html({ open: true });
    expect(uit).toContain('role="alertdialog"');
    expect(uit).toContain('Uitschrift verwijderen?');
    expect(uit).toContain('Wat je als bestand hebt opgeslagen, blijft in je map.');
    expect(uit).toContain('>Verwijderen<');
    expect(uit).toContain('>Annuleren<');
    // De knop waarmee je begon is dan vervangen door de vraag.
    expect(uit).not.toContain('Verwijderen…');
  });

  it('koppelt de vraag en de uitleg aan de dialoog, met ids die de ouder kan kiezen', () => {
    const uit = html({ open: true, id: 'verslag-wis' });
    expect(uit).toContain('aria-labelledby="verslag-wis-kop"');
    expect(uit).toContain('aria-describedby="verslag-wis-uitleg"');
    expect(uit).toContain('id="verslag-wis-kop"');
    expect(uit).toContain('id="verslag-wis-annuleer"');
  });

  it('gebruikt de tekst van de ouder voor de knoppen', () => {
    const dicht = html({ knopTekst: 'Opname verwijderen…' });
    expect(dicht).toContain('Opname verwijderen…');
    const open = html({ open: true, bevestigTekst: 'Ja, verwijderen' });
    expect(open).toContain('Ja, verwijderen');
  });

  it('tekent niets zolang de vraag dicht is en de knop uit staat (opent vanuit een menu)', () => {
    const uit = html({ knop: false });
    expect(uit).not.toContain('<button');
    expect(uit).not.toContain('alertdialog');
  });

  it('verbergt een lege uitleg', () => {
    expect(html({ open: true, uitleg: '' })).toContain('empty:hidden');
  });

  it('zet aria-disabled op Verwijderen zolang er iets loopt', () => {
    const uit = html({ open: true, bezig: true });
    expect(uit).toMatch(/aria-disabled="true"[^>]*>(?:<!---->)?Verwijderen/);
  });
});
