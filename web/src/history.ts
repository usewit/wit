// Revisions: name one, browse them and the daily copies, see what changed,
// go back to one.

import { renderDiff } from "./diffview";

export interface Revision { n: number; name: string; ms: number }

/** A named revision ("v3") or a daily copy of the working draft ("2026-10-12"). */
interface Entry { id: string; kind: "named" | "daily"; n: number; name: string; ms: number; words: number }

interface Deps {
  /** A new revision was named. */
  named(r: Revision): void;
  bookId(): string;
  /** Save pending edits, so the newest version is on disk. */
  flush(): Promise<void>;
  /** The text as it is now, in file form. */
  currentText(): string;
  baseHash(): string;
  /** Apply a successful restore (the server's response). */
  restored(data: { source: string; hash: string; revision: Revision | null; preview: unknown }): void;
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const numberFmt = new Intl.NumberFormat();
const dateFmt = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short" });

/** "v3", or "Mon 12 Oct" for a daily copy. */
function label(e: Entry): string {
  return e.kind === "named" ? `v${e.n}` : dayFmt.format(new Date(`${e.id}T12:00`));
}

export function initHistory(d: Deps) {
  const dialog = $<HTMLDialogElement>("history-dialog");
  const list = $("history-list");
  const textEl = $("history-text");
  const diffEl = $("history-diff");
  const summary = $("history-summary");
  const tools = $("history-tools");
  const restoreBtn = $<HTMLButtonElement>("history-restore");
  const confirmBox = $("history-confirm");
  const errorEl = $("history-error");

  let snaps: Entry[] = [];
  let sel = -1;
  let mode: "text" | "changes" = "text";
  let against: "prev" | "now" = "prev";
  const texts = new Map<string, Promise<string>>();

  const nextN = () => Math.max(0, ...snaps.map((s) => s.n)) + 1;

  function text(id: string) {
    if (!texts.has(id)) {
      texts.set(id, fetch(`/api/books/${d.bookId()}/history/${encodeURIComponent(id)}`)
        .then(async (r) => (r.ok ? (await r.json()).text : Promise.reject(new Error("Couldn't open this revision."))))
        .catch((e) => { texts.delete(id); throw e; }));
    }
    return texts.get(id)!;
  }

  function setPressed(attr: string, value: string) {
    document.querySelectorAll<HTMLButtonElement>(`[${attr}]`).forEach((b) =>
      b.setAttribute("aria-pressed", String(b.getAttribute(attr) === value)));
  }

  function showError(msg: string | null) {
    errorEl.hidden = !msg;
    errorEl.textContent = msg ?? "";
  }

  function asking(on: boolean) {
    confirmBox.hidden = !on;
    restoreBtn.closest("menu")!.hidden = on;
  }

  let shown = 0;

  async function show() {
    const ticket = ++shown; // a newer choice wins over a slower, older load
    asking(false);
    showError(null);
    const none = sel < 0;
    tools.hidden = none;
    restoreBtn.disabled = none;
    $("history-against").hidden = mode !== "changes";
    if (none) {
      textEl.hidden = false;
      diffEl.hidden = summary.hidden = true;
      textEl.textContent = "Choose a revision on the left.";
      textEl.classList.add("placeholder");
      return;
    }
    const s = snaps[sel];
    try {
      if (mode === "text") {
        const t = await text(s.id);
        if (ticket !== shown) return;
        textEl.textContent = t;
        textEl.classList.remove("placeholder");
        textEl.hidden = false;
        diffEl.hidden = summary.hidden = true;
        return;
      }
      let older: string, newer: string, heading: string;
      if (against === "prev") {
        const prev = snaps[sel + 1];
        older = prev ? await text(prev.id) : "";
        newer = await text(s.id);
        heading = prev ? `From ${label(prev)} to ${label(s)}` : `${label(s)} is the oldest, so all of it is shown as new`;
      } else {
        older = await text(s.id);
        newer = d.currentText();
        heading = `From ${label(s)} to the working draft`;
      }
      if (ticket !== shown) return;
      const sum = renderDiff(older, newer, diffEl);
      const words = [
        sum.added ? `<ins>+${numberFmt.format(sum.added)}</ins>` : "",
        sum.removed ? `<del>−${numberFmt.format(sum.removed)}</del>` : "",
      ].filter(Boolean).join(" ");
      const counts = sum.paragraphs === 0
        ? "No changes"
        : `${words ? `${words} words · ` : ""}${sum.paragraphs} paragraph${sum.paragraphs === 1 ? "" : "s"} changed`;
      summary.innerHTML = `<span>${heading}</span><span>${counts}</span>`;
      if (sum.paragraphs === 0) diffEl.innerHTML = `<p class="empty-note"><strong>Nothing changed</strong>The two are the same.</p>`;
      textEl.hidden = true;
      diffEl.hidden = summary.hidden = false;
    } catch (e) {
      if (ticket !== shown) return;
      textEl.hidden = false;
      diffEl.hidden = summary.hidden = true;
      textEl.textContent = (e as Error).message;
      textEl.classList.add("placeholder");
    }
  }

  function select(i: number, focus = false) {
    sel = i;
    const buttons = list.querySelectorAll<HTMLButtonElement>("button[data-i]");
    buttons.forEach((b) => b.setAttribute("aria-pressed", String(Number(b.dataset.i) === i)));
    if (focus) list.querySelector<HTMLButtonElement>(`button[data-i="${i}"]`)?.focus();
    show();
  }

  const nameInput = $<HTMLInputElement>("name-input");

  async function load() {
    sel = -1;
    texts.clear();
    show();
    const res = await fetch(`/api/books/${d.bookId()}/history`);
    snaps = res.ok ? await res.json() : [];
    $("name-next").textContent = `v${nextN()}`;
    list.innerHTML = "";
    document.querySelector(".history")!.classList.toggle("is-empty", snaps.length === 0);
    if (snaps.length === 0) {
      list.innerHTML = `<li class="empty-note"><strong>No revisions yet</strong>
        Name one above whenever the draft reaches a point you want to keep.</li>`;
      return;
    }
    snaps.forEach((s, i) => {
      if (s.kind !== snaps[i - 1]?.kind) {
        const head = document.createElement("li");
        head.className = "group-head";
        head.textContent = s.kind === "named" ? "Named revisions" : "Daily copies";
        list.append(head);
      }
      const older = snaps[i + 1];
      const delta = older ? s.words - older.words : 0;
      const li = document.createElement("li");
      const b = document.createElement("button");
      b.dataset.i = String(i);
      b.setAttribute("aria-pressed", "false");
      const title = s.kind === "named"
        ? `<b>v${s.n}</b> · ${escapeHtml(s.name)}`
        : `<b>${label(s)}</b>`;
      const when = s.kind === "named" ? `${dateFmt.format(new Date(s.ms))} · ` : "";
      b.innerHTML = `<span>${title}</span>
        <small>${when}${numberFmt.format(s.words)} words${delta ? ` · ${delta > 0 ? "+" : "−"}${numberFmt.format(Math.abs(delta))}` : ""}</small>`;
      b.onclick = () => select(i);
      li.append(b);
      list.append(li);
    });
  }

  const open = async (naming = false) => {
    // Alt+H again closes it.
    if (dialog.open && !naming) { dialog.close(); return; }
    await d.flush();
    list.innerHTML = "";
    nameInput.value = "";
    if (!dialog.open) dialog.showModal();
    if (naming) nameInput.focus();
    await load();
  };

  $<HTMLFormElement>("name-form").onsubmit = async (e) => {
    e.preventDefault();
    const btn = $<HTMLFormElement>("name-form").querySelector<HTMLButtonElement>("button")!;
    btn.disabled = true;
    showError(null);
    try {
      await d.flush();
      const res = await fetch(`/api/books/${d.bookId()}/history`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: nameInput.value, base_hash: d.baseHash() }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || res.statusText);
      d.named(data.revision);
      nameInput.value = "";
      await load();
      select(0, true);
    } catch (err) {
      showError(`Couldn't name the revision: ${(err as Error).message}`);
    } finally {
      btn.disabled = false;
    }
  };

  // Up and down move through the versions.
  list.onkeydown = (e) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const n = Math.max(0, Math.min(snaps.length - 1, sel + (e.key === "ArrowDown" ? 1 : -1)));
    select(n, true);
  };

  document.querySelectorAll<HTMLButtonElement>("[data-hmode]").forEach((b) => (b.onclick = () => {
    mode = b.dataset.hmode as typeof mode;
    setPressed("data-hmode", mode);
    show();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-against]").forEach((b) => (b.onclick = () => {
    against = b.dataset.against as typeof against;
    setPressed("data-against", against);
    show();
  }));

  $("history-close").onclick = () => dialog.close();
  // Esc while "Make this your working draft?" is asked only takes the question back.
  dialog.addEventListener("cancel", (e) => {
    if (confirmBox.hidden) return;
    e.preventDefault();
    asking(false);
    restoreBtn.focus();
  });
  dialog.addEventListener("click", (e) => { if (e.target === dialog) dialog.close(); });

  // Restoring asks first, inside the dialog.
  restoreBtn.onclick = () => {
    if (sel < 0) return;
    $("history-confirm-text").innerHTML =
      `Make <b>${label(snaps[sel])}</b> your working draft? The draft you have now is kept first, as <b>v${nextN()}</b>.`;
    asking(true);
    $("history-confirm-no").focus();
  };
  $("history-confirm-no").onclick = () => { asking(false); restoreBtn.focus(); };
  $("history-confirm-yes").onclick = async () => {
    const yes = $<HTMLButtonElement>("history-confirm-yes");
    yes.disabled = true;
    try {
      const res = await fetch(`/api/books/${d.bookId()}/history/${encodeURIComponent(snaps[sel].id)}/restore`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ base_hash: d.baseHash() }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || res.statusText);
      d.restored(data);
      dialog.close();
    } catch (e) {
      asking(false);
      showError(`Couldn't restore: ${(e as Error).message}`);
    } finally {
      yes.disabled = false;
    }
  };

  return { open };
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}
