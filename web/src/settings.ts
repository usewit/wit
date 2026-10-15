// Page setup drawer: edits book.yaml through /api/books/{id}/settings.
// Changes apply automatically, like the text autosave.

import type { Preview } from "./main";

interface Preset { name: string; label: string; group: string; w_mm: number; h_mm: number; kdp: boolean }
interface StylePreset { id: string; name: string; description: string; kinds: string[]; values: Settings }
interface FontList { open: { family: string; note: string }[]; installed: string[] }
interface ImageCheck { path: string; placement: string; dpi: number | null; ok: boolean; note: string }
type Settings = Record<string, string | number | boolean>;

interface Deps {
  bookId: () => string;
  pageCount: () => number;
  /** "book", "essay", "screenplay" or "play": decides which settings apply. */
  kind: () => string;
  onPreview: (p: Preview) => void;
  /** The drawer opened or closed (closing hands focus back to the writing). */
  toggled: (open: boolean) => void;
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

// Fields in the form, by book.yaml key. Type decides how values are read back.
const NUMBERS = [
  "bleed_mm", "margin_mm", "margin_inner_mm", "body_font_size_pt", "line_height_mult",
  "para_gap_lines", "chapter_title_font_size_pt", "page_number_font_size_pt", "trim_w_mm", "trim_h_mm",
];
const BOOLS = [
  "justify", "hyphenate", "first_line_indent", "chapter_new_page",
  "chapter_recto", "include_title_page", "dark_background", "scene_heading_bold", "scene_numbers",
];
const CHOICES = ["trim_preset", "chapter_title_align", "page_number_position", "body_font_name"];

let timer: number | undefined;
let images: ImageCheck[] = [];
// Values as last loaded or saved; only changed keys are sent, so book.yaml
// isn't filled with defaults the user never touched.
let saved: Settings = {};
let styles: StylePreset[] = [];
let presets: Preset[] = [];
let fonts: FontList | null = null;
let kind = "book";
const isScript = () => kind === "screenplay" || kind === "play";

function field(key: string) {
  return document.querySelector<HTMLInputElement | HTMLSelectElement>(`#settings-form [name="${key}"]`)!;
}

function fill(s: Settings) {
  for (const k of NUMBERS) field(k).value = String(s[k] ?? "");
  for (const k of BOOLS) (field(k) as HTMLInputElement).checked = Boolean(s[k]);
  for (const k of CHOICES) field(k).value = String(s[k] ?? "");
  syncCustom();
}

function read(): Settings {
  const out: Settings = {};
  for (const k of NUMBERS) {
    const v = parseFloat(field(k).value);
    if (!Number.isNaN(v)) out[k] = v;
  }
  for (const k of BOOLS) out[k] = (field(k) as HTMLInputElement).checked;
  for (const k of CHOICES) out[k] = field(k).value;
  if (out.trim_preset !== "custom") { delete out.trim_w_mm; delete out.trim_h_mm; }
  // Settings that don't apply to this kind of document aren't compared or sent.
  document.querySelectorAll<HTMLElement>(`#settings-form [data-for]`).forEach((el) => {
    if (!el.hidden) return;
    el.querySelectorAll<HTMLInputElement>("[name]").forEach((i) => delete out[i.name]);
  });
  return out;
}

function syncCustom() {
  $("custom-size").hidden = field("trim_preset").value !== "custom";
}

/// The style preset the form currently matches, if any.
function matchingStyle(): StylePreset | undefined {
  const now = read();
  return styles.find((st) =>
    Object.entries(st.values).every(([k, v]) =>
      typeof v === "number" ? Math.abs(Number(now[k]) - v) < 0.001 : now[k] === v));
}

function syncStyle() {
  const st = matchingStyle();
  $<HTMLSelectElement>("style-preset").value = st?.id ?? "custom";
  $("style-desc").textContent = st
    ? st.description
    : "Your own combination of settings. Pick a preset to start from a tested look.";
}

function setMsg(text: string, bad = false) {
  const m = $("settings-msg");
  m.textContent = text;
  m.classList.toggle("bad", bad);
}

const MM = 25.4;

/// KDP's inside (gutter) margin for a page count, in mm.
function kdpGutter(pages: number): number {
  if (pages <= 150) return 0.375 * MM;
  if (pages <= 300) return 0.5 * MM;
  if (pages <= 500) return 0.625 * MM;
  if (pages <= 700) return 0.75 * MM;
  return 0.875 * MM;
}

export function renderChecklist(pages: number) {
  const list = $("checklist");
  list.innerHTML = "";
  const add = (ok: boolean, text: string) => {
    const li = document.createElement("li");
    li.className = ok ? "ok" : "warn";
    li.textContent = `${ok ? "✓" : "!"}  ${text}`;
    list.append(li);
  };
  const num = (k: string) => parseFloat(field(k)?.value ?? "") || 0;
  const preset = presets.find((p) => p.name === field("trim_preset")?.value);
  if (kind === "screenplay" && pages > 0) {
    add(true, `${pages} script pages, about ${pages} minutes on screen (a page is roughly a minute)`);
  } else if (preset?.kdp && !isScript()) {
    // Amazon KDP's paperback rules, from its print guide.
    if (pages > 0) add(pages >= 24, pages >= 24 ? `${pages} pages (KDP needs at least 24)` : `${pages} pages: KDP needs at least 24`);
    const need = kdpGutter(Math.max(pages, 1));
    const inner = num("margin_inner_mm");
    add(inner + 0.05 >= need, inner + 0.05 >= need
      ? `Spine-side margin ${inner} mm is enough for KDP at ${pages} pages (needs ${need.toFixed(1)} mm)`
      : `Spine-side margin ${inner} mm: KDP needs ${need.toFixed(1)} mm at ${pages} pages`);
    const bleed = num("bleed_mm");
    const outside = num("margin_mm");
    const minOut = bleed > 0 ? 0.375 * MM : 0.25 * MM;
    add(outside + 0.05 >= minOut, `Outside margins ${outside} mm (KDP needs at least ${minOut.toFixed(1)} mm${bleed > 0 ? " with bleed" : ""})`);
    if (bleed > 0) {
      add(Math.abs(bleed - 3.2) < 0.15, Math.abs(bleed - 3.2) < 0.15
        ? "Bleed 3.2 mm (0.125 in), as KDP asks"
        : `Bleed ${bleed} mm: KDP uses 3.2 mm (0.125 in), or 0 for no bleed`);
    }
  } else if (pages > 0 && !isScript()) {
    const extra = (4 - (pages % 4)) % 4;
    add(extra === 0, extra === 0
      ? `${pages} pages, a multiple of 4 (good for stapled binding)`
      : `${pages} pages: add ${extra} (blank or illustration) for a multiple of 4`);
  }
  if (!isScript()) {
    if (images.length === 0) add(true, "No illustrations placed yet");
    for (const im of images) {
      add(im.ok, `${im.path.replace(/^images\//, "")} (${im.placement}): ${im.note}`);
    }
  }
  add(true, "Print PDF always has white pages; dark pages are for proofing only");
}

function fillFonts(current: string) {
  const sel = field("body_font_name") as HTMLSelectElement;
  sel.innerHTML = "";
  const open = document.createElement("optgroup");
  open.label = "Included with Wit (open fonts)";
  for (const f of fonts?.open ?? []) open.append(new Option(`${f.family} · ${f.note}`, f.family));
  const local = document.createElement("optgroup");
  local.label = "On this computer";
  for (const f of fonts?.installed ?? []) local.append(new Option(f, f));
  sel.append(open, local);
  // A font named in book.yaml but not found here stays selected, marked missing.
  if (current && ![...sel.options].some((o) => o.value === current)) {
    sel.prepend(new Option(`${current} (not on this computer)`, current));
  }
}

function fillSizes() {
  const sel = field("trim_preset") as HTMLSelectElement;
  sel.innerHTML = "";
  const groups = new Map<string, HTMLOptGroupElement>();
  for (const p of presets) {
    let g = groups.get(p.group);
    if (!g) { g = document.createElement("optgroup"); g.label = p.group; groups.set(p.group, g); sel.append(g); }
    g.append(new Option(p.label, p.name));
  }
  sel.add(new Option("Custom size…", "custom"));
}

/// Show only the settings that apply to this kind of document.
function showFor() {
  document.querySelectorAll<HTMLElement>("#settings [data-for]").forEach((el) => {
    el.hidden = !el.dataset.for!.split(" ").includes(isScript() ? "script" : "prose");
  });
}

export function initSettings(deps: Deps) {
  const drawer = $("settings");

  async function open() {
    setMsg("Loading…");
    drawer.hidden = false;
    deps.toggled(true);
    const res = await fetch(`/api/books/${deps.bookId()}/settings`);
    const data = await res.json().catch(() => null);
    if (!data) { setMsg("Page setup needs the Wit server restarted.", true); return; }
    if (!res.ok) { setMsg(data.error, true); return; }
    kind = deps.kind();
    showFor();
    presets = data.presets;
    fillSizes();
    if (!fonts) fonts = await fetch("/api/fonts").then((r) => r.json()).catch(() => null);
    fillFonts(String(data.settings.body_font_name ?? ""));
    styles = (data.styles as StylePreset[] ?? []).filter((st) => st.kinds.includes(kind));
    const stSel = $<HTMLSelectElement>("style-preset");
    stSel.innerHTML = "";
    for (const st of styles) stSel.add(new Option(st.name, st.id));
    stSel.add(new Option("Custom", "custom"));
    fill(data.settings);
    saved = read();
    syncStyle();
    images = data.images;
    renderChecklist(deps.pageCount());
    setMsg("Changes apply automatically.");
    $("settings-close").focus();
  }

  function close() {
    drawer.hidden = true;
    deps.toggled(false);
  }

  async function apply() {
    const now = read();
    const changes: Settings = {};
    for (const [k, v] of Object.entries(now)) if (saved[k] !== v) changes[k] = v;
    // The size is one setting: send preset and dimensions together.
    if ("trim_w_mm" in changes || "trim_h_mm" in changes || "trim_preset" in changes) {
      changes.trim_preset = now.trim_preset;
      if (now.trim_preset === "custom") { changes.trim_w_mm = now.trim_w_mm; changes.trim_h_mm = now.trim_h_mm; }
    }
    if (Object.keys(changes).length === 0) return;
    setMsg("Applying…");
    const res = await fetch(`/api/books/${deps.bookId()}/settings`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(changes),
    });
    const data = await res.json().catch(() => ({ error: "Unexpected server reply" }));
    if (!res.ok) { setMsg(data.error, true); return; }
    saved = now;
    images = data.images;
    deps.onPreview(data.preview);
    renderChecklist(deps.pageCount());
    setMsg("Saved to book.yaml.");
  }

  $("settings-close").onclick = close;
  drawer.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  $("style-preset").addEventListener("change", (e) => {
    e.stopPropagation();
    const st = styles.find((x) => x.id === $<HTMLSelectElement>("style-preset").value);
    if (!st) { syncStyle(); return; }
    fill({ ...read(), ...st.values });
    syncStyle();
    clearTimeout(timer);
    apply();
  });
  $("style-preset").addEventListener("input", (e) => e.stopPropagation());
  $("settings-form").addEventListener("input", () => {
    syncCustom();
    syncStyle();
    // Don't apply while a custom size is half-typed.
    clearTimeout(timer);
    timer = window.setTimeout(apply, 600);
  });
  $("settings-form").addEventListener("submit", (e) => e.preventDefault());

  return { toggle: () => (drawer.hidden ? open() : close()) };
}
