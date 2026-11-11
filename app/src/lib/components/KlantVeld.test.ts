import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import KlantVeld from './KlantVeld.svelte';

// Het veld Klant met suggesties (UI-E): de ARIA-attributen van het patroon "combobox with list
// autocomplete" (WAI-ARIA APG). De toetsen zelf staan in klanten.test.ts (`comboToets`); de
// effecten van Svelte draaien hier niet (servermodus).

const suggesties = ['Gemeente Vlaardingen', 'Gemeente Meerwijk', 'Provincie Zuid-Holland'];
const stil = (): void => undefined;

function html(props: Record<string, unknown> = {}): string {
  return render(KlantVeld, {
    props: {
      id: 'verslag-klant',
      waarde: '',
      suggesties,
      bijWijzig: stil,
      bijKlaar: stil,
      ...props
    }
  }).body;
}

describe('het veld Klant', () => {
  it('is een combobox met lijst-autocomplete, dicht zolang er niet getypt is', () => {
    const uit = html();
    expect(uit).toContain('role="combobox"');
    expect(uit).toContain('aria-autocomplete="list"');
    expect(uit).toContain('aria-expanded="false"');
    expect(uit).toContain('aria-controls="verslag-klant-lijst"');
    expect(uit).not.toContain('aria-activedescendant');
    expect(uit).not.toContain('role="listbox"');
  });

  it('toont de waarde als gewone tekst met een grens van 80 tekens, zonder autocomplete van de browser', () => {
    const uit = html({ waarde: 'Gemeente' });
    expect(uit).toContain('value="Gemeente"');
    expect(uit).toContain('maxlength="80"');
    expect(uit).toContain('autocomplete="off"');
  });

  it('verwijst naar het voorbeeld van de bestandsnaam als beschrijving', () => {
    expect(html({ beschrijving: 'verslag-voorbeeld' })).toContain(
      'aria-describedby="verslag-voorbeeld"'
    );
  });

  it('opent met een lijst waarvan de opties role="option" hebben en een actieve die aria-selected is', () => {
    const uit = html({ waarde: 'gem', startOpen: true, startActief: 1 });
    expect(uit).toContain('aria-expanded="true"');
    expect(uit).toContain('role="listbox"');
    expect(uit).toContain('id="verslag-klant-lijst"');
    // Twee gemeenten passen bij "gem", de provincie niet.
    expect(uit.match(/role="option"/g)).toHaveLength(2);
    expect(uit).toContain('id="verslag-klant-optie-0"');
    expect(uit).toContain('id="verslag-klant-optie-1"');
    // Alleen de actieve is geselecteerd, en de combobox wijst ernaar met aria-activedescendant.
    expect(uit).toContain('aria-activedescendant="verslag-klant-optie-1"');
    const opties = [...uit.matchAll(/<li[^>]*role="option"[^>]*>/g)].map((m) => m[0]);
    expect(opties[0]).toContain('aria-selected="false"');
    expect(opties[1]).toContain('aria-selected="true"');
  });

  it('markeert het gevonden stuk met <mark>', () => {
    const uit = html({ waarde: 'gem', startOpen: true });
    expect(uit).toMatch(/<mark[^>]*>Gem<\/mark>(<!--]-->)?eente Vlaardingen/);
    expect(uit).not.toContain('Provincie');
  });

  it('zegt voor de schermlezer hoeveel suggesties er zijn, in een status die blijft bestaan', () => {
    expect(html({ waarde: 'gem', startOpen: true })).toMatch(
      /<span[^>]*role="status"[^>]*>2 suggesties<\/span>/
    );
    // Dicht: wel de status, zonder tekst.
    expect(html()).toMatch(/<span[^>]*role="status"[^>]*><\/span>/);
  });

  it('heeft geen lijst en geen activedescendant als er niets past', () => {
    const uit = html({ waarde: 'xyz', startOpen: true, startActief: 0 });
    expect(uit).toContain('aria-expanded="false"');
    expect(uit).not.toContain('role="listbox"');
    expect(uit).not.toContain('aria-activedescendant');
  });
});
