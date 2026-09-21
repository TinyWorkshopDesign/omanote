# Omanote

Note veloci sincronizzate con **Joplin Server**, con l'estetica di
**Omarchy**. Una sola app per **Linux, macOS, iOS e Android**, in 8 lingue.

Il server non viene toccato: Omanote parla lo stesso protocollo dei client Joplin
ufficiali (sync target versione 3), scrive gli stessi file `<id>.md` e usa la stessa
crittografia end-to-end. Joplin e Omanote convivono sullo stesso account.

## Funzioni

**Note veloci**
- Si apre su una nota nuova; `⌘[` / `⌘]` o lo swipe a due dita scorrono tra le note.
  Andare oltre la più recente ne crea una nuova; una nota lasciata vuota si cancella da sola.
- Menu nascosto: compare avvicinando il mouse al bordo alto della nota (sempre visibile su touch).
- Barra inferiore a scomparsa: ‹ un puntino per nota › e «+» per una nota
  nuova; compare vicino al bordo basso o per un attimo quando cambi nota.
- Pannello delle cartelle ad albero (notebook → cartelle → note): rami apribili, nota
  corrente evidenziata, azioni su ogni cartella, trascina una nota su una cartella per spostarla.
- Parole chiave sulla prima riga (anche tradotte: `lista`, `somma`, `media`…):
  - `list` / `list: Titolo`: ogni riga diventa una casella; `/x` a fine riga la spunta;
  - `math`: risultati automatici anche senza `=`;
  - `sum`, `avg`: somma o media di tutti i numeri della nota;
  - `count`: elementi, righe, parole, caratteri;
  - `code`: niente formattazione.
- Caselle `[] ` o `- [ ] `, elenchi puntati e numerati che si continuano con Invio, `//` per
  commentare una riga, markdown semplice (`**grassetto**`, `*corsivo*`, `__sottolineato__`, `~~barrato~~`).
- Calcoli: scrivi `=` alla fine della riga e il risultato compare subito dopo: `2+3*4=`,
  `100 € + iva =`, `20% di 50 =`, `sqrt(16) =`. Senza `=` la riga resta testo. Le variabili
  (`iva = 22%`) si memorizzano senza mostrare nulla; `totale =` / `media =` sommano il blocco
  sopra. Nelle note che iniziano con `math` i risultati compaiono sempre. Clic sul risultato
  per copiarlo.
- **Timer** su qualsiasi riga, poi Invio: `timer` (cronometro), `timer 5` / `timer 3:30` (conto
  alla rovescia), `timer 9am` / `timer 21:15` (fino a un orario), `timer 5: Pasta` (con nome),
  `timer 25 5` / `timer pomo` (pomodoro), `timer p` / `r` / `s` (pausa, riavvia, stop). Il
  timer continua con la finestra nascosta, notifica alla fine e compare nella menu bar su macOS.
- **Immagini**: trascinando o incollando un'immagine Omanote chiede se inserirla come
  **immagine** (`I`) o come **testo OCR** (`T`). Le immagini diventano allegati Joplin
  (cifrati con la E2EE) e si vedono anche nei client Joplin; quelle allegate in Joplin si
  vedono in Omanote.
- **OCR**: con «Testo» il contenuto dell'immagine finisce nella nota. `⌘⇧O` cattura una
  zona dello schermo. Tutto sul dispositivo: Apple Vision su macOS/iOS, tesseract su Linux
  (già incluso in Omarchy, rispetta `OMARCHY_OCR_LANGS`).

**Joplin**
- **Notebook di lavoro**: Omanote vede solo un notebook di Joplin (e i suoi sotto-notebook,
  che diventano le cartelle). Lo scegli e lo cambi quando vuoi dalle Impostazioni, così il resto
  del tuo Joplin resta intatto.
- E2EE completa: legge `KeyV1`, `StringV1`, `FileV1` e i metodi storici `SJCL1a`, `SJCL1b`,
  `SJCL3`, `SJCL4`; scrive con `StringV1` come Joplin. Non supportato solo il vecchio OCB2
  (pre-2020): Joplin desktop propone di aggiornare quelle chiavi.
- Conflitti gestiti come Joplin (la copia locale finisce nella cartella "Conflitti").
- Eliminare una nota la sposta nel cestino di Joplin.

**Omarchy**
- Segue il tema di sistema (`~/.config/omarchy/current/theme`) e cambia al volo con il menu
  temi; altrove include i 22 temi di Omarchy. Font JetBrains Mono.
- Scorciatoie da tastiera con `Ctrl` al posto di `⌘`: nessun conflitto con Omarchy, che usa `Super`.
- Scorciatoia globale su Hyprland, in `~/.config/hypr/bindings.conf`:

  ```
  bindd = SUPER ALT, N, Omanote, exec, omanote --toggle
  ```

  (`omanote --new` apre una nota nuova, `omanote --capture` avvia l'OCR dello schermo.)
  Su macOS la scorciatoia globale è `⌥A`.

**AI friendly**
- `omanote-cli` lavora sulle stesse note dell'app: `list`, `show`, `search`, `new`, `append`,
  `edit`, `move`, `trash`, `sync`, con `--json` per gli script.
- `omanote-cli mcp` è un server MCP: `claude mcp add omanote -- omanote-cli mcp`.
- L'app si aggiorna da sola quando un agente modifica le note.
- `AGENTS.md` descrive architettura e regole del progetto per chi sviluppa con l'AI.

## Scorciatoie

| | |
| --- | --- |
| `⌘N` | nuova nota |
| `⌘[` `⌘]` | nota precedente / successiva |
| `⌘1` / `⌘⇧1` | vai alla più recente / porta in cima |
| `⌘D` | elimina nota |
| `⌘F` | cerca (Invio senza risultati crea una nota) |
| `⌘⇧K` / `⌘⇧M` | spunta riga / casella → punto → numero |
| `⌘B` `⌘I` `⌘U` `⌘⇧X` | grassetto, corsivo, sottolineato, barrato |
| `⌘⇧H` / `⌘/` | livello titolo / commento |
| `⌥↑` `⌥↓` | sposta riga |
| `⌘P` / `⌘W` | finestra in primo piano / nascondi |
| `⌘+` `⌘−` | dimensione testo |
| `⌘\` / `⌘E` | cartelle / sposta nota |
| `⌘⇧O` / `⌘S` / `⌘,` | OCR schermo / sincronizza / impostazioni |
| `Esc` | ferma il timer |

## Com'è fatto

| Parte | Tecnologia |
| --- | --- |
| Core (`crates/omanote-core`) | Rust: formato Joplin, E2EE, client server, SQLite, sync |
| CLI + MCP (`crates/omanote-cli`) | Rust, stesso database dell'app |
| App (`app/src-tauri`) | Tauri 2: webview di sistema, binari di pochi MB |
| UI (`app/src`) | Svelte 5 + CodeMirror 6, 616 KB compresi i font |

## Sviluppo

```bash
npm install --prefix app
npm run --prefix app tauri dev      # app desktop
npm run --prefix app dev            # solo interfaccia nel browser, con dati finti
cargo test                          # test Rust
npm run --prefix app test           # test di calcolatrice e modalità
cargo install --path crates/omanote-cli
```

I test E2EE usano vettori generati con lo stesso codice JavaScript di Joplin.

## Stato

Verificato end-to-end contro un Joplin Server reale con E2EE, incluso il caso dei conflitti.
Da fare: tag, build firmate per iOS e Android,
OCR su Android, notifiche del timer programmate su mobile.
