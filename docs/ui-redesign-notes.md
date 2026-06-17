# UI Redesign Notes

Date: 2026-06-14

## Target Reference

Reference file: `draft-area-redesign.html`

The target design reads as a quiet workbench:
- Two clear panes with stable heights, light borders, and soft elevation.
- Panel headers use a subtle surface treatment, not a heavy card stack.
- Draft categories are scannable rows with count pills.
- Draft items have a small icon tile, a visible active bar, status chips, and predictable hover feedback.
- The editor panel has a stronger hierarchy between metadata, warnings, and publish actions.

## Current UI Issues

- The draft publishing area already has the right content, but it visually feels like panels nested inside panels because `admin-panel`, `draft-browser`, and `publish-editor` all add their own padding/backgrounds.
- The draft list uses good active/hover affordances, but the row icon is not framed and the row spacing is wider than the reference, which makes dense scanning harder.
- The publish editor action bar uses negative margins to escape padding. This is fragile and makes responsive layout harder to reason about.
- Global surfaces use similar shadows and borders across sidebar, market panes, admin panes, and forms, so primary work surfaces do not always stand out from secondary controls.
- Body backgrounds use radial highlights. They add atmosphere but compete with the precise, tool-like target direction.
- Mobile admin draft layout needs tighter panel headers and fixed row behavior so labels/actions do not crowd each other.

## Implementation Log

- Added shared surface tokens for elevated panels, muted surfaces, panel headers, accent focus rings, and hover shadows.
- Removed radial background highlights and kept a quieter linear background plus subtle grid texture.
- Unified primary work surfaces across market, settings, local scan, data table, and admin panels with consistent borders, radius, and elevation.
- Tightened list-pane and admin navigation states so active rows use a clear left accent and soft blue surface.
- Rebuilt the admin draft workbench visually: stable two-pane grid, integrated panel headers, denser draft list, framed file icons, count pills, status chips, and stable publish actions.
- Added an explicit publish-editor empty state and disabled publish actions until a draft is selected.
- Removed the emoji from the no-source warning copy to better match the professional tool UI.

## Test Log

- Passed: `npm.cmd run build` in `fronted`.
- Passed: `git diff --check -- fronted/src/App.tsx fronted/src/styles.css docs/ui-redesign-notes.md`.
- Partial: Vite preview returned HTTP 200 for the built `fronted/dist` bundle at `http://127.0.0.1:4173/`.
- Blocked: `agent-browser.cmd` could navigate to the app title once, but `agent-browser doctor` reports Chrome launch failure: `CDP response channel closed`. Screenshot, viewport, and snapshot commands were not reliable in this environment.
- Attempted fallback: `agent-browser --engine lightpanda` is unavailable because Lightpanda is not installed.

## Open Risks

- Browser visual verification is still recommended after fixing the local `agent-browser` Chrome launch issue or running the Tauri app manually.
- The app is a Tauri/Vite frontend with mocked browser behavior outside Tauri. Browser smoke tests can validate layout and interaction affordances, but native dialogs and filesystem-backed flows still need an app-level run for full coverage.

## Iteration 2 - App Shell And Marketplace Modernization

Date: 2026-06-14

### Audit Findings

- Product positioning was implicit. Without a written product/design baseline, UI decisions could drift toward decorative marketplace styling instead of a reliable desktop workbench.
- The sidebar active state mixed a left accent bar, row fill, border, and count badge, creating more emphasis than the navigation needs.
- The app depended on a Google Fonts import. That is fragile for a Windows desktop tool that may run offline or inside an internal network.
- Topbar headings used fluid sizing and could feel closer to a landing page hero than a compact product shell.
- Long sync/status text could stretch the topbar instead of truncating within a stable pill.
- Marketplace rows lacked enough secondary identity, so similarly named skills were harder to compare quickly.
- Market detail tags used a second cyan-like treatment even though the product should have one blue accent vocabulary.
- Radius and surface treatments were inconsistent across panels, inputs, cache cards, project tiles, governance rows, and dialogs.
- Hover motion included lateral row movement, which adds visual noise in repeated lists.

### Implementation Log

- Added `PRODUCT.md` to define Skill Hub as a calm skill operations workbench.
- Added `DESIGN.md` with extracted/current tokens, component rules, and product-specific do/don't guidance.
- Removed the remote Google Fonts import and switched to a system font stack.
- Tightened root tokens: 6/8/10px radius scale, softer panel shadow, quieter background grid, neutral control surfaces, and a reusable active navigation tint.
- Reworked sidebar navigation to use a dedicated icon slot and active icon fill instead of a hard left stripe.
- Added `aria-current` to the active navigation item and retained the lower-left single-icon theme switch.
- Updated the status pill markup and CSS so long sync text truncates safely.
- Tuned the topbar to fixed product-scale typography.
- Rebalanced the marketplace grid toward a workbench layout with a wider detail pane and denser list.
- Added a small skill row icon tile and namespace/id metadata for faster scanning.
- Unified marketplace tags, install previews, inputs, tabs, cache cards, project tiles, governance rows, and dialog summaries around the same blue/neutral vocabulary.
- Added a global `prefers-reduced-motion` guard and removed lateral draft row hover movement.

### Test Plan For This Iteration

- Passed: `npm.cmd run build` from `fronted`.
- Passed: `git diff --check -- PRODUCT.md DESIGN.md fronted/src/App.tsx fronted/src/styles.css docs/ui-redesign-notes.md` with only Git LF/CRLF warnings.
- Passed: Vite preview returned HTTP 200 at `http://127.0.0.1:4173/`.
- Passed: `agent-browser.cmd` snapshot found the main nav, lower-left theme switch, marketplace filters, search box, and refresh action.
- Passed: browser errors and console output were empty during the preview smoke test.
- Passed: theme switch changed `.app-shell[data-theme]` from `light` to `dark`.
- Passed: 390px mobile viewport retained nav, theme switch, page title, marketplace filters, search, and refresh controls.
- Captured temporary screenshots for desktop and mobile inspection during the run; they were deleted after verification.

### Remaining UI Risks

- The source file currently displays mojibake in PowerShell output for Chinese strings, so copy-level UI polish should be handled in a dedicated encoding-safe pass.
- The browser-only frontend still mocks or cannot execute some Tauri-native flows such as dialogs and filesystem selection.
- Admin publishing forms are visually improved, but a deeper interaction pass is still needed for validation messaging and publish readiness.

## Iteration 3 - Non-Market Pages And Preview Reliability

Date: 2026-06-14

### Audit Findings

- The production `npm run preview` build was not using the browser mock API, so UI smoke tests could show Tauri IPC error text instead of real app content.
- Browser mock data was too sparse. Local, projects, updates, and settings pages mostly rendered empty states, making layout density and row behavior hard to evaluate.
- Empty states used a single flat sentence, which did not teach the workflow or explain the next expected state.
- Project binding showed a primary action even when no project folder was selected.
- Settings actions lacked the icon vocabulary used elsewhere.
- Table rows, cache rows, local scan rows, project tiles, and target root rows did not share the same hover and row rhythm.

### Implementation Log

- Changed the frontend browser API fallback so any non-Tauri browser environment uses mock data, including production preview.
- Expanded browser mock data with installed bindings, cached packages, local scan rows, projects, target roots, and one update candidate.
- Added a reusable `EmptyState` component with title/body structure for local, cache, project, update, and settings states.
- Added section count feedback to the local management toolbar.
- Disabled the project binding primary action until a project folder is selected.
- Added a save icon to target root save actions.
- Unified table row, cache row, local scan row, project tile, and target root hover behavior.
- Gave local scan rows a framed status icon tile so they match the marketplace/cache row vocabulary.

### Test Log

- Passed: `npm.cmd run build` from `fronted`.
- Passed: `git diff --check -- PRODUCT.md DESIGN.md fronted/src/App.tsx fronted/src/api.ts fronted/src/styles.css docs/ui-redesign-notes.md` with only Git LF/CRLF warnings.
- Passed: Vite preview returned HTTP 200 at `http://127.0.0.1:4173/`.
- Passed: production preview now rendered mock counts: market 2, local 2, projects 2, updates 1, settings 2.
- Passed: market page no longer showed `window.__T` / Tauri IPC error text in browser preview.
- Passed: local page showed binding, cache, and local scan tabs with dense rows and visible row actions.
- Passed: project page showed bound projects and disabled the bind action until a folder path is present.
- Passed: update page showed a version transition row and upgrade action.
- Passed: settings page showed Codex and Claude target roots with choose/save actions.
- Passed: browser errors and console output were empty after page checks.
- Partial: one `agent-browser.cmd snapshot` call hit `CDP response channel closed`, but subsequent text/error/console checks on the same page succeeded.

### Remaining UI Risks

- Browser preview now covers layout density better, but native Tauri dialogs and filesystem selection still require app-level verification.
- Admin draft publishing still needs a deeper pass for validation copy, publish readiness, and destructive action affordances.
- A copy/encoding pass should still review all Chinese strings in source with UTF-8-safe tooling.
