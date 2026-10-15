// Per-viewer conveniences in localStorage; the app must work when storage is unavailable.

// The app was called Quire: copy its saved preferences and drafts over once.
try {
  if (!Object.keys(localStorage).some((k) => k.startsWith("wit."))) {
    for (const k of Object.keys(localStorage)) {
      if (k.startsWith("quire.")) localStorage.setItem("wit." + k.slice(6), localStorage.getItem(k)!);
    }
  }
} catch { /* storage unavailable */ }

export const store = {
  get(k: string): string | null {
    try { return localStorage.getItem("wit." + k); } catch { return null; }
  },
  set(k: string, v: string) {
    try { localStorage.setItem("wit." + k, v); } catch { /* ignore */ }
  },
  del(k: string) {
    try { localStorage.removeItem("wit." + k); } catch { /* ignore */ }
  },
};
