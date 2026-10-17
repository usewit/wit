// Every keyboard shortcut on one page: on the welcome screen, and in an
// overlay from anywhere (Alt+?). Built from the command list, so it can't
// drift from what the keys actually do.

import { keyParts, labelKbd, shortcutGroups } from "./commands";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

/** The editor's own keys, which aren't commands. */
const EDITING: [string, string[]][] = [
  ["Undo", ["Mod+Z"]],
  ["Redo", ["Mod+Shift+Z", "Mod+Y"]],
  ["Find and replace", ["Mod+F"]],
  ["Select all", ["Mod+A"]],
  ["Dialogue dash (type two hyphens)", ["-", "-"]],
  ["Script: next element", ["Enter"]],
  ["Script: choose what an empty line is", ["Tab"]],
  ["Script: a plain new line", ["Shift+Enter"]],
];

function keys(specs: string[], joiner: string): HTMLElement {
  const box = document.createElement("span");
  box.className = "sc-keys";
  specs.forEach((spec, i) => {
    if (i) {
      const or = document.createElement("span");
      or.className = "sc-or";
      or.textContent = joiner;
      box.append(or);
    }
    const combo = document.createElement("span");
    combo.className = "sc-combo";
    for (const part of keyParts(spec)) {
      const k = document.createElement("kbd");
      k.textContent = part;
      combo.append(k);
    }
    box.append(combo);
  });
  return box;
}

function group(title: string, rows: [string, HTMLElement][]): HTMLElement {
  const sec = document.createElement("section");
  sec.className = "sc-group";
  const h = document.createElement("h3");
  h.textContent = title;
  const dl = document.createElement("dl");
  for (const [name, k] of rows) {
    const div = document.createElement("div");
    const dt = document.createElement("dt");
    dt.textContent = name;
    const dd = document.createElement("dd");
    dd.append(k);
    div.append(dt, dd);
    dl.append(div);
  }
  sec.append(h, dl);
  return sec;
}

/** Fill a container with the shortcut page. */
export function renderShortcuts(box: HTMLElement) {
  box.innerHTML = "";
  for (const [category, cmds] of shortcutGroups()) {
    box.append(group(category, cmds.map((c) => [c.title, keys(c.keys!, "or")])));
  }
  box.append(group("Editing", EDITING.map(([name, k]) => [name, keys(k, name.startsWith("Dialogue") ? "then" : "or")])));
}

export function initShortcuts() {
  const dialog = $<HTMLDialogElement>("shortcuts-dialog");
  document.querySelectorAll<HTMLElement>("#welcome [data-key], #shortcuts-dialog [data-key]")
    .forEach(labelKbd);
  $("shortcuts-close").onclick = () => dialog.close();
  dialog.addEventListener("click", (e) => { if (e.target === dialog) dialog.close(); });
  return {
    open() {
      if (dialog.open) { dialog.close(); return; }
      renderShortcuts($("shortcuts-list"));
      dialog.showModal();
      $("shortcuts-close").focus();
    },
    /** The copy on the welcome screen. */
    renderWelcome: () => renderShortcuts($("welcome-shortcuts")),
  };
}
