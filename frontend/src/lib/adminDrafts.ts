import type { AdminDraftPlugin, AdminDraftSkill, PublishMeta } from "../types";

export type AdminArtifactKind = "skill" | "plugin";

export const pluginBuiltinTargets = ["codex", "claude"];

export const isPublishedDraft = (draft?: AdminDraftSkill | null) => draft?.status.trim() === "已发布";

export function publishMetaMissingFields(meta: PublishMeta, kind: AdminArtifactKind = "skill") {
  const missing: string[] = [];
  if (!meta.name.trim()) {
    missing.push("名称");
  }
  if (!meta.summary.trim()) {
    missing.push("摘要");
  }
  if (kind === "plugin") {
    if (!meta.version?.trim()) {
      missing.push("版本");
    }
    if (meta.levels.length === 0) {
      missing.push("作用域");
    }
  }
  if (meta.publishScope === "project") {
    if (!meta.publishProjectSlug) {
      missing.push("项目");
    }
  } else if (!meta.publishCategorySlug) {
    missing.push("公共分类");
  }
  return missing;
}

export function publishMetaMissingMessage(meta: PublishMeta, kind: AdminArtifactKind = "skill") {
  const missing = publishMetaMissingFields(meta, kind);
  if (missing.length === 0) {
    return "";
  }
  return `请补齐${missing.join("、")}`;
}

export function draftCategoryPath(draft: AdminDraftSkill) {
  const path = draft.gitlabCategoryPath?.map((item) => item.trim()).filter(Boolean) ?? [];
  if (path.length > 0) {
    return path;
  }
  return (draft.gitlabCategoryCode ?? "")
    .split("/")
    .map((item) => item.trim())
    .filter(Boolean);
}

export function draftPrimaryCategory(draft: AdminDraftSkill) {
  return draftCategoryPath(draft)[0] ?? "未分类";
}

export function draftSecondaryCategory(draft: AdminDraftSkill) {
  const path = draftCategoryPath(draft);
  return path.length > 1 ? path.slice(1).join("/") : null;
}

export function draftCategoryLabel(category: string) {
  return category.includes("/") ? category.split("/").join(" / ") : category;
}

export function draftSkillLabel(draft: AdminDraftSkill) {
  return draft.draftSlug ?? draft.gitlabSourcePath.split("/").pop() ?? draft.gitlabSourcePath;
}

export function draftSearchText(draft: AdminDraftSkill) {
  return [
    draftSkillLabel(draft),
    draft.gitlabSourcePath,
    draftCategoryPath(draft).join("/"),
    draftCategoryPath(draft).map(draftCategoryLabel).join(" "),
    draft.status,
    draft.validationStatus ?? "",
    draft.version ?? "",
    draft.author ?? "",
    draft.publishMeta?.name ?? "",
    draft.publishMeta?.summary ?? "",
    draft.publishMeta?.tags.join(" ") ?? ""
  ]
    .join(" ")
    .toLocaleLowerCase();
}

export type DraftStatusFilter =
  | "all"
  | "draft"
  | "published"
  | "upgradable"
  | "incomplete"
  | "failed"
  | "risk"
  | "missing-source"
  | "archived";

export type DraftStatusKey = Exclude<DraftStatusFilter, "all">;

export const draftStatusFilterLabels: Record<DraftStatusFilter, string> = {
  all: "全部",
  draft: "待发布",
  published: "已发布",
  upgradable: "可升级",
  incomplete: "待补充",
  failed: "校验失败",
  risk: "版本风险",
  "missing-source": "源缺失",
  archived: "已下架"
};

export const draftStatusFilterOrder: DraftStatusKey[] = [
  "draft",
  "upgradable",
  "published",
  "incomplete",
  "failed",
  "risk",
  "missing-source",
  "archived"
];

export function draftStatusClass(draft: AdminDraftSkill): DraftStatusKey {
  const status = draft.status.trim();
  const validation = (draft.validationStatus ?? "").trim().toLocaleLowerCase();
  const failedValidation = ["failed", "failure", "error", "invalid"].some((keyword) => validation.includes(keyword));
  const incompleteValidation = ["incomplete", "missing", "metadata"].some((keyword) => validation.includes(keyword));

  if (status === "已下架") {
    return "archived";
  }
  if (status.includes("校验失败") || failedValidation) {
    return "failed";
  }
  if (status.includes("元数据待补充") || status.includes("待补充") || incompleteValidation) {
    return "incomplete";
  }
  if (status.includes("版本回退风险") || status.includes("风险")) {
    return "risk";
  }
  if (!draft.sourceAvailable) {
    return "missing-source";
  }
  if (status === "已发布") {
    return "published";
  }
  if (status === "可升级") {
    return "upgradable";
  }
  return "draft";
}

export function sortDrafts(drafts: AdminDraftSkill[]) {
  return [...drafts].sort((first, second) =>
    draftSkillLabel(first).localeCompare(draftSkillLabel(second), undefined, {
      numeric: true,
      sensitivity: "base"
    })
  );
}

export function pluginDraftLabel(draft: AdminDraftPlugin) {
  return draft.name ?? draft.pluginId ?? draft.draftSlug ?? draft.gitlabSourcePath;
}

export function pluginDraftCategoryPath(draft: AdminDraftPlugin) {
  return draft.gitlabCategoryPath?.map((item) => item.trim()).filter(Boolean) ?? [];
}

export function pluginDraftPrimaryCategory(draft: AdminDraftPlugin) {
  return pluginDraftCategoryPath(draft)[0] ?? "未分类";
}

export function pluginDraftSecondaryCategory(draft: AdminDraftPlugin) {
  const path = pluginDraftCategoryPath(draft);
  return path.length > 1 ? path.slice(1).join("/") : null;
}

export function pluginDraftSearchText(draft: AdminDraftPlugin) {
  return [
    pluginDraftLabel(draft),
    draft.gitlabSourcePath,
    pluginDraftCategoryPath(draft).join("/"),
    pluginDraftCategoryPath(draft).map(draftCategoryLabel).join(" "),
    draft.status,
    pluginDraftStatusLabel(draft.status),
    draft.validationStatus ?? "",
    draft.namespace ?? "",
    draft.pluginId ?? "",
    draft.version ?? "",
    draft.summary ?? "",
    draft.targets.join(" "),
    draft.scopes.join(" "),
    draft.components.join(" "),
    draft.riskLevel ?? "",
    draft.publishMeta?.name ?? "",
    draft.publishMeta?.summary ?? "",
    draft.publishMeta?.tags.join(" ") ?? ""
  ]
    .join(" ")
    .toLocaleLowerCase();
}

export function sortPluginDrafts(drafts: AdminDraftPlugin[]) {
  return [...drafts].sort((first, second) =>
    pluginDraftLabel(first).localeCompare(pluginDraftLabel(second), undefined, {
      numeric: true,
      sensitivity: "base"
    })
  );
}

export function pluginDraftStatusClass(status: string): DraftStatusKey {
  if (status === "published") return "published";
  if (status === "archived") return "archived";
  if (status === "ready_to_publish") return "draft";
  if (status.endsWith("_missing") || status === "source_missing") return "missing-source";
  if (status === "metadata_incomplete") return "incomplete";
  return "risk";
}

export function pluginDraftStatusLabel(status: string) {
  const labels: Record<string, string> = {
    source_missing: "源文件缺失",
    metadata_incomplete: "元数据待补充",
    ready_to_publish: "待发布",
    published: "已发布",
    archived: "已下架"
  };
  return labels[status] ?? status;
}

export function emptyPublishMeta(): PublishMeta {
  return {
    namespace: "community",
    skillId: "",
    version: null,
    name: "",
    summary: "",
    tags: [],
    targets: [],
    levels: ["personal", "project"],
    publishScope: "public",
    publishCategorySlug: null,
    publishProjectSlug: null,
    changelog: ""
  };
}

export function defaultMetaFromDraft(draft: AdminDraftSkill): PublishMeta {
  const slug = draftSkillLabel(draft);
  return {
    ...emptyPublishMeta(),
    version: draft.version ?? null,
    skillId: slug,
    name: slug,
    summary: draft.author ? `由 ${draft.author} 维护的 skill` : ""
  };
}

export function defaultMetaFromPluginDraft(draft: AdminDraftPlugin): PublishMeta {
  const pluginId = draft.pluginId ?? draft.draftSlug ?? "";
  const categorySlug = draft.gitlabCategoryPath[0] ?? null;
  return {
    ...emptyPublishMeta(),
    namespace: draft.namespace ?? "community",
    skillId: pluginId,
    version: draft.version ?? "0.1.0",
    name: pluginDraftLabel(draft),
    summary: draft.summary ?? "",
    tags: [],
    targets: [...pluginBuiltinTargets],
    levels: draft.scopes.length > 0 ? draft.scopes : ["user", "project"],
    publishCategorySlug: categorySlug
  };
}

export function normalizeMetaForSave(meta: PublishMeta): PublishMeta {
  return {
    ...meta,
    version: meta.version?.trim() || null,
    publishCategorySlug: meta.publishScope === "project" ? null : meta.publishCategorySlug || null,
    publishProjectSlug: meta.publishScope === "project" ? meta.publishProjectSlug : null
  };
}

export function splitCsv(value: string) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}
