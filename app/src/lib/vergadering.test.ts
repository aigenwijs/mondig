import { describe, expect, it } from 'vitest';
import {
  aankondiging,
  beginNaam,
  beurtNaam,
  beurtNaamGetoond,
  dagVan,
  heeftSprekers,
  kanAanSpreker,
  sprekerKant,
  sprekersVan,
  wistBinnenkort,
  isBewaarDagen,
  bewaarTot,
  VERSLAGEN_ZICHTBAAR,
  zichtbareVerslagen,
  BEWAAR_DAGEN,
  STANDAARD_BEWAAR_DAGEN,
  duurMinuten,
  isNieuwer,
  kantNaam,
  klokTijd,
  naamMelding,
  sprekerlabels,
  nieuweStap,
  standaardNaam,
  type Sprekerstap,
  neemtOp,
  schermVan,
  toontLabels,
  zonderSpreker,
  metSysteemgeluid,
  vraagtHerinnering,
  zichtbareOpnames,
  andereSoorten,
  opnamesVanSoort,
  soortVan,
  type Vergadersoort,
  SOORTEN,
  WACHTEND_ZICHTBAAR,
  MAX_SPREKERS,
  type BeurtInfo,
  type OpnameInfo,
  type StandToestand,
  type VerslagInfo,
  type VergaderingStand
} from './vergadering';
import { en } from './i18n/en';
import { nl } from './i18n/nl';

function stand(toestand: VergaderingStand['toestand'], versie = 1): VergaderingStand {
  return { versie, toestand, opnames: [], verslagen: [], fout: null, laatsteSoort: 'online' };
}

describe('de stand van Vergadering', () => {
  it('een oudere stand draait een nieuwere niet terug', () => {
    expect(isNieuwer(stand({ type: 'klaar' }, 5), 4)).toBe(true);
    expect(isNieuwer(stand({ type: 'klaar' }, 4), 4)).toBe(false);
  });

  it('weet of er wordt opgenomen', () => {
    expect(neemtOp(stand({ type: 'klaar' }))).toBe(false);
    expect(neemtOp(stand({ type: 'starten', soort: 'ruimte' }))).toBe(true);
    expect(
      neemtOp(
        stand({
          type: 'neemtOp',
          soort: 'online',
          beginMs: 0,
          duurMs: 1000,
          niveaus: null,
          systeemStil: null,
          meldingen: []
        })
      )
    ).toBe(true);
  });
});

const NAMEN = {
  sprekerJij: 'Jij',
  sprekerAnderen: 'Anderen',
  vergaderSpreker: (n: number): string => `Spreker ${String(n)}`
};

function beurt(deel: Partial<BeurtInfo> = {}): BeurtInfo {
  return {
    index: 0,
    kant: 'anderen',
    sprekernummer: null,
    spreker: 'Anderen',
    sprekerGetoond: deel.spreker === undefined ? 'Anderen' : deel.spreker,
    eigenNaam: false,
    startMs: 0,
    eindMs: 1000,
    tekst: 'Hallo.',
    markeringen: [],
    veranderd: false,
    ...deel
  };
}

const NEEMT_OP: StandToestand = {
  type: 'neemtOp',
  soort: 'online',
  beginMs: 0,
  duurMs: 0,
  niveaus: null,
  systeemStil: null,
  meldingen: []
};
const UITSCHRIJVEN: StandToestand = {
  type: 'uitschrijven',
  taak: 2,
  soort: 'online',
  beginMs: 0,
  duurMs: null,
  voortgang: 34,
  model: 'Parakeet v3'
};
const VERSLAG: StandToestand = {
  type: 'verslag',
  taak: 1,
  soort: 'online',
  beginMs: 0,
  duurMs: 60_000,
  model: 'Parakeet v3',
  soevereiniteit: 'lokaal',
  meldingen: [],
  weggelaten: 0,
  beurten: [],
  audioAanwezig: true,
  persoon: { vervangen: false, eigen: [] },
  naam: null,
  naamGetoond: null,
  wistMs: null,
  bewarenMislukt: false,
  modelId: 'parakeet-ultra-q8',
  opnameAanwezig: true,
  modelRol: 'snel',
  verbeterd: 0,
  geluidMs: 120_000,
  opnieuw: null,
  naOpnieuw: null,
  vorigeVersie: false,
  sprekerstap: null,
  notulen: { info: null, onleesbaar: false, maken: null }
};

describe('het deel van het scherm', () => {
  it('leidt het scherm af uit de toestand van Rust', () => {
    expect(schermVan({ type: 'klaar' })).toBe('klaar');
    expect(schermVan({ type: 'starten', soort: 'online' })).toBe('starten');
    expect(
      schermVan({
        type: 'voorbereiden',
        soort: 'ruimte',
        beginMs: 0,
        duurMs: null,
        herstellen: true
      })
    ).toBe('voorbereiden');
    expect(schermVan(NEEMT_OP)).toBe('opname');
    expect(schermVan(UITSCHRIJVEN)).toBe('uitschrijven');
    expect(schermVan(VERSLAG)).toBe('verslag');
  });

  it('toont alleen online een label bij elke beurt, en in de ruimte alleen met sprekers', () => {
    expect(toontLabels('online')).toBe(true);
    expect(toontLabels('ruimte')).toBe(false);
    expect(toontLabels('alleen')).toBe(false);
    const zonder = [beurt({ kant: 'jij', spreker: null })];
    const met = [beurt({ kant: 'jij', sprekernummer: 1, spreker: 'Spreker 1' })];
    expect(toontLabels('ruimte', zonder)).toBe(false);
    expect(toontLabels('ruimte', met)).toBe(true);
    // Alleen ik heeft nooit sprekers, ook niet als er toch een nummer zou staan.
    expect(toontLabels('alleen', met)).toBe(false);
  });
});

describe('Alleen ik en de andere soorten', () => {
  it('kent drie soorten in de volgorde van de schakelaar', () => {
    expect(SOORTEN).toEqual(['online', 'ruimte', 'alleen']);
  });

  it('neemt alleen online het geluid van de computer op', () => {
    expect(metSysteemgeluid('online')).toBe(true);
    expect(metSysteemgeluid('ruimte')).toBe(false);
    expect(metSysteemgeluid('alleen')).toBe(false);
  });

  it('vraagt bij Alleen ik niet om het de anderen te vertellen', () => {
    expect(vraagtHerinnering('online')).toBe(true);
    expect(vraagtHerinnering('ruimte')).toBe(true);
    expect(vraagtHerinnering('alleen')).toBe(false);
  });
});

describe('de lijst met wachtende opnames', () => {
  const opname = (id: string, beginMs: number): OpnameInfo => ({
    id,
    soort: 'online',
    beginMs,
    duurMs: 60_000,
    afgesloten: true,
    onzeker: false,
    wistMs: null
  });
  // Twaalf opnames, oudste eerst zoals Rust ze geeft: o1 tot en met o12.
  const lijst = Array.from({ length: 12 }, (_, i) => opname(`o${String(i + 1)}`, i + 1));

  it('toont de nieuwste tien, nieuwste eerst', () => {
    expect(WACHTEND_ZICHTBAAR).toBe(10);
    const zicht = zichtbareOpnames(lijst, false).map((o) => o.id);
    expect(zicht).toHaveLength(10);
    expect(zicht[0]).toBe('o12');
    expect(zicht[9]).toBe('o3');
  });

  it('toont na "Alle tonen" alles, nieuwste eerst, zonder de lijst te wijzigen', () => {
    expect(zichtbareOpnames(lijst, true)).toHaveLength(12);
    expect(zichtbareOpnames(lijst, true)[11]?.id).toBe('o1');
    expect(lijst[0]?.id).toBe('o1');
  });

  it('toont bij tien of minder alles', () => {
    expect(zichtbareOpnames(lijst.slice(0, 10), false)).toHaveLength(10);
    expect(zichtbareOpnames(lijst.slice(0, 2), false)).toHaveLength(2);
    expect(zichtbareOpnames([], false)).toEqual([]);
  });
});

describe('de lijst per soort', () => {
  const NU = new Date(2026, 10, 2, 12, 0).getTime();
  const UUR = 3_600_000;
  const opname = (id: string, soort: Vergadersoort, wistMs: number | null = null): OpnameInfo => ({
    id,
    soort,
    beginMs: 1,
    duurMs: 60_000,
    afgesloten: true,
    onzeker: false,
    wistMs
  });

  it('telt een onbekende soort als online', () => {
    expect(soortVan({ soort: 'iets' as Vergadersoort })).toBe('online');
    expect(soortVan({ soort: 'ruimte' })).toBe('ruimte');
  });

  it('filtert op de soort in de schakelaar', () => {
    const lijst = [
      opname('a', 'online'),
      opname('b', 'ruimte'),
      opname('c', 'alleen'),
      opname('d', 'onbekend' as Vergadersoort)
    ];
    expect(opnamesVanSoort(lijst, 'online').map((o) => o.id)).toEqual(['a', 'd']);
    expect(opnamesVanSoort(lijst, 'ruimte').map((o) => o.id)).toEqual(['b']);
    expect(opnamesVanSoort(lijst, 'alleen').map((o) => o.id)).toEqual(['c']);
  });

  it('telt de andere soorten in de volgorde van de schakelaar, zonder lege', () => {
    const lijst = [opname('a', 'alleen'), opname('b', 'online'), opname('c', 'online')];
    expect(andereSoorten(lijst, 'ruimte', NU)).toEqual([
      { soort: 'online', aantal: 2, wist: null, aantalWist: 0 },
      { soort: 'alleen', aantal: 1, wist: null, aantalWist: 0 }
    ]);
    expect(andereSoorten(lijst, 'online', NU)).toEqual([
      { soort: 'alleen', aantal: 1, wist: null, aantalWist: 0 }
    ]);
    expect(andereSoorten([], 'online', NU)).toEqual([]);
  });

  it('waarschuwt als een opname van een andere soort vandaag of morgen gewist wordt', () => {
    const lijst = [
      opname('a', 'online', NU + 20 * UUR),
      opname('b', 'online', NU + 30 * 24 * UUR),
      opname('c', 'alleen', NU + 2 * UUR),
      opname('d', 'alleen', NU + 20 * UUR)
    ];
    expect(andereSoorten(lijst, 'ruimte', NU)).toEqual([
      { soort: 'online', aantal: 2, wist: 'morgen', aantalWist: 1 },
      { soort: 'alleen', aantal: 2, wist: 'vandaag', aantalWist: 1 }
    ]);
  });
});

describe('tijd en duur', () => {
  it('zet de klok om in m:ss, en vanaf een uur in u:mm:ss', () => {
    expect(klokTijd(0)).toBe('0:00');
    expect(klokTijd(754_000)).toBe('12:34');
    expect(klokTijd(3_725_000)).toBe('1:02:05');
    expect(klokTijd(754_999)).toBe('12:34');
  });

  it('houdt de klok niet onder nul', () => {
    expect(klokTijd(-5000)).toBe('0:00');
  });

  it('rondt de duur af op hele minuten, minstens een', () => {
    expect(duurMinuten(0)).toBe(1);
    expect(duurMinuten(20_000)).toBe(1);
    expect(duurMinuten(47 * 60_000)).toBe(47);
    expect(duurMinuten(52 * 60_000 + 40_000)).toBe(53);
  });

  it('weet of een moment vandaag, gisteren of eerder was', () => {
    const nu = new Date(2026, 10, 2, 9, 30).getTime();
    expect(dagVan(new Date(2026, 10, 2, 0, 5).getTime(), nu)).toBe('vandaag');
    expect(dagVan(new Date(2026, 10, 1, 23, 59).getTime(), nu)).toBe('gisteren');
    expect(dagVan(new Date(2026, 10, 1, 0, 0).getTime(), nu)).toBe('gisteren');
    expect(dagVan(new Date(2026, 9, 31, 14, 0).getTime(), nu)).toBe('eerder');
  });

  it('meldt een opname die binnen 24 uur wordt gewist: vandaag of morgen', () => {
    const nu = new Date(2026, 10, 2, 9, 30).getTime();
    const uur = 3_600_000;
    // Over 2 uur: dezelfde dag. Over 20 uur: morgen. Al voorbij: Mondig ruimt zo op.
    expect(wistBinnenkort(nu + 2 * uur, nu)).toBe('vandaag');
    expect(wistBinnenkort(nu + 20 * uur, nu)).toBe('morgen');
    expect(wistBinnenkort(nu - uur, nu)).toBe('vandaag');
    // Meer dan 24 uur weg: geen melding.
    expect(wistBinnenkort(nu + 25 * uur, nu)).toBeNull();
    // Onbekend (Mondig kent de bewaartermijn niet): ook geen melding.
    expect(wistBinnenkort(null, nu)).toBeNull();
    expect(wistBinnenkort(nu + 6 * 24 * uur, nu)).toBeNull();
    // Om 01:00 is over 20 uur nog dezelfde dag niet meer waar (21:00 is wel vandaag).
    const nacht = new Date(2026, 10, 2, 1, 0).getTime();
    expect(wistBinnenkort(nacht + 20 * uur, nacht)).toBe('vandaag');
    expect(wistBinnenkort(nacht + 24 * uur, nacht)).toBe('morgen');
  });

  it('kent de bewaartermijnen 1, 7 en 30 dagen met 7 als standaard', () => {
    expect([...BEWAAR_DAGEN]).toEqual([1, 7, 30]);
    expect(STANDAARD_BEWAAR_DAGEN).toBe(7);
    expect(isBewaarDagen(30)).toBe(true);
    expect(isBewaarDagen(14)).toBe(false);
  });

  it('telt gisteren ook over de maandgrens', () => {
    const nu = new Date(2026, 10, 1, 8, 0).getTime();
    expect(dagVan(new Date(2026, 9, 31, 22, 0).getTime(), nu)).toBe('gisteren');
  });
});

describe('het label of de naam van een beurt', () => {
  it('toont Jij of Anderen als er geen eigen naam is', () => {
    expect(beurtNaam(beurt({ kant: 'jij', spreker: 'Jij' }), NAMEN)).toBe('Jij');
    expect(beurtNaam(beurt(), NAMEN)).toBe('Anderen');
  });

  it('toont de standaardnaam in de taal van de app, niet die van Rust', () => {
    const engels = {
      sprekerJij: 'You',
      sprekerAnderen: 'Others',
      vergaderSpreker: (n: number): string => `Speaker ${String(n)}`
    };
    expect(beurtNaam(beurt(), engels)).toBe('Others');
    expect(kantNaam('jij', engels)).toBe('You');
  });

  it('toont de eigen naam als die er is', () => {
    expect(beurtNaam(beurt({ spreker: 'Wethouder De Vries', eigenNaam: true }), NAMEN)).toBe(
      'Wethouder De Vries'
    );
  });

  it('valt terug op de kant bij een lege eigen naam', () => {
    expect(beurtNaam(beurt({ spreker: '', eigenNaam: true }), NAMEN)).toBe('Anderen');
    expect(beurtNaam(beurt({ spreker: null, eigenNaam: true }), NAMEN)).toBe('Anderen');
  });

  it('toont Spreker N als de sprekers herkend zijn en er geen eigen naam is', () => {
    expect(beurtNaam(beurt({ sprekernummer: 2, spreker: 'Spreker 2' }), NAMEN)).toBe('Spreker 2');
    const engels = {
      sprekerJij: 'You',
      sprekerAnderen: 'Others',
      vergaderSpreker: (n: number): string => `Speaker ${String(n)}`
    };
    expect(beurtNaam(beurt({ sprekernummer: 2, spreker: 'Spreker 2' }), engels)).toBe('Speaker 2');
    // Een eigen naam gaat voor het nummer.
    expect(beurtNaam(beurt({ sprekernummer: 2, spreker: 'Marieke', eigenNaam: true }), NAMEN)).toBe(
      'Marieke'
    );
  });

  it('begint het naamveld leeg bij de standaardnaam en gevuld bij een eigen naam', () => {
    expect(beginNaam(beurt())).toBe('');
    expect(beginNaam(beurt({ spreker: 'Zaal', eigenNaam: true }))).toBe('Zaal');
    expect(beginNaam(beurt({ spreker: null }))).toBe('');
  });
});

describe('de naam op het label met Vervangen aan', () => {
  it('toont een gemarkeerd gegeven in een eigen naam zoals de export, en laat de rest staan', () => {
    const met = beurt({
      spreker: 'Jan 06 12345678',
      sprekerGetoond: 'Jan [TELEFOON]',
      eigenNaam: true
    });
    expect(beurtNaamGetoond(met, NAMEN)).toBe('Jan [TELEFOON]');
    // Het naamveld begint met de echte naam, want daar verbetert de gebruiker.
    expect(beginNaam(met)).toBe('Jan 06 12345678');
    // Zonder markering is het gewoon de naam, en een standaardnaam volgt de taal van de app.
    expect(beurtNaamGetoond(beurt({ spreker: 'Zaal', eigenNaam: true }), NAMEN)).toBe('Zaal');
    expect(beurtNaamGetoond(beurt(), NAMEN)).toBe('Anderen');
  });
});

describe('de sprekerlabels voor Rust', () => {
  it('geeft de standaardnamen in de taal van de app, per sprekernummer', () => {
    const nl8 = sprekerlabels(nl);
    expect(nl8.jij).toBe('Jij');
    expect(nl8.anderen).toBe('Anderen');
    expect(nl8.sprekers).toHaveLength(MAX_SPREKERS);
    expect(nl8.sprekers[0]).toBe('Spreker 1');
    const en8 = sprekerlabels(en);
    expect(en8).toEqual({
      jij: 'You',
      anderen: 'Others',
      sprekers: Array.from({ length: MAX_SPREKERS }, (_, i) => `Speaker ${String(i + 1)}`)
    });
    // Geen Nederlands label in de Engelse namen.
    const alle = [en8.jij, en8.anderen, ...en8.sprekers].join(' ');
    expect(alle).not.toMatch(/Jij|Anderen|Spreker /);
  });
});

describe('de lijst Eerder opgenomen', () => {
  const verslag = (id: string, beginMs: number): VerslagInfo => ({
    id,
    naam: null,
    soort: 'online',
    beginMs,
    duurMs: 60_000,
    sprekers: null,
    wistMs: null,
    opnameAanwezig: true
  });
  // Negen verslagen, in de volgorde van Rust (nieuwste eerst): v9 tot en met v1.
  const lijst = Array.from({ length: 9 }, (_, i) => verslag(`v${String(9 - i)}`, 9 - i));

  it('toont de nieuwste vijf, nieuwste eerst', () => {
    expect(VERSLAGEN_ZICHTBAAR).toBe(5);
    const zicht = zichtbareVerslagen(lijst, false).map((v) => v.id);
    expect(zicht).toEqual(['v9', 'v8', 'v7', 'v6', 'v5']);
  });

  it('toont na "Alle N tonen" alles, ook als de volgorde door elkaar staat', () => {
    const door = [...lijst].reverse();
    expect(zichtbareVerslagen(door, true).map((v) => v.id)).toEqual(lijst.map((v) => v.id));
    expect(door[0]?.id).toBe('v1');
  });

  it('toont bij vijf of minder alles', () => {
    expect(zichtbareVerslagen(lijst.slice(0, 5), false)).toHaveLength(5);
    expect(zichtbareVerslagen(lijst.slice(0, 2), false)).toHaveLength(2);
    expect(zichtbareVerslagen([], false)).toEqual([]);
  });
});

describe('tot wanneer Mondig een verslag bewaart', () => {
  const nu = new Date(2026, 10, 4, 15, 0).getTime();
  const op = (dag: number, uur = 14): number => new Date(2026, 10, dag, uur, 0).getTime();

  it('kent de dag van wissen niet als de bewaartermijn onbekend is', () => {
    expect(bewaarTot(null, nu)).toBeNull();
  });

  it('noemt vandaag en morgen', () => {
    expect(bewaarTot(op(4, 23), nu)).toEqual({ soort: 'vandaag' });
    expect(bewaarTot(nu - 1000, nu)).toEqual({ soort: 'vandaag' });
    expect(bewaarTot(op(5, 1), nu)).toEqual({ soort: 'morgen' });
  });

  it('noemt binnen een week de weekdag, daarna de datum', () => {
    expect(bewaarTot(op(10), nu)).toEqual({ soort: 'weekdag', ms: op(10) });
    expect(bewaarTot(op(11), nu)).toEqual({ soort: 'datum', ms: op(11) });
    expect(bewaarTot(op(30), nu)).toEqual({ soort: 'datum', ms: op(30) });
  });
});

describe('wat de schermlezer meldt', () => {
  const klaar = stand({ type: 'klaar' }, 1);

  it('meldt niets bij de eerste stand', () => {
    expect(aankondiging(null, stand(NEEMT_OP, 2))).toBeNull();
  });

  it('meldt het begin van de opname, het uitschrijven en het verslag', () => {
    expect(aankondiging(klaar, stand(NEEMT_OP, 2))).toBe('gestart');
    expect(aankondiging(stand(NEEMT_OP, 2), stand(UITSCHRIJVEN, 3))).toBe('uitschrijven');
    expect(aankondiging(stand(UITSCHRIJVEN, 3), stand(VERSLAG, 4))).toBe('verslag');
  });

  it('meldt niets zolang de toestand gelijk blijft, ook niet bij een nieuw niveau', () => {
    expect(aankondiging(stand(NEEMT_OP, 2), stand(NEEMT_OP, 3))).toBeNull();
  });

  it('meldt niets bij het terug naar klaar (dat meldt de handeling zelf)', () => {
    expect(aankondiging(stand(NEEMT_OP, 2), klaar)).toBeNull();
  });
});

describe('sprekers in het verslag', () => {
  const jijBeurt = beurt({ index: 0, kant: 'jij', sprekernummer: null, spreker: 'Jij' });
  const anderenBeurt = beurt({ index: 1, sprekernummer: 2, spreker: 'Spreker 2' });
  const verslag = [
    jijBeurt,
    anderenBeurt,
    beurt({ index: 2, sprekernummer: 1, spreker: 'Marieke', eigenNaam: true }),
    beurt({ index: 3, sprekernummer: 2, spreker: 'Spreker 2' }),
    beurt({ index: 4, sprekernummer: 1, spreker: 'Marieke', eigenNaam: true })
  ];

  it('weet of de sprekers herkend zijn', () => {
    expect(heeftSprekers(verslag)).toBe(true);
    expect(heeftSprekers([jijBeurt])).toBe(false);
    expect(heeftSprekers([])).toBe(false);
  });

  it('geeft de sprekers oplopend op nummer, met de naam van hun eerste beurt', () => {
    expect(sprekersVan(verslag, NAMEN)).toEqual([
      { nummer: 1, naam: 'Marieke' },
      { nummer: 2, naam: 'Spreker 2' }
    ]);
    expect(sprekersVan([], NAMEN)).toEqual([]);
  });

  it('kent per soort de kant met sprekers', () => {
    expect(sprekerKant('online')).toBe('anderen');
    expect(sprekerKant('ruimte')).toBe('jij');
    expect(sprekerKant('alleen')).toBeNull();
  });

  it('laat alleen een beurt van de sprekerkant aan een spreker geven, in een verslag met sprekers', () => {
    const jij = jijBeurt;
    const anderen = anderenBeurt;
    expect(kanAanSpreker('online', verslag, anderen)).toBe(true);
    // Online heeft "Jij" geen sprekers.
    expect(kanAanSpreker('online', verslag, jij)).toBe(false);
    // In de ruimte is de microfoon de sprekerkant.
    const ruimteBeurt = beurt({ kant: 'jij', sprekernummer: 1, spreker: 'Spreker 1' });
    const ruimte = [ruimteBeurt];
    expect(kanAanSpreker('ruimte', ruimte, ruimteBeurt)).toBe(true);
    // Online zonder herkende sprekers blijft alles zoals het was.
    expect(kanAanSpreker('online', [jij], beurt())).toBe(false);
    expect(kanAanSpreker('alleen', ruimte, ruimteBeurt)).toBe(false);
  });

  it('laat in de ruimte ook zonder herkende sprekers een alinea aan een spreker geven', () => {
    const alinea = beurt({ kant: 'jij', sprekernummer: null, spreker: 'Jij' });
    // De eerste spreker maakt de gebruiker zelf; de labelkolom blijft weg tot dan.
    expect(kanAanSpreker('ruimte', [alinea], alinea)).toBe(true);
    expect(kanAanSpreker('ruimte', [], alinea)).toBe(true);
    expect(toontLabels('ruimte', [alinea])).toBe(false);
    // Alleen ik heeft nooit sprekers, en online "Jij" ook niet.
    expect(kanAanSpreker('alleen', [alinea], alinea)).toBe(false);
    expect(kanAanSpreker('online', [alinea], alinea)).toBe(false);
  });

  it('weet welke alinea in de ruimte niemand aanwees: geen nummer en geen eigen naam', () => {
    const alinea = beurt({ kant: 'jij', sprekernummer: null, spreker: null, eigenNaam: false });
    expect(zonderSpreker('ruimte', alinea)).toBe(true);
    expect(zonderSpreker('ruimte', { ...alinea, sprekernummer: 1 })).toBe(false);
    expect(zonderSpreker('ruimte', { ...alinea, eigenNaam: true })).toBe(false);
    // Online heeft "Jij" en "Anderen" een naam, en Alleen ik hoort er niet bij.
    expect(zonderSpreker('online', alinea)).toBe(false);
    expect(zonderSpreker('alleen', alinea)).toBe(false);
  });
});

describe('de melding na een naam', () => {
  const t = nl;
  const stap = (deel: Partial<Sprekerstap>): Sprekerstap => ({
    volgnummer: 1,
    kant: 'anderen',
    samengevoegd: [],
    naam: null,
    nummer: null,
    ...deel
  });

  it('noemt na leegmaken de naam die er echt staat', () => {
    expect(naamMelding(stap({ nummer: 2 }), t)).toBe('Terug naar Spreker 2.');
    expect(naamMelding(stap({}), t)).toBe('Terug naar Anderen.');
    expect(standaardNaam({ kant: 'jij', sprekernummer: null }, t)).toBe('Jij');
  });

  it('gebruikt de naam zoals Rust hem teruggaf, niet wat er getypt is', () => {
    // " NOOR " getypt, Noor bestond al: de melding zegt Noor.
    expect(naamMelding(stap({ samengevoegd: [1, 2], naam: 'Noor', nummer: 1 }), t)).toBe(
      'Spreker 1 en Spreker 2 zijn nu Noor.'
    );
    // "spreker 1" getypt bij Spreker 2 zonder namen: samen, zonder eigen naam.
    expect(naamMelding(stap({ samengevoegd: [1, 2], naam: null, nummer: 1 }), t)).toBe(
      'Spreker 1 en Spreker 2 zijn nu Spreker 1.'
    );
    expect(naamMelding(stap({ samengevoegd: [1, 3, null], naam: 'Noor', nummer: 1 }), t)).toBe(
      'Spreker 1, Spreker 3 en Anderen zijn nu Noor.'
    );
    expect(naamMelding(stap({ kant: 'jij', naam: 'Bram' }), t)).toBe('Naam gegeven: Bram.');
    expect(en.vergaderZijnNu(['Speaker 1', 'Speaker 2'], 'Noor')).toBe(
      'Speaker 1 and Speaker 2 are now Noor.'
    );
  });
});

describe('een nieuwe stap aan de sprekers', () => {
  const stap: Sprekerstap = {
    volgnummer: 7,
    kant: 'anderen',
    samengevoegd: [1, 2],
    naam: 'Noor',
    nummer: 1
  };
  it('alleen als Rust echt een nieuwe stap maakte', () => {
    expect(nieuweStap(null, stap)).toBe(stap);
    expect(nieuweStap(6, stap)).toBe(stap);
    // Opslaan zonder wijziging: dezelfde stap, dus geen nieuwe melding met een oude inhoud.
    expect(nieuweStap(7, stap)).toBeNull();
    expect(nieuweStap(7, null)).toBeNull();
  });
});
