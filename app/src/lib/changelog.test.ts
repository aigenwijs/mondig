import { describe, expect, it } from 'vitest';
import {
  allePunten,
  beperkPunten,
  eerdereUitgaven,
  eerdereUitgavenVoor,
  details,
  eersteZin,
  isMacPunt,
  isDevVersie,
  leesChangelog,
  NOG_NIET_UITGEBRACHT,
  uitgaveVoor,
  voorPlatform
} from './changelog';
import { uitgaven as echteUitgaven } from './wijzigingen';

const VOORBEELD = `# Changelog

Alle merkbare wijzigingen staan hier.

## [Nog niet uitgebracht]

### Toegevoegd

- Een punt met \`code\` erin.
- Een lang punt dat over
  twee regels loopt.

### Gewijzigd

- Iets veranderd.

## [0.2.0] - 2026-10-01

### Opgelost

- Een fout weg.

### Beveiliging

## [0.1.0] - 2026-09-01

### Toegevoegd

- Begin.

[Nog niet uitgebracht]: https://example.invalid
`;

describe('leesChangelog', () => {
  const uitgaven = leesChangelog(VOORBEELD);
  const puntenVan = (i: number): string[] =>
    allePunten(uitgaven[i] ?? { kop: '', datum: null, categorieen: [] });

  it('leest de uitgaven in de volgorde van het bestand', () => {
    expect(uitgaven.map((u) => u.kop)).toEqual([NOG_NIET_UITGEBRACHT, '0.2.0', '0.1.0']);
    expect(uitgaven[1]?.datum).toBe('2026-10-01');
    expect(uitgaven[0]?.datum).toBeNull();
  });

  it('zet punten over meerdere regels aan elkaar en haalt opmaak weg', () => {
    expect(puntenVan(0)).toEqual([
      'Een punt met code erin.',
      'Een lang punt dat over twee regels loopt.',
      'Iets veranderd.'
    ]);
  });

  it('laat lege categorieën weg', () => {
    expect(uitgaven[1]?.categorieen.map((c) => c.naam)).toEqual(['Opgelost']);
  });

  it('negeert verwijzingen onderaan', () => {
    expect(puntenVan(2)).toEqual(['Begin.']);
  });

  it('geeft niets bij een lege tekst', () => {
    expect(leesChangelog('').length).toBe(0);
  });
});

describe('uitgaveVoor', () => {
  const uitgaven = leesChangelog(VOORBEELD);

  it('geeft bij een dev-versie de sectie Nog niet uitgebracht', () => {
    expect(uitgaveVoor(uitgaven, '0.3.0-dev')?.kop).toBe(NOG_NIET_UITGEBRACHT);
    expect(uitgaveVoor(uitgaven, '0.3.0-dev+abc1234')?.kop).toBe(NOG_NIET_UITGEBRACHT);
  });

  it('geeft bij een uitgebrachte versie die sectie', () => {
    expect(uitgaveVoor(uitgaven, '0.2.0')?.kop).toBe('0.2.0');
  });

  it('geeft null als de versie niet in de changelog staat', () => {
    expect(uitgaveVoor(uitgaven, '9.9.9')).toBeNull();
  });
});

describe('isDevVersie', () => {
  it('herkent -dev', () => {
    expect(isDevVersie('0.3.0-dev')).toBe(true);
    expect(isDevVersie('0.3.0')).toBe(false);
    expect(isDevVersie('0.3.0-beta.1')).toBe(false);
  });
});

describe('beperkPunten', () => {
  const uitgave = leesChangelog(VOORBEELD)[0] ?? { kop: '', datum: null, categorieen: [] };

  it('toont de eerste punten over categorieën heen en telt de rest', () => {
    const r = beperkPunten(uitgave, 2);
    expect(r.categorieen.map((c) => c.punten.length)).toEqual([2]);
    expect(r.verborgen).toBe(1);
  });

  it('gaat door in de volgende categorie als er ruimte is', () => {
    const r = beperkPunten(uitgave, 3);
    expect(r.categorieen.map((c) => c.naam)).toEqual(['Toegevoegd', 'Gewijzigd']);
    expect(r.verborgen).toBe(0);
  });
});

describe('eerdereUitgaven', () => {
  const uitgaven = leesChangelog(VOORBEELD);

  it('laat bij een release geen niet-uitgebracht werk en geen huidige uitgave zien', () => {
    expect(eerdereUitgaven(uitgaven, '0.2.0').map((u) => u.kop)).toEqual(['0.1.0']);
  });

  it('laat bij een dev-versie de uitgaven zien, zonder de huidige sectie', () => {
    expect(eerdereUitgaven(uitgaven, '0.3.0-dev').map((u) => u.kop)).toEqual(['0.2.0', '0.1.0']);
  });

  it('laat secties zonder punten weg', () => {
    const leeg = leesChangelog(
      ['## [Nog niet uitgebracht]', '', '## [0.1.0] - 2026-01-01', '', '### Toegevoegd', ''].join(
        '\n'
      )
    );
    expect(eerdereUitgaven(leeg, '0.2.0')).toEqual([]);
  });
});

describe('eersteZin', () => {
  it('knipt na de eerste zin', () => {
    expect(eersteZin('Eerste start: een korte uitleg. Daarna meer.')).toBe(
      'Eerste start: een korte uitleg.'
    );
  });

  it('knipt niet in een versienummer en houdt een punt zonder leesteken heel', () => {
    expect(eersteZin('Mondig 1.0 is er. Klaar.')).toBe('Mondig 1.0 is er.');
    expect(eersteZin('Zonder punt')).toBe('Zonder punt');
  });
});

describe('punten voor een platform', () => {
  const uitgave = leesChangelog(
    [
      '## [Nog niet uitgebracht]',
      '',
      '### Toegevoegd',
      '',
      '- Een punt voor iedereen.',
      '- Op de Mac: iets met de Command-toets.',
      '',
      '### Gewijzigd',
      '',
      "- Op de Mac kun je slepen uit Foto's.",
      ''
    ].join('\n')
  )[0] ?? { kop: '', datum: null, categorieen: [] };

  it('herkent een punt voor alleen de Mac aan zijn begin', () => {
    expect(isMacPunt('Op de Mac: iets')).toBe(true);
    expect(isMacPunt('Op de Mac kun je slepen')).toBe(true);
    expect(isMacPunt('Dicteren werkt ook op de Mac')).toBe(false);
  });

  it('laat de Mac-punten weg op Windows en laat een lege categorie vallen', () => {
    const windows = voorPlatform(uitgave, false);
    expect(allePunten(windows)).toEqual(['Een punt voor iedereen.']);
    expect(windows.categorieen.map((c) => c.naam)).toEqual(['Toegevoegd']);
  });

  it('toont op de Mac alles', () => {
    expect(allePunten(voorPlatform(uitgave, true))).toHaveLength(3);
  });
});

describe('details', () => {
  it('geeft wat na de eerste zin komt', () => {
    expect(details('Eerste zin. Dit is meer. En nog iets.')).toBe('Dit is meer. En nog iets.');
  });

  it('is leeg zonder details', () => {
    expect(details('Alleen een zin.')).toBe('');
    expect(details('Zonder punt')).toBe('');
  });
});

describe('de echte changelog', () => {
  const nogNiet = echteUitgaven.find((u) => u.kop === NOG_NIET_UITGEBRACHT);

  it('heeft werk dat nog niet is uitgebracht', () => {
    expect(nogNiet).toBeDefined();
  });

  it('begint elk punt met één zin in gewone taal, zonder ontwikkelaarstaal', () => {
    // Alle uitgaven: vlak na een uitgave is "Nog niet uitgebracht" leeg.
    for (const punt of echteUitgaven.flatMap((u) => allePunten(u))) {
      const zin = eersteZin(punt);
      expect(zin.length, zin).toBeLessThanOrEqual(260);
      expect(zin, zin).not.toMatch(/AZERTY|QWERTZ|klembordbeheer|terminal|spraakengine|taalmodel/i);
    }
  });
});

describe('eerdereUitgavenVoor', () => {
  const uitgaven = leesChangelog(
    [
      '## [0.2.0] - 2026-10-01',
      '',
      '### Toegevoegd',
      '',
      '- Voor iedereen.',
      '- Op de Mac: iets met de Command-toets.',
      '',
      '## [0.1.0] - 2026-09-01',
      '',
      '### Toegevoegd',
      '',
      '- Op de Mac kun je slepen.',
      ''
    ].join('\n')
  );

  it('laat Mac-punten weg op Windows en een uitgave die daardoor leeg is', () => {
    const windows = eerdereUitgavenVoor(uitgaven, '0.3.0-dev', false);
    expect(windows.map((u) => u.kop)).toEqual(['0.2.0']);
    expect(allePunten(windows[0] ?? { kop: '', datum: null, categorieen: [] })).toEqual([
      'Voor iedereen.'
    ]);
  });

  it('toont op de Mac alles', () => {
    expect(eerdereUitgavenVoor(uitgaven, '0.3.0-dev', true).map((u) => u.kop)).toEqual([
      '0.2.0',
      '0.1.0'
    ]);
  });
});
