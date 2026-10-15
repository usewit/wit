// The Changes view: in Split mode, the right pane can show the chapter you're
// writing in with everything changed since the latest named revision marked
// (added words underlined, removed ones struck through), instead of the pages.

import type { EditorView } from "@codemirror/view";
import { diffArrays, diffWordsWithSpace } from "diff";
import type { Revision } from "./history";
import type { EditorText } from "./paras";
import { store } from "./store";

interface Deps {
  view: EditorView;
  bookId(): string;
  revision(): Revision | null;
  isScript(): boolean;
  /** The file's text as the editor shows it (one line per paragraph for prose). */
  fromFile(src: string): EditorText;
  goTo(line: number): void;
  nameRevision(): void;
}

/** One paragraph of the current text, or one that was removed. */
interface Block {
  /** Editor line; a removed paragraph sits half a line after the one before it. */
  line: number;
  kind: "same" | "mod" | "ins" | "del";
  /** Plain text, or for "mod" the word-level changes. */
  parts: { text: string; mark?: "ins" | "del" }[];
}

interface Chapter { line: number; title: string; added: number; removed: number }

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const numberFmt = new Intl.NumberFormat();
const countWords = (s: string) => s.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu)?.length ?? 0;

/** Where a chapter (or, in a script, an act or scene) begins. */
function isChapterStart(line: string, script: boolean): boolean {
  if (!script) return /^#\s/.test(line);
  return /^#(?!#)/.test(line) || /^\.[^.]/.test(line) || /^(INT|EXT|EST|INT\.?\/EXT|I\/E)[ .]/i.test(line);
}

function compare(older: string[], newer: { line: number; text: string }[]): Block[] {
  const blocks: Block[] = [];
  const parts = diffArrays(older, newer.map((p) => p.text));
  let ni = 0; // the next paragraph of the current text
  const prevLine = () => (ni > 0 ? newer[ni - 1].line : 0) + 0.5;
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    if (!part.added && !part.removed) {
      for (const text of part.value) blocks.push({ line: newer[ni++].line, kind: "same", parts: [{ text }] });
      continue;
    }
    // A removal followed by an addition is usually the same paragraphs edited.
    const removed = part.removed ? part.value : [];
    const added = part.removed && parts[i + 1]?.added ? parts[++i].value : part.added ? part.value : [];
    const pairs = Math.min(removed.length, added.length);
    for (let k = 0; k < pairs; k++) {
      const words = diffWordsWithSpace(removed[k], added[k]).map((w) => ({
        text: w.value, mark: w.added ? "ins" as const : w.removed ? "del" as const : undefined,
      }));
      blocks.push({ line: newer[ni++].line, kind: "mod", parts: words });
    }
    for (const text of removed.slice(pairs)) blocks.push({ line: prevLine(), kind: "del", parts: [{ text, mark: "del" }] });
    for (const text of added.slice(pairs)) blocks.push({ line: newer[ni++].line, kind: "ins", parts: [{ text, mark: "ins" }] });
  }
  return blocks;
}

export function initChanges(d: Deps) {
  const pane = document.querySelector<HTMLElement>(".preview-pane")!;
  const body = $("changes");
  const since = $("changes-since");
  let on = store.get("rightPane") === "changes";
  let older: { key: string; lines: string[] } | null = null;
  let blocks: Block[] = [];
  let chapters: Chapter[] = [];
  let shownChapter = -1;
  let timer: number | undefined;

  function setOn(v: boolean) {
    on = v;
    store.set("rightPane", v ? "changes" : "pages");
    pane.dataset.view = v ? "changes" : "pages";
    document.querySelectorAll<HTMLButtonElement>("[data-pane]").forEach((b) =>
      b.setAttribute("aria-pressed", String((b.dataset.pane === "changes") === v)));
    if (v) refresh();
  }

  /** The latest revision's text, fetched once per revision. */
  async function olderLines(r: Revision): Promise<string[]> {
    const key = `${d.bookId()}/v${r.n}`;
    if (older?.key !== key) {
      const res = await fetch(`/api/books/${d.bookId()}/history/v${r.n}`);
      if (!res.ok) throw new Error("Couldn't open the latest revision.");
      const lines = d.fromFile((await res.json()).text).text.split("\n").map((l) => l.trim()).filter(Boolean);
      older = { key, lines };
    }
    return older.lines;
  }

  function cursorLine(): number {
    return d.view.state.doc.lineAt(d.view.state.selection.main.head).number;
  }

  function chapterAt(line: number): number {
    let c = 0;
    chapters.forEach((ch, i) => { if (ch.line <= line) c = i; });
    return c;
  }

  let ticket = 0;
  async function refresh() {
    if (!on) return;
    const mine = ++ticket;
    const r = d.revision();
    if (!r) {
      since.textContent = "";
      body.innerHTML = `<div class="empty-note"><strong>No named revision yet</strong>
        Changes are shown since the latest named revision.
        <button type="button" class="link-btn" id="changes-name">Name a revision…</button></div>`;
      $("changes-name").onclick = () => d.nameRevision();
      return;
    }
    since.textContent = `Since v${r.n} · ${r.name}`;
    let old: string[];
    try {
      old = await olderLines(r);
    } catch (e) {
      body.innerHTML = `<p class="empty-note">${(e as Error).message}</p>`;
      return;
    }
    if (mine !== ticket) return;

    const doc = d.view.state.doc;
    const script = d.isScript();
    const newer: { line: number; text: string }[] = [];
    chapters = [{ line: 1, title: "Opening", added: 0, removed: 0 }];
    for (let n = 1; n <= doc.lines; n++) {
      const text = doc.line(n).text.trim();
      if (!text) continue;
      if (isChapterStart(text, script)) {
        const title = text.replace(/^#+\s*|^\./, "") || "Untitled";
        // A chapter heading on the first line replaces the "Opening" stretch.
        if (newer.length === 0) chapters[0] = { line: n, title, added: 0, removed: 0 };
        else chapters.push({ line: n, title, added: 0, removed: 0 });
      }
      newer.push({ line: n, text });
    }
    blocks = compare(old, newer);
    for (const b of blocks) {
      const ch = chapters[chapterAt(b.line)];
      for (const p of b.parts) {
        if (p.mark === "ins") ch.added += countWords(p.text);
        if (p.mark === "del") ch.removed += countWords(p.text);
      }
    }
    shownChapter = -1;
    render();
  }

  function counts(added: number, removed: number): string {
    return [
      added ? `<ins>+${numberFmt.format(added)}</ins>` : "",
      removed ? `<del>−${numberFmt.format(removed)}</del>` : "",
    ].filter(Boolean).join(" ");
  }

  function render() {
    const cur = chapterAt(cursorLine());
    shownChapter = cur;
    const scrollTop = pane.scrollTop;
    body.innerHTML = "";

    const changed = chapters.filter((c) => c.added || c.removed);
    const total = changed.reduce((a, c) => [a[0] + c.added, a[1] + c.removed], [0, 0]);
    const head = document.createElement("div");
    head.className = "changes-head";
    if (!changed.length) {
      head.innerHTML = `<p class="changes-none">No changes yet.</p>`;
    } else {
      const sum = document.createElement("p");
      sum.className = "changes-total";
      sum.innerHTML = `${counts(total[0], total[1])} words in ${changed.length} ${chapters.length > 1 ? (changed.length === 1 ? "part" : "parts") : "place"}`;
      head.append(sum);
      if (chapters.length > 1) {
        const nav = document.createElement("div");
        nav.className = "changes-chapters";
        nav.setAttribute("aria-label", "Parts with changes");
        for (const c of changed) {
          const i = chapters.indexOf(c);
          const b = document.createElement("button");
          b.type = "button";
          if (i === cur) b.setAttribute("aria-current", "true");
          b.innerHTML = `<span></span> ${counts(c.added, c.removed)}`;
          b.querySelector("span")!.textContent = c.title;
          b.onclick = () => d.goTo(c.line);
          nav.append(b);
        }
        head.append(nav);
      }
    }
    body.append(head);

    const ch = chapters[cur];
    const end = chapters[cur + 1]?.line ?? Infinity;
    const text = document.createElement("div");
    text.className = "changes-text";
    for (const b of blocks) {
      if (b.line < ch.line || b.line >= end) continue;
      const p = document.createElement("p");
      p.className = `c-${b.kind}`;
      p.dataset.line = String(Math.ceil(b.line));
      for (const part of b.parts) {
        if (part.mark) {
          const m = document.createElement(part.mark);
          m.textContent = part.text;
          p.append(m);
        } else p.append(part.text);
      }
      p.onclick = () => d.goTo(Math.ceil(b.line));
      text.append(p);
    }
    body.append(text);
    pane.scrollTop = scrollTop;
    follow();
  }

  /** Keep the paragraph the cursor is in on screen, and show which one it is. */
  function follow() {
    const line = cursorLine();
    let el: HTMLElement | null = null;
    body.querySelectorAll<HTMLElement>(".changes-text p:not(.c-del)").forEach((p) => {
      p.classList.remove("here");
      if (Number(p.dataset.line) <= line) el = p;
    });
    if (el) {
      (el as HTMLElement).classList.add("here");
      (el as HTMLElement).scrollIntoView({ block: "nearest" });
    }
  }

  document.querySelectorAll<HTMLButtonElement>("[data-pane]").forEach((b) => (b.onclick = () => setOn(b.dataset.pane === "changes")));
  setOn(on);

  return {
    isOn: () => on,
    setOn,
    /** The text changed: compare again once typing pauses. */
    docChanged() {
      if (!on) return;
      clearTimeout(timer);
      timer = window.setTimeout(refresh, 400);
    },
    cursorMoved() {
      if (!on || !blocks.length) return;
      if (chapterAt(cursorLine()) !== shownChapter) render();
      else follow();
    },
    /** A new revision, a restore, or another book. */
    revisionChanged() {
      older = null;
      refresh();
    },
  };
}
