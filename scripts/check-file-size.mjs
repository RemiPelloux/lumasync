import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { extname } from "node:path";

// Include new source files, respect gitignore, never traverse build/vendor trees.
const MAX_LINES = 349;
const GENERATED = new Set(["package-lock.json", "src-tauri/Cargo.lock"]);
const SOURCE = new Set([".rs", ".ts", ".tsx", ".css", ".md", ".json", ".toml",
  ".yml", ".yaml", ".html", ".js", ".mjs", ".svg"]);
const files = execFileSync("git", ["ls-files", "--cached", "--others",
  "--exclude-standard", "-z"], { encoding: "utf8" }).split("\0").filter(Boolean);
let checked = 0;
let failures = 0;
let largest = { lines: 0, file: "" };
for (const file of new Set(files)) {
  if (GENERATED.has(file) || !SOURCE.has(extname(file))) continue;
  let content;
  try { content = readFileSync(file, "utf8"); }
  catch (error) { if (error.code === "ENOENT") continue; throw error; }
  const lines = content.replace(/\r/g, "").replace(/\n$/, "").split("\n").length;
  checked++;
  if (lines > largest.lines) largest = { lines, file };
  if (lines > MAX_LINES) {
    console.error(`${file}: ${lines} lines (maximum ${MAX_LINES})`);
    failures++;
  }
}
console.log(`Checked ${checked} authored files; largest: ${largest.file} (${largest.lines} lines).`);
console.log("Generated dependency lockfiles excluded; preserved for reproducible builds.");
process.exitCode = failures ? 1 : 0;
