// Screenplays and stage plays in Fountain: the editor lays each line out the
// way it will print (characters and dialogue indented, transitions on the
// right), and offers commands for the script elements.

import { EditorSelection, Prec, StateEffect, StateField, type Extension, type Line, type Text } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, keymap, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";

export type ScriptKind = "screenplay" | "play";

const SCENE = /^(int|ext|est|int\.?\/ext|i\/e)[. ]/i;
const blank = (doc: Text, n: number) => n < 1 || n > doc.lines || doc.line(n).text.trim() === "";
const isCue = (t: string) => {
  const name = t.replace(/\^$/, "").split("(")[0].trim();
  return /\p{L}/u.test(name) && !/\p{Ll}/u.test(name);
};
const isTransition = (t: string) => t.endsWith("TO:") && !/\p{Ll}/u.test(t);

/** Title page keys a script may start with (any case); as TITLE_KEYS in fountain.rs. */
const TITLE_KEYS = new Set([
  "title", "credit", "author", "authors", "source", "draft date", "date", "contact", "contact info",
  "copyright", "notes", "revision", "format", "watermark", "font",
]);

/** Lines of the title page at the top ("Title: …" up to the first blank line), if any. */
export function titleEnd(doc: Text): number {
  const first = doc.line(1).text;
  const key = /^(\S[^:]*):/.exec(first);
  if (!key || !TITLE_KEYS.has(key[1].trim().toLowerCase())) return 0;
  let n = 1;
  while (n <= doc.lines && !blank(doc, n)) n++;
  return n - 1;
}

/** What a line is, as a CSS class suffix, or null for plain action. */
export function classify(doc: Text, n: number, kind: ScriptKind, title: number): string | null {
  if (n <= title) return "title";
  const t = doc.line(n).text.trim();
  if (!t) return null;
  if (/^={3,}$/.test(t)) return "break";
  if (t.startsWith("#")) return "section";
  if (t.startsWith("=")) return "synopsis";
  if (t.startsWith("!")) return null;
  if (t.startsWith(".") && !t.startsWith("..")) return "scene";
  if (t.startsWith(">") && t.endsWith("<")) return "center";
  if (t.startsWith(">")) return "trans";
  if (t.startsWith("~")) return "lyric";
  if (blank(doc, n - 1) && SCENE.test(t)) return "scene";
  if (blank(doc, n - 1) && blank(doc, n + 1) && isTransition(t)) return "trans";
  if (t.startsWith("@") || (blank(doc, n - 1) && !blank(doc, n + 1) && isCue(t))) return "cue";
  // Inside a block that opened with a character cue: dialogue.
  let start = n;
  while (start > 1 && !blank(doc, start - 1)) start--;
  if (start < n) {
    const head = doc.line(start).text.trim();
    // Forced action, transitions, lyrics and sections aren't characters, though in capitals.
    if (head.startsWith("@") || (!blank(doc, start + 1) && isCue(head) && !SCENE.test(head) && !/^[.!>~#=]/.test(head))) {
      return /^\(.*\)$/.test(t) ? "paren" : "dia";
    }
  }
  return kind === "play" ? "direction" : null;
}

const cache = new Map<string, Decoration>();
const lineDeco = (c: string) => cache.get(c) ?? cache.set(c, Decoration.line({ class: `cm-f-${c}` })).get(c)!;

// Blank out notes [[ ]] and boneyard (slash-star to star-slash), keeping every
// line, as strip_hidden in fountain.rs does.
export function stripHidden(src: string): string {
  return src.replace(/\/\*[\s\S]*?(?:\*\/|$)|\[\[[\s\S]*?(?:\]\]|$)/g, (m) => m.replace(/[^\n]/g, " "));
}

// After the Character command on an empty line: the start of that line, whose
// typing goes in capitals until the cursor leaves it (Enter goes on to the
// dialogue line).
const armCaps = StateEffect.define<number | null>();
const capsLine = StateField.define<number | null>({
  create: () => null,
  update(at, tr) {
    for (const e of tr.effects) if (e.is(armCaps)) return e.value;
    if (at === null) return null;
    const line = tr.state.doc.lineAt(tr.changes.mapPos(at, -1));
    return tr.state.doc.lineAt(tr.state.selection.main.head).number === line.number ? line.from : null;
  },
});
const capsInput = EditorView.inputHandler.of((view, from, to, text) => {
  const at = view.state.field(capsLine, false);
  const up = text.toUpperCase();
  if (at == null || up === text) return false;
  const line = view.state.doc.lineAt(at);
  if (from < line.from || to > line.to) return false;
  view.dispatch({ changes: { from, to, insert: up }, selection: EditorSelection.cursor(from + up.length), userEvent: "input.type" });
  return true;
});

// Notes and boneyard don't print, so they look different: dimmed, in italics.
const hiddenMark = Decoration.mark({ class: "cm-f-hidden" });
const hiddenPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = this.build(view); }
    update(u: ViewUpdate) { if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view); }
    build(view: EditorView) {
      const marks = [];
      for (const m of view.state.doc.toString().matchAll(/\/\*[\s\S]*?(?:\*\/|$)|\[\[[\s\S]*?(?:\]\]|$)/g)) {
        if (m[0].length) marks.push(hiddenMark.range(m.index!, m.index! + m[0].length));
      }
      return Decoration.set(marks);
    }
  },
  { decorations: (v) => v.decorations },
);

// ---------------------------------------------------------------- Tab and Enter
// As in screenwriting programs: Enter moves on to the element that usually
// comes next, and Tab on an empty line chooses what the line will be.

/** The element a line is, judged as it's being typed (a cue before its dialogue exists). */
function typing(view: EditorView, kind: ScriptKind, line: Line): string | null {
  const doc = view.state.doc;
  const t = line.text.trim();
  const c = classify(doc, line.number, kind, titleEnd(doc));
  if (c || !t) return c;
  if (view.state.field(capsLine, false) === line.from) return "cue";
  // A name in capitals on its own line, with nothing under it yet.
  if (blank(doc, line.number - 1) && blank(doc, line.number + 1) && isCue(t) && !SCENE.test(t) && !isTransition(t)) return "cue";
  return null;
}

function enter(kind: ScriptKind) {
  return (view: EditorView): boolean => {
    const { head, empty } = view.state.selection.main;
    const line = view.state.doc.lineAt(head);
    if (!empty || head !== line.to) return false;
    const c = typing(view, kind, line);
    // After a name or a parenthetical: the line they speak. After a speech, a
    // scene heading or a transition: a blank line, ready for what's next.
    const insert = c === "cue" || c === "paren" ? "\n" : c === "dia" || c === "scene" || c === "trans" || c === "section" ? "\n\n" : null;
    if (!insert) return false;
    view.dispatch({
      changes: { from: head, insert },
      selection: EditorSelection.cursor(head + insert.length),
      scrollIntoView: true,
      userEvent: "input",
    });
    return true;
  };
}

/** Line texts Tab leaves on an empty line while choosing, in order. */
const SCREEN_CYCLE = ["", "@cue", "INT. ", "> "];

function tab(kind: ScriptKind) {
  return (view: EditorView): boolean => {
    const { head, empty } = view.state.selection.main;
    if (!empty) return false;
    const line = view.state.doc.lineAt(head);
    const t = line.text;
    const doc = view.state.doc;
    // Under a name (or inside a speech), an empty line becomes a parenthetical.
    if (!t.trim() && !blank(doc, line.number - 1)) {
      const above = classify(doc, line.number - 1, kind, titleEnd(doc)) ?? typing(view, kind, doc.line(line.number - 1));
      if (above === "cue" || above === "dia" || above === "paren") {
        view.dispatch({ changes: { from: line.from, to: line.to, insert: "()" }, selection: EditorSelection.cursor(line.from + 1), userEvent: "input" });
        return true;
      }
    }
    if (kind === "play") {
      // Empty → character → act → scene → plain again.
      if (!t.trim() && view.state.field(capsLine, false) !== line.from) return character(view);
      if (!t.trim() || /^#{1,2}( |$)/.test(t)) {
        view.dispatch({ effects: armCaps.of(null) });
        return sceneHeading(view, kind);
      }
      return false;
    }
    const armed = view.state.field(capsLine, false) === line.from;
    const now = !t.trim() ? (armed ? "@cue" : "") : t;
    const i = SCREEN_CYCLE.indexOf(now);
    if (i < 0) return false;
    const next = SCREEN_CYCLE[(i + 1) % SCREEN_CYCLE.length];
    if (next === "@cue") return character(view);
    view.dispatch({ changes: { from: line.from, to: line.to, insert: next }, selection: EditorSelection.cursor(line.from + next.length), userEvent: "input" });
    // A transition is typed in capitals too; back to plain action, nothing is.
    view.dispatch({ effects: armCaps.of(next === "> " ? line.from : null) });
    return true;
  };
}

export function fountainPlugin(kind: ScriptKind): Extension {
  const build = (view: EditorView): DecorationSet => {
    const b = new RangeSetBuilder<Decoration>();
    const doc = view.state.doc;
    const title = titleEnd(doc);
    for (const { from, to } of view.visibleRanges) {
      for (let n = doc.lineAt(from).number; n <= doc.lineAt(to).number; n++) {
        const c = classify(doc, n, kind, title);
        if (c) b.add(doc.line(n).from, doc.line(n).from, lineDeco(c));
      }
    }
    return b.finish();
  };
  return [
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) { this.decorations = build(view); }
        update(u: ViewUpdate) { if (u.docChanged || u.viewportChanged) this.decorations = build(u.view); }
      },
      { decorations: (v) => v.decorations },
    ),
    capsLine,
    capsInput,
    hiddenPlugin,
    Prec.high(keymap.of([
      { key: "Enter", run: enter(kind) },
      { key: "Tab", run: tab(kind) },
    ])),
  ];
}

// ---------------------------------------------------------------- commands

function current(view: EditorView): Line {
  return view.state.doc.lineAt(view.state.selection.main.head);
}

/** Replace a line's text and put the cursor at its end, with a blank line above it. */
function setLine(view: EditorView, line: Line, text: string, after = "") {
  const doc = view.state.doc;
  const needsGap = line.number > 1 && doc.line(line.number - 1).text.trim() !== "";
  const before = needsGap ? "\n" : "";
  const insert = before + text + after;
  view.dispatch({
    changes: { from: line.from, to: line.to, insert },
    selection: EditorSelection.cursor(line.from + insert.length),
    scrollIntoView: true,
    userEvent: "input.format",
  });
  view.focus();
  return true;
}

/** Screenplay: a scene heading (INT. / EXT.). Stage play: act, then scene, then plain again. */
export function sceneHeading(view: EditorView, kind: ScriptKind) {
  const line = current(view);
  const t = line.text.trim();
  if (kind === "play") {
    const m = /^(#{1,2})(?!#)\s*(.*)$/.exec(t);
    // "# ACT " on its own is the placeholder from the first press: a scene replaces it.
    const next = !m ? "# " + (t || "ACT ") : m[1] === "##" ? m[2] : "## " + (m[2] === "ACT" ? "" : m[2]);
    return setLine(view, line, next);
  }
  if (!t) return setLine(view, line, "INT. ");
  const up = t.toUpperCase();
  return setLine(view, line, SCENE.test(up) || up.startsWith(".") ? up : "." + up);
}

/** A character cue: capitals, then a new line for what they say. */
export function character(view: EditorView) {
  const line = current(view);
  const t = line.text.trim();
  if (!t) {
    setLine(view, line, "");
    view.dispatch({ effects: armCaps.of(current(view).from) });
    return true;
  }
  return setLine(view, line, t.toUpperCase(), "\n");
}

/** A parenthetical under a character: "(quietly)". */
export function parenthetical(view: EditorView) {
  const line = current(view);
  const t = line.text.trim();
  if (/^\(.*\)$/.test(t)) return true;
  const text = `(${t})`;
  view.dispatch({
    changes: { from: line.from, to: line.to, insert: text },
    selection: EditorSelection.cursor(line.from + text.length - 1),
    userEvent: "input.format",
  });
  view.focus();
  return true;
}

/** A transition on the right: "CUT TO:". */
export function transition(view: EditorView) {
  const line = current(view);
  const t = line.text.trim().replace(/^>\s*/, "").toUpperCase();
  if (!t) return setLine(view, line, "CUT TO:", "\n\n");
  return setLine(view, line, isTransition(t) ? t : "> " + t, "\n\n");
}

/** Centred text: "> THE END <". Toggles. */
export function centered(view: EditorView) {
  const line = current(view);
  const t = line.text.trim();
  const m = t.match(/^>\s*(.*?)\s*<$/);
  return setLine(view, line, m ? m[1] : `> ${t} <`);
}
