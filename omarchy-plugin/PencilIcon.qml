import QtQuick
import qs.Commons
import qs.Ui

// The Omanote mark: the pencil glyph of the Nerd Font the shell draws its own bar
// icons with, shown through the same OpticalGlyph component BarIconButton uses,
// so it lands at the same optical size and colour as the rest of the bar.
//
// Same mark as the app's interface (app/src/lib/icons.ts: "pencil", U+F03EB in
// app/src/lib/symbols.generated.ts), so nothing is drawn twice. Style.font.family
// is the fontconfig alias `omarchy-font-set` writes, and every font Omarchy
// installs is a Nerd Font, so the glyph is always there.
Item {
  id: root

  property real iconSize: Style.bar.iconCanvas
  property color color: Color.foreground

  // Material Design Icons "pencil". Built from the code point because QML string
  // escapes hold 4 hex digits and this glyph is above U+FFFF.
  readonly property string glyph: String.fromCodePoint(0xF03EB)

  // The bar keeps its glyphs at Style.bar.iconFont inside the iconCanvas slot;
  // the panel asks for a larger icon, so keep that ratio.
  readonly property real glyphSize:
    Math.round(iconSize * Style.bar.iconFont / Style.bar.iconCanvas)

  width: iconSize
  height: iconSize
  implicitWidth: iconSize
  implicitHeight: iconSize

  OpticalGlyph {
    anchors.fill: parent
    text: root.glyph
    fontFamily: Style.font.family
    fontSize: root.glyphSize
    color: root.color
  }
}
