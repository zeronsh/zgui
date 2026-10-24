#!/usr/bin/env node
// Writes the frame profiler's Lucide icons as SVG bodies for Luau.
// Usage: node scripts/import_profiler_icons.mjs /path/to/node_modules/lucide
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const lucide = process.argv[2];
if (!lucide) throw new Error("pass the lucide package directory");
const target = join(dirname(fileURLToPath(import.meta.url)), "../crates/zgx-script/examples/frame_profiler");
// The design's Feather names, mapped to the Lucide icon that draws them.
const names = {
  activity: "activity", layout: "panels-top-left", cpu: "cpu", database: "database", list: "list",
  layers: "layers", "chevron-down": "chevron-down", "chevron-up": "chevron-up", pause: "pause", play: "play",
  search: "search", more: "ellipsis", sliders: "sliders-horizontal", alert: "triangle-alert",
  maximize: "maximize-2", "chevron-left": "chevron-left", "chevron-right": "chevron-right",
  crosshair: "crosshair", info: "info", "chevrons-up-down": "chevrons-up-down", gauge: "gauge",
  "scroll-text": "scroll-text", check: "check", plus: "plus", x: "x", "skip-forward": "skip-forward",
  flag: "flag", aperture: "aperture", compare: "git-pull-request", upload: "upload", download: "download",
  pin: "map-pin", slash: "ban", command: "command", "arrow-up": "arrow-up", "arrow-right": "arrow-right",
  enter: "corner-down-left", "arrow-up-down": "arrow-up-down", "arrow-right-to-line": "arrow-right-to-line",
  type: "type", copy: "copy", braces: "braces", image: "image", "file-text": "file-text", clipboard: "clipboard",
  link: "link", edit: "pen-line", "zoom-in": "zoom-in", "zoom-out": "zoom-out", lock: "lock", "folder-open": "folder-open",
  "alert-circle": "circle-alert", timer: "timer", infinity: "infinity", zap: "zap", hourglass: "hourglass",
  "mouse-pointer-click": "mouse-pointer-click", "chevrons-down": "chevrons-down", option: "option",
};
const attrs = (a) => Object.entries(a).map(([k, v]) => `${k}="${v}"`).join(" ");
const lines = ["-- Lucide icons (see assets/Lucide-LICENSE), as SVG bodies stroked in white for `tint`.", "local icons: { [string]: string } = {"];
for (const [name, file] of Object.entries(names)) {
  const { default: node } = await import(pathToFileURL(join(lucide, "dist/esm/icons", `${file}.js`)).href);
  lines.push(`\t["${name}"] = [[${node.map(([tag, a]) => `<${tag} ${attrs(a)}/>`).join("")}]],`);
}
lines.push("}", "", "return icons", "");
writeFileSync(join(target, "icons.luau"), lines.join("\n"));
const license = readFileSync(join(lucide, "LICENSE"), "utf8").split("\n").map((l) => l.trimEnd()).join("\n");
writeFileSync(join(target, "assets/Lucide-LICENSE"), license.endsWith("\n") ? license : license + "\n");
console.log(`Wrote ${Object.keys(names).length} icons to ${target}/icons.luau`);
