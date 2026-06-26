import type { Category, MarketPlugin, MarketProject, MarketSkill } from "../types";

export type ViewKey = "market" | "installed" | "projects" | "updates" | "settings" | "admin";

export type MarketMode = "public" | "project";

export type MarketArtifactKind = "skill" | "plugin";

export type AdminTab = "projects" | "drafts" | "archive" | "audit";

export type GovernanceTab = "project" | "general";

export type GovernanceDialog =
  | { kind: "project-create" }
  | { kind: "project-edit"; project: MarketProject }
  | { kind: "project-delete"; project: MarketProject }
  | { kind: "category-create" }
  | { kind: "category-edit"; category: Category }
  | { kind: "category-delete"; category: Category };

export type InstalledArtifactKind = "skill" | "plugin";

export type UpdateArtifactKind = "skill" | "plugin";

export type UpdateStatusFilter = "ready" | "blocked";

export type InstalledTab = "bindings" | "cache" | "local";

export const targetLabels: Record<string, string> = {
  codex: "Codex",
  claude: "Claude"
};

export const levelLabels: Record<string, string> = {
  personal: "个人",
  project: "项目"
};

export const isProjectMarketSkill = (skill: MarketSkill) =>
  skill.categories.some((category) => category.startsWith("project:"));

export const isProjectMarketPlugin = (plugin: MarketPlugin) =>
  plugin.categories.some((category) => category.startsWith("project:"));

export function skillKey(skill: MarketSkill) {
  return `${skill.sourceId ?? "local"}:${skill.namespace}/${skill.id}`;
}

export function pluginKey(plugin: MarketPlugin) {
  return `${plugin.sourceId ?? "local"}:${plugin.namespace}/${plugin.id}`;
}

export function viewTitle(view: ViewKey) {
  switch (view) {
    case "market":
      return "Object-store marketplace";
    case "installed":
      return "Activation matrix";
    case "projects":
      return "Folder-scoped skills";
    case "updates":
      return "Version queue";
    case "settings":
      return "Local preferences";
    case "admin":
      return "Publishing control";
  }
}

export function viewHeadline(view: ViewKey) {
  switch (view) {
    case "market":
      return "Skill 市场";
    case "installed":
      return "本地生效管理";
    case "projects":
      return "项目级绑定";
    case "updates":
      return "更新中心";
    case "settings":
      return "本地设置";
    case "admin":
      return "管理发布";
  }
}
