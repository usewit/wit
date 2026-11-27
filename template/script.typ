// Wit script template: screenplays and stage plays written in Fountain.
// Values come from config.json (the book's book.yaml); the server replaces the
// #include line at the end with the script, converted from Fountain.
//
// Screenplay layout is the industry one: 12 pt Courier-style type, six lines to
// the inch, 1.5 in left margin and a 6 in line, characters at 3.7 in from the
// page edge, dialogue at 2.5 in, parentheticals at 3.1 in, transitions on the
// right, page numbers top right from page 2. A4 keeps the same left margin and
// line length. Stage plays (US manuscript format) centre the character names,
// run dialogue across the line and indent stage directions in italics.

#let cfg = json("config.json")
#let play = cfg.document_type == "play"
#let dark = cfg.dark_background
// In the app's dark themes the preview may use plain dark pages instead.
#let plain = cfg.preview_bg != ""
#let ink = if plain { rgb(cfg.preview_ink) } else if dark { white } else { black }
#let size = cfg.body_font_size_pt * 1pt
// One line of type: 12 pt at 12 pt size, six lines to the inch.
#let line = size

#let page-w = cfg.trim_w_mm * 1mm
#let page-h = cfg.trim_h_mm * 1mm
#let margin-left = 1.5in
#let measure = calc.min(6in, page-w - margin-left - 0.75in)

#let palette = (
  rgb(77, 26, 26), rgb(26, 51, 26), rgb(77, 26, 26), rgb(26, 26, 51),
  rgb(51, 26, 51), rgb(51, 38, 26), rgb(38, 38, 51), rgb(64, 38, 38),
)
#let page-bg = if plain {
  rect(width: 100%, height: 100%, fill: rgb(cfg.preview_bg))
} else if dark {
  context rect(width: 100%, height: 100%, fill: palette.at(calc.rem(here().page() - 1, palette.len())))
} else { none }

#set document(title: cfg.title, author: cfg.author)
#set page(
  width: page-w,
  height: page-h,
  margin: (left: margin-left, right: page-w - margin-left - measure, top: 1in, bottom: 1in),
  background: page-bg,
  header-ascent: 0.5in - 0.5em,
  // Script pages count from the first page after the title page; that first
  // page carries no number.
  header: context {
    let n = counter(page).get().first()
    if n > 1 { align(right, text(fill: ink, str(n) + ".")) }
  },
)
#set text(
  font: cfg.body_font_name,
  size: size,
  lang: cfg.lang,
  fill: ink,
  hyphenate: false,
  top-edge: 0.8em,
  bottom-edge: -0.2em,
)
#set par(justify: false, leading: 0pt, spacing: 0pt, first-line-indent: 0pt)
// Typewriter manners: straight quotes, and "--" stays two hyphens.
#set smartquote(enabled: false)
#show strong: set text(weight: "bold")

// ---- where on the page ----------------------------------------------------
//
// A speech that crosses a page should break at the end of a sentence. Where
// the page ends is only known once everything before it is laid out, and
// every such break moves what follows, so asking the layout where things
// landed would take one more pass per page. Instead the script arrives as a
// list of its parts, and `script` goes through them keeping its own count of
// the lines used on the page, by the same rules the layout follows (spacing
// between blocks, two lines of a paragraph kept at each end, headings kept
// with what follows). A speech then knows how many lines are left for it.
//
// `typst compile --input wit-check=1` marks (<wit-drift>) every block that
// landed on another line than the count says.

#let gap = line
// Lines on a page, and the (MORE) line under a speech that goes on.
#let cap = calc.floor((page-h - 2in) / line + 0.01)
#let foot = if play { 0 } else { 1 }
// used: lines taken on this page; below: the space the last block wants
// under it; hold: where the headings waiting for their text start; at: where
// the last block started; plan: where the last speech is cut.
#let fresh = (used: 0, below: 0, hold: none, at: 0, plan: none)
// How many lines `body` takes (needs context).
#let count(body, width: measure) = calc.round(std.measure(width: width, body).height / line)
// Text without its source-line markers, for measuring: a marker in what is
// measured ties the count to where things landed, and it would run again on
// every pass.
#let bare(body) = if body.func() == metadata { none } else if body.has("children") { body.children.filter(c => c.func() != metadata).join() } else { body }

// How many of a paragraph's `l` lines go on a page with `room` lines left:
// never one line alone at either end. `fit-rest`: the paragraph began on an
// earlier page.
#let fit-first(l, room) = if l <= room { l } else if room < 2 or l < 4 { 0 } else if room == l - 1 { l - 2 } else { room }
#let fit-rest(l, room) = if l <= room { l } else if room == l - 1 { calc.max(0, l - 2) } else { calc.max(0, room) }
#let flow-par(l, avail) = {
  let k = fit-first(l, avail)
  if k == l { return (ok: true, crossed: false, last: l) }
  if k == 0 { return (ok: false) }
  let rest = l - k
  while rest > cap {
    let k = fit-rest(rest, cap)
    if k <= 0 { break }
    rest -= k
  }
  (ok: true, crossed: true, last: rest)
}
#let flow-whole(l, avail) = (ok: l <= avail, crossed: false, last: l)

// Add a block to the count. `flow(avail)` says what the block does with
// `avail` lines: `ok` (it can start here), `crossed` (it ends on a later
// page), `last` (the lines it takes on the page it ends on).
#let put(s, above, below, sticky, flow) = {
  let gap = if s.used == 0 { 0 } else { calc.max(s.below, above) }
  let start = s.used + gap
  let hold = s.hold
  let r = flow(cap - start)
  if not r.ok and start > 0 {
    // To the next page, and the headings waiting for it come along.
    let carried = hold != none and hold > 0
    start = if carried { s.used - hold + gap } else { 0 }
    hold = if carried { 0 } else { none }
    r = flow(cap - start)
  }
  if not r.ok { r = (crossed: false, last: cap - start) }
  (
    used: if r.crossed { r.last } else { start + r.last },
    below: below,
    hold: if not sticky { none } else if hold != none { hold } else { start },
    at: start,
    plan: r.at("plan", default: none),
  )
}
#let checking = sys.inputs.at("wit-check", default: "") != ""
#let seen(want) = if checking {
  context {
    let got = calc.floor((here().position().y - 1in) / line + 0.1)
    if want != got [#metadata((page: here().page(), want: want, got: got))<wit-drift>]
  }
}

// ---- the parts of a converted script --------------------------------------

// A block of text: `above` and `below` in lines, `wrap` lays the text out.
#let piece(above, below, body, wrap, sticky: false) = (kind: "piece", above: above, below: below, sticky: sticky, body: body, wrap: wrap)
#let newpage = (kind: "page")
#let scene(body, num: none) = piece(2, 1, sticky: true, body, body => {
  set text(weight: if cfg.scene_heading_bold { "bold" } else { "regular" })
  if play {
    align(center, upper(body))
  } else if num != none and cfg.scene_numbers {
    // The left number in the margin before the heading; the right one ends
    // with the line, 7.5 in from the page edge (A4 too), the heading kept
    // clear of it.
    place(dx: -0.75in, upper(num))
    place(top + right, upper(num))
    pad(right: 0.5in, upper(body))
  } else {
    upper(body)
  }
})
#let action(body) = piece(1, 1, body, body => if play { pad(left: 2in, emph(body)) } else { body })
// Dialogue. A forced name ("@McCLANE") prints as written.
#let cue-name(body, forced) = if forced { body } else { upper(body) }
#let paren-inner(body, col: false) = if col { pad(left: 0.4in, right: 0.4in, if play { emph(body) } else { body }) } else if play { pad(left: 1in, right: 1in, emph(body)) } else { pad(left: 1.6in, block(width: 2in, body)) }
// The parentheticals and speech of `dialogue`; `col`: in one column of dual
// dialogue.
#let paren(body, col: false) = block(above: 0pt, below: 0pt, sticky: true, width: 100%, paren-inner(body, col: col))
#let speech(body, col: false) = block(above: 0pt, below: 0pt, width: 100%)[
  #if play or col { body } else { pad(left: 1in, block(width: 3.5in, body)) }
]
// A character's name and everything they say until the next blank line. `n`
// is the name's line in the script; `items` are `(paren: […])` and
// `(say: lines)`, each line an array of sentences. `contd`: the name as
// written already says it continues.
#let dialogue(n, name, items, forced: false, contd: false) = (kind: "dialogue", n: n, name: cue-name(name, forced), items: items, contd: contd)
// Dual dialogue: two characters speaking at once, side by side across the
// line, kept on one page.
#let dual(first, second) = (kind: "dual", first: first, second: second)
#let transition(body) = piece(1, 1, body, body => align(right, upper(body)))
#let centered(body) = piece(1, 1, body, body => align(center, body))
#let lyric(body) = piece(1, 1, body, body => pad(left: if play { 0in } else { 1in }, emph(body)))
// Stage plays: acts (#) start a new page; scenes (##) are centred headings.
#let section(depth, body) = (
  ..if depth == 1 { (newpage,) } else { () },
  piece(2, 2, sticky: true, body, body => align(center, if depth == 1 { underline(upper(strong(body))) } else { upper(body) })),
)

// ---- a speech across pages ------------------------------------------------

// Where a speech that doesn't fit is cut, page by page. `items`: `(p: lines)`
// for a parenthetical, or for a paragraph `(l: lines, a: …, b: …)`, where
// `a.at(k - 1)` is how many lines its first k sentences take and
// `b.at(k - 1)` the rest (none: not to be cut there). The cuts in `plan` are `(item, 0)`, before that
// item, or `(item, k)`, after sentence k of that paragraph.
//
// On each page the speech is cut after the last sentence that ends there
// with two lines or more on each side; if none does, the page takes the lines
// that fit. A parenthetical stays with the words after it.
#let flow-dialogue(items, total, avail) = {
  if 1 + total <= avail { return (ok: true, crossed: false, last: 1 + total) }
  // Up to three lines can't be split, two staying on each side.
  if total <= 3 { return (ok: false) }
  // The lines item i needs to start a page.
  let lead(i) = {
    let it = items.at(i)
    if "p" not in it { if it.l < 4 { it.l } else { 2 } } else if i + 1 < items.len() { it.p + lead(i + 1) } else { it.p }
  }
  let room = avail - 1 - foot
  if lead(0) > room { return (ok: false) }
  let cuts = ()
  // The first item of the page, the lines of it on earlier pages, and the
  // sentence its paragraph now starts after.
  let (i, done, base) = (0, 0, 0)
  let last = 0
  for _ in range(200) {
    // What fits if nothing is cut.
    let (r, placed, j) = (room, 0, i)
    let brk = none
    while j < items.len() {
      let it = items.at(j)
      if "p" in it {
        if lead(j) > r and placed > 0 { brk = (0, r); break }
        placed += it.p
        r -= it.p
      } else {
        let rem = if j > i { it.l } else if base > 0 { it.b.at(base - 1) - done } else { it.l - done }
        let k = if j == i and done > 0 { fit-rest(rem, r) } else { fit-first(rem, r) }
        if k < rem { brk = (k, r); break }
        placed += rem
        r -= rem
      }
      j += 1
    }
    if brk == none { last = 1 + placed; break }
    let (k, r) = brk
    let it = items.at(j)
    let cut = none
    if "p" in it {
      cut = (j, 0)
    } else {
      let (at, from) = if j == i { (done, base) } else { (0, 0) }
      let a0 = if from > 0 { it.a.at(from - 1) } else { 0 }
      // A paragraph that starts after a cut may run one line longer.
      let slack = if from > 0 { 1 } else { 0 }
      let after = items.slice(j + 1).map(x => x.at("p", default: 0) + x.at("l", default: 0)).sum(default: 0)
      let s = it.a.len()
      while s > from and cut == none {
        let a = it.a.at(s - 1)
        if a != none {
          let lo = a - a0 - at
          if lo >= 1 and lo + slack <= r and placed + lo >= 2 and it.b.at(s - 1) + after >= 2 { cut = (j, s) }
        }
        s -= 1
      }
      if cut == none {
        // The end of the paragraph before, ahead of its parentheticals.
        let (q, before) = (j, placed)
        while q > i and "p" in items.at(q - 1) { q -= 1; before -= items.at(q).p }
        if q > i and q < j and before >= 2 { cut = (q, 0) }
      }
    }
    if cut != none {
      cuts.push(cut)
      (i, done, base) = (cut.at(0), 0, cut.at(1))
    } else if j == i {
      if k == 0 and placed == 0 { last = cap; break }
      done += k
    } else {
      (i, done, base) = (j, k, 0)
    }
    room = cap - 1 - foot
  }
  (ok: true, crossed: true, last: last, plan: cuts)
}

// A paragraph's sentences in a row, each with whether a new line starts
// before it; and some of them as text.
#let flat(lines, strip: false) = lines.map(l => l.enumerate().map(((x, s)) => (x == 0, if strip { bare(s) } else { s }))).sum(default: ())
#let run(sents) = sents.enumerate().map(((x, (br, s))) => if x == 0 { s } else if br { linebreak() + s } else { [ ] + s }).join()

// A speech and the lines it takes, for `flow-dialogue` (needs context).
#let dialogue-lines(d) = {
  let tall(body) = count(body, width: if play { measure } else { 3.5in })
  let lines = d.items.map(it => if "paren" in it { (p: count(paren-inner(bare(it.paren)))) } else { (l: tall(run(flat(it.say, strip: true)))) })
  let total = lines.map(x => x.at("p", default: 0) + x.at("l", default: 0)).sum(default: 0)
  if total > 3 {
    lines = d.items.zip(lines).map(((it, x)) => if "p" in x { x } else {
      let f = flat(it.say, strip: true)
      // Each sentence end is measured twice over, so a very long paragraph
      // is only cut at every second or third.
      let step = calc.ceil((f.len() - 1) / 24)
      let ends = range(1, f.len()).map(k => if calc.rem(k, step) == 0 { k })
      (l: x.l, a: ends.map(k => if k != none { tall(run(f.slice(0, k))) }), b: ends.map(k => if k != none { tall(run(f.slice(k))) }))
    })
  }
  (lines, total)
}

// The name stays with the first two lines of speech. When the speech crosses
// a page, a screenplay prints (MORE) under its last line on that page and the
// name again with (CONT'D) on the next; a stage play prints no (MORE) and the
// name with (continuing).
//
// How: a speech that stays on one page (`cuts` is none) is a block kept
// together. One that crosses is a one-column grid whose header (the name) and
// footer repeat on every page it runs to. Both always take one line, so the
// pages come out the same whatever they print; each then looks up which page
// it is on. The header on a later page adds (CONT'D). The footer after the
// last words is the blank line under the speech; the others print (MORE),
// moved up under the last line of speech on their page when lines were held
// back for the next one. A cut ends the paragraph there and sends the rest to
// the next page; without one the paragraph breaks where the page ends.
#let dialogue-block(d, cuts, at-line) = {
  let (n, name, items) = (d.n, d.name, d.items)
  let at(body) = if play { align(center, body) } else { pad(left: 2.2in, body) }
  if cuts == none {
    return block(above: gap, below: gap, width: 100%, breakable: false, {
      at[#seen(at-line)#metadata(n)<q>#name]
      items.map(it => if "paren" in it { paren(it.paren) } else { speech(run(flat(it.say))) }).join()
    })
  }
  let head = label("wit-cue-" + str(n))
  let said = label("wit-said-" + str(n))
  block(above: gap, below: gap, width: 100%, {
    grid(
      columns: 1fr,
      grid.header(repeat: true, context {
        let pages = query(head).map(m => m.location().page())
        [#metadata(none)#head]
        // The source-line marker goes with the first name only.
        at(if pages.len() == 0 or here().page() == calc.min(..pages) [#seen(at-line)#metadata(n)<q>#name] else if d.contd { name } else if play [#name (continuing)] else [#name (CONT'D)])
      }),
      {
        // An invisible mark on every line said, for the footer to find.
        set par.line(numbering: _ => [#metadata(none)#said], number-clearance: 0pt)
        for (x, it) in items.enumerate() {
          if (x, 0) in cuts { colbreak(weak: true) }
          if "paren" in it { paren(it.paren) } else {
            let f = flat(it.say)
            let from = 0
            for k in range(1, f.len()) {
              if (x, k) in cuts {
                speech(run(f.slice(from, k)))
                colbreak(weak: true)
                from = k
              }
            }
            speech(run(f.slice(from)))
          }
        }
      },
      ..if not play {
        (grid.footer(repeat: true, context {
          let pages = query(head).map(m => m.location().page())
          let me = here().position()
          if pages.len() == 0 or me.page == calc.max(..pages) {
            v(line)
          } else {
            // Marks sit on the baseline, 0.2em above the bottom of their line.
            let ys = query(said).map(m => m.location().position()).filter(p => p.page == me.page).map(p => p.y)
            let up = if ys.len() == 0 { 0pt } else { calc.max(0pt, me.y - calc.max(..ys) - 0.2 * size) }
            at(move(dy: -up, [(MORE)]))
          }
        }),)
      } else { () },
    )
    // The footer's blank line is the gap below.
    if not play { v(-gap) }
  })
}

// The script: its parts in order, each added to the count as it is set.
#let script(parts) = context {
  let s = fresh
  let out = ()
  for e in parts.flatten() {
    if e.kind == "page" {
      s = fresh
      out.push(pagebreak(weak: true))
    } else if e.kind == "dialogue" {
      let (lines, total) = dialogue-lines(e)
      s = put(s, 1, 1, false, avail => flow-dialogue(lines, total, avail))
      out.push(dialogue-block(e, s.plan, s.at))
    } else if e.kind == "dual" {
      // Side by side across the line, never split.
      let side(d, strip) = {
        align(center, if strip { d.name } else [#metadata(d.n)<q>#d.name])
        d.items.map(it => if "paren" in it { paren(col: true, if strip { bare(it.paren) } else { it.paren }) } else { speech(col: true, run(flat(it.say, strip: strip))) }).join()
      }
      let both(strip) = grid(columns: (1fr, 1fr), column-gutter: 0.3in, side(e.first, strip), side(e.second, strip))
      let l = count(both(true))
      s = put(s, 1, 1, false, avail => flow-whole(l, avail))
      out.push(block(above: gap, below: gap, breakable: false, width: 100%, seen(s.at) + both(false)))
    } else {
      let l = count((e.wrap)(bare(e.body)))
      s = put(s, e.above, e.below, e.sticky, avail => flow-par(l, avail))
      // A heading may yet move on with what follows it, so it isn't checked.
      out.push(block(above: e.above * gap, below: e.below * gap, sticky: e.sticky, width: 100%, if e.sticky { (e.wrap)(e.body) } else { seen(s.at) + (e.wrap)(e.body) }))
    }
  }
  out.join()
}

// The title page: from the script's own title page if it has one, otherwise
// from the book's title and author.
#let titlepage(title: none, credit: none, author: none, source: none, draft: none, contact: none) = {
  if not cfg.include_title_page { return }
  let title = if title != none { title } else { cfg.title }
  let author = if author != none { author } else if cfg.author != "" { cfg.author } else { none }
  page(header: none)[
    #v(3.3in)
    #align(center)[
      #upper(strong(title))
      #if credit != none or author != none {
        v(2 * line)
        if credit != none { credit } else { [Written by] }
        if author != none { v(line); author }
      }
      #if source != none { v(2 * line); source }
    ]
    #place(bottom + left, block(width: 50%, contact))
    #place(bottom + right, block(width: 45%, align(right, draft)))
    // The next page is the script's page 1.
    #counter(page).update(0)
  ]
}

#include "content.typ"
