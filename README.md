# Omanote

Note veloci sincronizzate con **Joplin Server**, con l'estetica di
**Omarchy**. Una sola app per **Linux, macOS, iOS e Android**.

Il server non viene toccato: Omanote parla lo stesso protocollo dei client Joplin
ufficiali (sync target versione 3), scrive gli stessi file `<id>.md` e usa la stessa
crittografia end-to-end. Puoi tenere Joplin e Omanote sullo stesso account e usarli
insieme.

## Com'è fatto

| Parte | Tecnologia | Perché |
| --- | --- | --- |
| Core | Rust (`crates/omanote-core`) | Sync, E2EE, database e modello dati: un solo codice nativo per tutte le piattaforme |
| App | Tauri 2 (`app/src-tauri`) | Usa la webview di sistema: binari di pochi MB, niente Chromium incluso |
| UI | Svelte 5 + CodeMirror 6 (`app/src`) | Editor leggero con checkbox, elenchi e calcoli in linea |

```
crates/omanote-core/
  item.rs    formato delle note Joplin (serializzazione e parsing)
  e2ee.rs    crittografia: AES-256-GCM + PBKDF2 (attuale), AES-CCM/SJCL (storica)
  api.rs     client di Joplin Server
  store.rs   database locale SQLite
  sync.rs    motore di sincronizzazione (lock, upload, delta, conflitti)
app/
  src/lib/calc.ts    calcolatrice in linea
  src/lib/editor.ts  editor CodeMirror
  src/lib/theme.ts   temi (segue Omarchy quando disponibile)
  src-tauri/         comandi, tray, hotkey, portachiavi, tema di sistema
tools/gen-themes.py  rigenera i temi dalle palette ufficiali di Omarchy
```

## Compatibilità con Joplin

- **Formato**: le note restano note Joplin normali (titolo = prima riga, corpo Markdown).
  I campi sconosciuti vengono conservati, così le note scritte da versioni più recenti
  di Joplin non si danneggiano passando da Omanote.
- **E2EE**: Omanote decifra i metodi `KeyV1`, `StringV1`, `FileV1` (AES-256-GCM) e quelli
  storici `SJCL1a`, `SJCL1b`, `SJCL3`, `SJCL4` (AES-CCM); scrive con il metodo attuale
  di Joplin (`StringV1`). L'unico caso non supportato è il vecchio OCB2 (`SJCL`, `SJCL2`,
  pre-2020): apri Joplin desktop una volta, si offrirà di aggiornare quelle chiavi.
- **Master key**: Omanote non crea né modifica `info.json`. Attiva la crittografia da
  Joplin; Omanote la riconosce e chiede la password master.
- **Conflitti**: come Joplin. Vince la versione remota e quella locale resta come nota
  separata con `is_conflict = 1` (in Joplin appare nella cartella "Conflitti").
- **Cestino**: eliminare una nota imposta `deleted_time`, cioè il cestino di Joplin.

## Funzioni

- Si apre su una nota nuova; si scorre tra le note con `⌘[` / `⌘]` o con uno swipe.
- Cartelle: la nota vive in un notebook di Joplin scelto come radice; le sottocartelle
  sono normali sotto-notebook.
- Checklist: `- [ ]` diventa una casella cliccabile, `⌘↵` la spunta, l'invio continua l'elenco.
- Calcoli in linea: `2+3*4`, `iva = 22%`, `100 € + iva`, `20% di 50`, `totale`, `media`,
  `sqrt(16)`, `ans`. Clic sul risultato per copiarlo.
- Ricerca rapida con `⌘K`, spostamento nota con `⌘⇧M`, cartelle con `⌘\`.
- Desktop: icona nella menu bar/tray e scorciatoia globale (`⌘⇧Spazio`) per aprire al volo.
- Temi: su Omarchy segue il tema di sistema e cambia insieme a `omarchy-theme-set`;
  altrove ci sono i 22 temi di Omarchy inclusi.

## Sviluppo

```bash
npm install --prefix app
npm run --prefix app tauri dev     # app desktop
npm run --prefix app dev           # solo interfaccia nel browser, con dati finti
cargo test                         # test del core (formato, E2EE, database, sync)
npm run --prefix app test          # test della calcolatrice
```

I test di compatibilità E2EE girano su vettori generati con lo stesso codice JavaScript
usato da Joplin (`crates/omanote-core/tests/joplin_vectors.json`).

## Stato

Funzionante e verificato end-to-end contro un Joplin Server reale con E2EE attiva:
le note create da Joplin arrivano in Omanote e viceversa.

Da fare: allegati/immagini, tag, build e firma per iOS e Android, ricerca full-text
indicizzata, cestino consultabile dall'app.
