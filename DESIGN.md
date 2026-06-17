---
name: Skill Hub
description: A calm desktop workbench for browsing, installing, and publishing Codex and Claude skills.
colors:
  accent-primary: "#4f7cff"
  accent-primary-hover: "#3d6ae8"
  accent-secondary: "#7c9aff"
  background: "#f8f9fb"
  surface: "#ffffff"
  surface-muted: "#f5f7fb"
  control-bg: "#f6f8fb"
  text-primary: "#1a1d2e"
  text-secondary: "#4a5568"
  muted: "#8792a5"
  border: "#e2e5ea"
  success: "#10b981"
  warning: "#f59e0b"
  danger: "#ef4444"
typography:
  display:
    fontFamily: "Segoe UI Variable, Segoe UI, Microsoft YaHei UI, system-ui, sans-serif"
    fontSize: "32px"
    fontWeight: 750
    lineHeight: 1.08
    letterSpacing: "0"
  title:
    fontFamily: "Segoe UI Variable, Segoe UI, Microsoft YaHei UI, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 700
    lineHeight: 1.2
  body:
    fontFamily: "Segoe UI Variable, Segoe UI, Microsoft YaHei UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: 1.45
  label:
    fontFamily: "Segoe UI Variable, Segoe UI, Microsoft YaHei UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.2
rounded:
  sm: "6px"
  md: "8px"
  lg: "10px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.accent-primary}"
    textColor: "{colors.surface}"
    rounded: "{rounded.sm}"
    padding: "0 16px"
    height: "44px"
  button-secondary:
    backgroundColor: "{colors.control-bg}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "42px"
  panel:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.lg}"
  input:
    backgroundColor: "{colors.control-bg}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "38px"
---

# Design System: Skill Hub

## 1. Overview

**Creative North Star: "The Skill Operations Desk"**

Skill Hub should look like a precise desktop workbench: quiet, fast to scan, and confident about state. The interface uses familiar product UI patterns because the user is doing repeated operational work, not browsing a marketing surface.

The system rejects decorative gradients, floating glass cards, ornamental motion, and deeply nested containers. Depth comes from clear structure, thin borders, restrained shadows, and state-specific accent use.

**Key Characteristics:**
- Stable left navigation, dense work panels, and predictable action placement.
- One blue accent reserved for selection, focus, primary action, and live status.
- Compact rows, chips, and form controls that keep Chinese and English labels readable.

## 2. Colors

The palette is a cool neutral workbench with a single vivid blue accent.

### Primary
- **Workbench Blue**: Used for selected navigation icons, primary actions, focus rings, active chips, and install/publish emphasis.

### Neutral
- **Canvas Paper**: The app background and subtle grid texture.
- **Panel White**: Primary panes and admin surfaces.
- **Control Mist**: Inputs, secondary buttons, segmented controls, and nested neutral rows.
- **Ink Text**: Main content text.
- **Quiet Text**: Metadata, helper copy, and secondary labels.
- **Hairline Border**: Panel borders, dividers, and row separation.

### Named Rules

**The Accent Rarity Rule.** Blue is functional only. Use it for current selection, primary action, focus, and status, never as page decoration.

## 3. Typography

**Display Font:** Segoe UI Variable with Segoe UI, Microsoft YaHei UI, and system fallbacks.
**Body Font:** The same family.
**Label/Mono Font:** Cascadia Code or Consolas only for technical eyebrows and paths.

**Character:** The type scale is restrained and fixed. Product headings should be clear without becoming hero copy.

### Hierarchy
- **Display** (750, 32px, 1.08): Top-level view titles only.
- **Headline** (700, 25px, 1.12): Detail pane titles and selected skill names.
- **Title** (700, 18px, 1.2): Panel headers and section titles.
- **Body** (500, 14px, 1.45): Rows, descriptions, form values, and operational copy.
- **Label** (600, 12px, 1.2): Metadata, chips, counts, toolbar hints, and compact field labels.

### Named Rules

**The No-Hero Rule.** Never use fluid or oversized hero typography inside this product surface.

## 4. Elevation

Skill Hub uses a hybrid of tonal layering, borders, and soft shadows. Main work surfaces get one low ambient shadow; inner controls rely on background contrast and borders instead of nested card shadows.

### Shadow Vocabulary
- **Panel Shadow** (`0 10px 26px rgba(15, 23, 42, 0.055)`): Main panes only.
- **Hover Shadow** (`0 8px 18px rgba(79, 124, 255, 0.12)`): Short-lived hover response for actionable rows and buttons.

### Named Rules

**The One Lift Rule.** A surface may either be a main panel or an inner control, never both.

## 5. Components

### Buttons
- **Shape:** Compact and slightly rounded (6px for buttons, 8px for tool buttons).
- **Primary:** Blue background, white text, 44px height, reserved for install, save, publish, and other committing actions.
- **Hover / Focus:** Border or shadow changes only. Focus always uses the blue focus ring.
- **Secondary:** Neutral control background with a thin border.

### Chips
- **Style:** Small rounded pills with neutral or blue-tinted backgrounds.
- **State:** Strong status chips use semantic color and text, not color alone.

### Cards / Containers
- **Corner Style:** Main panes use 10px; inner rows and controls use 8px.
- **Background:** Main panes are white; controls use the mist neutral.
- **Shadow Strategy:** Only main work surfaces use panel shadow.
- **Border:** Every pane has a thin neutral border.
- **Internal Padding:** 12-20px depending on density and role.

### Inputs / Fields
- **Style:** 38px height, neutral mist background, 1px border, 8px radius.
- **Focus:** Blue border and focus ring.
- **Error / Disabled:** Error copy uses red-tinted strips; disabled fields reduce opacity and use muted text.

### Navigation
- **Style:** Left rail with 30px icon slots, 44px rows, 8px radius.
- **Default / Hover / Active:** Default is text-only quiet. Hover fills the row lightly. Active fills the icon slot in blue and uses a blue-tinted row background.
- **Mobile:** Sidebar becomes a horizontal top area; counts may hide to preserve labels.

## 6. Do's and Don'ts

### Do:
- **Do** keep the first viewport focused on real workflows: marketplace list, detail actions, local state, and admin publishing.
- **Do** use stable grid columns, fixed control heights, and ellipsis for long MinIO, path, and status text.
- **Do** reserve shadows for primary work panes and transient hover feedback.
- **Do** keep the theme switch as a single icon button in the lower-left sidebar.

### Don't:
- **Don't** use marketing hero layouts, decorative gradients, floating glass cards, and ornamental effects.
- **Don't** create deeply nested cards around lists, forms, and details.
- **Don't** use oversized headings inside dense work surfaces.
- **Don't** rely on color alone for install, sync, draft validation, or publish state.
- **Don't** use modal-heavy flows for routine work unless the action is destructive or security-sensitive.
