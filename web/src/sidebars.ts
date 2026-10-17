// Two sidebars, each toggled on its own: the outline on the left (chapters in a
// book, sections in an essay, scenes in a script) and the numbers on the right
// (words, characters, paragraphs, chapters, pages), with ring gauges toward a word
// or page target. Targets are kept per book, on this computer.

import { Text } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { Command } from "./commands";
import { classify, stripHidden, titleEnd, type ScriptKind } from "./fountain";
import { store } from "./store";

type DocKind = "book" | "essay" | ScriptKind;

interface Deps {
  view: EditorView;
  kind(): DocKind;
  bookId(): string;
  pageCount(): number;
  /** The latest named revision, if there is one. */
  revision(): { n: number; name: string } | null;
  openHistory(): void;
  nameRevision(): void;
  /** Put the cursor on an editor line and show it (and its page). */
  goTo(line: number): void;
  /** A sidebar appeared or went, so the editor can measure again. */
  changed(): void;
  enabled(): boolean;
  /** Sidebars don't show in Focus mode; leave it to show one. */
  leaveFocus(): void;
}

interface Item { line: number; level: number; title: string; scene: boolean; words: number }
interface Stats {
  words: number;
  chars: number;
  charsNoSpaces: number;
  paragraphs: number;
  /** Chapters (top-level headings) in prose; scenes in a script. */
  parts: number;
  speeches: number;
  outline: Item[];
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const WORD = /[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu;
const fmt = new Intl.NumberFormat();
const countWords = (s: string) => s.match(WORD)?.length ?? 0;

/** Inline Markdown down to the words that print. */
function plainInline(s: string): string {
  return s
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    // Only the HTML Wit understands is markup; any other <word> prints as typed.
    .replace(/<!--.*?-->/g, "")
    .replace(/<\/?u>|<br\s*\/?>/gi, "")
    .replace(/\\(.)/g, "$1")
    .replace(/[*_~`]/g, "");
}

/** Fountain inline marks, notes and element prefixes down to the printed words. */
function plainScript(s: string): string {
  return s
    .replace(/\[\[.*?\]\]/g, "")
    .replace(/#[^#\s]+#\s*$/, "")
    .replace(/^[.@>!~]/, "")
    .replace(/<$/, "")
    .replace(/[*_]/g, "")
    .trim();
}

function analyzeProse(doc: Text, kind: DocKind): Stats {
  const s: Stats = { words: 0, chars: 0, charsNoSpaces: 0, paragraphs: 0, parts: 0, speeches: 0, outline: [] };
  const lineWords: number[] = [];
  for (let n = 1; n <= doc.lines; n++) {
    const raw = doc.line(n).text.trim();
    let text = "";
    const h = /^(#{1,6})\s+(.*)$/.exec(raw);
    if (!raw || raw.startsWith("![") || /^([-*_]\s*){3,}$/.test(raw) || /^<!--.*-->$/.test(raw)) {
      // Blank, an illustration, a scene break or a comment: nothing that prints as words.
    } else if (h) {
      text = plainInline(h[2]);
      s.outline.push({ line: n, level: h[1].length, title: text.trim() || "Untitled", scene: false, words: 0 });
    } else {
      text = plainInline(raw.replace(/^>\s*/, "").replace(/^([-*+]|\d+[.)])\s+/, ""));
      if (text.trim()) s.paragraphs++;
    }
    const w = countWords(text);
    lineWords.push(w);
    s.words += w;
    s.chars += Array.from(text).length;
    s.charsNoSpaces += Array.from(text.replace(/\s/g, "")).length;
  }
  const top = Math.min(...s.outline.map((i) => i.level));
  // Essays: every heading is a section. Books: chapters are the top level.
  s.parts = kind === "essay" ? s.outline.length : s.outline.filter((i) => i.level === top).length;
  sumWords(s.outline, lineWords);
  return s;
}

function analyzeScript(src: Text, kind: ScriptKind): Stats {
  const s: Stats = { words: 0, chars: 0, charsNoSpaces: 0, paragraphs: 0, parts: 0, speeches: 0, outline: [] };
  const lineWords: number[] = [];
  // Notes and boneyard, even across lines, are blanked out first, as in print.
  const doc = Text.of(stripHidden(src.toString()).split("\n"));
  const title = titleEnd(doc);
  let section = 0;
  let inBlock = false;
  for (let n = 1; n <= doc.lines; n++) {
    const raw = doc.line(n).text.trim();
    const c = raw ? classify(doc, n, kind, title) : null;
    let text = "";
    if (!raw) inBlock = false;
    else if (c === "title" || c === "synopsis" || c === "break") {
      // Not part of the script's words (title page, synopses, page breaks).
    } else if (c === "section") {
      const level = /^#+/.exec(raw)![0].length;
      section = level;
      s.outline.push({ line: n, level, title: raw.replace(/^#+\s*/, "") || "Untitled", scene: false, words: 0 });
    } else {
      text = plainScript(raw);
      if (c === "scene") {
        s.parts++;
        s.outline.push({ line: n, level: section + 1, title: text.toUpperCase(), scene: true, words: 0 });
      } else if (!inBlock) {
        s.paragraphs++;
        inBlock = true;
      }
      if (c === "cue") s.speeches++;
    }
    const w = countWords(text);
    lineWords.push(w);
    s.words += w;
    s.chars += Array.from(text).length;
    s.charsNoSpaces += Array.from(text.replace(/\s/g, "")).length;
  }
  sumWords(s.outline, lineWords);
  return s;
}

/** Each entry's words: from its line up to the next entry at the same level or above. */
function sumWords(outline: Item[], lineWords: number[]) {
  const before = [0];
  for (const w of lineWords) before.push(before[before.length - 1] + w);
  outline.forEach((item, i) => {
    const next = outline.slice(i + 1).find((o) => o.level <= item.level);
    const end = next ? next.line - 1 : lineWords.length;
    item.words = before[end] - before[item.line - 1];
  });
}

/** Everything counted in the text, the way it will print. */
export function analyzeDoc(doc: Text, kind: DocKind): Stats {
  return kind === "screenplay" || kind === "play" ? analyzeScript(doc, kind) : analyzeProse(doc, kind);
}

const NAMES: Record<DocKind, { side: string; part: string; parts: string; empty: string }> = {
  book: { side: "Chapters", part: "chapter", parts: "Chapters", empty: "Start a line with # and a space to begin a chapter." },
  essay: { side: "Sections", part: "section", parts: "Sections", empty: "Start a line with # and a space to begin a section." },
  screenplay: { side: "Scenes", part: "scene", parts: "Scenes", empty: "Start a line with INT. or EXT. to begin a scene." },
  play: { side: "Scenes", part: "scene", parts: "Scenes", empty: "Start a line with # for an act, or . for a scene." },
};

export function initSidebars(d: Deps) {
  const outlineEl = $("outline");
  const statsEl = $("stats");
  const list = $("outline-list");
  const wordsIn = $<HTMLInputElement>("target-words");
  const pagesIn = $<HTMLInputElement>("target-pages");
  let stats: Stats | null = null;
  let current = -1;
  let timer: number | undefined;

  const isScript = () => d.kind() === "screenplay" || d.kind() === "play";
  const outlineOn = () => store.get("outline") === "1";
  const statsOn = () => store.get("stats") === "1";

  // ---------------------------------------------------------------- showing and hiding
  function show() {
    const book = d.enabled();
    outlineEl.hidden = !(book && outlineOn());
    statsEl.hidden = !(book && statsOn());
    document.querySelectorAll('[data-cmd="outline"]').forEach((b) => b.setAttribute("aria-pressed", String(outlineOn())));
    document.querySelectorAll('[data-cmd="stats"]').forEach((b) => b.setAttribute("aria-pressed", String(statsOn())));
    $<HTMLInputElement>("prefs-outline").checked = outlineOn();
    $<HTMLInputElement>("prefs-stats").checked = statsOn();
    d.changed();
  }
  function setOutline(on: boolean) {
    store.set("outline", on ? "1" : "0");
    show();
    if (on) { analyze(); renderCurrent(true); }
  }
  function setStats(on: boolean) {
    store.set("stats", on ? "1" : "0");
    show();
    if (on) analyze();
  }
  $("outline-close").onclick = () => { setOutline(false); d.view.focus(); };
  $("stats-history").onclick = () => d.openHistory();
  $("stats-name").onclick = () => d.nameRevision();
  $("stats-close").onclick = () => { setStats(false); d.view.focus(); };
  $<HTMLInputElement>("prefs-outline").onchange = (e) => setOutline((e.target as HTMLInputElement).checked);
  $<HTMLInputElement>("prefs-stats").onchange = (e) => setStats((e.target as HTMLInputElement).checked);

  // ---------------------------------------------------------------- counting
  function analyze() {
    clearTimeout(timer);
    if (outlineEl.hidden && statsEl.hidden) { stats = null; return; }
    stats = analyzeDoc(d.view.state.doc, d.kind());
    renderOutline();
    renderStats();
  }
  /** After an edit: count again once typing pauses. */
  function later() {
    clearTimeout(timer);
    timer = window.setTimeout(analyze, 300);
  }

  // ---------------------------------------------------------------- outline
  function renderOutline() {
    if (!stats || outlineEl.hidden) return;
    const names = NAMES[d.kind()];
    $("outline-title").textContent = names.side;
    list.innerHTML = "";
    current = -1;
    if (stats.outline.length === 0) {
      const li = document.createElement("li");
      li.className = "empty-note";
      li.innerHTML = `<strong>No ${names.parts.toLowerCase()} yet</strong>`;
      li.append(names.empty);
      list.append(li);
      return;
    }
    const top = Math.min(...stats.outline.map((i) => i.level));
    let scene = 0;
    for (const item of stats.outline) {
      const li = document.createElement("li");
      const b = document.createElement("button");
      b.type = "button";
      b.style.setProperty("--lvl", String(Math.min(item.level - top, 4)));
      b.classList.toggle("top", item.level === top && !item.scene);
      const t = document.createElement("span");
      t.className = "t";
      t.textContent = item.title;
      const meta = document.createElement("span");
      meta.className = "meta";
      const words = `${fmt.format(item.words)} ${item.words === 1 ? "word" : "words"}`;
      meta.textContent = item.scene ? `Scene ${++scene} · ${words}` : words;
      b.append(t, meta);
      b.onclick = () => d.goTo(item.line);
      li.append(b);
      list.append(li);
    }
    renderCurrent(false);
  }

  /** Mark the entry the cursor is in. */
  function renderCurrent(scroll: boolean) {
    if (!stats || outlineEl.hidden) return;
    const here = d.view.state.doc.lineAt(d.view.state.selection.main.head).number;
    let idx = -1;
    stats.outline.forEach((o, i) => { if (o.line <= here) idx = i; });
    if (idx === current && !scroll) return;
    current = idx;
    const buttons = list.querySelectorAll("button");
    buttons.forEach((b, i) => {
      if (i === idx) b.setAttribute("aria-current", "true");
      else b.removeAttribute("aria-current");
    });
    buttons[idx]?.scrollIntoView({ block: "nearest" });
  }

  // ---------------------------------------------------------------- numbers
  type Target = { words: number; pages: number };
  function target(): Target {
    try {
      const t = JSON.parse(store.get(`target.${d.bookId()}`) ?? "{}");
      return { words: Number(t.words) || 0, pages: Number(t.pages) || 0 };
    } catch { return { words: 0, pages: 0 }; }
  }
  function setTarget(t: Target) {
    if (!t.words && !t.pages) store.del(`target.${d.bookId()}`);
    else store.set(`target.${d.bookId()}`, JSON.stringify(t));
    renderStats();
  }
  const readTarget = (i: HTMLInputElement) => Math.max(0, Math.round(Number(i.value) || 0));
  for (const i of [wordsIn, pagesIn]) {
    i.oninput = () => setTarget({ words: readTarget(wordsIn), pages: readTarget(pagesIn) });
    // Enter in a target field goes back to the text.
    i.onkeydown = (e) => { if (e.key === "Enter") { e.preventDefault(); d.view.focus(); } };
  }

  /** One progress tile: the count, and with a target a ring and what's left. */
  function tile(key: "words" | "pages", value: number | null, goal: number, input: HTMLInputElement) {
    if (document.activeElement !== input) input.value = goal ? String(goal) : "";
    $(`v-${key}`).textContent = value === null ? "…" : fmt.format(value);
    const ring = $(`ring-${key}`);
    const note = $(`n-${key}`);
    ring.hidden = !goal || value === null;
    if (ring.hidden || value === null) { note.textContent = ""; return; }
    const pct = value / goal;
    ring.classList.toggle("done", pct >= 1);
    ring.setAttribute("aria-valuemax", String(goal));
    ring.setAttribute("aria-valuenow", String(Math.min(value, goal)));
    ring.setAttribute("aria-valuetext", `${Math.floor(pct * 100)}%, ${fmt.format(value)} of ${fmt.format(goal)}`);
    ring.querySelector(".fill")!.setAttribute("stroke-dasharray", `${Math.min(pct, 1) * 100} 100`);
    ring.querySelector(".pct")!.textContent = pct >= 1 ? "✓" : `${Math.floor(pct * 100)}%`;
    note.textContent = pct >= 1 ? "Target reached" : `${fmt.format(goal - value)} to go`;
    note.classList.toggle("done", pct >= 1);
  }

  function rows(id: string, list: [string, string][]) {
    const dl = $(id);
    dl.innerHTML = "";
    for (const [k, v] of list) {
      const div = document.createElement("div");
      const dt = document.createElement("dt");
      const dd = document.createElement("dd");
      dt.textContent = k;
      dd.textContent = v;
      div.append(dt, dd);
      dl.append(div);
    }
  }

  // A script's pages count from after its title page, which prints when the
  // book's Title page setting is on. Asked again when the page count changes,
  // as turning the title page on or off does.
  const titlePage = { key: "", pages: 1 };
  function titlePages(): number {
    const key = `${d.bookId()}:${d.pageCount()}`;
    if (titlePage.key !== key) {
      titlePage.key = key;
      fetch(`/api/books/${d.bookId()}/settings`).then((r) => r.json()).then((j) => {
        const n = j.settings?.include_title_page === false ? 0 : 1;
        if (titlePage.key === key && n !== titlePage.pages) { titlePage.pages = n; renderStats(); }
      }).catch(() => {});
    }
    return titlePage.pages;
  }

  function renderStats() {
    if (!stats || statsEl.hidden) return;
    const names = NAMES[d.kind()];
    const all = d.pageCount();
    const pages = isScript() && all ? Math.max(0, all - titlePages()) : all;
    const t = target();
    tile("words", stats.words, t.words, wordsIn);
    tile("pages", all ? pages : null, t.pages, pagesIn);

    // Reading at about 230 words a minute; a script page is about a minute on screen.
    const minutes = isScript() ? pages : Math.round(stats.words / 230);
    rows("stats-length", [
      [isScript() ? "Screen time" : "Reading time", duration(minutes)],
      ["Characters", fmt.format(stats.chars)],
      ["Without spaces", fmt.format(stats.charsNoSpaces)],
    ]);
    const structure: [string, string][] = [[names.parts, fmt.format(stats.parts)]];
    if (stats.parts > 1) structure.push([`Words per ${names.part}`, fmt.format(Math.round(stats.words / stats.parts))]);
    if (isScript()) structure.push(["Speeches", fmt.format(stats.speeches)]);
    structure.push(["Paragraphs", fmt.format(stats.paragraphs)]);
    rows("stats-structure", structure);
    const r = d.revision();
    $("v-revision").textContent = r ? `v${r.n}` : "None yet";
    $("v-revision-name").textContent = r ? r.name : "";
  }

  function duration(min: number): string {
    if (min < 1) return "Under a minute";
    if (min < 60) return `${min} min`;
    const h = Math.floor(min / 60);
    const m = min % 60;
    return m ? `${h} h ${m} min` : `${h} h`;
  }

  function focusTarget(i: HTMLInputElement) {
    d.leaveFocus();
    if (!statsOn()) setStats(true);
    i.focus();
    i.select();
  }

  show();

  const commands: Command[] = [
    { id: "outline", category: "View", get title() { return `${NAMES[d.kind()].side} sidebar`; }, keys: ["Alt+["], macKeys: ["Mod+Alt+S", "Alt+["], run: () => setOutline(!outlineOn()), checked: outlineOn, enabled: d.enabled },
    { id: "stats", category: "View", title: "Statistics sidebar", keys: ["Alt+]"], macKeys: ["Mod+Alt+I", "Alt+]"], run: () => setStats(!statsOn()), checked: statsOn, enabled: d.enabled },
    { id: "target-words", category: "Target", title: "Set a word target…", run: () => focusTarget(wordsIn), enabled: d.enabled },
    { id: "target-pages", category: "Target", title: "Set a page target…", run: () => focusTarget(pagesIn), enabled: d.enabled },
  ];

  return {
    commands,
    /** The text changed. */
    docChanged: later,
    /** The cursor moved without an edit. */
    cursorMoved: () => renderCurrent(false),
    /** New pages from the preview. */
    pagesChanged: renderStats,
    /** A revision was named, or another book opened. */
    revisionChanged: renderStats,
    /** Another book (or none) is open. */
    bookChanged() { show(); analyze(); renderCurrent(true); },
  };
}
