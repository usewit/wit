// Every action in one list: the keyboard shortcuts and the command palette
// (Ctrl+Shift+P, like VS Code) both read from it, so they can't drift apart.

export interface Command {
  id: string;
  /** Shown before the title in the palette: "View", "Format", "Theme"… */
  category: string;
  title: string;
  /** Shortcuts like "Mod+Shift+P" (Mod is Ctrl, or ⌘ on a Mac). The first is shown. */
  keys?: string[];
  /** Different shortcuts on a Mac, where macOS keeps some for itself (⌘Space is
   *  Spotlight, ⇧⌘3–5 take screenshots, ⌘M minimises). Replaces `keys` there. */
  macKeys?: string[];
  run(): void;
  /** Writing commands: their keys only act in the editor, not in other fields. */
  editor?: boolean;
  /** Shows a tick in the palette: the current theme, a toggle that's on. */
  checked?(): boolean;
  /** Hidden from the palette and its keys do nothing while false (e.g. no book open). */
  enabled?(): boolean;
}

const isMac = /Mac|iP(hone|ad)/.test(navigator.platform);

const CODES: Record<string, string[]> = {
  ",": ["Comma"], ".": ["Period"], "/": ["Slash"], "=": ["Equal", "NumpadAdd"],
  "-": ["Minus", "NumpadSubtract"], "\\": ["Backslash"], Space: ["Space"],
  "[": ["BracketLeft"], "]": ["BracketRight"],
};

interface Combo { mod: boolean; shift: boolean; alt: boolean; key: string }

function parse(spec: string): Combo {
  const parts = spec.split("+");
  // "Mod+=" splits fine; "Alt+-" too. A literal "+" key isn't used.
  const key = parts.pop()!;
  return { mod: parts.includes("Mod"), shift: parts.includes("Shift"), alt: parts.includes("Alt"), key };
}

function matches(c: Combo, e: KeyboardEvent): boolean {
  const mod = isMac ? e.metaKey : e.ctrlKey;
  // "?" needs Shift on most layouts, so Shift isn't part of its shortcut.
  const shiftOk = c.key === "?" || e.shiftKey === c.shift;
  if (mod !== c.mod || !shiftOk || e.altKey !== c.alt) return false;
  if (isMac ? e.ctrlKey : e.metaKey) return false;
  const k = c.key;
  if (/^[a-z]$/i.test(k)) return e.code === `Key${k.toUpperCase()}` || e.key.toLowerCase() === k.toLowerCase();
  if (/^\d$/.test(k)) return e.code === `Digit${k}` || e.code === `Numpad${k}`;
  if (k === "?") return e.key === "?" || (e.code === "Slash" && e.shiftKey);
  if (CODES[k]) return CODES[k].includes(e.code);
  return e.key === k;
}

/** Fill a `<kbd data-key>` with its label; `data-mac-key`, when present, wins on a Mac. */
export function labelKbd(el: HTMLElement) {
  el.textContent = keyLabel((isMac && el.dataset.macKey) || el.dataset.key!);
}

/** "Mod+Shift+P" → "Ctrl+Shift+P", or "⇧⌘P" on a Mac. */
export function keyLabel(spec: string): string {
  const c = parse(spec);
  const key = c.key.length === 1 ? c.key.toUpperCase() : c.key;
  if (isMac) return `${c.alt ? "⌥" : ""}${c.shift ? "⇧" : ""}${c.mod ? "⌘" : ""}${key}`;
  return [c.mod && "Ctrl", c.alt && "Alt", c.shift && "Shift", key].filter(Boolean).join("+");
}

const fixed: Command[] = [];
const providers: (() => Command[])[] = [];

export function addCommands(list: Command[]) {
  for (const c of list) if (isMac && c.macKeys) c.keys = c.macKeys;
  fixed.push(...list);
}
/** Commands that change with the app (one per book, for instance). */
export function addCommandSource(f: () => Command[]) { providers.push(f); }

function allCommands(): Command[] {
  return [...fixed, ...providers.flatMap((f) => f())].filter((c) => c.enabled?.() ?? true);
}

export function runCommand(id: string) {
  allCommands().find((c) => c.id === id)?.run();
}

/** A shortcut as separate keys, for keycaps: ["Ctrl", "Shift", "P"], or ["⇧", "⌘", "P"]. */
export function keyParts(spec: string): string[] {
  const c = parse(spec);
  const key = c.key.length === 1 ? c.key.toUpperCase() : c.key;
  const mods = isMac
    ? [c.alt && "⌥", c.shift && "⇧", c.mod && "⌘"]
    : [c.mod && "Ctrl", c.alt && "Alt", c.shift && "Shift"];
  return [...mods.filter((m): m is string => !!m), key];
}

/** Every command with a shortcut, grouped by category in the order they were added. */
export function shortcutGroups(): [string, Command[]][] {
  const groups = new Map<string, Command[]>();
  for (const c of fixed) {
    if (!c.keys?.length) continue;
    if (!groups.has(c.category)) groups.set(c.category, []);
    groups.get(c.category)!.push(c);
  }
  return [...groups];
}

/** The label a tooltip should carry: "Focus mode (Alt+4)". */
export function titleWithKey(id: string): string {
  const c = fixed.find((x) => x.id === id);
  if (!c) return "";
  return c.keys?.length ? `${c.title} (${keyLabel(c.keys[0])})` : c.title;
}

function inOtherField(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el || el.closest?.(".cm-editor")) return false;
  return el.matches?.("input, textarea, select, [contenteditable='true']") ?? false;
}

// ---------------------------------------------------------------- palette

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

const RECENT_KEY = "wit.recentCommands";
function recent(): string[] {
  try { return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]"); } catch { return []; }
}
function remember(id: string) {
  try { localStorage.setItem(RECENT_KEY, JSON.stringify([id, ...recent().filter((x) => x !== id)].slice(0, 5))); } catch { /* ignore */ }
}

/** Fuzzy match: every query letter in order. Returns a score and the matched positions. */
function fuzzy(text: string, q: string): { score: number; hits: number[] } | null {
  const t = text.toLowerCase();
  const hits: number[] = [];
  let score = 0;
  let from = 0;
  for (const ch of q.toLowerCase()) {
    if (ch === " ") continue;
    const i = t.indexOf(ch, from);
    if (i < 0) return null;
    // Reward letters that start a word or follow the previous hit.
    if (i === 0 || /[\s:—(-]/.test(t[i - 1])) score += 8;
    if (hits.length && i === hits[hits.length - 1] + 1) score += 5;
    score -= Math.min(i - from, 10) * 0.3;
    hits.push(i);
    from = i + 1;
  }
  if (t.includes(q.toLowerCase().trim())) score += 20;
  return { score, hits };
}

function highlight(text: string, hits: number[]): DocumentFragment {
  const f = document.createDocumentFragment();
  const set = new Set(hits);
  let run = "";
  let on = false;
  const flush = () => {
    if (!run) return;
    if (on) { const m = document.createElement("mark"); m.textContent = run; f.append(m); }
    else f.append(run);
    run = "";
  };
  [...text].forEach((ch, i) => {
    if (set.has(i) !== on) { flush(); on = set.has(i); }
    run += ch;
  });
  flush();
  return f;
}

let shown: Command[] = [];
let active = 0;

function render() {
  const q = $<HTMLInputElement>("palette-input").value;
  const list = $("palette-list");
  const cmds = allCommands();
  let rows: { c: Command; hits: number[] }[];
  if (q.trim()) {
    rows = cmds
      .map((c) => ({ c, m: fuzzy(`${c.category}: ${c.title}`, q) }))
      .filter((r) => r.m)
      .sort((a, b) => b.m!.score - a.m!.score)
      .map((r) => ({ c: r.c, hits: r.m!.hits }));
  } else {
    const rec = recent();
    const head = rec.map((id) => cmds.find((c) => c.id === id)).filter((c): c is Command => !!c);
    rows = [...head, ...cmds.filter((c) => !rec.includes(c.id))].map((c) => ({ c, hits: [] }));
  }
  shown = rows.map((r) => r.c);
  active = Math.min(active, Math.max(shown.length - 1, 0));
  list.innerHTML = "";
  const recentCount = q.trim() ? 0 : recent().filter((id) => cmds.some((c) => c.id === id)).length;
  rows.forEach(({ c, hits }, i) => {
    const li = document.createElement("li");
    li.id = `pal-${i}`;
    li.setAttribute("role", "option");
    li.setAttribute("aria-selected", String(i === active));
    // Like VS Code: "recently used" on the first row, "other commands" where the rest begin.
    const group = recentCount && i === 0 ? "recently used" : recentCount && i === recentCount ? "other commands" : "";
    if (recentCount && i === recentCount) li.classList.add("group-start");
    const label = `${c.category}: ${c.title}`;
    const name = document.createElement("span");
    name.className = "name";
    // Dim the category; matched letters stand out in either part.
    const cat = document.createElement("span");
    cat.className = "cat";
    cat.append(highlight(`${c.category}: `, hits.filter((h) => h < c.category.length + 2)));
    const title = document.createElement("span");
    title.append(highlight(c.title, hits.filter((h) => h >= c.category.length + 2).map((h) => h - c.category.length - 2)));
    name.append(cat, title);
    li.append(name);
    li.setAttribute("aria-label", label + (c.checked?.() ? ", on" : ""));
    if (c.checked?.()) {
      const tick = document.createElement("span");
      tick.className = "tick";
      tick.textContent = "✓";
      tick.setAttribute("aria-hidden", "true");
      li.append(tick);
    }
    if (group) {
      const g = document.createElement("span");
      g.className = "group";
      g.textContent = group;
      li.append(g);
    }
    if (c.keys?.length) {
      const keys = document.createElement("span");
      keys.className = "keys";
      keys.setAttribute("aria-label", keyLabel(c.keys[0]));
      for (const part of keyParts(c.keys[0])) {
        const k = document.createElement("kbd");
        k.textContent = part;
        keys.append(k);
      }
      li.append(keys);
    }
    li.onmousemove = () => { if (active !== i) { active = i; mark(); } };
    li.onclick = () => choose(i);
    list.append(li);
  });
  if (!rows.length) {
    const li = document.createElement("li");
    li.className = "none";
    li.textContent = "No matching command";
    list.append(li);
  }
  mark();
}

function mark() {
  const list = $("palette-list");
  list.querySelectorAll("[role=option]").forEach((el, i) => el.setAttribute("aria-selected", String(i === active)));
  const el = $(`pal-${active}`);
  $("palette-input").setAttribute("aria-activedescendant", el ? el.id : "");
  el?.scrollIntoView({ block: "nearest" });
}

function choose(i: number) {
  const c = shown[i];
  if (!c) return;
  closePalette();
  remember(c.id);
  // Let the dialog close (and focus go back) before the command runs.
  requestAnimationFrame(() => c.run());
}

export function openPalette() {
  const d = $<HTMLDialogElement>("palette");
  if (d.open) return closePalette();
  try { localStorage.setItem("wit.paletteUsed", "1"); } catch { /* ignore */ }
  const input = $<HTMLInputElement>("palette-input");
  input.value = "";
  active = 0;
  render();
  d.showModal();
  input.focus();
}

export function closePalette() {
  const d = $<HTMLDialogElement>("palette");
  if (d.open) d.close();
}

export function initCommands() {
  const input = $<HTMLInputElement>("palette-input");
  input.oninput = () => { active = 0; render(); };
  input.onkeydown = (e) => {
    const n = shown.length;
    if (e.key === "ArrowDown") { e.preventDefault(); active = (active + 1) % Math.max(n, 1); mark(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); active = (active - 1 + n) % Math.max(n, 1); mark(); }
    else if (e.key === "PageDown") { e.preventDefault(); active = Math.min(n - 1, active + 8); mark(); }
    else if (e.key === "PageUp") { e.preventDefault(); active = Math.max(0, active - 8); mark(); }
    else if (e.key === "Enter") { e.preventDefault(); choose(active); }
  };
  // Clicking the dimmed backdrop closes it.
  $("palette").addEventListener("click", (e) => { if (e.target === e.currentTarget) closePalette(); });

  // Shortcuts are caught before the editor sees them, so they work anywhere.
  window.addEventListener("keydown", (e) => {
    if (e.isComposing) return;
    const modal = document.querySelector("dialog[open]");
    for (const c of allCommands()) {
      if (!c.keys?.some((k) => matches(parse(k), e))) continue;
      // An open dialog keeps its keys, except that the palette's key closes the palette.
      if (modal && !(c.id === "palette" && modal.id === "palette")) return;
      if (c.editor && inOtherField(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      c.run();
      return;
    }
  }, true);
}
