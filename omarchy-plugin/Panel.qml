import QtQuick 6.11
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Omanote in the Omarchy bar. The panel works like the app on a phone, a
// navigation stack Folders -> Notes -> Note:
//   - Folders: All Notes and the folder tree; Enter or click opens a folder.
//   - Notes: search field and the folder's notes in date sections. Typing
//     searches; ↑/↓ pick a note and Enter opens it; Enter with no note picked
//     saves the text as a new note in the folder (quick capture).
//   - Note: the note itself, editable, saved as you type. Calculations show their
//     results as in the app (Calc.js is the app's calculator), images and sketches
//     show in place, and "Draw" opens a simple canvas whose drawing is saved as an
//     editable sketch, like the app's. Esc goes back.
// Right-click shows/hides the app window, middle-click opens a new note in it.
//
// All note data goes through omanote-cli, which shares the app's database, so
// the app sees changes at once and syncs them with Joplin Server. Nothing runs
// in the background: data is read when the panel opens or a level is shown.
Panel {
  id: root
  moduleName: "tinyworkshop.omanote"
  ipcTarget: "tinyworkshop.omanote"

  // ------------------------------------------------------------------ state

  property string level: "list"          // "folders" | "list" | "note" | "sketch"
  property string folderId: ""           // "" = All Notes
  property var folders: []
  property var notes: []
  property var results: []
  property string query: ""
  property int folderCursor: -1
  property int cursor: -1
  property string status: ""
  property bool failed: false

  // The note on screen. Every opened note or new draft gets a token, so a save
  // still running for the previous note lands on that note, not on this one.
  property int noteToken: 0
  property var noteInfo: ({})            // token -> { id, parent }
  property string noteId: ""
  property string noteParent: ""
  property real noteUpdated: 0
  property bool noteEncrypted: false
  property bool noteLoading: false
  property string noteText: ""
  property string savedText: ""
  property var pendingSaves: []          // [{ token, text }], one per token
  property var trashAfterSave: ({})      // token -> true
  property bool confirmTrash: false

  // The note split around its images (Model.splitBlocks), the calculator results
  // per text block, and where the cursor was (a new sketch goes after that line).
  property var blocks: []
  property var calcBlocks: []
  property int focusBlock: 0
  property int focusPos: 0
  property bool textFocus: false
  property var fxRates: undefined
  property int imageVersion: 0           // bumped to reload images after a sketch edit

  // The drawing canvas.
  property var strokes: []
  property var liveStroke: null
  property string sketchReplace: ""      // attachment id of the sketch being edited
  property int sketchBlock: -1
  property int sketchImage: 0
  property string inkTool: "pen"         // "pen" | "marker" | "erase"
  property int inkSize: 1
  property string inkColor: ""

  readonly property bool searching: query.trim() !== ""
  readonly property var shownNotes: searching ? results : notes
  readonly property var noteRows: Model.noteRows(shownNotes)
  readonly property var folderRows: Model.folderRows(folders)
  readonly property string folderName: Model.folderTitle(folders, folderId)
  readonly property bool inNote: level === "note" || level === "sketch"
  readonly property bool dirty: inNote && !noteLoading && noteText !== savedText

  // Off keeps note contents out of sight on a shared screen: titles only.
  readonly property bool showPreview: setting("showPreview", true) !== false
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property color resultColor: Color.accent
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property string dataDir: (Quickshell.env("XDG_DATA_HOME") || (Quickshell.env("HOME") + "/.local/share")) + "/app.omanote"

  // Ink: the app's widths (CSS px) for its three sizes, and four theme colours.
  readonly property var inkWidths: ({ pen: [2.5, 5, 10], marker: [12, 20, 32], erase: [12, 28, 56] })
  readonly property var inkColors: Model.inkColors([Color.foreground, Color.accent, Color.urgent])

  function glyph(cp) { return String.fromCodePoint(cp) }

  // --------------------------------------------------------------- the CLI

  // One command per process at a time; a newer request waits and replaces any
  // request already waiting, and the stale result is dropped.
  function run(proc, command) {
    if (proc.running) { proc.queued = command; return }
    proc.queued = null
    proc.command = command
    proc.running = true
  }

  function runQueued(proc) {
    if (!proc.queued) return false
    var command = proc.queued
    Qt.callLater(function() { root.run(proc, command) })
    proc.queued = null
    return true
  }

  function cliError(exitCode, stderr, fallback) {
    return exitCode === 127 ? "omanote-cli is not installed" : (String(stderr || "").trim() || fallback)
  }

  function refreshFolders() { run(foldersProcess, ["omanote-cli", "folders", "--json"]) }

  function refreshNotes() {
    var command = ["omanote-cli", "list", "--json", "--limit", "500"]
    if (folderId !== "") command.push("--folder", folderId)
    run(listProcess, command)
    if (searching) runSearch()
  }

  function runSearch() {
    if (!searching) { results = []; return }
    run(searchProcess, ["omanote-cli", "search", "--json", query.trim()])
  }

  // ------------------------------------------------------------ navigation

  function textAreaAt(i) {
    var loader = blockRepeater.itemAt(i)
    return loader && loader.item && loader.item.area ? loader.item.area : null
  }

  function lastTextBlock() {
    for (var i = blocks.length - 1; i >= 0; i--) if (blocks[i].kind === "text") return i
    return -1
  }

  function focusText(i, atEnd) {
    var area = textAreaAt(i)
    if (!area) return false
    area.forceActiveFocus()
    if (atEnd === true) area.cursorPosition = area.length
    else if (atEnd === false) area.cursorPosition = 0
    return true
  }

  function focusLevel() {
    Qt.callLater(function() {
      if (root.level === "note") {
        if (!root.focusText(root.focusBlock)) {
          var last = root.lastTextBlock()
          if (last < 0 || !root.focusText(last, true)) keyCatcher.forceActiveFocus()
        }
      } else if (root.level === "sketch") sketchArea.forceActiveFocus()
      else if (root.level === "list") field.forceActiveFocus()
      else keyCatcher.forceActiveFocus()
    })
  }

  function showFolders() {
    leaveNote()
    level = "folders"
    field.text = ""
    folderCursor = 0
    for (var i = 0; i < folderRows.length; i++) if (folderRows[i].id === folderId) folderCursor = i
    refreshFolders()
    focusLevel()
  }

  function openFolder(id) {
    folderId = id || ""
    level = "list"
    field.text = ""
    cursor = -1
    notes = []
    status = ""
    failed = false
    refreshNotes()
    focusLevel()
  }

  function back() {
    if (level === "sketch") {
      closeSketch()
    } else if (level === "note") {
      leaveNote()
      level = "list"
      cursor = -1
      refreshNotes()
      focusLevel()
    } else if (level === "list") {
      showFolders()
    }
  }

  function beginNote(id, parent) {
    leaveNote()
    noteToken += 1
    noteInfo[noteToken] = { id: id, parent: parent }
    noteId = id
    noteParent = parent
    confirmTrash = false
    status = ""
    failed = false
    level = "note"
  }

  function openNote(note) {
    if (!note) return
    beginNote(String(note.id), String(note.parent_id || ""))
    noteUpdated = Number(note.updated_time || 0)
    noteEncrypted = !!note.encrypted
    noteLoading = true
    savedText = ""
    setNoteText("")
    focusBlock = -1
    run(showProcess, ["omanote-cli", "show", "--json", noteId])
  }

  // A new note in the folder on screen; it is created on the first save.
  function newNote(text) {
    beginNote("", folderId)
    noteUpdated = 0
    noteEncrypted = false
    noteLoading = false
    savedText = ""
    setNoteText(text || "")
    focusBlock = -1
    if (noteText !== "") edited()
    focusLevel()
  }

  function leaveNote() {
    if (level === "sketch") closeSketch()
    if (level !== "note") return
    autosave.stop()
    saveNow()
    confirmTrash = false
  }

  function openInApp() {
    if (noteId === "") return
    leaveNote()
    Quickshell.execDetached(["omanote", "--open", noteId])
    root.close()
  }

  // ------------------------------------------------------------------ notes

  // Replaces the whole note on screen (loading, or after inserting an image):
  // the text blocks are rebuilt.
  function setNoteText(text) {
    blocks = Model.splitBlocks(text)
    noteText = text
    recalc()
  }

  // The note changed by the panel itself: rebuild it and save.
  function applyText(text) {
    setNoteText(text)
    edited()
  }

  // A text block was typed in.
  function blockEdited(index, text) {
    var block = blocks[index]
    if (!block || block.kind !== "text" || block.text === text) return
    block.text = text
    noteText = Model.joinBlocks(blocks)
    recalc()
    edited()
  }

  function recalc() {
    calcBlocks = Model.blockResults(blocks, Model.lineResults(noteText, fxRates))
  }

  function edited() {
    if (!inNote || noteLoading) return
    if (!failed) status = ""
    confirmTrash = false
    autosave.restart()
  }

  function saveNow() {
    if (!inNote || noteLoading || noteEncrypted) return
    var text = noteText
    if (text === savedText) return
    if (noteId === "" && text.trim() === "") return
    var token = noteToken
    var queue = pendingSaves.filter(function(s) { return s.token !== token })
    queue.push({ token: token, text: text })
    pendingSaves = queue
    pumpSaves()
  }

  function pumpSaves() {
    if (saveProcess.running || pendingSaves.length === 0) return
    var job = pendingSaves[0]
    pendingSaves = pendingSaves.slice(1)
    var info = noteInfo[job.token] || {}
    var command
    if (info.id) {
      command = ["omanote-cli", "edit", info.id, Model.cliText(job.text)]
    } else {
      command = ["omanote-cli", "new", "--json"]
      if (info.parent) command.push("--folder", info.parent)
      command.push(Model.cliText(job.text))
    }
    saveProcess.token = job.token
    saveProcess.sentText = job.text
    saveProcess.command = command
    saveProcess.running = true
  }

  // Quick capture from the search field: a new note, without leaving the list.
  function capture() {
    var text = query.trim()
    if (text === "" || captureProcess.running) return
    var command = ["omanote-cli", "new"]
    if (folderId !== "") command.push("--folder", folderId)
    command.push(Model.cliText(text))
    captureProcess.command = command
    captureProcess.running = true
  }

  function trashNote() {
    if (!confirmTrash) {
      confirmTrash = true
      status = "Click the bin again to move this note to the trash"
      failed = false
      trashTimer.restart()
      return
    }
    confirmTrash = false
    autosave.stop()
    var token = noteToken
    var id = noteId
    pendingSaves = pendingSaves.filter(function(s) { return s.token !== token })
    if (id === "" && saveProcess.running && saveProcess.token === token) trashAfterSave[token] = true
    // Detach the editor from this note before leaving, so nothing saves it again.
    noteToken += 1
    savedText = noteText
    level = "list"
    cursor = -1
    if (id !== "") run(trashProcess, ["omanote-cli", "trash", id])
    else refreshNotes()
    focusLevel()
  }

  function removeImage(index, image) {
    if (noteEncrypted) return
    applyText(Model.removeImage(blocks, index, image))
    focusLevel()
  }

  // ---------------------------------------------------------------- drawing

  // A new drawing (index < 0) or the strokes of the sketch in block `index`.
  function openSketch(index, data, image) {
    if (noteEncrypted || noteLoading) return
    autosave.stop()
    saveNow()
    sketchBlock = index
    sketchImage = image || 0
    var picture = index >= 0 && blocks[index] && blocks[index].images ? blocks[index].images[sketchImage] : null
    sketchReplace = picture ? picture.id : ""
    strokes = data && data.strokes ? data.strokes : []
    liveStroke = null
    if (inkColor === "") inkColor = inkColors[0]
    status = ""
    failed = false
    level = "sketch"
    sketchCanvas.requestPaint()
    focusLevel()
  }

  function closeSketch() {
    if (level !== "sketch") return
    strokes = []
    liveStroke = null
    level = "note"
    focusLevel()
  }

  function insertSketch() {
    if (sketchProcess.running) return
    var ink = strokes.filter(function(s) { return s.kind !== "erase" })
    if (ink.length === 0) { closeSketch(); return }
    var command = ["omanote-cli", "sketch", "--json", "--background", Model.hex(Color.background)]
    if (sketchReplace !== "") command.push("--replace", sketchReplace)
    sketchProcess.token = noteToken
    sketchProcess.payload = JSON.stringify({ strokes: strokes })
    sketchProcess.command = command
    sketchProcess.running = true
  }

  function chooseTool(tool) {
    inkTool = tool
  }

  function undoStroke() {
    if (strokes.length === 0) return
    strokes = strokes.slice(0, strokes.length - 1)
    sketchCanvas.requestPaint()
  }

  function paintStroke(ctx, s) {
    var pts = s.pts || []
    if (pts.length === 0) return
    ctx.save()
    ctx.globalCompositeOperation = s.kind === "erase" ? "destination-out" : "source-over"
    ctx.globalAlpha = s.kind === "marker" ? 0.35 : (s.kind === "pencil" ? 0.8 : 1)
    ctx.strokeStyle = s.kind === "erase" ? "#000000" : (s.color || "#000000")
    ctx.fillStyle = ctx.strokeStyle
    ctx.lineWidth = Math.max(0.5, s.width)
    ctx.lineCap = "round"
    ctx.lineJoin = "round"
    if (pts.length === 1) {
      ctx.beginPath()
      ctx.arc(pts[0][0], pts[0][1], Math.max(0.5, s.width / 2), 0, Math.PI * 2)
      ctx.fill()
    } else {
      ctx.beginPath()
      ctx.moveTo(pts[0][0], pts[0][1])
      for (var i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1])
      ctx.stroke()
    }
    ctx.restore()
  }

  // ----------------------------------------------------------------- cursor

  function moveFolderCursor(dy) {
    if (folderRows.length === 0) { folderCursor = -1; return }
    folderCursor = Math.max(0, Math.min(folderRows.length - 1, folderCursor + dy))
  }

  function moveCursor(dy) {
    if (noteRows.length === 0) { cursor = -1; return }
    cursor = Math.max(-1, Math.min(noteRows.length - 1, cursor + dy))
  }

  function activate() {
    if (cursor >= 0 && cursor < noteRows.length) openNote(noteRows[cursor].note)
    else capture()
  }

  // Keeps the caret of a text block in view inside the note's scroll area.
  function ensureVisible(area) {
    var flick = editorScroll.contentItem
    if (!flick || !area) return
    var r = area.cursorRectangle
    var p = area.mapToItem(blockColumn, r.x, r.y)
    if (p.y < flick.contentY) flick.contentY = Math.max(0, p.y - Style.space(8))
    else if (p.y + r.height > flick.contentY + flick.height)
      flick.contentY = p.y + r.height - flick.height + Style.space(8)
  }

  onQueryChanged: {
    cursor = -1
    if (!captureProcess.running) { status = ""; failed = false }
    searchDelay.restart()
  }

  onFxRatesChanged: if (inNote) recalc()

  onOpenedChanged: {
    if (opened) {
      status = ""
      failed = false
      confirmTrash = false
      fxFile.reload()
      refreshFolders()
      if (level === "list") {
        cursor = -1
        refreshNotes()
      } else if (level === "note" && noteId !== "" && !dirty && !saveProcess.running && pendingSaves.length === 0) {
        run(showProcess, ["omanote-cli", "show", "--json", noteId])
      }
      focusLevel()
    } else if (level === "note") {
      leaveNote()
    }
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Timer { id: autosave; interval: 700; onTriggered: root.saveNow() }
  Timer { id: searchDelay; interval: 200; onTriggered: root.runSearch() }
  Timer { id: trashTimer; interval: 4000; onTriggered: if (root.confirmTrash) { root.confirmTrash = false; root.status = "" } }

  // The app's exchange rates, for "100 € in $ =".
  FileView {
    id: fxFile
    path: root.dataDir + "/fx-rates.json"
    printErrors: false
    onLoaded: root.fxRates = Model.ratesFrom(text())
  }

  // -------------------------------------------------------------- processes

  Process {
    id: foldersProcess
    property var queued: null
    stdout: StdioCollector { id: foldersOut; waitForEnd: true }
    stderr: StdioCollector { id: foldersErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (root.runQueued(foldersProcess)) return
      if (exitCode === 0) root.folders = Model.parseList(foldersOut.text)
      else if (root.level === "folders") {
        root.failed = true
        root.status = root.cliError(exitCode, foldersErr.text, "Could not read the folders")
      }
    }
  }

  Process {
    id: listProcess
    property var queued: null
    stdout: StdioCollector { id: listOut; waitForEnd: true }
    stderr: StdioCollector { id: listErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (root.runQueued(listProcess)) return
      if (exitCode === 0) {
        root.notes = Model.parseList(listOut.text)
      } else {
        root.notes = []
        root.failed = true
        root.status = root.cliError(exitCode, listErr.text, "Could not read notes")
      }
    }
  }

  Process {
    id: searchProcess
    property var queued: null
    stdout: StdioCollector { id: searchOut; waitForEnd: true }
    stderr: StdioCollector { id: searchErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (root.runQueued(searchProcess)) return
      if (!root.searching) return
      if (exitCode !== 0) {
        root.results = []
        root.failed = true
        root.status = root.cliError(exitCode, searchErr.text, "Search failed")
        return
      }
      var found = Model.parseList(searchOut.text)
      var folder = root.folderId
      root.results = folder === "" ? found : found.filter(function(n) { return n.parent_id === folder })
    }
  }

  Process {
    id: showProcess
    property var queued: null
    stdout: StdioCollector { id: showOut; waitForEnd: true }
    stderr: StdioCollector { id: showErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (root.runQueued(showProcess)) return
      var note = exitCode === 0 ? Model.parseObject(showOut.text) : null
      if (!note || root.level !== "note" || String(note.id) !== root.noteId) {
        if (exitCode !== 0 && root.level === "note") {
          root.noteLoading = false
          root.noteEncrypted = true   // nothing to edit: keep it read-only
          root.failed = true
          root.status = root.cliError(exitCode, showErr.text, "Could not open the note")
        }
        return
      }
      if (root.dirty) return          // typed meanwhile: keep the user's text
      var text = String(note.text || "")
      root.noteLoading = true
      root.savedText = text
      if (text !== root.noteText || root.blocks.length === 0) root.setNoteText(text)
      root.noteEncrypted = !!note.encrypted
      root.noteParent = String(note.parent_id || root.noteParent)
      root.noteUpdated = Number(note.updated_time || root.noteUpdated)
      root.noteLoading = false
      root.focusLevel()
    }
  }

  Process {
    id: saveProcess
    property int token: 0
    property string sentText: ""
    stdout: StdioCollector { id: saveOut; waitForEnd: true }
    stderr: StdioCollector { id: saveErr; waitForEnd: true }
    onExited: function(exitCode) {
      var token = saveProcess.token
      var info = root.noteInfo[token] || {}
      if (exitCode === 0) {
        if (!info.id) {
          var created = Model.parseObject(saveOut.text)
          if (created && created.id) info.id = String(created.id)
          root.noteInfo[token] = info
        }
        if (root.trashAfterSave[token] && info.id) {
          delete root.trashAfterSave[token]
          root.run(trashProcess, ["omanote-cli", "trash", info.id])
        } else if (token === root.noteToken) {
          root.noteId = info.id || root.noteId
          root.savedText = saveProcess.sentText
          root.noteUpdated = Date.now()
          if (!root.confirmTrash) { root.status = ""; root.failed = false }
        }
      } else if (token === root.noteToken) {
        root.failed = true
        root.status = root.cliError(exitCode, saveErr.text, "Could not save the note")
      }
      if (root.pendingSaves.length > 0) Qt.callLater(root.pumpSaves)
      else if (!root.inNote) Qt.callLater(root.refreshNotes)
    }
  }

  Process {
    id: captureProcess
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector { id: captureErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode === 0) {
        field.text = ""
        root.status = "Saved"
        root.failed = false
        root.refreshNotes()
      } else {
        root.failed = true
        root.status = root.cliError(exitCode, captureErr.text, "Could not save the note")
      }
      root.focusLevel()   // the field was disabled while saving
    }
  }

  Process {
    id: trashProcess
    property var queued: null
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector { id: trashErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode === 0) {
        root.status = "Moved to the trash"
        root.failed = false
      } else {
        root.failed = true
        root.status = root.cliError(exitCode, trashErr.text, "Could not move the note to the trash")
      }
      if (!root.runQueued(trashProcess)) root.refreshNotes()
    }
  }

  // The drawing goes to the CLI as one JSON line on stdin; it renders the PNG like
  // the app (strokes inside, so it stays editable) and stores the attachment.
  Process {
    id: sketchProcess
    property int token: 0
    property string payload: ""
    stdinEnabled: true
    stdout: StdioCollector { id: sketchOut; waitForEnd: true }
    stderr: StdioCollector { id: sketchErr; waitForEnd: true }
    onStarted: {
      write(payload + "\n")
      payload = ""
    }
    onExited: function(exitCode) {
      if (sketchProcess.token !== root.noteToken) return
      var out = exitCode === 0 ? Model.parseObject(sketchOut.text) : null
      if (!out || !out.tag) {
        root.failed = true
        root.status = root.cliError(exitCode, sketchErr.text, "Could not save the drawing")
        return
      }
      var replaced = root.sketchReplace !== ""
      var index = root.sketchBlock
      root.strokes = []
      root.level = "note"
      if (replaced && root.blocks[index] && root.blocks[index].kind === "image") {
        root.applyText(Model.setImageWidth(root.blocks, index, root.sketchImage, out.width))
        root.imageVersion += 1
      } else {
        root.applyText(Model.insertImageLine(root.blocks, root.focusBlock, root.focusPos, out.tag))
      }
      root.focusLevel()
    }
  }

  // ------------------------------------------------------------------- bar

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    tooltipText: root.opened ? "" : "Omanote"
    iconComponent: Component {
      Item {
        PencilIcon {
          anchors.centerIn: parent
          iconSize: Style.bar.iconCanvas
          color: root.barForeground
        }
      }
    }
    onPressed: function(buttonCode) {
      if (buttonCode === Qt.RightButton) Quickshell.execDetached(["omanote", "--toggle"])
      else if (buttonCode === Qt.MiddleButton) Quickshell.execDetached(["omanote", "--new"])
      else root.toggle()
    }
  }

  // ----------------------------------------------------------------- panel

  Component {
    id: pencilIcon
    PencilIcon {
      iconSize: Style.font.display
      color: root.foreground
    }
  }

  Component {
    id: backButton
    PanelActionButton {
      iconText: root.glyph(0xF0141)
      tooltipText: root.level === "sketch" ? "Cancel" : root.level === "note" ? root.folderName : "Folders"
      foreground: root.foreground
      fontFamily: root.fontFamily
      onClicked: root.back()
    }
  }

  Component {
    id: newNoteButton
    PanelActionButton {
      iconText: root.glyph(0xF090C)
      tooltipText: "New note"
      foreground: root.foreground
      fontFamily: root.fontFamily
      onClicked: root.newNote(root.level === "list" ? root.query.trim() : "")
    }
  }

  Component {
    id: openInAppButton
    PanelActionButton {
      iconText: root.glyph(0xF03CC)
      tooltipText: "Open in Omanote"
      enabled: root.noteId !== ""
      foreground: root.foreground
      fontFamily: root.fontFamily
      onClicked: root.openInApp()
    }
  }

  Component {
    id: insertSketchButton
    PanelActionButton {
      iconText: root.glyph(0xF012C)
      tooltipText: "Insert drawing"
      enabled: !sketchProcess.running
      foreground: root.foreground
      fontFamily: root.fontFamily
      onClicked: root.insertSketch()
    }
  }

  Component { id: textBlockComponent; TextBlock {} }
  Component { id: imageBlockComponent; ImageBlock {} }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: root.level === "list" ? field
                 : root.level === "sketch" ? sketchArea
                 : root.level === "note" ? (root.textAreaAt(Math.max(0, root.focusBlock)) || keyCatcher)
                 : keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(column.implicitHeight, Style.space(640))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: field.activeFocus || root.textFocus || sketchArea.activeFocus
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onMoveRequested: function(dx, dy) {
        if (root.level !== "folders") return
        if (dy !== 0) root.moveFolderCursor(dy)
        else if (dx > 0 && root.folderCursor >= 0) root.openFolder(root.folderRows[root.folderCursor].id)
      }
      onActivateRequested: {
        if (root.level === "folders" && root.folderCursor >= 0)
          root.openFolder(root.folderRows[root.folderCursor].id)
      }
      onTextKey: function(t) { if (root.level === "folders" && t === "n") root.newNote("") }

      Column {
        id: column
        width: parent.width
        spacing: Style.space(12)

        PanelHero {
          width: parent.width
          title: root.level === "folders" ? "Omanote"
                 : root.level === "list" ? root.folderName
                 : root.level === "sketch" ? (root.sketchReplace !== "" ? "Edit drawing" : "Draw")
                 : (root.noteEncrypted ? "Encrypted note" : Model.titleOfText(root.noteText))
          meta: root.level === "folders" ? Model.count(root.folders.length, "folder", "folders")
                : root.level === "list" ? (root.searching ? Model.count(root.results.length, "match", "matches")
                                                          : Model.count(root.notes.length, "note", "notes"))
                : root.level === "sketch" ? "Enter inserts · Esc cancels"
                : root.noteMeta()
          foreground: root.foreground
          fontFamily: root.fontFamily
          iconComponent: root.level === "folders" ? pencilIcon : backButton
          trailingControl: root.level === "note" ? openInAppButton
                           : root.level === "sketch" ? insertSketchButton : newNoteButton
        }

        // ---------------------------------------------------- folders level

        ListView {
          id: folderList
          visible: root.level === "folders"
          width: parent.width
          height: Math.min(contentHeight, Style.space(460))
          spacing: Style.space(4)
          clip: true
          boundsBehavior: Flickable.StopAtBounds
          interactive: contentHeight > height
          ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
          model: root.folderRows
          currentIndex: root.folderCursor
          onCurrentIndexChanged: if (currentIndex >= 0) Qt.callLater(keepCurrentVisible)
          function keepCurrentVisible() {
            if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)
          }
          delegate: FolderRow {
            required property var modelData
            required property int index
            width: ListView.view.width
            folder: modelData
            rowIndex: index
          }
        }

        // ------------------------------------------------------ notes level

        TextField {
          id: field
          visible: root.level === "list"
          width: parent.width
          placeholderText: root.folderId === "" ? "Search, or type a new note…" : "Search " + root.folderName + ", or type a new note…"
          foreground: root.foreground
          font.family: root.fontFamily
          enabled: !captureProcess.running
          onTextChanged: root.query = text

          Keys.onPressed: function(event) {
            var ctrl = event.modifiers & Qt.ControlModifier
            if (event.key === Qt.Key_Escape) {
              if (text !== "") text = ""
              else root.close()
              event.accepted = true
            } else if (event.key === Qt.Key_Down) {
              root.moveCursor(1)
              event.accepted = true
            } else if (event.key === Qt.Key_Up) {
              root.moveCursor(-1)
              event.accepted = true
            } else if ((event.key === Qt.Key_Left && text === "") || (event.key === Qt.Key_Left && (event.modifiers & Qt.AltModifier))) {
              root.showFolders()
              event.accepted = true
            } else if (event.key === Qt.Key_Right && root.cursor >= 0) {
              root.activate()
              event.accepted = true
            } else if (ctrl && event.key === Qt.Key_N) {
              root.newNote(text.trim())
              event.accepted = true
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
              if (ctrl && root.cursor >= 0) {
                Quickshell.execDetached(["omanote", "--open", String(root.noteRows[root.cursor].note.id)])
                root.close()
              } else {
                root.activate()
              }
              event.accepted = true
            } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
              root.switchPanel(event.key === Qt.Key_Backtab ? -1 : 1)
              event.accepted = true
            }
          }
        }

        Text {
          textFormat: Text.PlainText
          width: parent.width
          visible: root.level === "list" && (root.status !== "" || root.searching)
          text: root.status !== "" ? root.status
                : (root.cursor >= 0 ? "Enter opens the note · Ctrl+Enter opens it in Omanote"
                                    : "Enter saves it as a new note · ↓ to pick a note")
          color: root.failed ? root.urgent : root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          wrapMode: Text.WordWrap
        }

        Text {
          visible: root.level === "list" && root.noteRows.length === 0
          width: parent.width
          text: root.searching ? "No matching notes." : "No notes here yet."
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          horizontalAlignment: Text.AlignHCenter
        }

        ListView {
          id: noteList
          visible: root.level === "list" && root.noteRows.length > 0
          width: parent.width
          height: Math.min(contentHeight, Style.space(440))
          spacing: Style.space(4)
          clip: true
          boundsBehavior: Flickable.StopAtBounds
          interactive: contentHeight > height
          ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
          model: root.noteRows
          currentIndex: root.cursor
          onCurrentIndexChanged: if (currentIndex >= 0) Qt.callLater(keepCurrentVisible)
          function keepCurrentVisible() {
            if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)
          }
          delegate: Column {
            id: noteDelegate
            required property var modelData
            required property int index
            width: ListView.view.width
            spacing: Style.space(4)

            Item {
              visible: noteDelegate.modelData.section !== "" && noteDelegate.index > 0
              width: 1
              height: Style.space(6)
            }

            PanelSectionHeader {
              visible: noteDelegate.modelData.section !== ""
              text: noteDelegate.modelData.section.toUpperCase()
              foreground: root.foreground
              fontFamily: root.fontFamily
            }

            NoteRow {
              width: parent.width
              note: noteDelegate.modelData.note
              rowIndex: noteDelegate.index
            }
          }
        }

        // ------------------------------------------------------- note level

        ScrollView {
          id: editorScroll
          visible: root.level === "note"
          width: parent.width
          height: Style.space(420)
          clip: true
          contentWidth: availableWidth
          ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
          background: BorderSurface {
            color: Style.controlFill(root.textFocus, false, root.foreground, Color.accent)
            borderSpec: Border.controlSpec(root.textFocus ? "focus" : "normal", root.foreground, Color.accent)
            radius: Style.cornerRadius
          }

          Column {
            id: blockColumn
            width: editorScroll.availableWidth
            spacing: 0

            Text {
              visible: root.noteLoading
              width: parent.width
              padding: Style.spacing.controlPaddingX
              text: "Loading…"
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
            }

            Repeater {
              id: blockRepeater
              model: root.noteLoading ? [] : root.blocks
              delegate: Loader {
                id: blockLoader
                required property var modelData
                required property int index
                width: blockColumn.width
                sourceComponent: modelData.kind === "image" ? imageBlockComponent : textBlockComponent
                onLoaded: {
                  item.block = Qt.binding(function() { return blockLoader.modelData })
                  item.blockIndex = Qt.binding(function() { return blockLoader.index })
                }
              }
            }

            // Clicks below the last block go to the end of the note.
            Item {
              width: parent.width
              height: Math.max(Style.space(24), editorScroll.availableHeight - y)
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.IBeamCursor
                onClicked: root.focusText(root.lastTextBlock(), true)
              }
            }
          }
        }

        RowLayout {
          visible: root.level === "note"
          width: parent.width
          spacing: Style.space(8)

          Text {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: root.status !== "" ? root.status
                  : (saveProcess.running || root.pendingSaves.length > 0) ? "Saving…"
                  : root.dirty ? "Editing…"
                  : "Saved as you type · Esc goes back"
            color: root.failed || root.confirmTrash ? root.urgent : root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            elide: Text.ElideRight
          }

          PanelActionButton {
            iconText: root.glyph(0xF0F49)
            tooltipText: "Draw (Ctrl+Shift+D)"
            enabled: !root.noteLoading && !root.noteEncrypted
            foreground: root.foreground
            fontFamily: root.fontFamily
            onClicked: root.openSketch(-1, null)
          }

          PanelActionButton {
            iconText: root.glyph(0xF0A7A)
            tooltipText: "Move to trash"
            enabled: !root.noteLoading && !root.noteEncrypted
            foreground: root.confirmTrash ? root.urgent : root.foreground
            hoverColor: root.urgent
            fontFamily: root.fontFamily
            onClicked: root.trashNote()
          }
        }

        // ----------------------------------------------------- sketch level

        Rectangle {
          id: sketchPaper
          visible: root.level === "sketch"
          width: parent.width
          height: Style.space(420)
          radius: Style.cornerRadius
          color: Color.background
          border.width: 1
          border.color: Qt.darker(root.foreground, 2.2)
          clip: true

          Canvas {
            id: sketchCanvas
            anchors.fill: parent
            onPaint: {
              var ctx = getContext("2d")
              ctx.reset()
              for (var i = 0; i < root.strokes.length; i++) root.paintStroke(ctx, root.strokes[i])
              if (root.liveStroke) root.paintStroke(ctx, root.liveStroke)
            }
          }

          Item {
            id: sketchArea
            anchors.fill: parent
            focus: root.level === "sketch"

            Keys.onPressed: function(event) {
              var ctrl = event.modifiers & Qt.ControlModifier
              if (event.key === Qt.Key_Escape) root.closeSketch()
              else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.insertSketch()
              else if (ctrl && event.key === Qt.Key_Z) root.undoStroke()
              else if (event.key === Qt.Key_P) root.chooseTool("pen")
              else if (event.key === Qt.Key_H) root.chooseTool("marker")
              else if (event.key === Qt.Key_E) root.chooseTool("erase")
              else if (event.key >= Qt.Key_1 && event.key <= Qt.Key_3) root.inkSize = event.key - Qt.Key_1
              else return
              event.accepted = true
            }

            MouseArea {
              anchors.fill: parent
              cursorShape: Qt.CrossCursor
              onPressed: function(mouse) {
                sketchArea.forceActiveFocus()
                var kind = root.inkTool
                root.liveStroke = {
                  kind: kind,
                  color: kind === "erase" ? "" : root.inkColor,
                  width: root.inkWidths[kind][root.inkSize],
                  pen: false,
                  pts: [[mouse.x, mouse.y, 0.5]]
                }
                sketchCanvas.requestPaint()
              }
              onPositionChanged: function(mouse) {
                if (!root.liveStroke) return
                var pts = root.liveStroke.pts
                var last = pts[pts.length - 1]
                if (Math.abs(last[0] - mouse.x) + Math.abs(last[1] - mouse.y) < 1) return
                pts.push([mouse.x, mouse.y, 0.5])
                sketchCanvas.requestPaint()
              }
              onReleased: {
                if (!root.liveStroke) return
                root.strokes = root.strokes.concat([root.liveStroke])
                root.liveStroke = null
                sketchCanvas.requestPaint()
              }
            }
          }

          Text {
            anchors.centerIn: parent
            visible: root.strokes.length === 0 && !root.liveStroke
            text: "Draw here"
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }
        }

        // A short toolbar: pen, marker, eraser · four colours · size · undo, clear.
        RowLayout {
          visible: root.level === "sketch"
          width: parent.width
          spacing: Style.space(4)

          ToolButton { glyphCode: 0xF03EA; tip: "Pen (P)"; active: root.inkTool === "pen"; onActivated: root.chooseTool("pen") }
          ToolButton { glyphCode: 0xF0652; tip: "Marker (H)"; active: root.inkTool === "marker"; onActivated: root.chooseTool("marker") }
          ToolButton { glyphCode: 0xF01FE; tip: "Eraser (E)"; active: root.inkTool === "erase"; onActivated: root.chooseTool("erase") }

          Item { Layout.preferredWidth: Style.space(6); Layout.preferredHeight: 1 }

          Repeater {
            model: root.inkColors
            delegate: Rectangle {
              required property var modelData
              Layout.preferredWidth: Style.space(20)
              Layout.preferredHeight: Style.space(20)
              radius: width / 2
              color: modelData
              border.width: root.inkColor === modelData && root.inkTool !== "erase" ? 2 : 1
              border.color: root.inkColor === modelData && root.inkTool !== "erase" ? root.foreground : Qt.darker(root.foreground, 2)
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                  root.inkColor = parent.modelData
                  if (root.inkTool === "erase") root.inkTool = "pen"
                }
              }
            }
          }

          Item { Layout.preferredWidth: Style.space(6); Layout.preferredHeight: 1 }

          // Size: click cycles small / medium / large (keys 1-3).
          PanelActionButton {
            tooltipText: "Size (1-3)"
            iconText: ["S", "M", "L"][root.inkSize]
            fontSize: Style.font.caption
            foreground: root.foreground
            fontFamily: root.fontFamily
            onClicked: root.inkSize = (root.inkSize + 1) % 3
          }

          Item { Layout.fillWidth: true; Layout.preferredHeight: 1 }

          PanelActionButton {
            iconText: root.glyph(0xF054C)
            tooltipText: "Undo (Ctrl+Z)"
            enabled: root.strokes.length > 0
            foreground: root.foreground
            fontFamily: root.fontFamily
            onClicked: root.undoStroke()
          }

          PanelActionButton {
            iconText: root.glyph(0xF0156)
            tooltipText: "Clear"
            enabled: root.strokes.length > 0
            foreground: root.foreground
            hoverColor: root.urgent
            fontFamily: root.fontFamily
            onClicked: { root.strokes = []; sketchCanvas.requestPaint() }
          }
        }

        Text {
          textFormat: Text.PlainText
          visible: root.level === "sketch" && root.status !== ""
          width: parent.width
          text: root.status
          color: root.failed ? root.urgent : root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          wrapMode: Text.WordWrap
        }
      }
    }
  }

  function noteMeta() {
    if (noteLoading) return "Loading…"
    var when = noteUpdated > 0 ? "Edited " + Model.rowDate(noteUpdated) : "New note"
    var where = noteParent !== "" ? Model.folderTitle(folders, noteParent) : (folderId === "" ? "" : folderName)
    return where !== "" ? when + " · " + where : when
  }

  // ----------------------------------------------------------- note blocks

  // A run of text lines, edited in place, with the calculator's results drawn
  // after their lines.
  component TextBlock: Item {
    id: tb
    property var block: null
    property int blockIndex: -1
    property bool ready: false
    property alias area: area
    readonly property var calc: root.calcBlocks[blockIndex] || []
    // The text layout settles after the first frame without notifying anything that
    // positionToRectangle depends on: re-read the result positions a moment later.
    property int relayout: 0
    Timer { id: relayoutTimer; interval: 30; onTriggered: tb.relayout += 1 }
    onWidthChanged: relayoutTimer.restart()
    Component.onCompleted: relayoutTimer.restart()

    implicitHeight: area.implicitHeight

    onBlockChanged: {
      if (block && !ready) {
        area.text = block.text
        ready = true
      }
    }

    TextArea {
      id: area
      width: parent.width
      readOnly: root.noteEncrypted || root.noteLoading
      wrapMode: TextEdit.Wrap
      textFormat: TextEdit.PlainText
      font.family: root.fontFamily
      font.pixelSize: Style.font.body
      color: root.foreground
      selectionColor: Style.selectionFillFor(root.foreground, Color.accent)
      selectedTextColor: root.foreground
      placeholderText: tb.blockIndex === 0 && root.blocks.length === 1
                       ? (root.noteEncrypted ? "This note is encrypted: open it in Omanote." : "Write here. The first line is the title.")
                       : ""
      placeholderTextColor: root.dim
      leftPadding: Style.spacing.controlPaddingX
      rightPadding: Style.spacing.controlPaddingX
      topPadding: tb.blockIndex === 0 ? Style.spacing.controlPaddingX : Style.space(2)
      bottomPadding: Style.space(2)
      background: null

      onTextChanged: {
        if (tb.ready) root.blockEdited(tb.blockIndex, text)
        relayoutTimer.restart()
      }
      onActiveFocusChanged: {
        if (activeFocus) {
          root.focusBlock = tb.blockIndex
          root.focusPos = cursorPosition
        }
        root.textFocus = activeFocus || root.textFocusElsewhere(tb.blockIndex)
      }
      onCursorPositionChanged: if (activeFocus) root.focusPos = cursorPosition
      onCursorRectangleChanged: if (activeFocus) root.ensureVisible(area)

      Keys.onPressed: function(event) {
        var ctrl = event.modifiers & Qt.ControlModifier
        var shift = event.modifiers & Qt.ShiftModifier
        if (event.key === Qt.Key_Escape || (event.key === Qt.Key_Left && (event.modifiers & Qt.AltModifier))) {
          root.back()
        } else if (ctrl && event.key === Qt.Key_S) {
          autosave.stop()
          root.saveNow()
        } else if (ctrl && event.key === Qt.Key_O) {
          root.openInApp()
        } else if (ctrl && event.key === Qt.Key_N) {
          root.newNote("")
        } else if (ctrl && shift && event.key === Qt.Key_D) {
          root.openSketch(-1, null)
        } else if (event.key === Qt.Key_Up && !shift && area.cursorRectangle.y <= area.topPadding + 1) {
          // From the first line to the text block above (images are skipped).
          for (var i = tb.blockIndex - 1; i >= 0; i--) if (root.blocks[i].kind === "text" && root.focusText(i, true)) break
          if (i < 0) return
        } else if (event.key === Qt.Key_Down && !shift
                   && area.cursorRectangle.y + area.cursorRectangle.height >= area.height - area.bottomPadding - 1) {
          for (var j = tb.blockIndex + 1; j < root.blocks.length; j++) if (root.blocks[j].kind === "text" && root.focusText(j, false)) break
          if (j >= root.blocks.length) return
        } else {
          return
        }
        event.accepted = true
      }

      // Results after "=" (or at the end of the line in "math" notes), as in the app.
      Repeater {
        model: tb.calc
        delegate: Text {
          required property var modelData
          readonly property rect at: {
            tb.relayout
            area.text
            area.width
            return area.positionToRectangle(Math.min(modelData.pos, area.length))
          }
          textFormat: Text.PlainText
          x: at.x + Style.space(modelData.summary ? 10 : 4)
          y: at.y
          width: Math.max(0, area.width - x - area.rightPadding)
          text: modelData.label
          color: root.resultColor
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          font.bold: true
          elide: Text.ElideRight
        }
      }
    }
  }

  // True when another text block has the focus (focus moving between blocks).
  function textFocusElsewhere(index) {
    for (var i = 0; i < blocks.length; i++) {
      if (i === index || blocks[i].kind !== "text") continue
      var area = textAreaAt(i)
      if (area && area.activeFocus) return true
    }
    return false
  }

  // A line of attachment images (usually one), one under the other.
  component ImageBlock: Column {
    id: imageLine
    property var block: null
    property int blockIndex: -1
    Repeater {
      model: imageLine.block ? imageLine.block.images : []
      delegate: ImageItem {
        required property var modelData
        required property int index
        width: imageLine.width
        picture: modelData
        blockIndex: imageLine.blockIndex
        imageIndex: index
      }
    }
  }

  // An attachment image or sketch, shown at its size (sketches: the width they were
  // drawn at). Clicking a sketch with strokes opens it on the canvas.
  component ImageItem: Item {
    id: ib
    property var picture: null
    property int blockIndex: -1
    property int imageIndex: 0
    property string path: ""
    property var sketchData: null
    property string problem: ""
    readonly property real maxWidth: width - Style.spacing.controlPaddingX * 2
    readonly property real naturalWidth: image.implicitWidth > 0
      ? (picture && picture.width > 0 ? picture.width : (picture && picture.sketch ? image.implicitWidth / 2 : image.implicitWidth))
      : 0
    readonly property real shownWidth: Math.min(maxWidth, naturalWidth)
    readonly property real shownHeight: image.implicitWidth > 0
      ? Math.min(Style.space(320), shownWidth * image.implicitHeight / image.implicitWidth)
      : Style.space(24)
    readonly property bool editable: !!(picture && picture.sketch && sketchData && sketchData.strokes)

    implicitHeight: shownHeight + Style.space(8)

    function load() {
      if (!picture || !picture.id) return
      if (resourceProcess.running) return
      resourceProcess.command = ["omanote-cli", "resource", "--json", picture.id]
      resourceProcess.running = true
    }

    onPictureChanged: load()
    Connections {
      target: root
      function onImageVersionChanged() { ib.path = ""; ib.load() }
    }

    Process {
      id: resourceProcess
      stdout: StdioCollector { id: resourceOut; waitForEnd: true }
      stderr: StdioCollector { id: resourceErr; waitForEnd: true }
      onExited: function(exitCode) {
        var out = exitCode === 0 ? Model.parseObject(resourceOut.text) : null
        if (out && out.path) {
          ib.problem = ""
          ib.sketchData = out.sketch || null
          ib.path = String(out.path)
        } else {
          ib.problem = String(resourceErr.text || "").trim() || "Image not available"
        }
      }
    }

    Image {
      id: image
      x: Style.spacing.controlPaddingX
      y: Style.space(4)
      width: ib.shownWidth
      height: ib.shownHeight
      source: ib.path !== "" ? "file://" + ib.path : ""
      cache: false
      asynchronous: true
      fillMode: Image.PreserveAspectFit
      horizontalAlignment: Image.AlignLeft
      sourceSize.width: Math.round(ib.maxWidth * 2)
      smooth: true
      mipmap: true
    }

    Text {
      x: Style.spacing.controlPaddingX
      y: Style.space(4)
      visible: image.status !== Image.Ready
      text: ib.problem !== "" ? ib.problem : "Loading image…"
      color: ib.problem !== "" ? root.urgent : root.dim
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }

    MouseArea {
      id: imageMouse
      x: image.x
      y: image.y
      width: Math.max(image.width, Style.space(60))
      height: image.height
      hoverEnabled: true
      cursorShape: ib.editable ? Qt.PointingHandCursor : Qt.ArrowCursor
      onClicked: if (ib.editable) root.openSketch(ib.blockIndex, ib.sketchData, ib.imageIndex)
    }

    Rectangle {
      anchors.fill: imageButtons
      anchors.margins: -Style.space(3)
      visible: imageButtons.visible
      radius: Style.cornerRadius
      color: Color.background
      opacity: 0.85
    }

    Row {
      id: imageButtons
      anchors.top: image.top
      anchors.topMargin: Style.space(4)
      anchors.right: parent.right
      anchors.rightMargin: Style.spacing.controlPaddingX + Style.space(4)
      spacing: Style.space(2)
      visible: !root.noteEncrypted && (imageMouse.containsMouse || editButton.hot || removeButton.hot)

      PanelActionButton {
        id: editButton
        property bool hot: false
        visible: ib.editable
        iconText: root.glyph(0xF03EB)
        tooltipText: "Edit drawing"
        foreground: root.foreground
        fontFamily: root.fontFamily
        bordered: true
        onHovered: function(h) { editButton.hot = h }
        onClicked: root.openSketch(ib.blockIndex, ib.sketchData, ib.imageIndex)
      }

      PanelActionButton {
        id: removeButton
        property bool hot: false
        iconText: root.glyph(0xF0156)
        tooltipText: "Remove from the note"
        foreground: root.foreground
        hoverColor: root.urgent
        fontFamily: root.fontFamily
        bordered: true
        onHovered: function(h) { removeButton.hot = h }
        onClicked: root.removeImage(ib.blockIndex, ib.imageIndex)
      }
    }
  }

  component ToolButton: PanelActionButton {
    property int glyphCode: 0
    property string tip: ""
    property bool active: false
    signal activated()
    iconText: root.glyph(glyphCode)
    tooltipText: tip
    bordered: active
    foreground: active ? Color.accent : root.foreground
    fontFamily: root.fontFamily
    onClicked: activated()
  }

  // ------------------------------------------------------------------- rows

  component FolderRow: CursorSurface {
    id: frow
    property var folder: null
    property int rowIndex: 0

    hasCursor: root.folderCursor === rowIndex
    foreground: root.foreground
    implicitHeight: folderLabel.implicitHeight + Style.spacing.rowPaddingX

    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onEntered: root.folderCursor = frow.rowIndex
      onClicked: root.openFolder(frow.folder ? frow.folder.id : "")
    }

    RowLayout {
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.space(10)
      anchors.rightMargin: Style.space(10)
      spacing: Style.space(8)

      Item {
        visible: frow.folder && frow.folder.depth > 0
        Layout.preferredWidth: frow.folder ? frow.folder.depth * Style.space(14) : 0
        Layout.preferredHeight: 1
      }

      Text {
        textFormat: Text.PlainText
        text: root.glyph(frow.folder && frow.folder.all ? 0xF06B9 : 0xF0256)
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
      }

      Text {
        id: folderLabel
        textFormat: Text.PlainText
        Layout.fillWidth: true
        text: frow.folder ? (frow.folder.title || "Untitled") : ""
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        font.bold: frow.folder && frow.folder.id === root.folderId
        elide: Text.ElideRight
      }

      Text {
        textFormat: Text.PlainText
        text: frow.folder ? String(frow.folder.note_count || 0) : ""
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      Text {
        textFormat: Text.PlainText
        text: root.glyph(0xF0142)
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
      }
    }
  }

  component NoteRow: CursorSurface {
    id: row
    property var note: null
    property int rowIndex: 0

    hasCursor: root.cursor === rowIndex
    foreground: root.foreground
    implicitHeight: rowContent.implicitHeight + Style.spacing.rowPaddingX

    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onEntered: root.cursor = row.rowIndex
      onClicked: root.openNote(row.note)
    }

    RowLayout {
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.space(10)
      anchors.rightMargin: Style.space(10)
      spacing: Style.space(8)

      ColumnLayout {
        id: rowContent
        Layout.fillWidth: true
        spacing: Style.space(1)

        Text {
          textFormat: Text.PlainText
          Layout.fillWidth: true
          text: Model.title(row.note)
          color: row.note && row.note.is_conflict ? root.urgent : root.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          elide: Text.ElideRight
        }

        Text {
          textFormat: Text.PlainText
          Layout.fillWidth: true
          visible: root.showPreview && text !== ""
          text: Model.preview(row.note)
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
        }
      }

      Text {
        textFormat: Text.PlainText
        text: row.note ? Model.rowDate(row.note.updated_time) : ""
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        Layout.alignment: Qt.AlignVCenter
      }
    }
  }
}
