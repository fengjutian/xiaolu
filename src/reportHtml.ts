export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function inlineHtml(value: string): string {
  return escapeHtml(value)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, "<code>$1</code>");
}

/** 将本应用生成的 Markdown 转为 HTML；先转义再加简单格式。 */
export function markdownToHtml(markdown: string): string {
  const lines = markdown.split(/\r?\n/);
  const out: string[] = [];
  let inCode = false;
  let inList = false;
  const closeList = () => {
    if (inList) {
      out.push("</ul>");
      inList = false;
    }
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.startsWith("```")) {
      closeList();
      if (inCode) {
        out.push("</code></pre>");
        inCode = false;
      } else {
        out.push("<pre><code>");
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      out.push(escapeHtml(line));
      continue;
    }
    if (!line.trim()) {
      closeList();
      continue;
    }
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      closeList();
      const level = heading[1].length;
      out.push(`<h${level}>${inlineHtml(heading[2])}</h${level}>`);
      continue;
    }
    if (line.trim().startsWith("|")) {
      closeList();
      const rows: string[][] = [];
      while (index < lines.length && lines[index].trim().startsWith("|")) {
        const cells = lines[index]
          .trim()
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split("|")
          .map((cell) => cell.trim());
        const separator = cells.every((cell) => /^:?-{3,}:?$/.test(cell));
        if (!separator) rows.push(cells);
        index += 1;
      }
      index -= 1;
      if (rows.length > 0) {
        const [header, ...body] = rows;
        const head = header.map((cell) => `<th>${inlineHtml(cell)}</th>`).join("");
        const bodyRows = body
          .map(
            (row) =>
              `<tr>${row.map((cell) => `<td>${inlineHtml(cell)}</td>`).join("")}</tr>`,
          )
          .join("");
        out.push(`<table><thead><tr>${head}</tr></thead><tbody>${bodyRows}</tbody></table>`);
      }
      continue;
    }
    if (/^[-*]\s+/.test(line)) {
      if (!inList) {
        out.push("<ul>");
        inList = true;
      }
      out.push(`<li>${inlineHtml(line.replace(/^[-*]\s+/, ""))}</li>`);
      continue;
    }
    closeList();
    out.push(`<p>${inlineHtml(line)}</p>`);
  }
  closeList();
  if (inCode) out.push("</code></pre>");
  return out.join("\n");
}
