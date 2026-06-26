import type { AppBootstrap } from "../types";

export function pluginScopeLabel(scope: string, projectPath?: string | null) {
  if (scope === "project") {
    return projectPath ? `项目：${projectPath}` : "项目";
  }
  if (scope === "user" || scope === "personal") {
    return "个人";
  }
  if (scope === "local") {
    return "本地";
  }
  return scope;
}

export function pluginBindingStatusLabel(status: string, enabled: boolean) {
  if (status === "missing") return "缺失";
  if (status === "installed") return enabled ? "已写入" : "已禁用";
  if (status === "cached") return "已缓存";
  return enabled ? status : `${status} / 禁用`;
}

export function pluginRiskLabel(riskLevel: string) {
  if (riskLevel === "low") return "低风险";
  if (riskLevel === "medium") return "中风险";
  if (riskLevel === "high") return "高风险";
  if (riskLevel === "critical") return "严重风险";
  return riskLevel || "未评估";
}

export function pluginLocalStatusLabel(plugin: AppBootstrap["localPlugins"][number]) {
  if (plugin.status === "missing") return "缺失";
  if (plugin.managedBySkillhub && plugin.enabled) return "Skill Hub 管理";
  if (plugin.status === "unmanaged") return "外部安装";
  if (plugin.enabled) return "启用";
  return "禁用";
}
