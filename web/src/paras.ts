// The editor shows one line per paragraph; Markdown on disk separates
// paragraphs with a blank line. These convert between the two, so the cursor
// never lands on an empty spacer line.

export interface EditorText {
  text: string;
  /// Disk line (1-based) of each editor line; index 0 = editor line 1.
  diskLines: number[];
}

/// Markdown file -> editor text. Every non-empty line is a paragraph, even
/// without a blank line after it: that's how the book is written, and it's
/// what the editor shows.
export function toEditor(md: string): EditorText {
  const out: string[] = [];
  const diskLines: number[] = [];
  md.replace(/\r\n/g, "\n").split("\n").forEach((raw, i) => {
    if (raw.trim() === "") return;
    out.push(raw.trimEnd());
    diskLines.push(i + 1);
  });
  return { text: out.join("\n"), diskLines };
}

/// Editor text -> Markdown file: every non-empty line becomes its own block.
export function toDisk(text: string): EditorText {
  const blocks = text.split("\n").map((l) => l.trimEnd()).filter((l) => l.trim() !== "");
  // Editor lines map to blocks; empty editor lines map to the next block.
  const diskLines: number[] = [];
  let block = 0;
  for (const l of text.split("\n")) {
    diskLines.push(2 * Math.min(block, Math.max(blocks.length - 1, 0)) + 1);
    if (l.trim() !== "") block++;
  }
  return { text: blocks.join("\n\n") + "\n", diskLines };
}

/// Scripts (Fountain) keep their blank lines, which carry meaning: the editor
/// shows the file exactly as it is.
export function asIs(text: string): EditorText {
  const t = text.replace(/\r\n/g, "\n");
  return { text: t, diskLines: t.split("\n").map((_, i) => i + 1) };
}
