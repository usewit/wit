// iA Writer-style focus mode: no chrome, only the current paragraph at full
// strength, and the cursor line kept near the middle of the screen.

import { Compartment, type EditorState, RangeSetBuilder } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

const dimLine = Decoration.line({ class: "cm-dim" });

/// The paragraph around the cursor. The editor shows one paragraph per line.
function currentParagraph(state: EditorState): [number, number] {
  const here = state.doc.lineAt(state.selection.main.head).number;
  return [here, here];
}

function dimDecorations(view: EditorView): DecorationSet {
  const [from, to] = currentParagraph(view.state);
  const b = new RangeSetBuilder<Decoration>();
  for (const { from: vf, to: vt } of view.visibleRanges) {
    let pos = vf;
    while (pos <= vt) {
      const line = view.state.doc.lineAt(pos);
      if (line.number < from || line.number > to) b.add(line.from, line.from, dimLine);
      pos = line.to + 1;
    }
  }
  return b.finish();
}

const dimPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) { this.decorations = dimDecorations(view); }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.viewportChanged) this.decorations = dimDecorations(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

/// Typewriter scrolling: re-centre the cursor after it moves or text changes.
const typewriter = EditorView.updateListener.of((u) => {
  if (!u.docChanged && !u.selectionSet) return;
  const head = u.state.selection.main.head;
  requestAnimationFrame(() => u.view.dispatch({ effects: EditorView.scrollIntoView(head, { y: "center" }) }));
});

export const focusCompartment = new Compartment();

export function setFocusMode(view: EditorView, on: boolean) {
  document.body.classList.toggle("focus", on);
  document.body.classList.remove("reveal");
  document.getElementById("focus-toggle")?.setAttribute("aria-pressed", String(on));
  view.dispatch({
    effects: [
      focusCompartment.reconfigure(on ? [dimPlugin, typewriter] : []),
      EditorView.scrollIntoView(view.state.selection.main.head, { y: "center" }),
    ],
  });
  view.requestMeasure();
  view.focus();
}

/// Show the toolbar while the mouse is near the top edge.
export function initReveal() {
  const bar = document.querySelector<HTMLElement>(".bar")!;
  document.getElementById("focus-reveal")!.addEventListener("mouseenter", () => document.body.classList.add("reveal"));
  // Hide again once the mouse is well below the toolbar.
  document.addEventListener("mousemove", (e) => {
    if (document.body.classList.contains("reveal") && e.clientY > bar.offsetHeight + 40) {
      document.body.classList.remove("reveal");
    }
  });
}
