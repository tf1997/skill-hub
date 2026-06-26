import type { AppBootstrap, MarketPlugin, MarketSkill, SkillBinding, TargetRoot } from "../types";

export type LevelChoice = "personal" | "project" | "download";

export function isInstalledSkill(skill: MarketSkill, bindings: SkillBinding[]) {
  return bindings.some(
    (binding) =>
      binding.namespace === skill.namespace &&
      binding.skillId === skill.id &&
      binding.status === "installed"
  );
}

export function marketStatusLabel(skill: MarketSkill, bindings: SkillBinding[]) {
  const related = bindings.filter(
    (binding) => binding.namespace === skill.namespace && binding.skillId === skill.id
  );
  if (related.some((binding) => binding.enabled)) return "已启用";
  if (related.length > 0) return "已安装";
  if (skill.cachedVersions.includes(skill.latestVersion)) return "已缓存";
  return "未安装";
}

export function getInstallState(
  skill: MarketSkill,
  bindings: SkillBinding[],
  target: string,
  level: LevelChoice,
  projectPath: string
) {
  if (level === "download") {
    const cached = skill.cachedVersions.includes(skill.latestVersion);
    return {
      label: cached ? "已下载到本地仓库" : "下载到本地仓库",
      disabled: cached,
      tone: cached ? ("cached" as const) : ("install" as const)
    };
  }

  const existing = bindings.find((binding) => {
    const sameScope =
      binding.target === target &&
      binding.level === level &&
      (level !== "project" || binding.projectPath === projectPath);
    return sameScope && binding.status === "installed";
  });

  if (existing) {
    return {
      label: existing.enabled ? "已安装并启用" : "已安装，当前禁用",
      disabled: true,
      tone: "installed" as const
    };
  }

  const cached = skill.cachedVersions.includes(skill.latestVersion);
  return {
    label: cached ? "从本地缓存安装" : "安装并启用",
    disabled: false,
    tone: cached ? ("cached" as const) : ("install" as const)
  };
}

export function getPluginInstallState(
  plugin: MarketPlugin | undefined,
  bindings: AppBootstrap["pluginBindings"],
  target: string,
  level: LevelChoice,
  projectPath: string,
  conflict: string | null
) {
  if (!plugin) {
    return { label: "安装并启用", disabled: true, tone: "install" as const };
  }
  if (!plugin.targets.includes(target)) {
    return { label: "当前平台不支持", disabled: true, tone: "install" as const };
  }
  if (target === "codex" && level === "project") {
    return { label: "Codex 仅支持个人级 plugin", disabled: true, tone: "install" as const };
  }
  if (conflict) {
    return { label: "存在范围冲突", disabled: true, tone: "install" as const };
  }
  const scope = level === "project" ? "project" : "user";
  if (!plugin.scopes.includes(scope)) {
    return { label: "当前范围不支持", disabled: true, tone: "install" as const };
  }
  if (level === "download") {
    const cached = plugin.cachedVersions.includes(plugin.latestVersion);
    return {
      label: cached ? "已下载到本地仓库" : "下载到本地仓库",
      disabled: cached,
      tone: cached ? ("cached" as const) : ("install" as const)
    };
  }
  const existing = bindings.find((binding) => {
    const sameScope =
      binding.target === target &&
      binding.scope === scope &&
      (scope !== "project" || binding.projectPath === projectPath);
    return sameScope && binding.namespace === plugin.namespace && binding.pluginId === plugin.id && binding.status === "installed";
  });
  if (existing) {
    const installedLabel =
      target === "codex"
        ? existing.enabled
          ? "已安装到 Codex"
          : "已安装，当前禁用"
        : target === "claude"
          ? existing.enabled
            ? "已安装到 Claude Code"
            : "已安装，当前禁用"
        : existing.enabled
          ? "已写入 marketplace"
          : "已写入，当前禁用";
    return {
      label: installedLabel,
      disabled: true,
      tone: "installed" as const
    };
  }
  const cached = plugin.cachedVersions.includes(plugin.latestVersion);
  const installLabel =
    target === "codex"
      ? cached
        ? "从缓存安装到 Codex"
        : "安装到 Codex"
      : target === "claude"
        ? cached
          ? "从缓存安装到 Claude Code"
          : "安装到 Claude Code"
        : cached
          ? "从本地缓存写入"
          : "写入 marketplace";
  return {
    label: installLabel,
    disabled: false,
    tone: cached ? ("cached" as const) : ("install" as const)
  };
}

export function pluginInstallPreview(target: string, level: LevelChoice, projectPath: string) {
  if (level === "download") {
    return "下载到 Skill Hub plugin-packages，本次不写 marketplace。";
  }
  if (level === "project") {
    if (target === "codex") {
      return "Codex plugin 不支持项目级生效";
    }
    if (!projectPath) {
      return "请选择项目。";
    }
    if (target === "claude") {
      return `${projectPath}/.claude/skillhub-plugin-marketplace/.claude-plugin/marketplace.json`;
    }
  }
  if (target === "codex") {
    return "~/.agents/plugins/marketplace.json";
  }
  return `Skill Hub 本地 ${target} marketplace`;
}

export function scopeConflict(bindings: SkillBinding[], target: string, level: LevelChoice) {
  if (level === "download") return null;
  const opposite = level === "personal" ? "project" : "personal";
  const conflict = bindings.find(
    (binding) => binding.target === target && binding.level === opposite && binding.enabled
  );
  if (!conflict) return null;
  return level === "personal"
    ? "该 skill 已在项目级启用，请先禁用项目级绑定。"
    : "该 skill 已在个人级启用，项目级不能再启用。";
}

export function pluginScopeConflict(bindings: AppBootstrap["pluginBindings"], target: string, level: LevelChoice) {
  if (level === "download") return null;
  if (target === "codex" && level === "project") {
    return "Codex plugin 当前只支持个人级安装，不支持项目级生效。";
  }
  const oppositeScopes = level === "personal" ? ["project"] : ["user", "personal"];
  const conflict = bindings.find(
    (binding) => binding.target === target && oppositeScopes.includes(binding.scope) && binding.enabled
  );
  if (!conflict) return null;
  return level === "personal"
    ? "该 plugin 已在项目级启用，请先禁用项目级绑定。"
    : "该 plugin 已在个人级启用，项目级不能再启用。";
}

export function getInstallPreview(
  target: string,
  level: LevelChoice,
  projectPath: string,
  targetRoots: TargetRoot[]
) {
  if (level === "download") return "Skill Hub 本地包仓库";
  if (level === "personal") {
    return targetRoots.find((root) => root.target === target)?.personalPath ?? "未配置个人级目录";
  }
  if (!projectPath) return "请选择项目根目录";
  const suffix = target === "codex" ? ".codex/skills" : target === "claude" ? ".claude/skills" : `.skillhub/${target}/skills`;
  return `${projectPath.replace(/\\/g, "/")}/${suffix}`;
}
