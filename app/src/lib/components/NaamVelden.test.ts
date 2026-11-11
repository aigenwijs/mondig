import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import NaamVelden from './NaamVelden.svelte';

// De kaart "Naam van het bestand" (UI-E): drie velden met labels, het voorbeeld eronder. Het
// gedrag (voorbeeld volgt de invoer, bewaren bij blur) staat in naamvelden.test.ts; de effecten
// van Svelte draaien hier niet (servermodus).

function html(props: Record<string, unknown> = {}): string {
  return render(NaamVelden, {
    props: {
      id: 'verslag',
      velden: { klant: 'Gemeente Vlaardingen', onderwerp: 'Intake', datum: '2026-11-02' },
      haalVoorbeeld: () => Promise.resolve(''),
      bewaar: () => Promise.resolve(null),
      voorbeeldStart: '2026-11-02 Gemeente Vlaardingen Intake.docx',
      ...props
    }
  }).body;
}

describe('de kaart "Naam van het bestand"', () => {
  it('heeft een kop en drie velden met een label, in de volgorde Datum, Klant, Onderwerp', () => {
    const uit = html();
    expect(uit).toContain('Naam van het bestand');
    expect(uit).toContain('role="group"');
    expect(uit).toContain('aria-labelledby="verslag-naam-kop"');
    const labels = [...uit.matchAll(/<label for="([^"]+)"[^>]*>([^<]+)<\/label>/g)].map((m) => [
      m[1],
      m[2]
    ]);
    expect(labels).toEqual([
      ['verslag-datum', 'Datum'],
      ['verslag-klant', 'Klant'],
      ['verslag-onderwerp', 'Onderwerp']
    ]);
  });

  it('toont de bewaarde waarden als echte tekst, niet als placeholder', () => {
    const uit = html();
    expect(uit).toContain('value="2026-11-02"');
    expect(uit).toContain('value="Gemeente Vlaardingen"');
    expect(uit).toContain('value="Intake"');
    expect(uit).not.toContain('placeholder');
  });

  it('toont eronder live "Wordt …" en verwijst er vanuit elk veld naar', () => {
    const uit = html();
    expect(uit).toMatch(/Wordt\s*<code[^>]*>2026-11-02 Gemeente Vlaardingen Intake\.docx<\/code>/);
    expect(uit.match(/aria-describedby="verslag-voorbeeld"/g)).toHaveLength(3);
  });

  it('heeft een veld Klant dat een combobox is', () => {
    expect(html()).toContain('role="combobox"');
  });

  it('toont niet zelf "Opgeslagen": dat doet de melding van de pagina al', () => {
    const uit = html();
    expect(uit).not.toContain('Opgeslagen');
    expect(uit).not.toContain('Tonen in Verkenner');
  });

  it('heeft ids met het voorvoegsel van de pagina, zodat twee kaarten elkaar niet raken', () => {
    expect(html({ id: 'transcript' })).toContain('id="transcript-onderwerp"');
  });
});
