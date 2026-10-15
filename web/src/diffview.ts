// Show what changed between two versions of the story, paragraph by paragraph,
// with the changed words marked. Unchanged stretches fold away.

import { diffArrays, diffWordsWithSpace } from "diff";

export interface DiffSummary {
  added: number;
  removed: number;
  paragraphs: number;
}

/** Paragraphs as the writer sees them: non-empty lines. */
const paragraphs = (text: string) => text.split("\n").map((l) => l.trim()).filter(Boolean);

const countWords = (s: string) => s.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu)?.length ?? 0;

/** Paragraphs of context kept on each side of a change. */
const CONTEXT = 1;

function para(kind: "same" | "ins" | "del" | "mod") {
  const p = document.createElement("p");
  p.className = `d-para d-${kind}`;
  return p;
}

function span(kind: "ins" | "del", text: string) {
  const el = document.createElement(kind);
  el.textContent = text;
  return el;
}

function folded(paras: string[], into: HTMLElement) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "d-fold";
  b.textContent = `${paras.length} unchanged paragraph${paras.length === 1 ? "" : "s"}`;
  b.onclick = () => {
    const frag = document.createDocumentFragment();
    for (const t of paras) {
      const p = para("same");
      p.textContent = t;
      frag.append(p);
    }
    b.replaceWith(frag);
  };
  into.append(b);
}

/** Render the changes from `older` to `newer` into `out`. */
export function renderDiff(older: string, newer: string, out: HTMLElement): DiffSummary {
  out.innerHTML = "";
  const sum: DiffSummary = { added: 0, removed: 0, paragraphs: 0 };
  const parts = diffArrays(paragraphs(older), paragraphs(newer));

  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    if (!part.added && !part.removed) {
      // Keep a little context next to changes; fold the rest.
      const same = part.value;
      const first = i === 0, last = i === parts.length - 1;
      const head = first ? 0 : CONTEXT;
      const tail = last ? 0 : CONTEXT;
      if (same.length <= head + tail + 1) {
        for (const t of same) { const p = para("same"); p.textContent = t; out.append(p); }
        continue;
      }
      for (const t of same.slice(0, head)) { const p = para("same"); p.textContent = t; out.append(p); }
      folded(same.slice(head, same.length - tail), out);
      for (const t of same.slice(same.length - tail)) { const p = para("same"); p.textContent = t; out.append(p); }
      continue;
    }

    // A removal followed by an addition is usually the same paragraphs edited:
    // pair them up and mark the words that changed.
    const removed = part.removed ? part.value : [];
    const added = part.removed && parts[i + 1]?.added ? parts[++i].value : part.added ? part.value : [];
    const pairs = Math.min(removed.length, added.length);
    for (let k = 0; k < pairs; k++) {
      const p = para("mod");
      for (const w of diffWordsWithSpace(removed[k], added[k])) {
        if (w.added) { sum.added += countWords(w.value); p.append(span("ins", w.value)); }
        else if (w.removed) { sum.removed += countWords(w.value); p.append(span("del", w.value)); }
        else p.append(w.value);
      }
      out.append(p);
      sum.paragraphs++;
    }
    for (const t of removed.slice(pairs)) {
      const p = para("del");
      p.append(span("del", t));
      sum.removed += countWords(t);
      sum.paragraphs++;
      out.append(p);
    }
    for (const t of added.slice(pairs)) {
      const p = para("ins");
      p.append(span("ins", t));
      sum.added += countWords(t);
      sum.paragraphs++;
      out.append(p);
    }
  }
  return sum;
}
