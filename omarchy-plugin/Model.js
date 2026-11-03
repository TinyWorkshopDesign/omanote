.pragma library
.import "Calc.js" as CalcJs

// Pure helpers for the Omanote panel: parsing `omanote-cli ... --json`, date
// sections, labels, the folder list, the note split around its images and the
// calculator results. No QML types here, so they stay easy to test.

function parseList(text) {
  try {
    var parsed = JSON.parse(String(text || "[]"))
    return parsed instanceof Array ? parsed : []
  } catch (e) {
    return []
  }
}

function parseObject(text) {
  try {
    var parsed = JSON.parse(String(text || "null"))
    return parsed && typeof parsed === "object" && !(parsed instanceof Array) ? parsed : null
  } catch (e) {
    return null
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

// The title of a note being edited: its first line with text, as the app does.
function titleOfText(text) {
  var lines = String(text || "").split("\n")
  for (var i = 0; i < lines.length; i++) {
    var line = plain(lines[i])
    if (line !== "") return line
  }
  return "New note"
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

// ------------------------------------------------------------------ dates

var DAY = 86400000
var MONTHS = ["January", "February", "March", "April", "May", "June", "July",
              "August", "September", "October", "November", "December"]
var WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]

function startOfDay(ms) {
  var d = new Date(ms)
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
}

function pad(n) { return n < 10 ? "0" + n : String(n) }

// The list section a note edited at `ms` falls in, as in the app's note list:
// Today, Yesterday, Previous 7 Days, Previous 30 Days, then months.
function sectionOf(ms, now) {
  now = now || Date.now()
  ms = Number(ms || 0)
  var today = startOfDay(now)
  if (ms >= today) return "Today"
  if (ms >= today - DAY) return "Yesterday"
  if (ms >= today - 7 * DAY) return "Previous 7 Days"
  if (ms >= today - 30 * DAY) return "Previous 30 Days"
  var d = new Date(ms)
  var sameYear = d.getFullYear() === new Date(now).getFullYear()
  return MONTHS[d.getMonth()] + (sameYear ? "" : " " + d.getFullYear())
}

// Row date: the time today, the weekday this week, otherwise the date.
function rowDate(ms, now) {
  now = now || Date.now()
  ms = Number(ms || 0)
  var d = new Date(ms)
  var today = startOfDay(now)
  if (ms >= today) return pad(d.getHours()) + ":" + pad(d.getMinutes())
  if (ms >= today - 6 * DAY) return WEEKDAYS[d.getDay()]
  var sameYear = d.getFullYear() === new Date(now).getFullYear()
  return d.getDate() + " " + MONTHS[d.getMonth()].slice(0, 3) + (sameYear ? "" : " " + String(d.getFullYear()).slice(2))
}

function fullDate(ms) {
  var d = new Date(Number(ms || 0))
  return d.getDate() + " " + MONTHS[d.getMonth()] + " " + d.getFullYear() + ", " + pad(d.getHours()) + ":" + pad(d.getMinutes())
}

// Rows for the note list: each note with the section header it opens, if any.
function noteRows(notes, now) {
  var rows = []
  var last = ""
  for (var i = 0; i < notes.length; i++) {
    var section = sectionOf(notes[i].updated_time, now)
    rows.push({ note: notes[i], section: section === last ? "" : section })
    last = section
  }
  return rows
}

// ---------------------------------------------------------------- folders

// The folders level: All Notes first, then the folder tree as the CLI prints it
// (depth-first, with depth for the indent).
function folderRows(folders) {
  var total = 0
  for (var i = 0; i < folders.length; i++) total += Number(folders[i].note_count || 0)
  var rows = [{ id: "", title: "All Notes", depth: 0, note_count: total, all: true }]
  for (var j = 0; j < folders.length; j++) rows.push(folders[j])
  return rows
}

function folderTitle(folders, id) {
  if (!id) return "All Notes"
  for (var i = 0; i < folders.length; i++) if (folders[i].id === id) return folders[i].title || "Untitled"
  return "Notes"
}

function count(n, one, many) {
  return n === 1 ? "1 " + one : n + " " + many
}

// ---------------------------------------------------------------- the CLI

// omanote-cli joins the text arguments, reads stdin when the text is "-" and
// takes "--json", "--folder", "-h" and "--help" as options wherever they are: never pass those
// alone as a note's text.
function cliText(text) {
  var t = String(text || "")
  return (t === "-" || t === "--json" || t === "--folder" || t === "-h" || t === "--help") ? t + "\n" : t
}

// ------------------------------------------------------------ note blocks

// A line holding only attachment images (one or more, as the app allows):
// Markdown `![alt](:/id)` or Joplin's `<img src=":/id" ...>` (sketches: alt
// "sketch-….png", width in CSS pixels). Anything else on the line keeps it text.
// ("[/]", not "\/": QML's engine doubles the backslash in RegExp.source.)
var IMAGE_TOKEN = /!\[([^\]\n]*)\]\(:[/]([0-9a-f]{32})\)|<img\s[^>]*?src=["']:[/]([0-9a-f]{32})["'][^>]*>/g

function attr(tag, name) {
  var m = new RegExp("\\b" + name + "=[\"']?([^\"'\\s>]*)", "i").exec(tag)
  return m ? m[1] : ""
}

function imageOf(line) {
  var images = []
  var rest = ""
  var last = 0
  var re = new RegExp(IMAGE_TOKEN.source, "g")
  var m
  while ((m = re.exec(line)) !== null) {
    rest += line.slice(last, m.index)
    last = m.index + m[0].length
    var alt = m[2] ? m[1] : attr(m[0], "alt")
    images.push({
      id: m[2] || m[3],
      alt: alt,
      width: Number(m[3] ? attr(m[0], "width") : 0) || 0,
      sketch: /^sketch-/.test(alt),
      start: m.index,
      end: last
    })
  }
  rest += line.slice(last)
  // &nbsp; is how Joplin's rich-text editor pads a line of images.
  if (images.length === 0 || rest.replace(/&nbsp;/g, "").trim() !== "") return null
  return { kind: "image", line: line, images: images }
}

// The note as text blocks and image blocks; joinBlocks(splitBlocks(t)) === t.
// Empty "virtual" text blocks sit after a final image and between two images, so
// there is always somewhere to type; they add nothing to the text while empty.
// Line 1 is the title: never an image.
function splitBlocks(text) {
  var lines = String(text || "").split("\n")
  var blocks = []
  var current = null
  for (var i = 0; i < lines.length; i++) {
    var image = i > 0 ? imageOf(lines[i]) : null
    if (image) {
      if (!current) blocks.push({ kind: "text", text: "", virtual: true })
      blocks.push(image)
      current = null
    } else if (current) {
      current.text += "\n" + lines[i]
    } else {
      current = { kind: "text", text: lines[i], virtual: false }
      blocks.push(current)
    }
  }
  if (blocks.length === 0 || blocks[blocks.length - 1].kind === "image") blocks.push({ kind: "text", text: "", virtual: true })
  return blocks
}

// True when the block adds lines to the note.
function counts(block) {
  return block.kind === "image" || !(block.virtual && block.text === "")
}

function joinBlocks(blocks) {
  var parts = []
  for (var i = 0; i < blocks.length; i++) {
    if (!counts(blocks[i])) continue
    parts.push(blocks[i].kind === "image" ? blocks[i].line : blocks[i].text)
  }
  return parts.join("\n")
}

// The note with `tag` on its own line after the line holding `pos` in text block
// `index` (as the app inserts a sketch). Returns the new text.
function insertImageLine(blocks, index, pos, tag) {
  var out = blocks.map(function(b) { return b.kind === "text" ? { kind: "text", text: b.text, virtual: b.virtual } : b })
  var block = out[index]
  if (!block || block.kind !== "text") {
    block = out[out.length - 1]
    index = out.length - 1
    pos = block.text.length
  }
  var text = block.text
  var end = text.indexOf("\n", Math.max(0, Math.min(pos, text.length)))
  if (end < 0) end = text.length
  var before = text.slice(0, end)
  var after = end < text.length ? text.slice(end + 1) : null
  // A drawing never goes on line 1: an empty note gets a title first.
  var first = true
  for (var i = 0; i < index; i++) if (counts(out[i])) first = false
  if (first && before.trim() === "") before = "Sketch"
  var replacement = [{ kind: "text", text: before, virtual: block.virtual && before === "" }, imageOf(tag) || { kind: "image", line: tag }]
  if (after !== null) replacement.push({ kind: "text", text: after, virtual: false })
  out.splice.apply(out, [index, 1].concat(replacement))
  return joinBlocks(out)
}

// The note without image `image` of block `index` (the whole line goes when it
// held only that image).
function removeImage(blocks, index, image) {
  var out = []
  for (var i = 0; i < blocks.length; i++) {
    var b = blocks[i]
    if (i !== index || b.kind !== "image") { out.push(b); continue }
    var img = b.images[image]
    var line = img ? b.line.slice(0, img.start) + b.line.slice(img.end) : b.line
    if (line.replace(/&nbsp;/g, "").trim() !== "") out.push({ kind: "image", line: line, images: [] })
  }
  return joinBlocks(out)
}

// The note with the width of image `image` of block `index` set (an edited sketch).
function setImageWidth(blocks, index, image, width) {
  return joinBlocks(blocks.map(function(b, i) {
    if (i !== index || b.kind !== "image" || !b.images[image]) return b
    var img = b.images[image]
    var tag = withWidth(b.line.slice(img.start, img.end), width)
    return { kind: "image", line: b.line.slice(0, img.start) + tag + b.line.slice(img.end), images: [] }
  }))
}

// Sets the width attribute of an <img> line (an edited sketch).
function withWidth(line, width) {
  if (/\bwidth=/.test(line)) return line.replace(/\bwidth=["']?\d+["']?/, "width=\"" + width + "\"")
  return line.replace(/\s*\/?>\s*$/, " width=\"" + width + "\">")
}

// ----------------------------------------------------------- calculator

function trimEnd(s) { return String(s).replace(/\s+$/, "") }

function modeSummary(mode, text) {
  var C = CalcJs.Calc
  if (mode === "sum" || mode === "avg") {
    var nums = C.numbersIn(text)
    if (!nums.length) return ""
    var total = 0
    for (var i = 0; i < nums.length; i++) total += nums[i]
    return C.formatResult({ value: mode === "sum" ? total : total / nums.length, unit: "", pct: false, show: true })
  }
  if (mode === "count") {
    var c = C.countIn(text)
    return c.items + " items · " + c.lines + " lines · " + c.words + " words · " + c.chars + " chars"
  }
  return ""
}

// The results the app shows, per line of the note: { pos, label } or null. `pos`
// is where the result sits in the line: right after a trailing "=", else at the
// end ("= 42"). Line 1 of a "sum"/"avg"/"count" note holds the summary.
function lineResults(text, fx) {
  var C = CalcJs.Calc
  var lines = String(text || "").split("\n")
  var out = []
  var mode = C.detectMode(lines[0]).mode
  if (mode === "code") return out
  var results = C.evaluate(String(text || ""), mode === "math", fx)
  var inFence = false
  for (var i = 0; i < lines.length; i++) {
    var s = lines[i]
    out.push(null)
    if (i === 0) {
      if (mode !== "plain") {
        var summary = modeSummary(mode, text)
        if (summary) out[0] = { pos: trimEnd(s).length, label: summary, summary: true }
        continue
      }
    } else {
      var fence = /^\s*```/.test(s)
      if (fence) inFence = !inFence
      if (inFence || fence || /^\s*\/\//.test(s)) continue
    }
    var r = results[i]
    if (!r || !r.show) continue
    var value = C.formatResult(r)
    var eq = s.search(/=\s*$/)
    out[i] = eq >= 0 ? { pos: eq + 1, label: value } : { pos: trimEnd(s).length, label: "= " + value }
  }
  return out
}

// lineResults mapped onto the text blocks: for each block, [{ pos, label }] with
// `pos` an offset in that block's text.
function blockResults(blocks, perLine) {
  var out = []
  var line = 0
  for (var b = 0; b < blocks.length; b++) {
    var list = []
    var block = blocks[b]
    if (block.kind === "text" && counts(block)) {
      var lines = block.text.split("\n")
      var offset = 0
      for (var j = 0; j < lines.length; j++) {
        var r = perLine[line + j]
        if (r) list.push({ pos: offset + Math.min(r.pos, lines[j].length), label: r.label, summary: !!r.summary })
        offset += lines[j].length + 1
      }
      line += lines.length
    } else if (counts(block)) {
      line += 1
    }
    out.push(list)
  }
  return out
}

// Exchange rates saved by the app (fx-rates.json), for "100 € in $ =".
function ratesFrom(json) {
  var table = parseObject(json)
  if (!table || !table.rates) return undefined
  try {
    return CalcJs.Calc.buildRates(table.rates, table.base || "EUR")
  } catch (e) {
    return undefined
  }
}

// ----------------------------------------------------------------- colours

function hex(color) {
  function two(v) {
    var s = Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16)
    return s.length < 2 ? "0" + s : s
  }
  return color ? "#" + two(color.r) + two(color.g) + two(color.b) : "#000000"
}

// Four ink colours: the theme's text, accent and urgent colours, without repeats
// (many themes use one colour for text and accent), topped up with green and blue.
function inkColors(themeColors) {
  var out = []
  var all = themeColors.map(hex).concat(["#2bff88", "#4aa8ff", "#ffb02e"])
  for (var i = 0; i < all.length && out.length < 4; i++) if (out.indexOf(all[i]) < 0) out.push(all[i])
  return out
}
