import { useEffect, useMemo, useState } from "react";
import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import { Badge } from "../../components/common/Badge";
import { EmptyState } from "../../components/common/EmptyState";
import { SourceChip } from "../../components/common/SourceChip";
import { targetLabels } from "../../app/viewModel";
import type { UpdateArtifactKind, UpdateStatusFilter } from "../../app/viewModel";
import { UpdateDetailCard } from "./UpdateDetailCard";

export function UpdatesView(props: {
  updates: UpdateCandidate[];
  onRefresh: () => void;
  onUpgrade: (update: UpdateCandidate) => void;
  busy: boolean;
}) {
  const [artifactKind, setArtifactKind] = useState<UpdateArtifactKind>("plugin");
  const [statusFilter, setStatusFilter] = useState<UpdateStatusFilter>("ready");
  const [selectedBindingId, setSelectedBindingId] = useState<string | null>(null);

  const updatesByKind = useMemo(
    () => ({
      skill: props.updates.filter((update) => update.kind !== "plugin"),
      plugin: props.updates.filter((update) => update.kind === "plugin")
    }),
    [props.updates]
  );
  const activeUpdates = updatesByKind[artifactKind];
  const readyUpdates = activeUpdates.filter((update) => !update.blockedReason);
  const blockedUpdates = activeUpdates.filter((update) => !!update.blockedReason);
  const filteredUpdates = statusFilter === "ready" ? readyUpdates : blockedUpdates;
  const selectedUpdate =
    filteredUpdates.find((update) => update.bindingId === selectedBindingId) ?? filteredUpdates[0] ?? null;
  const artifactLabel = artifactKind === "plugin" ? "Plugin" : "Skill";
  const statusLabel = statusFilter === "ready" ? "待更新" : "需处理";
  const availableCount = readyUpdates.length;
  const blockedCount = blockedUpdates.length;
  const allReadyUpdates = props.updates.filter((update) => !update.blockedReason);
  const allBlockedUpdates = props.updates.filter((update) => !!update.blockedReason);
  const tabCounts = {
    skill: updatesByKind.skill.length,
    plugin: updatesByKind.plugin.length
  };
  const statusTabs = [
    { key: "ready" as const, label: "待更新", count: availableCount },
    { key: "blocked" as const, label: "需处理", count: blockedCount }
  ];

  useEffect(() => {
    if (!selectedUpdate) {
      setSelectedBindingId(null);
      return;
    }
    if (selectedBindingId !== selectedUpdate.bindingId) {
      setSelectedBindingId(selectedUpdate.bindingId);
    }
  }, [selectedBindingId, selectedUpdate]);

  function selectArtifactKind(kind: UpdateArtifactKind) {
    setArtifactKind(kind);
    setSelectedBindingId(null);
  }

  function selectStatusFilter(status: UpdateStatusFilter) {
    setStatusFilter(status);
    setSelectedBindingId(null);
  }

  return (
    <section className="content-stack updates-view">
      <div className="section-toolbar">
        <div className="section-heading">
          <div className="section-title-line">
            <h2>{artifactLabel} 更新中心</h2>
            <Badge strong={filteredUpdates.length > 0}>{filteredUpdates.length} 项</Badge>
          </div>
          <p>
            {artifactKind === "plugin"
              ? "按插件包单独检查版本差异，升级时会自动生成平台目录并执行 Codex / Claude 同步。"
              : "按 skill 绑定检查版本差异，升级时保留原有范围、项目和启用状态。"}
          </p>
        </div>
        <div className="toolbar-actions">
          <button className="primary-soft" onClick={props.onRefresh} disabled={props.busy}>
            <RefreshCw size={17} />
            检查更新
          </button>
        </div>
      </div>

      <div className="update-summary-grid">
        <div className="update-summary-card">
          <strong>{allReadyUpdates.length}</strong>
          <span>全部可升级</span>
        </div>
        <div className="update-summary-card">
          <strong>{allBlockedUpdates.length}</strong>
          <span>全部需处理</span>
        </div>
        <div className="update-summary-card">
          <strong>{availableCount}</strong>
          <span>{artifactLabel} 待更新</span>
        </div>
        <div className="update-summary-card">
          <strong>{blockedCount}</strong>
          <span>{artifactLabel} 需处理</span>
        </div>
      </div>

      <div className="local-filter-bar update-filter-bar">
        <div className="segmented" role="tablist" aria-label="更新对象类型">
          {(["plugin", "skill"] as const).map((kind) => (
            <button
              key={kind}
              className={artifactKind === kind ? "active" : ""}
              onClick={() => selectArtifactKind(kind)}
              role="tab"
              aria-selected={artifactKind === kind}
            >
              {kind === "plugin" ? "Plugin" : "Skill"}
              <Badge>{tabCounts[kind]}</Badge>
            </button>
          ))}
        </div>

        <div className="tab-strip" role="tablist" aria-label={`${artifactLabel} 更新状态`}>
          {statusTabs.map((tab) => (
            <button
              key={tab.key}
              className={statusFilter === tab.key ? "active" : ""}
              onClick={() => selectStatusFilter(tab.key)}
              role="tab"
              aria-selected={statusFilter === tab.key}
            >
              {tab.label}
              <Badge>{tab.count}</Badge>
            </button>
          ))}
        </div>
      </div>

      <div className="updates-workspace">
        <div className="update-list-pane">
          <div className="section-toolbar compact">
            <div className="section-heading">
              <div className="section-title-line">
                <h2>
                  {artifactLabel} {statusLabel}
                </h2>
                <Badge strong={filteredUpdates.length > 0}>{filteredUpdates.length} 项</Badge>
              </div>
              <p>
                {statusFilter === "ready"
                  ? "这些绑定可以直接升级到市场最新版本。"
                  : "这些更新需要先处理阻塞原因，再执行升级。"}
              </p>
            </div>
          </div>

          {filteredUpdates.length > 0 ? (
            <div className="update-card-list">
              {filteredUpdates.map((update) => (
                <div
                  role="button"
                  tabIndex={0}
                  className={`update-card ${selectedUpdate?.bindingId === update.bindingId ? "active" : ""} ${
                    update.blockedReason ? "blocked" : ""
                  }`}
                  key={update.bindingId}
                  onClick={() => setSelectedBindingId(update.bindingId)}
                  onKeyDown={(event) => {
                    if (event.target !== event.currentTarget) return;
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelectedBindingId(update.bindingId);
                    }
                  }}
                >
                  <span className="update-card-main">
                    <strong className="skill-title-line">
                      {update.skillName}
                      <SourceChip tone="market">{artifactLabel}</SourceChip>
                    </strong>
                    {update.skillName !== update.skillId ? <small>{update.skillId}</small> : null}
                    <span className="update-card-meta">
                      <Badge>{targetLabels[update.target] ?? update.target}</Badge>
                      <Badge>{update.level === "project" ? update.projectPath ?? "项目级" : "个人级"}</Badge>
                      <span className="version-upgrade">
                        {update.currentVersion} → {update.latestVersion}
                      </span>
                    </span>
                  </span>
                  <span className="update-card-side">
                    <Badge strong={!update.blockedReason}>{update.blockedReason ?? "可升级"}</Badge>
                    <span className="row-actions">
                      <button
                        className="icon-button"
                        disabled={!!update.blockedReason || props.busy}
                        onClick={(event) => {
                          event.stopPropagation();
                          props.onUpgrade(update);
                        }}
                        title="升级到最新版本"
                        type="button"
                      >
                        <Rocket size={16} />
                      </button>
                    </span>
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <EmptyState
              title={`${artifactLabel} 暂无${statusLabel}`}
              body={
                statusFilter === "ready"
                  ? `当前没有可直接升级的 ${artifactLabel}。`
                  : `当前没有需要处理的 ${artifactLabel} 更新。`
              }
            />
          )}
        </div>

        {selectedUpdate ? (
          <UpdateDetailCard update={selectedUpdate} busy={props.busy} onUpgrade={props.onUpgrade} />
        ) : (
          <EmptyState
            title="等待选择更新项"
            body="选择左侧更新项后，这里会显示版本差异、自动动作和阻塞原因。"
          />
        )}
      </div>
    </section>
  );
}
