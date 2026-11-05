import { readFileSync } from "node:fs";

const json = JSON.parse(
  readFileSync(".tmp_downloads/system-lens-2026-11-05.json", "utf8"),
);
const serials = JSON.stringify(json).match(/"SerialNumber":"[^"]*"/g) ?? [];
console.log({
  schema: json.schemaVersion,
  host: json.system?.hostName,
  hasPid: JSON.stringify(json).includes('"pid"'),
  sections: Object.keys(json),
  errors: json.errors,
  serials: serials.slice(0, 3),
  processes: json.system?.processes?.length,
});
const markdown = readFileSync(
  ".tmp_downloads/system-lens-2026-11-05.md",
  "utf8",
);
console.log({
  markdownLines: markdown.split("\n").length,
  markdownHasHost: markdown.includes("C121041"),
  markdownHead: markdown.slice(0, 120),
});
