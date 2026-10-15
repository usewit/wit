// Open an existing book folder, or create a new one. The desktop app shows a
// native folder dialog (through the server); in a browser you type the path.

interface AppInfo {
  native_picker: boolean;
  default_parent: string | null;
  sep: string;
}

interface Deps {
  /** Called with the book's id once it is in the library. */
  opened(id: string): Promise<void>;
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

let app: AppInfo = { native_picker: false, default_parent: null, sep: "/" };

async function post(url: string, body: unknown) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

/** The native folder dialog; null if cancelled. */
async function pick(start: string): Promise<string | null> {
  const { path } = await post("/api/pick-folder", { start: start || null });
  return path;
}

/** A folder name from a title: "Café Noir?" → "Cafe Noir". */
function folderName(title: string) {
  return title
    .normalize("NFD").replace(/[\u0300-\u036f]/g, "")
    .replace(/[<>:"/\\|?*\x00-\x1f]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/, "");
}

function join(parent: string, name: string) {
  const sep = parent.includes("\\") ? "\\" : parent.includes("/") ? "/" : app.sep;
  return parent.replace(/[\\/]+$/, "") + sep + name;
}

export async function initBooks(deps: Deps) {
  try { app = await (await fetch("/api/app")).json(); } catch { /* browser defaults */ }

  const dialog = $<HTMLDialogElement>("book-dialog");
  const form = $<HTMLFormElement>("book-form");
  const err = $("book-error");
  const title = $<HTMLInputElement>("book-title");
  const author = $<HTMLInputElement>("book-author");
  const place = $<HTMLInputElement>("book-place");
  const target = $("book-target");
  let mode: "open" | "create" = "open";

  for (const b of document.querySelectorAll<HTMLButtonElement>(".browse")) b.hidden = !app.native_picker;

  function showError(msg: string | null) {
    err.hidden = !msg;
    err.textContent = msg ?? "";
  }

  function updateTarget() {
    const name = folderName(title.value);
    target.textContent = mode === "create" && place.value.trim() && name
      ? `New folder: ${join(place.value.trim(), name)}`
      : "";
  }

  function show(m: "open" | "create", path = "") {
    mode = m;
    form.dataset.mode = m;
    $("book-dialog-title").textContent = m === "open" ? "Open a folder" : "Start something new";
    $("book-hint").textContent = m === "open"
      ? "Choose the folder with the text (story.md, essay.md or a .fountain script) and its book.yaml."
      : "Wit makes a folder with the text and its settings. Screenplays and plays are written in Fountain: plain text that lays itself out as a script.";
    $("book-place-label").firstChild!.textContent = m === "open" ? "Folder " : "Put it in ";
    $<HTMLButtonElement>("book-ok").textContent = m === "open" ? "Open" : "Create";
    title.required = m === "create";
    if (m === "create") { title.value = ""; author.value = ""; }
    place.value = path || (m === "create" ? app.default_parent ?? "" : "");
    showError(null);
    updateTarget();
    if (!dialog.open) dialog.showModal();
    (m === "create" ? title : place).focus();
  }

  async function done(id: string) {
    dialog.close();
    await deps.opened(id);
  }

  async function openFolder(dir: string) {
    try {
      await done((await post("/api/books/open", { dir })).id);
    } catch (e) {
      show("open", dir);
      showError((e as Error).message);
    }
  }

  async function showOpen() {
    if (!app.native_picker) return show("open");
    try {
      const dir = await pick("");
      if (dir) await openFolder(dir);
    } catch (e) {
      show("open");
      showError((e as Error).message);
    }
  }

  $("book-browse").onclick = async () => {
    try {
      const dir = await pick(place.value.trim());
      if (dir) { place.value = dir; updateTarget(); }
    } catch (e) {
      showError((e as Error).message);
    }
  };

  title.oninput = updateTarget;
  place.oninput = updateTarget;
  $("book-cancel").onclick = () => dialog.close();

  form.onsubmit = async (ev) => {
    ev.preventDefault();
    const parent = place.value.trim();
    if (!parent) return showError(mode === "open" ? "Type the folder's path." : "Choose where to put it.");
    showError(null);
    const ok = $<HTMLButtonElement>("book-ok");
    ok.disabled = true;
    try {
      const data = mode === "open"
        ? await post("/api/books/open", { dir: parent })
        : await post("/api/books/create", {
            dir: join(parent, folderName(title.value)),
            title: title.value,
            author: author.value,
            kind: form.querySelector<HTMLInputElement>("input[name=doc-kind]:checked")?.value ?? "book",
          });
      await done(data.id);
    } catch (e) {
      showError((e as Error).message);
    } finally {
      ok.disabled = false;
    }
  };

  return { showOpen, showCreate: () => show("create") };
}
