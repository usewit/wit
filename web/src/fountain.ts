// Screenplays and stage plays in Fountain: the editor lays each line out the
// way it will print (characters and dialogue indented, transitions on the
// right), and offers commands for the script elements.

import { EditorSelection, type Line, type Text } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";

export type ScriptKind = "screenplay" | "play";

const SCENE = /^(int|ext|est|int\.?\/ext|i\/e)[. ]/i;
const blank = (doc: Text, n: number) => n < 1 || n > doc.lines || doc.line(n).text.trim() === "";
const isCue = (t: string) => {
  const name = t.replace(/\^$/, "").split("(")[0].trim();
  return /\p{L}/u.test(name) && !/\p{Ll}/u.test(name);
};
const isTransition = (t: string) => t.endsWith("TO:") && !/\p{Ll}/u.test(t);

/** Lines of the title page at the top ("Title: …" up to the first blank line), if any. */
export function titleEnd(doc: Text): number {
  const first = doc.line(1).text;
  if (!/^[^\s:][^:]{0,19}:/.test(first) || !/\p{Ll}/u.test(first.split(":")[0])) return 0;
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
    if (head.startsWith("@") || (!blank(doc, start + 1) && isCue(head) && !SCENE.test(head) && !head.startsWith("."))) {
      return /^\(.*\)$/.test(t) ? "paren" : "dia";
    }
  }
  return kind === "play" ? "direction" : null;
}

const cache = new Map<string, Decoration>();
const lineDeco = (c: string) => cache.get(c) ?? cache.set(c, Decoration.line({ class: `cm-f-${c}` })).get(c)!;

export function fountainPlugin(kind: ScriptKind) {
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
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) { this.decorations = build(view); }
      update(u: ViewUpdate) { if (u.docChanged || u.viewportChanged) this.decorations = build(u.view); }
    },
    { decorations: (v) => v.decorations },
  );
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
    const next = t.startsWith("## ") ? t.slice(3) : t.startsWith("# ") ? "## " + t.slice(2) : "# " + (t || "ACT ");
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
  if (!t) return setLine(view, line, "");
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
