import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const previewPanelPath = join(process.cwd(), "frontend", "src", "components", "preview", "PreviewPanel.tsx");
const componentsCssPath = join(process.cwd(), "frontend", "src", "styles", "components.css");

const panel = readFileSync(previewPanelPath, "utf8");
const css = readFileSync(componentsCssPath, "utf8");

function requireText(source, text, message) {
  assert.ok(source.includes(text), message);
}

for (const [text, message] of [
  ["const [isExpanded, setIsExpanded]", "preview panel should track expanded window state"],
  ["const [codeZoom, setCodeZoom]", "preview panel should track code zoom state"],
  ["selectAdjacentFile", "preview panel should support previous/next previewable file navigation"],
  ["expandAllFolders", "preview panel should support expanding the file tree"],
  ["collapseAllFolders", "preview panel should support collapsing the file tree"],
  ["navigator.clipboard.writeText", "preview panel should support copying preview text/path"],
  ['window.addEventListener("keydown"', "preview panel should support keyboard shortcuts"],
  ["Maximize2", "preview panel should render a maximize control"],
  ["ZoomIn", "preview panel should render a zoom-in control"],
  ["Copy", "preview panel should render copy controls"]
]) {
  requireText(panel, text, message);
}

for (const [text, message] of [
  [".preview-drawer.expanded", "preview drawer should have an expanded window style"],
  [".preview-toolbar", "preview drawer should have a toolbar"],
  [".preview-window-actions", "preview header should group window actions"],
  [".preview-zoom-control", "preview toolbar should style zoom controls"],
  ["var(--preview-code-font-size", "preview code font size should be CSS-variable driven"],
  [".preview-file-title", "preview file header should handle path and copy action"],
  [".preview-copy-status", "preview copy status should be styled"]
]) {
  requireText(css, text, message);
}
