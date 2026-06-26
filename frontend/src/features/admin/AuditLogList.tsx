import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";

export function AuditLogList(props: { logs: AdminAuditLog[] }) {
  if (props.logs.length === 0) {
    return (
      <div className="empty-state compact audit-empty">
        暂无审计记录。完成一次保存、发布、下架或删除操作后会出现在这里。
      </div>
    );
  }

  return (
    <div className="audit-log-list">
      {props.logs.map((log) => {
        const device = log.ipAddress?.trim() || log.macAddress?.trim() || "未记录";
        const actor = log.actor?.trim() || "未知管理员";
        const role = log.role?.trim() || "unknown";
        return (
          <article className="audit-log-row" key={log.objectPath}>
            <div className="audit-log-main">
              <div className="audit-log-title">
                <strong>{log.summary || adminAuditActionLabel(log.action)}</strong>
                <span>{adminAuditActionLabel(log.action)}</span>
              </div>
              <div className="audit-log-meta">
                <span>{formatAuditTime(log.createdAt)}</span>
                <span>{actor}</span>
                <span>{role}</span>
                <span>{device}</span>
              </div>
              <small>{log.objectPath}</small>
            </div>
            <div className="audit-log-target">
              <span>{log.target || "-"}</span>
            </div>
          </article>
        );
      })}
    </div>
  );
}

export function adminAuditActionLabel(action: string) {
  const labels: Record<string, string> = {
    savePublishMeta: "保存发布元数据",
    saveMarketProject: "保存项目",
    deleteMarketProject: "删除项目",
    saveMarketCategory: "保存公共分类",
    deleteMarketCategory: "删除公共分类",
    publishDraft: "发布草稿",
    savePluginPublishMeta: "保存 Plugin 发布元数据",
    publishPluginDraft: "发布 Plugin 草稿",
    quickRepublishArchivedSkill: "快速重新上架",
    archiveMarketSkill: "下架 skill",
    archiveMarketPlugin: "下架 plugin"
  };
  return labels[action] ?? action;
}

export function formatAuditTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value || "-";
  }
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false
  });
}
