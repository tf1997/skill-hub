import type { AppBootstrap, CachedSkillPackage, LocalSkill, MarketSkill, SkillBinding } from "../types";

const targetLabels: Record<string, string> = {
  codex: "Codex",
  claude: "Claude"
};

export type CachedSkillItem = {
  key: string;
  package: CachedSkillPackage;
  marketSkill?: MarketSkill;
};

export type LocalInstallLevel = "personal" | "project";

export type LocalInstallTarget = "codex" | "claude";

export type LocalInstallOptions = {
  target: LocalInstallTarget;
  level: LocalInstallLevel;
  projectPath: string | null;
};

export type LocalInstallDialogState =
  | { kind: "local"; skill: LocalSkill }
  | { kind: "cache"; item: CachedSkillItem };

export const localInstallTargets = ["codex", "claude"] as const;

export function displaySkillTags(skill: MarketSkill) {
  const values = [...skill.categories.filter((category) => !category.startsWith("project:")), ...skill.tags];
  const seen = new Set<string>();
  return values.filter((value) => {
    const normalized = value.trim();
    if (!normalized) return false;
    const key = normalized.toLocaleLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function localSkillStatusLabel(skill: LocalSkill) {
  if (skill.status === "missing") return "缺失";
  if (!skill.enabled) return "已禁用";
  if (skill.managedBySkillhub) return "Skill Hub";
  if (skill.status === "cached") return "已加入缓存";
  if (skill.origin === "market") return "来自市场";
  if (skill.origin === "unknown") return "可能来自市场";
  if (skill.origin === "local") return "用户自建";
  if (skill.status === "unmanaged") return "用户自建";
  return skill.status;
}

export function isLocalBinding(binding: SkillBinding) {
  return binding.sourceId === "__local__" || binding.namespace === "local";
}

export function bindingSourceTone(binding: SkillBinding): "market" | "local" {
  return isLocalBinding(binding) ? "local" : "market";
}

export function bindingSourceLabel(binding: SkillBinding) {
  return isLocalBinding(binding) ? "自建" : "市场";
}

export function canDeleteLocalSkillFromMatrix(skill: LocalSkill) {
  return !skill.managedBySkillhub && skill.origin === "local" && (skill.status === "cached" || skill.status === "disabled");
}

export function localPluginDisplayName(plugin: AppBootstrap["localPlugins"][number]) {
  return normalizedLabel(plugin.pluginId) ?? normalizedLabel(plugin.marketplaceName) ?? "本地 plugin";
}

export function normalizedLabel(value?: string | null) {
  const next = value?.trim();
  return next ? next : null;
}

export function cachedPackageKey(cachedPackage: CachedSkillPackage) {
  return `${cachedPackage.sourceId ?? ""}:${cachedPackage.namespace}/${cachedPackage.skillId}@${cachedPackage.version}`;
}

export function upsertCachedPackage(packages: CachedSkillPackage[], cachedPackage: CachedSkillPackage) {
  const key = cachedPackageKey(cachedPackage);
  const next = packages.filter((item) => cachedPackageKey(item) !== key);
  return [cachedPackage, ...next];
}

export function hasAvailableLocalInstallTarget(
  dialog: LocalInstallDialogState,
  bindings: SkillBinding[],
  localSkills: LocalSkill[]
) {
  return availableLocalInstallTargets(dialog, bindings, localSkills).length > 0;
}

export function availableLocalInstallTargets(
  dialog: LocalInstallDialogState,
  bindings: SkillBinding[],
  localSkills: LocalSkill[]
): LocalInstallTarget[] {
  const identity = localInstallIdentity(dialog);
  return localInstallTargets.filter((target) => {
    const bindingInstalled = bindings.some(
      (binding) =>
        binding.target === target &&
        binding.namespace === identity.namespace &&
        slugifyLocalSkillId(binding.skillId) === identity.skillId &&
        binding.status !== "missing"
    );
    if (bindingInstalled) return false;
    return !localSkills.some(
      (skill) =>
        skill.target === target &&
        skill.status !== "missing" &&
        !skill.managedBySkillhub &&
        localSkillInstallKey(skill) === identity.skillId
    );
  });
}

export function isLocalInstallTarget(value: string): value is LocalInstallTarget {
  return value === "codex" || value === "claude";
}

export function localInstallIdentity(dialog: LocalInstallDialogState) {
  if (dialog.kind === "cache") {
    return {
      namespace: dialog.item.package.namespace,
      skillId: slugifyLocalSkillId(dialog.item.package.skillId)
    };
  }
  return {
    namespace: "local",
    skillId: localSkillInstallKey(dialog.skill)
  };
}

export function localSkillInstallKey(skill: LocalSkill) {
  return slugifyLocalSkillId(skill.skillId || localPathName(skill.path) || skill.detectedManifest || "local-skill");
}

export function cachedPackageInstallTargets(
  cachedPackage: CachedSkillPackage,
  bindings: SkillBinding[],
  localSkills: LocalSkill[]
) {
  if (cachedPackage.origin !== "local") return [];
  const identity = {
    namespace: cachedPackage.namespace,
    skillId: slugifyLocalSkillId(cachedPackage.skillId)
  };
  const targets = new Set<LocalInstallTarget>();

  for (const binding of bindings) {
    if (!isLocalInstallTarget(binding.target)) continue;
    if (
      binding.namespace === identity.namespace &&
      slugifyLocalSkillId(binding.skillId) === identity.skillId &&
      binding.status !== "missing"
    ) {
      targets.add(binding.target);
    }
  }

  for (const skill of localCachedInstallations(cachedPackage, localSkills)) {
    if (isLocalInstallTarget(skill.target)) {
      targets.add(skill.target);
    }
  }

  return [...targets];
}

export function localCachedInstallations(cachedPackage: CachedSkillPackage, localSkills: LocalSkill[]) {
  const fingerprint = cachedLocalSkillFingerprint(cachedPackage);
  if (!fingerprint) return [];
  return localSkills.filter(
    (skill) =>
      skill.status !== "missing" &&
      !skill.managedBySkillhub &&
      skill.origin === "local" &&
      localSkillFingerprint(skill) === fingerprint
  );
}

export function hasBindingForLocalSkill(
  cachedPackage: CachedSkillPackage,
  skill: LocalSkill,
  bindings: SkillBinding[]
) {
  const skillId = slugifyLocalSkillId(cachedPackage.skillId);
  const projectPath = normalizeLocalPath(skill.projectPath);
  return bindings.some((binding) => {
    if (!isLocalBinding(binding)) return false;
    if (binding.target !== skill.target || binding.level !== skill.level) return false;
    if (slugifyLocalSkillId(binding.skillId) !== skillId) return false;
    if (skill.level === "project") {
      return normalizeLocalPath(binding.projectPath) === projectPath;
    }
    return true;
  });
}

export function cachedPackageInstallSummary(
  cachedPackage: CachedSkillPackage,
  bindings: SkillBinding[],
  localSkills: LocalSkill[]
) {
  const targets = cachedPackageInstallTargets(cachedPackage, bindings, localSkills);
  if (targets.length === 0) return cachedPackage.bindingCount > 0 ? `已安装 ${cachedPackage.bindingCount} 处` : "仅缓存";
  return `已安装 ${targets.map((target) => targetLabels[target] ?? target).join("、")}`;
}

export function markLocalSkillsCached(
  localSkills: LocalSkill[],
  cachedPackage: CachedSkillPackage,
  sourceSkill?: LocalSkill
) {
  const fingerprints = new Set(
    [cachedLocalSkillFingerprint(cachedPackage), sourceSkill ? localSkillFingerprint(sourceSkill) : null].filter(Boolean)
  );
  const paths = new Set(
    [cachedPackage.sourcePath, sourceSkill?.path].map(normalizeLocalPath).filter(Boolean)
  );

  return localSkills.map((skill) => {
    const pathMatched = paths.has(normalizeLocalPath(skill.path));
    const fingerprint = localSkillFingerprint(skill);
    const fingerprintMatched = Boolean(fingerprint && fingerprints.has(fingerprint));
    if (!pathMatched && !fingerprintMatched) return skill;
    return {
      ...skill,
      status: "cached",
      origin: "local",
      canImportToCache: false
    };
  });
}

export function cachedLocalSkillFingerprint(cachedPackage: CachedSkillPackage) {
  if (cachedPackage.origin !== "local") return null;
  const skillId = slugifyLocalSkillId(cachedPackage.skillId);
  if (!skillId) return null;
  return `${skillId}@${normalizeLocalSkillVersion(cachedPackage.version)}`;
}

export function localSkillFingerprint(skill: LocalSkill) {
  const skillId = slugifyLocalSkillId(skill.skillId || localPathName(skill.path) || skill.detectedManifest || "");
  if (!skillId) return null;
  return `${skillId}@${normalizeLocalSkillVersion(skill.version)}`;
}

export function normalizeLocalSkillVersion(version?: string | null) {
  return version?.trim() || "0.0.0-local";
}

export function normalizeLocalPath(path?: string | null) {
  return path?.replace(/\\/g, "/").toLocaleLowerCase() ?? "";
}

export function localPathName(path?: string | null) {
  const parts = path?.replace(/\\/g, "/").split("/").filter(Boolean) ?? [];
  return parts.length > 0 ? parts[parts.length - 1] : "";
}

export function slugifyLocalSkillId(value: string) {
  return value
    .trim()
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .toLocaleLowerCase();
}
