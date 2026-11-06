// De changelog van de repo, in de app meegeleverd (ADR 0016): geen netwerk nodig. Vite neemt
// de tekst bij het bouwen op in de frontend.

import changelog from '../../../CHANGELOG.md?raw';
import { leesChangelog, type Uitgave } from './changelog';

export const uitgaven: readonly Uitgave[] = leesChangelog(changelog);
