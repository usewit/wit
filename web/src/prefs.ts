// How the editor looks on this computer: theme, font, text size, spell check,
// and whether the toolbar and the formatting dock are showing. Stored per
// viewer; nothing here changes the printed book.

import { Compartment } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { Command } from "./commands";
import { keyLabel, labelKbd } from "./commands";
import { store } from "./store";

export type Theme = "light" | "dark" | "oled" | "system";
export const THEMES: { id: Theme; name: string }[] = [
  { id: "light", name: "Light" },
  { id: "dark", name: "Dark" },
  { id: "oled", name: "Dark OLED" },
  { id: "system", name: "Follow system" },
];

type Kind = "sans" | "serif" | "mono";
export const KINDS: { id: Kind; name: string }[] = [
  { id: "sans", name: "Sans serif" },
  { id: "serif", name: "Serif" },
  { id: "mono", name: "Monospace" },
];
/// `light`: the font has a Light (300) weight, used by the "Lighter text" setting.
export const FONTS: { id: string; name: string; kind: Kind; stack: string; light: boolean }[] = [
  { id: "atkinson", name: "Atkinson Hyperlegible", kind: "sans", stack: `"Atkinson Hyperlegible", system-ui, sans-serif`, light: false },
  { id: "inter", name: "Inter", kind: "sans", stack: `"Inter", system-ui, sans-serif`, light: true },
  { id: "source-sans", name: "Source Sans 3", kind: "sans", stack: `"Source Sans 3", system-ui, sans-serif`, light: true },
  { id: "literata", name: "Literata", kind: "serif", stack: `"Literata", Georgia, serif`, light: true },
  { id: "newsreader", name: "Newsreader", kind: "serif", stack: `"Newsreader", Georgia, serif`, light: true },
  { id: "merriweather", name: "Merriweather", kind: "serif", stack: `"Merriweather", Georgia, serif`, light: true },
  { id: "plex-mono", name: "IBM Plex Mono", kind: "mono", stack: `"IBM Plex Mono", ui-monospace, monospace`, light: true },
  { id: "jetbrains", name: "JetBrains Mono", kind: "mono", stack: `"JetBrains Mono", ui-monospace, monospace`, light: true },
  { id: "courier-prime", name: "Courier Prime", kind: "mono", stack: `"Courier Prime", "Courier New", monospace`, light: false },
];

export const SIZES = [16, 18, 21, 24, 28, 32, 38];

/// Line lengths for the editor, in em of the page (16 px); null fills the screen.
export const WIDTHS: { name: string; em: number | null }[] = [
  { name: "Very narrow", em: 30 },
  { name: "Narrow", em: 36 },
  { name: "Medium", em: 44 },
  { name: "Wide", em: 54 },
  { name: "Wider", em: 66 },
  { name: "Very wide", em: 80 },
  { name: "Full width", em: null },
];
const DEFAULT_WIDTH = 2;
const DEFAULT_SIZE = 21;

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

export const spellCompartment = new Compartment();
export const spellOn = () => store.get("spell") !== "0";
export const spellExtension = (on: boolean) =>
  EditorView.contentAttributes.of({ spellcheck: String(on), autocorrect: "off" });

interface Deps {
  view: EditorView;
  openPageSetup(): void;
  /** Called when the toolbar or dock appears or goes, so the layout can settle. */
  changed(): void;
  /** Preview pages changed look (dark or print colours): draw them again. */
  pagesChanged(): void;
}

export function initPrefs(d: Deps) {
  const root = document.documentElement;

  // ---------------------------------------------------------------- theme
  const theme = (): Theme => (root.dataset.theme as Theme | undefined) ?? "system";
  function setTheme(t: Theme) {
    if (t === "system") { delete root.dataset.theme; store.del("theme"); }
    else { root.dataset.theme = t; store.set("theme", t); }
    syncPages();
    sync();
  }

  // ---------------------------------------------------------------- page previews
  // In a dark theme the preview pages can be dark too, so a white page doesn't
  // glare. Only the preview: Print and Proof PDFs keep their real colours.
  const darkPagesOn = () => store.get("darkPages") !== "0";
  const systemDark = matchMedia("(prefers-color-scheme: dark)");
  function pageLook(): "print" | "dark" | "oled" {
    if (!darkPagesOn()) return "print";
    const t = theme();
    if (t === "oled") return "oled";
    return t === "dark" || (t === "system" && systemDark.matches) ? "dark" : "print";
  }
  let sentLook = "";
  async function syncPages() {
    const look = pageLook();
    if (look === sentLook) return;
    sentLook = look;
    try {
      await fetch("/api/view", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ pages: look }) });
    } catch { /* the preview keeps its colours */ }
    d.pagesChanged();
  }
  function setDarkPages(on: boolean) {
    store.set("darkPages", on ? "1" : "0");
    syncPages();
    sync();
  }
  systemDark.addEventListener("change", () => syncPages());
  function nextTheme() {
    const order: Theme[] = ["light", "dark", "oled"];
    const dark = matchMedia("(prefers-color-scheme: dark)").matches;
    const now = theme() === "system" ? (dark ? "dark" : "light") : theme();
    setTheme(order[(order.indexOf(now) + 1) % order.length]);
  }

  // ---------------------------------------------------------------- font and size
  const font = () => FONTS.find((f) => f.id === store.get("font")) ?? FONTS[0];
  function setFont(id: string) {
    const f = FONTS.find((x) => x.id === id) ?? FONTS[0];
    store.set("font", f.id);
    root.style.setProperty("--editor-font", f.stack);
    // Monospace fonts run wide; a little less line spacing keeps them even.
    root.dataset.editorFont = f.kind;
    applyLight();
    d.view.requestMeasure();
    sync();
  }

  // Lighter text: the font's Light weight instead of Regular, where it has one.
  const lightOn = () => store.get("light") === "1";
  function applyLight() {
    root.toggleAttribute("data-light-text", lightOn() && font().light);
  }
  function setLight(on: boolean) {
    store.set("light", on ? "1" : "0");
    applyLight();
    sync();
  }
  let size = Number(store.get("fontSize")) || DEFAULT_SIZE;
  if (!SIZES.includes(size)) size = DEFAULT_SIZE;
  function setSize(px: number) {
    size = Math.min(SIZES[SIZES.length - 1], Math.max(SIZES[0], px));
    root.style.setProperty("--editor-size", `${size}px`);
    store.set("fontSize", String(size));
    d.view.requestMeasure();
    sync();
  }
  const step = (dir: 1 | -1) => {
    const i = SIZES.indexOf(size);
    setSize(SIZES[Math.min(SIZES.length - 1, Math.max(0, i + dir))]);
  };

  // ---------------------------------------------------------------- text width
  let width = Number(store.get("width") ?? DEFAULT_WIDTH);
  if (!(width >= 0 && width < WIDTHS.length)) width = DEFAULT_WIDTH;
  function setWidth(i: number) {
    width = Math.min(WIDTHS.length - 1, Math.max(0, i));
    const em = WIDTHS[width].em;
    root.style.setProperty("--editor-width", em ? `${em}em` : "none");
    store.set("width", String(width));
    d.view.requestMeasure();
    sync();
  }

  // ---------------------------------------------------------------- spell check
  function setSpell(on: boolean) {
    store.set("spell", on ? "1" : "0");
    d.view.dispatch({ effects: spellCompartment.reconfigure(spellExtension(on)) });
    sync();
  }

  // ---------------------------------------------------------------- toolbar and dock
  // The toolbar starts hidden: the palette (Ctrl+Shift+P) reaches everything.
  const barOn = () => store.get("bar") === "1";
  const dockOn = () => store.get("dock") !== "0";
  function setBar(on: boolean) {
    store.set("bar", on ? "1" : "0");
    document.body.classList.toggle("bar-hidden", !on);
    d.changed();
    sync();
  }
  function setDock(on: boolean) {
    store.set("dock", on ? "1" : "0");
    document.body.classList.toggle("dock-hidden", !on);
    d.changed();
    sync();
  }

  // ---------------------------------------------------------------- settings dialog
  const dialog = $<HTMLDialogElement>("prefs-dialog");
  const fontBox = $("prefs-fonts");
  for (const k of KINDS) {
    const group = document.createElement("div");
    group.className = "font-group";
    group.setAttribute("role", "group");
    group.setAttribute("aria-label", k.name);
    const h = document.createElement("p");
    h.className = "font-kind";
    h.textContent = k.name;
    group.append(h);
    for (const f of FONTS.filter((x) => x.kind === k.id)) {
      const label = document.createElement("label");
      label.className = "font-choice";
      label.innerHTML = `<input type="radio" name="editor-font" value="${f.id}" />
        <span class="font-name"></span><span class="font-sample" style='font-family: ${f.stack}'>Bună, spuse Nori. Aa Șș Țț 123</span>`;
      label.querySelector(".font-name")!.textContent = f.name;
      // With Lighter text on, samples show the weight the editor will use.
      if (f.light) label.querySelector<HTMLElement>(".font-sample")!.classList.add("has-light");
      else {
        const no = document.createElement("small");
        no.className = "font-light";
        no.textContent = "no light weight";
        label.querySelector(".font-name")!.append(document.createElement("br"), no);
      }
      group.append(label);
    }
    fontBox.append(group);
  }
  fontBox.addEventListener("change", (e) => setFont((e.target as HTMLInputElement).value));
  dialog.querySelectorAll<HTMLButtonElement>("[data-theme-pick]").forEach((b) =>
    (b.onclick = () => setTheme(b.dataset.themePick as Theme)));
  $("prefs-size-down").onclick = () => step(-1);
  $("prefs-size-up").onclick = () => step(1);
  $("prefs-width-down").onclick = () => setWidth(width - 1);
  $("prefs-width-up").onclick = () => setWidth(width + 1);
  $<HTMLInputElement>("prefs-dark-pages").onchange = (e) => setDarkPages((e.target as HTMLInputElement).checked);
  $<HTMLInputElement>("prefs-light").onchange = (e) => setLight((e.target as HTMLInputElement).checked);
  $<HTMLInputElement>("prefs-spell").onchange = (e) => setSpell((e.target as HTMLInputElement).checked);
  $<HTMLInputElement>("prefs-bar").onchange = (e) => setBar((e.target as HTMLInputElement).checked);
  $<HTMLInputElement>("prefs-dock").onchange = (e) => setDock((e.target as HTMLInputElement).checked);
  $("prefs-page-setup").onclick = () => { dialog.close(); d.openPageSetup(); };
  $("prefs-close").onclick = () => dialog.close();
  dialog.addEventListener("close", () => d.view.focus());

  function openSettings() {
    sync();
    dialog.showModal();
    dialog.querySelector<HTMLInputElement>(`input[name="editor-font"]:checked`)?.focus();
  }

  /** Bring every control showing a preference up to date. */
  function sync() {
    const t = theme();
    dialog.querySelectorAll<HTMLButtonElement>("[data-theme-pick]").forEach((b) =>
      b.setAttribute("aria-pressed", String(b.dataset.themePick === t)));
    const f = font();
    fontBox.querySelectorAll<HTMLInputElement>("input").forEach((i) => (i.checked = i.value === f.id));
    $("prefs-size").textContent = `${size} px`;
    $("prefs-width").textContent = WIDTHS[width].name;
    $<HTMLInputElement>("prefs-light").checked = lightOn();
    $<HTMLInputElement>("prefs-dark-pages").checked = darkPagesOn();
    $("prefs-light-note").textContent = font().light ? "" : `${font().name} has no light weight, so it stays as it is.`;
    fontBox.querySelectorAll<HTMLElement>(".font-light").forEach((el) => (el.hidden = !lightOn()));
    $<HTMLInputElement>("prefs-spell").checked = spellOn();
    $<HTMLInputElement>("prefs-bar").checked = barOn();
    $<HTMLInputElement>("prefs-dock").checked = dockOn();
    $("bar-settings")?.setAttribute("aria-expanded", String(dialog.open));
  }

  // Shortcut hints in the dialog follow the platform (Ctrl or ⌘).
  dialog.querySelectorAll<HTMLElement>("[data-key]").forEach(labelKbd);

  setFont(font().id);
  setSize(size);
  setWidth(width);
  document.body.classList.toggle("bar-hidden", !barOn());
  document.body.classList.toggle("dock-hidden", !dockOn());

  const commands: Command[] = [
    { id: "settings", category: "Preferences", title: "Settings…", keys: ["Mod+,"], run: openSettings },
    { id: "toolbar", category: "View", title: "Toggle toolbar", keys: ["Alt+5"], run: () => setBar(!barOn()), checked: barOn },
    { id: "dock", category: "View", title: "Toggle formatting dock", keys: ["Mod+Space", "Alt+6"], macKeys: ["Mod+Alt+F", "Alt+6"], run: () => setDock(!dockOn()), checked: dockOn },
    { id: "dark-pages", category: "View", title: "Dark page previews in dark themes", run: () => setDarkPages(!darkPagesOn()), checked: darkPagesOn },
    { id: "light", category: "Editor", title: "Lighter text", keys: ["Alt+9"], run: () => setLight(!lightOn()), checked: lightOn },
    { id: "spell", category: "Editor", title: "Toggle spell check", keys: ["Alt+8"], run: () => setSpell(!spellOn()), checked: spellOn },
    { id: "size-up", category: "Text size", title: "Larger", keys: ["Alt+="], run: () => step(1) },
    { id: "size-down", category: "Text size", title: "Smaller", keys: ["Alt+-"], run: () => step(-1) },
    { id: "size-reset", category: "Text size", title: `Reset to ${DEFAULT_SIZE} px`, keys: ["Alt+0"], run: () => setSize(DEFAULT_SIZE) },
    ...SIZES.map((px): Command => ({
      id: `size-${px}`, category: "Text size", title: `${px} px`, run: () => setSize(px), checked: () => size === px,
    })),
    { id: "width-down", category: "Text width", title: "Narrower", keys: ["Alt+,"], run: () => setWidth(width - 1) },
    { id: "width-up", category: "Text width", title: "Wider", keys: ["Alt+."], run: () => setWidth(width + 1) },
    ...WIDTHS.map((w, i): Command => ({
      id: `width-${i}`, category: "Text width", title: w.name, run: () => setWidth(i), checked: () => width === i,
    })),
    { id: "theme-next", category: "Theme", title: "Next theme", keys: ["Alt+7"], run: nextTheme },
    ...THEMES.map((t): Command => ({
      id: `theme-${t.id}`, category: "Theme", title: t.name, run: () => setTheme(t.id), checked: () => theme() === t.id,
    })),
    ...FONTS.map((f): Command => ({
      id: `font-${f.id}`, category: "Editor font", title: `${f.name} (${KINDS.find((k) => k.id === f.kind)!.name.toLowerCase()})`,
      run: () => setFont(f.id), checked: () => font().id === f.id,
    })),
  ];

  /** Resolves once the server knows how to draw the preview pages. */
  const pagesReady = syncPages();
  return { commands, openSettings, nextTheme, step, pagesReady };
}
