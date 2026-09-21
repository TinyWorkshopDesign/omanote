import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Omanote in the Omarchy bar. The bar icon opens a panel with a capture field
// and the most recent notes:
//   - type and press Enter to save a new note (omanote-cli new);
//   - typing also filters the list; ↑/↓ pick a note, Enter opens it in the app
//     (omanote --open <id>).
// Right-click shows/hides the app window, middle-click opens a new note in it.
//
// All note data goes through omanote-cli, which shares the app's database, so
// the app sees new notes at once and syncs them with Joplin Server. Nothing
// runs in the background: the list is read only when the panel opens.
Panel {
  id: root
  moduleName: "tinyworkshop.omanote"
  ipcTarget: "tinyworkshop.omanote"

  property var notes: []
  property string query: ""
  property int cursor: -1
  property string status: ""
  property bool failed: false

  readonly property int limit: Math.max(3, Number(setting("recentNotes", 8)) || 8)
  readonly property var shown: Model.filter(notes, query, limit)
  // Off keeps note contents out of sight on a shared screen: titles only.
  readonly property bool showPreview: setting("showPreview", true) !== false
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  function refresh() {
    if (listProcess.running) return
    listProcess.running = true
  }

  function openNote(note) {
    if (!note) return
    Quickshell.execDetached(["omanote", "--open", String(note.id)])
    root.close()
  }

  function capture() {
    var text = query.trim()
    if (text === "" || createProcess.running) return
    createProcess.command = ["omanote-cli", "new", text]
    createProcess.running = true
  }

  function activate() {
    if (cursor >= 0 && cursor < shown.length) openNote(shown[cursor])
    else capture()
  }

  function moveCursor(dy) {
    if (shown.length === 0) { cursor = -1; return }
    cursor = Math.max(-1, Math.min(shown.length - 1, cursor + dy))
  }

  onQueryChanged: {
    cursor = -1
    status = ""
    failed = false
  }
  onOpenedChanged: if (opened) {
    status = ""
    failed = false
    cursor = -1
    refresh()
    Qt.callLater(function() { field.forceActiveFocus() })
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Process {
    id: listProcess
    command: ["omanote-cli", "list", "--json", "--limit", "200"]
    stdout: StdioCollector { id: listOut; waitForEnd: true }
    stderr: StdioCollector { id: listErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode === 0) {
        root.notes = Model.parseNotes(listOut.text)
        root.failed = false
      } else {
        root.failed = true
        root.status = exitCode === 127 ? "omanote-cli is not installed" : (listErr.text.trim() || "Could not read notes")
      }
    }
  }

  Process {
    id: createProcess
    command: []
    stdout: StdioCollector { waitForEnd: true }
    stderr: StdioCollector { id: createErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode === 0) {
        field.text = ""
        root.status = "Saved"
        root.failed = false
        root.refresh()
      } else {
        root.failed = true
        root.status = exitCode === 127 ? "omanote-cli is not installed" : (createErr.text.trim() || "Could not save the note")
      }
    }
  }

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

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: field
    contentWidth: panel.fittedContentWidth(Style.space(360))
    contentHeight: panel.fittedContentHeight(column.implicitHeight, Style.space(520))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Column {
        id: column
        width: parent.width
        spacing: Style.space(12)

        PanelHero {
          width: parent.width
          title: "Omanote"
          meta: root.notes.length === 1 ? "1 note" : root.notes.length + " notes"
          foreground: root.foreground
          fontFamily: root.fontFamily
          iconComponent: Component {
            PencilIcon {
              iconSize: Style.font.display
              color: root.foreground
            }
          }
          trailingControl: Component {
            PanelActionButton {
              iconText: "+"
              foreground: root.foreground
              fontFamily: root.fontFamily
              onClicked: {
                Quickshell.execDetached(["omanote", "--new"])
                root.close()
              }
            }
          }
        }

        TextField {
          id: field
          width: parent.width
          placeholderText: "New note, or search…"
          foreground: root.foreground
          font.family: root.fontFamily
          enabled: !createProcess.running
          onTextChanged: root.query = text

          Keys.onPressed: function(event) {
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
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
              root.activate()
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
          visible: root.status !== "" || root.query.trim() !== ""
          text: root.status !== "" ? root.status
                : (root.cursor >= 0 ? "Enter opens the note" : "Enter saves it as a new note · ↓ to pick a note")
          color: root.failed ? root.urgent : root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          wrapMode: Text.WordWrap
        }

        PanelSeparator { foreground: root.foreground }

        PanelSectionHeader {
          text: root.query.trim() === "" ? "RECENT NOTES" : "MATCHING NOTES"
          foreground: root.foreground
          fontFamily: root.fontFamily
        }

        Text {
          visible: root.shown.length === 0
          width: parent.width
          text: root.query.trim() === "" ? "No notes yet." : "No matching notes."
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          horizontalAlignment: Text.AlignHCenter
        }

        Column {
          id: noteColumn
          visible: root.shown.length > 0
          width: parent.width
          spacing: Style.space(4)

          Repeater {
            model: root.shown
            NoteRow {
              required property var modelData
              required property int index
              width: noteColumn.width
              note: modelData
              rowIndex: index
            }
          }
        }
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
        text: row.note ? Model.ago(row.note.updated_time) : ""
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        Layout.alignment: Qt.AlignVCenter
      }
    }
  }
}
