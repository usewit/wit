// The book menu under the toolbar's Open button: switch book, open a folder, start a new one.
// Drawn by Wit rather than a <select>, whose list Windows draws in its own
// (often light) colours.

export interface BookItem { id: string; title: string; dir: string; kind?: string }

const KIND_NAMES: Record<string, string> = { essay: "Essay", screenplay: "Screenplay", play: "Stage play" };

interface Handlers {
  choose(id: string): void;
  open(): void;
  create(): void;
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

const button = () => $<HTMLButtonElement>("bar-open");
const list = () => $("book-list");

function items() {
  return Array.from(list().querySelectorAll<HTMLButtonElement>("button"));
}

export function closeBookMenu(focusButton = false) {
  if (list().hidden) return;
  list().hidden = true;
  button().setAttribute("aria-expanded", "false");
  if (focusButton) button().focus();
}

function openMenu() {
  list().hidden = false;
  button().setAttribute("aria-expanded", "true");
  (list().querySelector<HTMLButtonElement>('[aria-checked="true"]') ?? items()[0])?.focus();
}

export function initBookMenu(h: Handlers) {
  button().onclick = () => (list().hidden ? openMenu() : closeBookMenu());
  button().onkeydown = (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); openMenu(); }
  };
  list().onkeydown = (e) => {
    const all = items();
    const i = all.indexOf(document.activeElement as HTMLButtonElement);
    const go = (n: number) => { e.preventDefault(); all[(n + all.length) % all.length].focus(); };
    if (e.key === "ArrowDown") go(i + 1);
    else if (e.key === "ArrowUp") go(i - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(all.length - 1);
    else if (e.key === "Escape") { e.preventDefault(); closeBookMenu(true); }
    else if (e.key === "Tab") closeBookMenu();
  };
  document.addEventListener("pointerdown", (e) => {
    if (!$("book-menu").contains(e.target as Node)) closeBookMenu();
  });
  list().onclick = (e) => {
    const b = (e.target as HTMLElement).closest<HTMLButtonElement>("button");
    if (!b) return;
    closeBookMenu(true);
    const act = b.dataset.act;
    if (act === "open") h.open();
    else if (act === "new") h.create();
    else if (b.dataset.id) h.choose(b.dataset.id);
  };
}

/** The last two folders of a path ("…/projects/my-novel"); the hover shows all of it. */
function shortDir(dir: string) {
  const parts = dir.split(/[\\/]+/).filter(Boolean);
  return parts.length > 2 ? "…/" + parts.slice(-2).join("/") : dir;
}

export function renderBookMenu(books: BookItem[], current: string) {
  const el = list();
  el.innerHTML = "";
  for (const b of books) {
    const item = document.createElement("button");
    item.setAttribute("role", "menuitemradio");
    item.setAttribute("aria-checked", String(b.id === current));
    item.dataset.id = b.id;
    item.title = b.dir;
    const t = document.createElement("span");
    t.className = "t";
    t.textContent = b.title;
    const d = document.createElement("small");
    d.textContent = (KIND_NAMES[b.kind ?? ""] ? KIND_NAMES[b.kind!] + " · " : "") + shortDir(b.dir);
    item.append(t, d);
    el.append(item);
  }
  if (books.length) el.append(document.createElement("hr"));
  for (const [act, icon, label] of [["open", "↗", "Open a folder…"], ["new", "+", "New book, essay or script…"]]) {
    const item = document.createElement("button");
    item.setAttribute("role", "menuitem");
    item.dataset.act = act;
    item.innerHTML = `<span class="i" aria-hidden="true">${icon}</span><span>${label}</span>`;
    el.append(item);
  }
}
