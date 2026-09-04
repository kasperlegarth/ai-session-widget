# Claude Code Session Widget — Design

## Formål

En lille, headless Windows-widget der viser alle igangværende Claude Code-sessioner på maskinen, så man med et hurtigt kig kan se hvad hver session laver, og om nogen venter på godkendelse. Klik på en session fokuserer dens vindue.

## Ikke i scope (MVP)

- Detaljeret aktivitetsvisning ("Reading foo.ts", "Running npm test")
- System tray-ikon
- Cross-machine / remote sessions
- Persistering af vinduesposition/always-on-top mellem genstarter (kan tilføjes senere)

## Tech stack

Tauri (Rust-backend + HTML/CSS/TypeScript-frontend). Valgt over WPF for fri CSS-styling, og over Electron for lavere footprint på en så lille app.

## Datakilde

Claude Code skriver selv et lille session-register:

- `~/.claude/sessions/<pid>.json` — én fil pr. kørende interaktiv session
  - Felter der bruges: `pid`, `sessionId`, `cwd`, `name`, `status` (`"idle"` når til stede, ellers fraværende = aktivt arbejde), `updatedAt`
- `~/.claude/projects/<cwd-hash>/<sessionId>.jsonl` — append-only transskript for sessionen, bruges kun til at læse de sidste par linjer (status-heuristik, se nedenfor)

Ingen ændringer i Claude Code selv er nødvendige — widgeten er en ren consumer af eksisterende filer.

## Session-scanning (backend, Rust)

Poller `~/.claude/sessions/*.json` hvert 2. sekund:

1. Parse hver fil (`pid`, `sessionId`, `cwd`, `name`, `status`)
2. Verificér at processen med det `pid` stadig kører (via `sysinfo`-crate). Findes processen ikke længere, ignoreres/springes filen over (stale fil fra en session der crashede uden oprydning)
3. Beregn status (se herunder)
4. Send opdateret liste til frontend via Tauri event/command

### Status-heuristik (3 states)

For hver session, læs de sidste ~5 linjer af dens `.jsonl`-transskript:

| State | Betingelse | Visning |
|---|---|---|
| **Working** | `status` ikke sat i sessions-filen (aktivt igang) | Grøn dot |
| **Needs input** | `status:"idle"` **og** seneste besked i transskriptet er et `tool_use` uden efterfølgende matchende `tool_result` | Rød/orange dot — det er typisk fordi Claude Code venter på brugerens godkendelse af et værktøjskald |
| **Waiting** | `status:"idle"` og seneste besked er almindelig assistant-tekst (intet pending tool_use) | Grå/blå dot — færdig, venter på næste besked |

Dette er en heuristik, ikke et garanteret signal fra Claude Code — der findes ikke et eksplicit "needs approval"-flag i dag.

## Vinduesfokusering (klik på en session)

1. `EnumWindows` + `GetWindowThreadProcessId` for at finde et synligt topvindue der ejes direkte af sessionens `pid`
2. Findes intet, gå op ad procestræet (parent process) og prøv igen, indtil et vindue findes eller roden nås
3. Kald `SetForegroundWindow` på det fundne vindue

**Kendt begrænsning:** Virker præcist når hver Claude Code-session har sit eget konsolvindue (bekræftet tilfælde på brugerens maskine — separate `claude.exe`-processer med hver sit Console-session). I opsætninger hvor flere sessioner deler ét terminal-værtsprogram med faner (fx flere faner i samme Windows Terminal-vindue), vil fokusering ramme hele terminalvinduet, ikke den specifikke fane.

## UI

- Frameless vindue (ingen titlebar, ingen minimer/maksimer/luk-knapper)
- Always-on-top som default
- Transparent baggrund, rundede hjørner
- Kompakt liste, én række pr. session: navn (`name`-feltet), mappe (sidste segment af `cwd`, fuld sti som tooltip), status-dot
- Klik på en række → fokusér sessionens vindue
- Listen opdaterer sig selv automatisk hvert 2. sekund (matcher backend-scan-intervallet)
- Højreklik åbner en custom kontekstmenu (ikke Windows' systemmenu) med tre punkter:
  - **Always on top** (checkbox/toggle)
  - **Refresh** (manuel tving-opdatering, ud over auto-refresh)
  - **Close** (lukker widgeten)
- Intet system tray-ikon

## Fejlhåndtering

- Manglende/tom `~/.claude/sessions/`-mappe → vis tom liste med en let besked ("Ingen aktive sessioner")
- Fil der ikke kan parses (korrupt/delvist skrevet under polling) → springes over denne cyklus, prøves igen næste poll
- Vindue kan ikke findes ved klik → ingen crash, blot no-op (evt. kort visuel feedback senere, ikke MVP)

## Testing

- Manuel verifikation: start flere reelle Claude Code-sessioner (interactive + evt. en der venter på tool-approval), bekræft at widgeten viser korrekt navn/mappe/status for hver, og at klik fokuserer det rette vindue
- Rust-side: unit-test af status-heuristikken med syntetiske `.jsonl`-uddrag (working / needs-input / waiting-cases)
