import "./style.css";
import "./fonts.css";
import { EditorView, drawSelection, keymap, Decoration, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { Compartment, EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { initSettings, renderChecklist } from "./settings";
import { initBooks } from "./books";
import { initBookMenu, renderBookMenu, type BookItem } from "./bookmenu";
import { initHistory, type Revision } from "./history";
import { initChanges } from "./changes";
import { focusCompartment, initReveal, setFocusMode } from "./focus";
import { asIs, toDisk, toEditor, type EditorText } from "./paras";
import { centered, character, fountainPlugin, parenthetical, sceneHeading, transition, type ScriptKind } from "./fountain";
import { addCommands, addCommandSource, initCommands, keyLabel, openPalette, runCommand, titleWithKey, type Command } from "./commands";
import { continueList, insertLink, toggleList, toggleWrap, underlinePlugin } from "./format";
import { initPrefs, spellCompartment, spellExtension, spellOn } from "./prefs";
import { initSidebars } from "./sidebars";
import { initShortcuts } from "./shortcuts";
import { store } from "./store";

// ---------------------------------------------------------------- types

interface PageRef { url: string }
export interface Preview {
  pages: PageRef[];
  /** [markdown line, page] for each paragraph and heading, sorted by line. */
  lines: [number, number][];
  error: string | null;
  ms: number;
}
type DocKind = "book" | "essay" | ScriptKind;
interface BookData {
  settings: { document_type: DocKind } | null;
  id: string;
  source: string;
  hash: string;
  images: string[];
  settings_error: string | null;
  revision: Revision | null;
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

// ---------------------------------------------------------------- state

let bookId = "";
let baseHash = "";
let images: string[] = [];
let saving = false;
let saveAgain = false;
let conflict = false;
let saveTimer: number | undefined;
/// Editor text as last saved (the editor shows one line per paragraph; see paras.ts).
let lastSaved = "";
/// Markdown line where each editor line's paragraph starts on disk.
let diskLines: number[] = [];
let pageCount = 0;

const SAVE_DELAY_MS = 800;

// ---------------------------------------------------------------- editor

const mdStyle = HighlightStyle.define([
  { tag: t.heading1, fontSize: "1.5em", fontWeight: "700" },
  { tag: t.heading2, fontSize: "1.25em", fontWeight: "700" },
  { tag: t.heading, fontWeight: "700" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: [t.processingInstruction, t.meta, t.contentSeparator], color: "var(--muted)" },
  { tag: [t.link, t.url], color: "var(--muted)" },
  { tag: t.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: t.comment, color: "var(--muted)" },
]);

// Colour the dialogue dash at the start of a line, so speech is easy to spot.
const dashMatcher = new MatchDecorator({
  regexp: /^— /g,
  decoration: Decoration.mark({ class: "cm-dash" }),
});
const dashPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = dashMatcher.createDeco(view); }
    update(u: ViewUpdate) { this.decorations = dashMatcher.updateDeco(u, this.decorations); }
  },
  { decorations: (v) => v.decorations },
);

const editorTheme = EditorView.theme({
  "&": { backgroundColor: "transparent", color: "var(--text)" },
  ".cm-dash": { color: "var(--mark)", fontWeight: "700" },
});

function insertDash(view: EditorView): boolean {
  const { from, to } = view.state.selection.main;
  const line = view.state.doc.lineAt(from);
  // At the start of a line it opens dialogue; mid-line it's an em dash with spaces.
  const atStart = from === line.from;
  const text = atStart ? "— " : " — ";
  view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length }, scrollIntoView: true });
  view.focus();
  return true;
}

/// Typing a second hyphen turns "--" into a dash, the way Ctrl+D does. A third
/// at the start of a line gives back "---", the scene break.
const doubleHyphen = EditorView.inputHandler.of((view, from, to, text) => {
  if (text !== "-" || from !== to) return false;
  const line = view.state.doc.lineAt(from);
  if (view.state.sliceDoc(line.from, from) === "— ") {
    view.dispatch({ changes: { from: line.from, to, insert: "---" }, selection: { anchor: line.from + 3 } });
    return true;
  }
  if (view.state.sliceDoc(from - 1, from) !== "-") return false;
  const atStart = from - 1 === line.from;
  const before = view.state.sliceDoc(from - 2, from - 1);
  // Keep the spacing the writer typed: "word -- word" and "word--word" both
  // become "word — word"; at the start of a line it opens dialogue.
  const dash = atStart ? "— " : before === " " ? "—" : " — ";
  view.dispatch({ changes: { from: from - 1, to, insert: dash }, selection: { anchor: from - 1 + dash.length } });
  return true;
});

const langCompartment = new Compartment();
const proseExtensions = [
  // No Markdown Enter handling: Enter simply starts a new paragraph (or list item).
  markdown({ addKeymap: false }),
  syntaxHighlighting(mdStyle),
  dashPlugin,
  underlinePlugin,
  doubleHyphen,
  keymap.of([{ key: "Enter", run: continueList }]),
];

/// What is being written. Scripts are Fountain, shown line for line; books and
/// essays are Markdown, shown one line per paragraph.
let kind: DocKind = "book";
const isScript = () => kind === "screenplay" || kind === "play";
const fromFile = (src: string): EditorText => (isScript() ? asIs(src) : toEditor(src));
const toFile = (text: string): EditorText => (isScript() ? asIs(text) : toDisk(text));

function setKind(k: DocKind) {
  kind = k;
  document.body.dataset.kind = k;
  document.body.classList.toggle("script", isScript());
  view.dispatch({ effects: langCompartment.reconfigure(isScript() ? fountainPlugin(k as ScriptKind) : proseExtensions) });
}

const view = new EditorView({
  parent: $("editor"),
  state: EditorState.create({
    doc: "",
    extensions: [
      history(),
      // Draw our own thick cursor instead of the browser's 1px caret.
      drawSelection({ cursorBlinkRate: 1000 }),
      EditorView.lineWrapping,
      // Markdown for books and essays, Fountain for scripts (see setKind).
      langCompartment.of(proseExtensions),
      highlightSelectionMatches(),
      editorTheme,
      focusCompartment.of([]),
      EditorView.contentAttributes.of({ lang: "ro", "aria-label": "Book text" }),
      spellCompartment.of(spellExtension(spellOn())),
      // Shortcuts like Ctrl+B live in the command list (main.ts, commands.ts).
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        ...searchKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) { onEdit(); sidebars?.docChanged(); changes?.docChanged(); }
        if (u.selectionSet && !u.docChanged) { syncPreviewToCursor(); sidebars?.cursorMoved(); changes?.cursorMoved(); }
      }),
    ],
  }),
});

function setDoc(text: string) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

// ---------------------------------------------------------------- status

let statusText = "";
let statusBad = false;
let revision: Revision | null = null;

/** The latest named revision, from a server reply (left alone when the reply has none). */
function setRevision(r: Revision | null | undefined) {
  if (r === undefined) return;
  revision = r;
  sidebars?.revisionChanged();
  changes?.revisionChanged();
}

function setStatus(text: string, bad = false) {
  statusText = text;
  statusBad = bad;
  const el = $("status");
  el.textContent = text;
  el.classList.toggle("bad", bad);
  updateFocusStatus();
}

let wordTimer: number | undefined;

/// The quiet line in the corner: empty unless saving has a problem. Words and
/// the version number are in the statistics sidebar.
function updateFocusStatus() {
  clearTimeout(wordTimer);
  wordTimer = window.setTimeout(() => {
    const quiet = $("quiet-status");
    if (!bookId) { quiet.textContent = ""; return; }
    quiet.textContent = statusBad ? statusText : "";
    quiet.classList.toggle("bad", statusBad);
  }, 150);
}

/** A note above the work area. `bad` is for problems that need a decision. */
function showBanner(html: string | null, bad = false) {
  const b = $("banner");
  if (html === null) { b.hidden = true; b.innerHTML = ""; return; }
  b.innerHTML = html;
  b.classList.toggle("bad", bad);
  b.hidden = false;
}

// ---------------------------------------------------------------- saving

function onEdit() {
  if (conflict) return;
  store.set(`draft.${bookId}`, toFile(view.state.doc.toString()).text);
  setStatus("Edited");
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(save, SAVE_DELAY_MS);
}

function saveNow() {
  clearTimeout(saveTimer);
  save();
}

async function save() {
  if (conflict || !bookId) return;
  if (saving) { saveAgain = true; return; }
  const editorText = view.state.doc.toString();
  if (editorText === lastSaved) { setStatus("Saved"); return; }
  const disk = toFile(editorText);
  const content = disk.text;
  saving = true;
  setStatus("Saving…");
  try {
    const res = await fetch(`/api/books/${bookId}/source`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ content, base_hash: baseHash }),
    });
    if (res.status === 409) {
      conflict = true;
      setStatus("Not saved", true);
      showBanner(
        `<span>The story file was changed outside Wit, so autosave is paused to protect both versions.
         Your text here is kept as a draft in this browser.</span>
         <button id="reload-disk">Load the version on disk</button>
         <button id="keep-mine" class="primary">Keep mine and overwrite</button>`,
        true,
      );
      $("reload-disk").onclick = () => loadBook(bookId, true);
      $("keep-mine").onclick = async () => {
        const d = await (await fetch(`/api/books/${bookId}`)).json();
        baseHash = d.hash;
        conflict = false;
        showBanner(null);
        lastSaved = "";
        saveNow();
      };
      return;
    }
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || res.statusText);
    baseHash = data.hash;
    setRevision(data.revision);
    lastSaved = editorText;
    diskLines = disk.diskLines;
    store.del(`draft.${bookId}`);
    renderPreview(data.preview);
    setStatus(view.state.doc.toString() === editorText ? "Saved" : "Edited");
  } catch (e) {
    setStatus("Not saved — retrying", true);
    saveAgain = true;
    await new Promise((r) => setTimeout(r, 3000));
  } finally {
    saving = false;
    if (saveAgain) { saveAgain = false; save(); }
  }
}

window.addEventListener("beforeunload", (e) => {
  if (view.state.doc.toString() !== lastSaved) e.preventDefault();
});

// ---------------------------------------------------------------- preview

function renderPreview(p: Preview) {
  const err = $("error");
  if (p.error) {
    // Errors name Markdown file lines; point at the editor's line instead.
    const msg = p.error.replace(/Line (\d+)/g, (_, n) => `Line ${editorLineOf(Number(n))}`);
    err.textContent = "The pages couldn't be updated:\n" + msg;
    err.hidden = false;
    return; // keep the last good pages on screen
  }
  err.hidden = true;
  const box = $("pages");
  const existing = Array.from(box.children) as HTMLElement[];
  p.pages.forEach((pg, i) => {
    let fig = existing[i];
    if (!fig) {
      fig = document.createElement("figure");
      fig.className = "page";
      fig.style.margin = "0";
      const img = document.createElement("img");
      img.alt = `Page ${i + 1}`;
      img.decoding = "async";
      const cap = document.createElement("span");
      cap.textContent = String(i + 1);
      fig.append(img, cap);
      fig.title = "Go to this page's text";
      fig.onclick = () => jumpToPage(Number(fig!.dataset.page));
      box.append(fig);
    }
    fig.dataset.page = String(i + 1);
    const img = fig.querySelector("img")!;
    // Only swap pages whose content changed, so the view doesn't flicker.
    if (img.getAttribute("src") !== pg.url) img.src = pg.url;
  });
  existing.slice(p.pages.length).forEach((el) => el.remove());
  const n = p.pages.length;
  pageCount = n;
  linePages = p.lines ?? [];
  markCurrentPage(pageOfLine(view.state.doc.lineAt(view.state.selection.main.head).number));
  if (!$("settings").hidden) renderChecklist(n);
  sidebars.pagesChanged();
  const note = n % 4 === 0 || isScript() ? "" : ` · ${4 - (n % 4)} more for a multiple of 4`;
  $("page-count").textContent = `${n} pages${note} · ${p.ms} ms`;
}

// ---------------------------------------------------------------- editor ⇄ pages

let linePages: [number, number][] = [];
let syncTimer: number | undefined;

/// Editor line whose paragraph contains a Markdown file line.
function editorLineOf(diskLine: number): number {
  let line = 1;
  diskLines.forEach((d, i) => { if (d <= diskLine) line = i + 1; });
  return line;
}

/// Page showing an editor line: the page of the last marker at or before it.
function pageOfLine(editorLine: number): number {
  const line = diskLines[editorLine - 1] ?? editorLine;
  let page = linePages.length ? linePages[0][1] : 1;
  for (const [l, p] of linePages) {
    if (l > line) break;
    page = p;
  }
  return page;
}

function markCurrentPage(page: number): HTMLElement | null {
  let current: HTMLElement | null = null;
  document.querySelectorAll<HTMLElement>(".page").forEach((f) => {
    const on = Number(f.dataset.page) === page;
    f.classList.toggle("current", on);
    if (on) current = f;
  });
  return current;
}

function syncPreviewToCursor() {
  clearTimeout(syncTimer);
  syncTimer = window.setTimeout(() => {
    const fig = markCurrentPage(pageOfLine(view.state.doc.lineAt(view.state.selection.main.head).number));
    if (fig && !$("main").classList.contains("layout-write")) fig.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, 150);
}

/// Move the cursor to the first paragraph on a page (or the nearest one before
/// it, for illustration-only pages).
function jumpToPage(page: number) {
  const hit = linePages.find(([, p]) => p === page) ?? [...linePages].reverse().find(([, p]) => p < page);
  if (!hit) return;
  const line = view.state.doc.line(Math.min(editorLineOf(hit[0]), view.state.doc.lines));
  view.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: "start", yMargin: 80 }) });
  markCurrentPage(page);
  if (!$("main").classList.contains("layout-pages")) view.focus();
}

async function refreshPreview() {
  const res = await fetch(`/api/books/${bookId}/preview`);
  renderPreview(await res.json());
}

const zoom = $<HTMLInputElement>("zoom");
function applyZoom() {
  $("pages").style.setProperty("--page-w", `${zoom.value}%`);
  store.set("zoom", zoom.value);
}
zoom.value = store.get("zoom") ?? "70";
zoom.oninput = applyZoom;
applyZoom();

// ---------------------------------------------------------------- loading

async function loadBook(id: string, discardDraft = false) {
  bookId = id;
  renderBookMenu(bookList, id);
  store.set("book", id);
  conflict = false;
  showBanner(null);
  setStatus("Loading…");
  const res = await fetch(`/api/books/${id}`);
  const data: BookData = await res.json();
  baseHash = data.hash;
  setRevision(data.revision);
  images = data.images;
  setKind(data.settings?.document_type ?? "book");
  const ed = fromFile(data.source);
  lastSaved = ed.text;
  diskLines = ed.diskLines;

  // Drafts are stored in Markdown file form.
  const draft = discardDraft ? null : store.get(`draft.${id}`);
  setDoc(ed.text);
  if (discardDraft) store.del(`draft.${id}`);
  if (draft && draft !== data.source) {
    showBanner(
      `<span>There is unsaved text from your last session.</span>
       <button id="restore-draft" class="primary">Restore it</button>
       <button id="drop-draft">Discard it</button>`,
    );
    $("restore-draft").onclick = () => { showBanner(null); setDoc(fromFile(draft).text); };
    $("drop-draft").onclick = () => { showBanner(null); store.del(`draft.${id}`); };
  }
  // Files with paragraphs on consecutive lines (no blank line between) print
  // as one long paragraph; save once so each line becomes its own paragraph.
  if (!draft && toFile(ed.text).text !== data.source) {
    lastSaved = "";
    window.setTimeout(saveNow, 0);
  }
  if (data.settings_error) showBanner(`<span>book.yaml problem: ${escapeHtml(data.settings_error)}</span>`, true);

  document.title = `${bookList.find((b) => b.id === id)?.title ?? "Book"} — Wit`;
  setStatus("Saved");
  sidebars.bookChanged();
  view.focus();
  refreshPreview();
}

function escapeHtml(s: string) {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

let bookList: BookItem[] = [];

/** Refresh the book menu; returns the books. */
async function loadBookList() {
  bookList = await (await fetch("/api/books")).json();
  renderBookMenu(bookList, bookId);
  showWelcome(bookList.length === 0);
  return bookList;
}

/** No book yet: a calm first page instead of an empty editor. */
function showWelcome(on: boolean) {
  document.body.classList.toggle("no-book", on);
  $("welcome").hidden = !on;
  // The menu would only offer the two actions the page already shows.
  $("book-menu").hidden = on;
  if (on) { setStatus("No book open"); shortcuts.renderWelcome(); }
  sidebars.bookChanged();
}

async function init() {
  const bookUi = await initBooks({
    async opened(id) {
      if (bookId && view.state.doc.toString() !== lastSaved) await save();
      await loadBookList();
      await loadBook(id);
    },
  });
  initBookMenu({
    async choose(id) {
      if (id === bookId) return view.focus();
      if (view.state.doc.toString() !== lastSaved) await save();
      loadBook(id);
    },
    open: bookUi.showOpen,
    create: bookUi.showCreate,
  });
  $("welcome-new").onclick = bookUi.showCreate;
  $("welcome-open").onclick = bookUi.showOpen;
  $("welcome-theme").onclick = () => prefs.nextTheme();
  addCommands([
    { id: "book-open", category: "Document", title: "Open a folder…", keys: ["Alt+O", "Mod+O"], run: bookUi.showOpen },
    { id: "book-new", category: "Document", title: "New book, essay, screenplay or play…", keys: ["Alt+N"], macKeys: ["Mod+Alt+N", "Alt+N"], run: bookUi.showCreate },
  ]);

  await prefs.pagesReady;
  const books = await loadBookList();
  const start = books.find((b) => b.id === store.get("book"))?.id ?? books[0]?.id;
  if (start) await loadBook(start);
}

// ---------------------------------------------------------------- layout

function setLayout(l: string) {
  $("main").className = `layout-${l}`;
  document.querySelectorAll<HTMLButtonElement>("[data-layout]").forEach((b) =>
    b.setAttribute("aria-pressed", String(b.dataset.layout === l)));
  store.set("layout", l);
  view.requestMeasure();
}
setLayout(store.get("layout") ?? "split");
const layout = () => $("main").className.replace("layout-", "");

// ---------------------------------------------------------------- images

const dialog = $<HTMLDialogElement>("image-dialog");
let picked = "";
async function pickImage() {
  // Re-read the folder so newly added pictures show up without a reload.
  try {
    const res = await fetch(`/api/books/${bookId}`);
    if (res.ok) images = (await res.json()).images;
  } catch { /* keep the list we have */ }
  const list = $("image-list");
  list.innerHTML = "";
  picked = "";
  $<HTMLButtonElement>("image-ok").disabled = true;
  if (images.length === 0) {
    list.innerHTML = `<p class="empty-note"><strong>No pictures yet</strong>
      Put PNG or JPG files in the book's <code>images/</code> folder, then choose Image again.</p>`;
  }
  for (const path of images) {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("aria-pressed", "false");
    const img = document.createElement("img");
    img.src = `/api/books/${bookId}/asset/${encodeURI(path)}`;
    img.alt = "";
    img.loading = "lazy";
    b.append(img, path.replace(/^images\//, ""));
    b.onclick = () => {
      picked = path;
      list.querySelectorAll("button").forEach((x) => x.setAttribute("aria-pressed", String(x === b)));
      $<HTMLButtonElement>("image-ok").disabled = false;
    };
    list.append(b);
  }
  dialog.showModal();
}
dialog.addEventListener("close", () => {
  if (dialog.returnValue !== "ok" || !picked) { view.focus(); return; }
  const placement = (dialog.querySelector<HTMLInputElement>("input[name=placement]:checked")?.value) ?? "";
  const title = placement ? ` "${placement}"` : "";
  // <...> lets Markdown accept file names with spaces or brackets.
  const dest = /[\s()<>]/.test(picked) ? `<${picked}>` : picked;
  const md = `![](${dest}${title})`;
  const { from } = view.state.selection.main;
  const line = view.state.doc.lineAt(from);
  // Put the image on its own paragraph after the current line.
  const insert = `\n${md}`;
  view.dispatch({ changes: { from: line.to, insert }, selection: { anchor: line.to + insert.length } });
  view.focus();
});

const pageSetup = initSettings({
  bookId: () => bookId,
  pageCount: () => pageCount,
  kind: () => kind,
  onPreview: renderPreview,
  toggled(open) { if (!open) view.focus(); },
});

// ---------------------------------------------------------------- focus mode

let focusOn = false;
let hintTimer: number | undefined;
function toggleFocus(on = !focusOn) {
  focusOn = on;
  store.set("focus", on ? "1" : "0");
  setFocusMode(view, on);
  updateFocusStatus();
  // A brief reminder of the way out, then nothing.
  clearTimeout(hintTimer);
  document.body.classList.toggle("focus-enter", on);
  if (on) hintTimer = window.setTimeout(() => document.body.classList.remove("focus-enter"), 3000);
}
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && focusOn && !document.querySelector("dialog[open]")) toggleFocus(false);
});
initReveal();
if (store.get("focus") === "1") toggleFocus(true);

// ---------------------------------------------------------------- history

const historyUi = initHistory({
  named: setRevision,
  bookId: () => bookId,
  async flush() { if (view.state.doc.toString() !== lastSaved) await save(); },
  currentText: () => toFile(view.state.doc.toString()).text,
  baseHash: () => baseHash,
  restored(data) {
    // Set the saved state first so loading the text doesn't trigger a save.
    const ed = fromFile(data.source);
    lastSaved = ed.text;
    diskLines = ed.diskLines;
    baseHash = data.hash;
    setRevision(data.revision);
    setDoc(ed.text);
    store.del(`draft.${bookId}`);
    renderPreview(data.preview as Preview);
    setStatus("Restored");
  },
});

// ---------------------------------------------------------------- sidebars

/// Cursor to the start of an editor line, from the chapter or scene list.
function goToLine(n: number) {
  const line = view.state.doc.line(Math.min(n, view.state.doc.lines));
  view.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: "start", yMargin: 80 }) });
  if (layout() === "pages") markCurrentPage(pageOfLine(n))?.scrollIntoView({ block: "start", behavior: "smooth" });
  else view.focus();
}

// Declared with `var` so the editor's update listener, created above, can
// test for it before it exists.
var sidebars: ReturnType<typeof initSidebars> = initSidebars({
  view,
  kind: () => kind,
  bookId: () => bookId,
  pageCount: () => pageCount,
  revision: () => revision,
  openHistory: () => historyUi.open(),
  nameRevision: () => historyUi.open(true),
  goTo: goToLine,
  changed: () => view.requestMeasure(),
  enabled: () => !!bookId && !document.body.classList.contains("no-book"),
});

// ---------------------------------------------------------------- changes

// `var` for the same reason as `sidebars`.
var changes: ReturnType<typeof initChanges> = initChanges({
  view,
  bookId: () => bookId,
  revision: () => revision,
  isScript,
  fromFile,
  goTo: goToLine,
  nameRevision: () => historyUi.open(true),
});

/** Show the changes on the right (switching to Split mode), or the pages again. */
function toggleChanges() {
  if (layout() !== "split") { setLayout("split"); changes.setOn(true); return; }
  changes.setOn(!changes.isOn());
}

// ---------------------------------------------------------------- commands

const shortcuts = initShortcuts();

const prefs = initPrefs({
  view,
  openPageSetup: () => pageSetup.toggle(),
  changed: () => view.requestMeasure(),
  pagesChanged: () => { if (bookId) refreshPreview(); },
});

type FileKind = "print" | "proof" | "docx" | "odt";

// The desktop app saves files through a native dialog; a browser opens or downloads them.
let nativeSave = false;
fetch("/api/app").then((r) => r.json()).then((a) => { nativeSave = !!a.native_save; }).catch(() => {});

let bannerTimer: number | undefined;
function note(html: string, bad = false) {
  clearTimeout(bannerTimer);
  showBanner(html, bad);
  if (!bad) bannerTimer = window.setTimeout(() => showBanner(null), 8000);
}

/** A PDF (print or proof) or a Word or OpenDocument file of the book. */
async function makeFile(kind: FileKind) {
  if (!bookId) return;
  if (view.state.doc.toString() !== lastSaved) await save();
  const pdf = kind === "print" || kind === "proof";
  if (!nativeSave) {
    const a = document.createElement("a");
    a.href = pdf ? `/api/books/${bookId}/pdf/${kind}` : `/api/books/${bookId}/export/${kind}`;
    if (pdf) { a.target = "_blank"; a.rel = "noopener"; } else a.download = "";
    a.click();
    return;
  }
  setStatus(pdf ? "Making the PDF…" : "Making the file…");
  try {
    const res = await fetch(`/api/books/${bookId}/save/${kind}`, { method: "POST" });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || res.statusText);
    if (data.path) {
      const where = document.createElement("span");
      where.textContent = data.path;
      note(`<span>Saved to <b>${where.innerHTML}</b>${pdf ? "" : ". Send it to your editor, or open it in a word processor."}</span>`);
      setStatus("Saved");
    } else setStatus("");
  } catch (e) {
    note(`<span>Couldn't make the file: ${(e as Error).message.replace(/</g, "&lt;")}</span>`, true);
  }
}

/** One place for every file Wit can make. */
function openExport() {
  const d = $<HTMLDialogElement>("export-dialog");
  if (d.open) { d.close(); return; }
  d.querySelectorAll<HTMLButtonElement>("[data-export]").forEach((b) => {
    b.hidden = isScript() && (b.dataset.export === "docx" || b.dataset.export === "odt");
    b.onclick = () => { d.close(); makeFile(b.dataset.export as FileKind); };
  });
  d.showModal();
  d.querySelector<HTMLButtonElement>("[data-export]")?.focus();
}
$("export-close").onclick = () => $<HTMLDialogElement>("export-dialog").close();

const hasBook = () => !!bookId && !document.body.classList.contains("no-book");
// Writing commands need the editor on screen.
const canWrite = () => hasBook() && (layout() !== "pages" || focusOn);
const fmt = (f: () => boolean): (() => void) => () => { f(); };
const canProse = () => canWrite() && !isScript();
const canScript = () => canWrite() && isScript();

addCommands([
  { id: "palette", category: "View", title: "Show all commands", keys: ["Mod+Shift+P", "F1"], run: openPalette },
  { id: "help", category: "View", title: "Keyboard shortcuts", keys: ["Alt+?"], run: () => shortcuts.open() },
  { id: "save", category: "Document", title: "Save now", keys: ["Alt+S", "Mod+S"], run: saveNow, enabled: hasBook },
  { id: "print", category: "Document", title: "Print PDF (white pages)", keys: ["Alt+P", "Mod+P"], run: () => makeFile("print"), enabled: hasBook },
  { id: "proof", category: "Document", title: "Proof PDF (as set up)", keys: ["Alt+R"], run: () => makeFile("proof"), enabled: hasBook },
  { id: "export-docx", category: "Document", title: "Export as Word (.docx)", run: () => makeFile("docx"), enabled: () => hasBook() && !isScript() },
  { id: "export-odt", category: "Document", title: "Export as OpenDocument (.odt)", run: () => makeFile("odt"), enabled: () => hasBook() && !isScript() },
  { id: "export", category: "Document", title: "Export or print…", keys: ["Alt+E"], macKeys: ["Mod+Shift+E"], run: openExport, enabled: hasBook },
  { id: "page-setup", category: "Document", title: "Page setup…", keys: ["Alt+L"], run: () => pageSetup.toggle(), enabled: hasBook },
  { id: "history", category: "Document", title: "Revisions…", keys: ["Alt+H"], run: () => historyUi.open(), enabled: hasBook },
  { id: "name-revision", category: "Document", title: "Name a revision…", keys: ["Alt+Shift+S"], run: () => historyUi.open(true), enabled: hasBook },
  { id: "layout-write", category: "View", title: "Write mode (text only)", keys: ["Alt+1"], run: () => setLayout("write"), checked: () => layout() === "write", enabled: hasBook },
  { id: "layout-split", category: "View", title: "Split mode (text and pages)", keys: ["Alt+2"], run: () => setLayout("split"), checked: () => layout() === "split", enabled: hasBook },
  { id: "layout-pages", category: "View", title: "Page mode (pages only)", keys: ["Alt+3"], run: () => setLayout("pages"), checked: () => layout() === "pages", enabled: hasBook },
  { id: "changes", category: "View", title: "Changes since the latest revision", keys: ["Alt+C"], run: toggleChanges, checked: () => changes.isOn() && layout() === "split", enabled: hasBook },
  { id: "focus", category: "View", title: "Focus mode", keys: ["Alt+4"], run: () => toggleFocus(), checked: () => focusOn, enabled: hasBook },
  { id: "bold", category: "Format", title: "Bold", keys: ["Mod+B"], editor: true, run: fmt(() => toggleWrap(view, "**")), enabled: canWrite },
  { id: "italic", category: "Format", title: "Italic", keys: ["Mod+I"], editor: true, run: fmt(() => toggleWrap(view, "*")), enabled: canWrite },
  { id: "underline", category: "Format", title: "Underline", keys: ["Mod+U"], editor: true, run: fmt(() => (isScript() ? toggleWrap(view, "_") : toggleWrap(view, "<u>", "</u>"))), enabled: canWrite },
  { id: "dash", category: "Format", title: "Dialogue dash", keys: ["Mod+D"], editor: true, run: fmt(() => insertDash(view)), enabled: canProse },
  { id: "bullets", category: "Format", title: "Bulleted list", keys: ["Mod+Shift+8"], editor: true, run: fmt(() => toggleList(view, "bullet")), enabled: canProse },
  { id: "numbers", category: "Format", title: "Numbered list", keys: ["Mod+Shift+7"], editor: true, run: fmt(() => toggleList(view, "number")), enabled: canProse },
  { id: "link", category: "Format", title: "Link", keys: ["Mod+K"], editor: true, run: fmt(() => insertLink(view)), enabled: canProse },
  { id: "image", category: "Format", title: "Insert illustration…", keys: ["Mod+M"], macKeys: ["Mod+Shift+M"], editor: true, run: pickImage, enabled: canProse },
  { id: "scene", category: "Script", get title() { return kind === "play" ? "Act or scene heading" : "Scene heading"; }, keys: ["Mod+Shift+1"], macKeys: ["Mod+Alt+1"], editor: true, run: fmt(() => sceneHeading(view, kind as ScriptKind)), enabled: canScript },
  { id: "character", category: "Script", title: "Character", keys: ["Mod+Shift+2"], macKeys: ["Mod+Alt+2"], editor: true, run: fmt(() => character(view)), enabled: canScript },
  { id: "parenthetical", category: "Script", title: "Parenthetical", keys: ["Mod+Shift+3"], macKeys: ["Mod+Alt+3"], editor: true, run: fmt(() => parenthetical(view)), enabled: canScript },
  { id: "transition", category: "Script", title: "Transition", keys: ["Mod+Shift+4"], macKeys: ["Mod+Alt+4"], editor: true, run: fmt(() => transition(view)), enabled: canScript },
  { id: "centered", category: "Script", title: "Centred text", keys: ["Mod+Shift+5"], macKeys: ["Mod+Alt+5"], editor: true, run: fmt(() => centered(view)), enabled: canScript },
  ...prefs.commands,
  ...sidebars.commands,
]);
// One command per book, so the palette can switch books too.
addCommandSource(() => bookList.filter((b) => b.id !== bookId).map((b): Command => ({
  id: `book-${b.id}`,
  category: "Switch to",
  title: b.title,
  async run() {
    if (view.state.doc.toString() !== lastSaved) await save();
    loadBook(b.id);
  },
})));
initCommands();

// Toolbar and dock buttons run commands; their tooltips show the shortcut.
document.querySelectorAll<HTMLButtonElement>("[data-cmd]").forEach((b) => {
  const id = b.dataset.cmd!;
  if (id !== "book-menu") {
    b.title = titleWithKey(id);
    b.onclick = () => runCommand(id);
  } else {
    b.title = `Open a book (${keyLabel("Alt+O")} for a folder)`;
  }
});
// The dock mustn't take the cursor out of the text.
$("dock").addEventListener("mousedown", (e) => e.preventDefault());

// Until the palette has been used once, say how to reach it.
if (!store.get("paletteUsed")) {
  const hint = $("start-hint");
  hint.textContent = `${keyLabel("Mod+Shift+P")} for every command · ${keyLabel("Alt+?")} for shortcuts · ${keyLabel("Alt+5")} for the toolbar`;
  document.body.classList.add("show-start-hint");
  window.setTimeout(() => document.body.classList.remove("show-start-hint"), 9000);
}

init();
