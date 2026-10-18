// Wit book template. Values come from config.json, written by the server
// from the book's book.yaml. The server replaces the #include line at the end
// with the book's content, converted from Markdown.

#let cfg = json("config.json")

#let mm-of(v) = v * 1mm
#let trim-w = mm-of(cfg.trim_w_mm)
#let trim-h = mm-of(cfg.trim_h_mm)
#let bleed = mm-of(cfg.bleed_mm)
#let margin = mm-of(cfg.margin_mm)
#let margin-inner = mm-of(cfg.margin_inner_mm)
#let dark = cfg.dark_background

// Proofing palette (dark pages are for the author's review only, never print).
#let palette = (
  rgb(77, 26, 26), rgb(26, 51, 26), rgb(77, 26, 26), rgb(26, 26, 51),
  rgb(51, 26, 51), rgb(51, 38, 26), rgb(38, 38, 51), rgb(64, 38, 38),
)
// In the app's dark themes the preview may use plain dark pages instead.
#let plain = cfg.preview_bg != ""
#let ink = if plain { rgb(cfg.preview_ink) } else if dark { white } else { black }

// Pages left empty so a chapter opens on a right-hand page: strictly between
// the page where the previous text ended and the chapter's first page.
// A marker at the very top of its page means nothing came before it there
// (e.g. right after the title page), so that page is empty too.
#let is-blank(p) = {
  let ends = query(<chapter-before>).map(m => {
    let pos = m.location().position()
    if pos.y <= bleed + margin + 1pt { pos.page - 1 } else { pos.page }
  })
  let starts = query(<chapter-start>).map(m => m.location().page())
  ends.zip(starts).any(((e, s)) => e < p and p < s)
}

#let folio = context {
  let p = here().page()
  if p == 1 or is-blank(p) { return }
  let n = text(size: cfg.page_number_font_size_pt * 1pt, str(p))
  let y = trim-h + bleed - margin / 2
  if cfg.page_number_position == "bottom-center" {
    place(top + center, dy: y, n)
  } else if calc.odd(p) {
    place(top + right, dx: -(bleed + margin), dy: y, n)
  } else {
    place(top + left, dx: bleed + margin, dy: y, n)
  }
}

#let page-bg = if plain {
  rect(width: 100%, height: 100%, fill: rgb(cfg.preview_bg))
} else if dark {
  context {
    let p = here().page()
    rect(width: 100%, height: 100%, fill: palette.at(calc.rem(p - 1, palette.len())))
  }
} else { none }

#set document(title: cfg.title, author: cfg.author)
#set page(
  width: trim-w + 2 * bleed,
  height: trim-h + 2 * bleed,
  // Inside = spine side; mirrored on facing pages (page 1 is a right-hand page).
  margin: (inside: bleed + margin-inner, outside: bleed + margin, top: bleed + margin, bottom: bleed + margin),
  header: none,
  footer: none,
  background: page-bg,
  foreground: folio,
)
#let size = cfg.body_font_size_pt * 1pt
#set text(
  font: cfg.body_font_name,
  size: size,
  lang: cfg.lang,
  fill: ink,
  hyphenate: cfg.hyphenate,
  // A 1em line box, so line_height_mult is exactly baseline-to-baseline / size.
  top-edge: 0.8em,
  bottom-edge: -0.2em,
)
#let line-gap = (cfg.line_height_mult - 1) * size
#set par(
  justify: cfg.justify,
  leading: line-gap,
  // para_gap_lines: extra space between paragraphs, as a fraction of a line.
  spacing: line-gap + cfg.para_gap_lines * cfg.line_height_mult * size,
  first-line-indent: if cfg.first_line_indent { (amount: 1.2em, all: false) } else { 0pt },
)
#set strong(delta: 300)

// ---- building blocks used by content.typ -----------------------------------

#let chapter(title) = {
  [#metadata(none)<chapter-before>]
  if cfg.chapter_new_page { pagebreak(weak: true, to: if cfg.chapter_recto { "odd" } else { none }) }
  [#metadata(none)<chapter-start>]
  v(margin * 0.5)
  block(width: 100%, below: cfg.chapter_title_spacing_mult * cfg.body_font_size_pt * 1pt)[
    #set align(if cfg.chapter_title_align == "center" { center } else { left })
    #set par(first-line-indent: 0pt, justify: false)
    #text(size: cfg.chapter_title_font_size_pt * 1pt, weight: "bold", title)
  ]
}

#let section(title) = block(above: 1.6em, below: 1em, sticky: true)[
  #text(size: cfg.body_font_size_pt * 1.15pt, weight: "bold", title)
]

#let scenebreak() = block(above: 1.2em, below: 1.2em, width: 100%)[
  #align(center, text(tracking: 0.6em, [\*\*\*]))
]

#let spot(path, width: 70%) = block(above: 1em, below: 1em, width: 100%, breakable: false)[
  #align(center, image(path, width: width))
]

#let fullbleed(path) = page(margin: 0pt, background: none, foreground: none, fill: none)[
  #image(path, width: 100%, height: 100%, fit: "cover")
]

#let quoteblock(body) = block(inset: (left: 1.2em), above: 1em, below: 1em, emph(body))

// ---- title page ------------------------------------------------------------

#if cfg.include_title_page {
  page(foreground: none)[
    #set align(center)
    #set par(justify: false)
    #v(30%)
    #text(size: cfg.title_page_title_font_size_pt * 1pt, weight: "bold", cfg.title)
    #if cfg.author != "" {
      v(1.5em)
      text(size: cfg.title_page_author_font_size_pt * 1pt, cfg.author)
    }
  ]
}

#include "content.typ"
