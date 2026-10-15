// Markdown formatting for the dock and the keyboard: bold, italic, underline,
// lists and links. Every command toggles, so pressing it again undoes it.

import { EditorSelection, type Line } from "@codemirror/state";
import { Decoration, EditorView, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

/// Wrap the selection in `open`…`close`, or unwrap it if it already is. With
/// nothing selected, insert the pair and put the cursor between.
export function toggleWrap(view: EditorView, open: string, close = open): boolean {
  const { state } = view;
  const tr = state.changeByRange((r) => {
    const before = state.sliceDoc(r.from - open.length, r.from);
    const after = state.sliceDoc(r.to, r.to + close.length);
    // A "*" that belongs to "**" is bold, not italic ("***" is both).
    const lone = open !== "*" || state.sliceDoc(r.from - 2, r.from) !== "**" || state.sliceDoc(r.from - 3, r.from) === "***";
    if (before === open && after === close && lone) {
      return {
        changes: [{ from: r.from - open.length, to: r.from }, { from: r.to, to: r.to + close.length }],
        range: EditorSelection.range(r.from - open.length, r.to - open.length),
      };
    }
    const inner = state.sliceDoc(r.from, r.to);
    if (inner.startsWith(open) && inner.endsWith(close) && inner.length >= open.length + close.length) {
      return {
        changes: { from: r.from, to: r.to, insert: inner.slice(open.length, inner.length - close.length) },
        range: EditorSelection.range(r.from, r.to - open.length - close.length),
      };
    }
    return {
      changes: [{ from: r.from, insert: open }, { from: r.to, insert: close }],
      range: EditorSelection.range(r.from + open.length, r.to + open.length),
    };
  });
  view.dispatch(state.update(tr, { scrollIntoView: true, userEvent: "input.format" }));
  view.focus();
  return true;
}

const BULLET = /^[-*+] /;
const NUMBER = /^(\d+)[.)] /;

function selectedLines(view: EditorView): Line[] {
  const { doc, selection } = view.state;
  const seen = new Set<number>();
  const out: Line[] = [];
  for (const r of selection.ranges) {
    for (let n = doc.lineAt(r.from).number; n <= doc.lineAt(r.to).number; n++) {
      if (!seen.has(n)) { seen.add(n); out.push(doc.line(n)); }
    }
  }
  return out.sort((a, b) => a.number - b.number);
}

/// Make the selected lines a bulleted or numbered list, or plain paragraphs
/// again if they already are that kind of list.
export function toggleList(view: EditorView, kind: "bullet" | "number"): boolean {
  const lines = selectedLines(view);
  const re = kind === "bullet" ? BULLET : NUMBER;
  const all = lines.every((l) => re.test(l.text));
  // A numbered list continues the count of the item just above it.
  const prev = lines[0].number > 1 ? view.state.doc.line(lines[0].number - 1).text.match(NUMBER) : null;
  let n = prev ? Number(prev[1]) : 0;
  const changes = lines.map((l) => {
    const old = l.text.match(BULLET) ?? l.text.match(NUMBER);
    const oldLen = old ? old[0].length : 0;
    const insert = all ? "" : kind === "bullet" ? "- " : `${++n}. `;
    return { from: l.from, to: l.from + oldLen, insert };
  });
  const set = view.state.changes(changes);
  // Keep a cursor at the start of a line after the new "- ", not before it.
  view.dispatch({ changes: set, selection: view.state.selection.map(set, 1), scrollIntoView: true, userEvent: "input.format" });
  view.focus();
  return true;
}

/// Enter on a list item starts the next item; Enter on an empty item ends the list.
export function continueList(view: EditorView): boolean {
  const { state } = view;
  const r = state.selection.main;
  if (!r.empty) return false;
  const line = state.doc.lineAt(r.head);
  const m = line.text.match(BULLET) ?? line.text.match(NUMBER);
  if (!m || r.head < line.from + m[0].length) return false;
  if (line.text.trim() === m[0].trim()) {
    view.dispatch({ changes: { from: line.from, to: line.to }, selection: { anchor: line.from } });
    return true;
  }
  const next = m[1] ? `${Number(m[1]) + 1}. ` : m[0];
  const insert = "\n" + next;
  view.dispatch({ changes: { from: r.head, insert }, selection: { anchor: r.head + insert.length }, scrollIntoView: true, userEvent: "input" });
  return true;
}

/// Turn the selection into a link. Selected text becomes the link's words and
/// the cursor waits for the address; a selected address waits for the words.
export function insertLink(view: EditorView): boolean {
  const { state } = view;
  const r = state.selection.main;
  const sel = state.sliceDoc(r.from, r.to);
  const isUrl = /^(https?:\/\/|www\.|mailto:)\S+$/i.test(sel.trim());
  const insert = isUrl ? `[](${sel.trim()})` : `[${sel}]()`;
  const cursor = isUrl ? r.from + 1 : r.from + sel.length + 3;
  view.dispatch({ changes: { from: r.from, to: r.to, insert }, selection: { anchor: cursor }, scrollIntoView: true, userEvent: "input.format" });
  view.focus();
  return true;
}

// Show <u>…</u> underlined, with the tags themselves quiet.
const tagMark = Decoration.mark({ class: "cm-tag" });
const underMark = Decoration.mark({ class: "cm-u" });
const underlineMatcher = new MatchDecorator({
  regexp: /<u>(.+?)<\/u>/gi,
  decorate(add, from, to) {
    add(from, from + 3, tagMark);
    add(from + 3, to - 4, underMark);
    add(to - 4, to, tagMark);
  },
});
export const underlinePlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = underlineMatcher.createDeco(view); }
    update(u: ViewUpdate) { this.decorations = underlineMatcher.updateDeco(u, this.decorations); }
  },
  { decorations: (v) => v.decorations },
);
