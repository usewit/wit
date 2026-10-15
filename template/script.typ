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

// ---- building blocks used by the converted script -------------------------

#let gap = line
#let scene(body, num: none) = block(above: 2 * gap, below: gap, sticky: true, width: 100%)[
  #set text(weight: if cfg.scene_heading_bold { "bold" } else { "regular" })
  #if play {
    align(center, upper(body))
  } else if num != none and cfg.scene_numbers {
    place(dx: -0.75in, upper(num))
    upper(body)
    place(top + right, dx: 0.75in, upper(num))
  } else {
    upper(body)
  }
]
#let action(body) = block(above: gap, below: gap, width: 100%)[
  #if play { pad(left: 2in, emph(body)) } else { body }
]
// A character's name stays with the first words they say.
#let cue(body) = block(above: gap, below: 0pt, sticky: true, width: 100%)[
  #if play { align(center, upper(body)) } else { pad(left: 2.2in, upper(body)) }
]
#let paren(body) = block(above: 0pt, below: 0pt, sticky: true, width: 100%)[
  #if play { pad(left: 1in, right: 1in, emph(body)) } else { pad(left: 1.6in, block(width: 2in, body)) }
]
#let speech(body) = block(above: 0pt, below: gap, width: 100%)[
  #if play { body } else { pad(left: 1in, block(width: 3.5in, body)) }
]
#let transition(body) = block(above: gap, below: gap, width: 100%, align(right, upper(body)))
#let centered(body) = block(above: gap, below: gap, width: 100%, align(center, body))
#let lyric(body) = block(above: gap, below: gap, width: 100%, pad(left: if play { 0in } else { 1in }, emph(body)))
// Stage plays: acts (#) start a new page; scenes (##) are centred headings.
#let section(depth, body) = {
  if depth == 1 { pagebreak(weak: true) }
  block(above: 2 * gap, below: 2 * gap, sticky: true, width: 100%,
    align(center, if depth == 1 { underline(upper(strong(body))) } else { upper(body) }))
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
