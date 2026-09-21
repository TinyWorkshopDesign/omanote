.pragma library

// Pure helpers for the Omanote panel: parsing `omanote-cli list --json`,
// filtering and labels. No QML types here, so they stay easy to test.

function parseNotes(text) {
  try {
    var parsed = JSON.parse(String(text || "[]"))
    return parsed instanceof Array ? parsed : []
  } catch (e) {
    return []
  }
}

var ENTITIES = { "&nbsp;": " ", "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": "\"", "&#39;": "'" }

// Notes are Markdown with Joplin rich-text HTML inside: show plain text.
function plain(text) {
  return String(text || "")
    .replace(/<[^>]+>/g, "")
    .replace(/&[a-z#0-9]+;/gi, function(m) { return ENTITIES[m.toLowerCase()] || " " })
    // Images, including a reference cut off at the end of a preview.
    .replace(/!\[[^\]\n]*(\]\([^)\n]*\)?)?/g, "")
    .replace(/^\s*(#{1,6}\s+|[-*+]\s+(\[[ xX]\]\s+)?|\d+[.)]\s+|>\s*)/, "")
    .replace(/\s+/g, " ")
    .trim()
}

function title(note) {
  if (!note) return ""
  if (note.encrypted) return "Encrypted note"
  return plain(note.title) || "Untitled"
}

function preview(note) {
  if (!note || note.encrypted) return ""
  return plain(note.preview)
}

function matches(note, query) {
  var q = String(query || "").trim().toLowerCase()
  if (q === "") return true
  if (!note || note.encrypted) return false
  return (String(note.title || "") + "\n" + String(note.preview || "")).toLowerCase().indexOf(q) !== -1
}

function filter(notes, query, limit) {
  var out = []
  for (var i = 0; i < notes.length && out.length < limit; i++) {
    if (matches(notes[i], query)) out.push(notes[i])
  }
  return out
}

function ago(ms, now) {
  var s = Math.max(0, Math.round(((now || Date.now()) - Number(ms || 0)) / 1000))
  if (s < 60) return "now"
  if (s < 3600) return Math.floor(s / 60) + " min"
  if (s < 86400) return Math.floor(s / 3600) + " h"
  if (s < 86400 * 30) return Math.floor(s / 86400) + " d"
  return new Date(Number(ms)).toISOString().slice(0, 10)
}
