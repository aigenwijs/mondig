// Controles die `node scripts/versie.mjs` (zonder argument) naast het gelijkstaan van de versies doet.
// Pure functies zonder bijwerkingen, zodat de tests geen echte repo nodig hebben.

/** Een ontwikkelversie eindigt op `-dev`; die hoort nog niet in de changelog. */
export function isDevVersie(versie) {
  return /-dev$/.test(versie);
}

const naarRegex = (tekst) => tekst.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Een uitgebrachte versie (niet op `-dev`) moet een eigen kop in de changelog hebben, in de vorm
 * `## [0.5.0] - 2026-11-11`. Geeft een melding terug, of `null` als alles klopt.
 */
export function controleerChangelog(versie, changelog) {
  if (isDevVersie(versie)) return null;
  const kop = new RegExp(`^## \\[${naarRegex(versie)}\\] - \\d{4}-\\d{2}-\\d{2}\\s*$`, 'm');
  if (kop.test(changelog)) return null;
  return (
    `versie ${versie} is geen ontwikkelversie, maar CHANGELOG.md heeft er geen kop voor. ` +
    `Zet "## [${versie}] - JJJJ-MM-DD" boven de wijzigingen van deze uitgave ` +
    `(of zet een ontwikkelversie met: node scripts/versie.mjs <volgende>-dev).`
  );
}

/**
 * Staat de tag `v<versie>` al op een andere commit dan HEAD, dan is die versie uitgebracht en mag
 * er niet op doorgebouwd worden. `tagCommit` is de commit van de tag, of `null` als de tag niet
 * bestaat (of git ontbreekt): dan valt er niets te controleren.
 */
export function controleerUitgebracht(versie, tagCommit, headCommit) {
  if (isDevVersie(versie)) return null;
  if (!tagCommit || !headCommit) return null;
  if (tagCommit === headCommit) return null;
  return (
    `versie ${versie} is al uitgebracht (tag v${versie} staat op een andere commit dan HEAD). ` +
    `Zet de volgende ontwikkelversie met: node scripts/versie.mjs <volgende>-dev`
  );
}

/**
 * Een uitgave maak je van een versie zonder `-dev`, met een tag die precies `v<versie>` heet.
 * Een andere voorlopige versie (`1.0.0-beta.1` met tag `v1.0.0-beta.1`) mag wel. Geeft een
 * melding terug, of `null` als alles klopt.
 */
export function controleerUitgave(versie, tag) {
  if (isDevVersie(versie)) {
    return (
      `versie ${versie} is een ontwikkelversie en kan geen uitgave zijn. Zet de versie van de ` +
      `uitgave met: node scripts/versie.mjs <versie>, werk CHANGELOG.md bij en tag die commit.`
    );
  }
  if (typeof tag !== 'string' || tag.trim() === '') {
    return `er is geen tag opgegeven. Verwacht: v${versie}`;
  }
  if (tag !== `v${versie}`) {
    return `de tag "${tag}" hoort niet bij versie ${versie} in de code. Verwacht: v${versie}`;
  }
  return null;
}
