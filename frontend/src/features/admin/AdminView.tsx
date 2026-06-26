import { useState } from "react";
import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, ScrollText, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import { Badge } from "../../components/common/Badge";
import { BindingDots } from "../../components/common/BindingDots";
import { EmptyState } from "../../components/common/EmptyState";
import { SourceChip } from "../../components/common/SourceChip";
import { getInstallPreview, getInstallState, getPluginInstallState, isInstalledSkill, marketStatusLabel, pluginInstallPreview, pluginScopeConflict, scopeConflict } from "../../lib/installState";
import type { LevelChoice } from "../../lib/installState";
import { availableLocalInstallTargets, bindingSourceLabel, bindingSourceTone, cachedPackageInstallSummary, cachedPackageInstallTargets, canDeleteLocalSkillFromMatrix, displaySkillTags, hasAvailableLocalInstallTarget, hasBindingForLocalSkill, isLocalBinding, localCachedInstallations, localPluginDisplayName, localSkillStatusLabel } from "../../lib/localSkills";
import type { CachedSkillItem } from "../../lib/localSkills";
import { pluginBindingStatusLabel, pluginLocalStatusLabel, pluginRiskLabel, pluginScopeLabel } from "../../lib/plugins";
import { categoryNameFromSlug, emptyMarketCategory, emptyMarketProject, nextCategoryOrder, nextProjectOrder, normalizeProjectList } from "../../lib/categories";
import { defaultMetaFromDraft, defaultMetaFromPluginDraft, draftCategoryLabel, draftPrimaryCategory, draftSearchText, draftSecondaryCategory, draftSkillLabel, draftStatusClass, draftStatusFilterLabels, draftStatusFilterOrder, isPublishedDraft, normalizeMetaForSave, pluginDraftCategoryPath, pluginDraftLabel, pluginDraftPrimaryCategory, pluginDraftSearchText, pluginDraftSecondaryCategory, pluginDraftStatusClass, pluginDraftStatusLabel, publishMetaMissingMessage, sortDrafts, sortPluginDrafts, splitCsv } from "../../lib/adminDrafts";
import type { AdminArtifactKind, DraftStatusFilter, DraftStatusKey } from "../../lib/adminDrafts";
import { levelLabels, pluginKey, skillKey, targetLabels } from "../../app/viewModel";
import type { AdminTab, GovernanceDialog, GovernanceTab, InstalledArtifactKind, InstalledTab, MarketArtifactKind, MarketMode, UpdateArtifactKind, UpdateStatusFilter } from "../../app/viewModel";
import { AuditLogList } from "./AuditLogList";
import { DraftList } from "./DraftList";
import { GovernanceDialogView } from "./GovernanceDialogView";
import { PluginDraftList } from "./PluginDraftList";

export function AdminView(props: {
  session: AdminSession | null;
  activeTab: AdminTab;
  onActiveTab: (value: AdminTab) => void;
  governanceTab: GovernanceTab;
  onGovernanceTab: (value: GovernanceTab) => void;
  governanceDialog: GovernanceDialog | null;
  governanceDialogError: string | null;
  busy: boolean;
  onGovernanceDialog: (value: GovernanceDialog | null) => void;
  drafts: AdminDraftSkill[];
  pluginDrafts: AdminDraftPlugin[];
  auditLogs: AdminAuditLog[];
  onRefreshAuditLogs: () => void;
  selectedDraftPath: string | null;
  selectedPluginDraftPath: string | null;
  onRefreshDrafts: () => void;
  onRefreshPluginDrafts: () => void;
  onSelectDraft: (draft: AdminDraftSkill) => void;
  onSelectPluginDraft: (draft: AdminDraftPlugin) => void;
  meta: PublishMeta;
  onMeta: (value: PublishMeta) => void;
  pluginMeta: PublishMeta;
  onPluginMeta: (value: PublishMeta) => void;
  onSaveMeta: () => void;
  onSavePluginMeta: () => void;
  onPreview: () => void;
  onPreviewPlugin: () => void;
  onPublish: () => void;
  onPublishPlugin: () => void;
  onQuickRepublish: () => void;
  projects: MarketProject[];
  projectDraft: MarketProject;
  onProjectDraft: (value: MarketProject) => void;
  onSaveProject: () => void;
  onDeleteProject: (project: MarketProject) => void;
  categories: Category[];
  categoryDraft: Category;
  onCategoryDraft: (value: Category) => void;
  onSaveCategory: () => void;
  onDeleteCategory: (category: Category) => void;
  skills: MarketSkill[];
  plugins: MarketPlugin[];
  canManageProject: (slug: string) => boolean;
  canManageSkill: (skill: MarketSkill) => boolean;
  canManagePlugin: (plugin: MarketPlugin) => boolean;
  onArchiveSkill: (skill: MarketSkill) => void;
  onArchivePlugin: (plugin: MarketPlugin) => void;
}) {
  const [archiveQuery, setArchiveQuery] = useState("");
  const [publishKind, setPublishKind] = useState<AdminArtifactKind>("skill");
  const [archiveKind, setArchiveKind] = useState<AdminArtifactKind>("skill");
  const selectedDraft = props.drafts.find((draft) => draft.gitlabSourcePath === props.selectedDraftPath);
  const selectedPluginDraft = props.pluginDrafts.find((draft) => draft.gitlabSourcePath === props.selectedPluginDraftPath);
  const isSystem = props.session?.role === "system";
  const manageableProjects = normalizeProjectList(props.projects).filter((project) => props.canManageProject(project.slug));
  const manageableSkills = props.skills.filter((skill) => props.canManageSkill(skill));
  const manageablePlugins = props.plugins.filter((plugin) => props.canManagePlugin(plugin));
  const updateMeta = <K extends keyof PublishMeta>(key: K, value: PublishMeta[K]) =>
    props.onMeta({ ...props.meta, [key]: value });
  const updatePluginMeta = <K extends keyof PublishMeta>(key: K, value: PublishMeta[K]) =>
    props.onPluginMeta({ ...props.pluginMeta, [key]: value });
  const projectOptions = manageableProjects;
  const activeGovernanceTab: GovernanceTab = isSystem ? props.governanceTab : "project";
  const selectedDraftPublished = isPublishedDraft(selectedDraft);
  const selectedDraftNeedsSource = Boolean(selectedDraft && !selectedDraft.sourceAvailable);
  const missingMetaMessage = publishMetaMissingMessage(props.meta);
  const metaIncomplete = Boolean(selectedDraft && missingMetaMessage);
  const canPublishSelectedDraft = Boolean(
    selectedDraft && selectedDraft.sourceAvailable && !selectedDraftPublished && !metaIncomplete
  );
  const selectedPluginDraftPublished = Boolean(
    selectedPluginDraft &&
      (selectedPluginDraft.status === "published" || selectedPluginDraft.status === "已发布") &&
      selectedPluginDraft.publishedVersion === selectedPluginDraft.version
  );
  const pluginMissingMetaMessage = publishMetaMissingMessage(props.pluginMeta, "plugin");
  const pluginReadmeIncomplete = Boolean(
    selectedPluginDraft && selectedPluginDraft.sourceAvailable && !selectedPluginDraft.readmeMetadataComplete
  );
  const pluginMetaIncomplete = Boolean(selectedPluginDraft && (pluginMissingMetaMessage || pluginReadmeIncomplete));
  const canPublishSelectedPluginDraft = Boolean(
    selectedPluginDraft &&
      selectedPluginDraft.sourceAvailable &&
      !selectedPluginDraftPublished &&
      !pluginReadmeIncomplete &&
      !pluginMetaIncomplete
  );
  const sessionName = props.session?.name?.trim();
  const sessionRoleLabel = props.session?.role === "system" ? "system" : "project";
  const sessionShortMac = props.session?.macAddress?.slice(-8);
  const sessionTitle = [
    sessionName,
    `role: ${props.session?.role ?? "unknown"}`,
    props.session?.macAddress ? `mac: ${props.session.macAddress}` : null
  ]
    .filter(Boolean)
    .join(" · ");
  const publishTitle = !selectedDraft
    ? "请选择草稿"
    : selectedDraftNeedsSource
      ? "需要 GitLab 源文件才能发布"
      : selectedDraftPublished
        ? "当前版本已发布"
        : metaIncomplete
          ? missingMetaMessage
          : "发布到市场";
  const pluginPublishTitle = !selectedPluginDraft
    ? "请选择 Plugin 草稿"
    : !selectedPluginDraft.sourceAvailable
      ? "缺少通用插件源文件"
      : selectedPluginDraftPublished
        ? "当前版本已发布"
        : pluginMetaIncomplete
          ? pluginReadmeIncomplete
            ? "README.md 元数据待补充"
            : pluginMissingMetaMessage
          : "发布到市场";
  const normalizedArchiveQuery = archiveQuery.trim().toLocaleLowerCase();
  const archiveProjectName = (slug: string) =>
    props.projects.find((project) => project.slug === slug)?.name ?? slug;
  const archivePublicCategoryName = (slug: string) =>
    props.categories.find((category) => category.id === slug)?.name ?? categoryNameFromSlug(slug);
  const archiveMatchesQuery = (artifact: MarketSkill | MarketPlugin) => {
    if (!normalizedArchiveQuery) return true;
    return [
      artifact.name,
      artifact.id,
      artifact.namespace,
      artifact.summary,
      artifact.latestVersion,
      artifact.tags.join(" "),
      artifact.categories.join(" "),
      ...artifact.categories.map((category) =>
        category.startsWith("project:")
          ? archiveProjectName(category.slice("project:".length))
          : archivePublicCategoryName(category)
      )
    ]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalizedArchiveQuery);
  };
  const archiveSkills = manageableSkills
    .filter(archiveMatchesQuery)
    .sort((first, second) => first.name.localeCompare(second.name, undefined, { sensitivity: "base" }));
  const archivePlugins = manageablePlugins
    .filter(archiveMatchesQuery)
    .sort((first, second) => first.name.localeCompare(second.name, undefined, { sensitivity: "base" }));
  const activeArchiveItems = archiveKind === "skill" ? archiveSkills : archivePlugins;
  const archivePublicGroups = new Map<string, Array<MarketSkill | MarketPlugin>>();
  const archiveProjectGroups = new Map<string, Array<MarketSkill | MarketPlugin>>();
  for (const artifact of activeArchiveItems) {
    const projectCategory = artifact.categories.find((category) => category.startsWith("project:"));
    if (projectCategory) {
      const slug = projectCategory.slice("project:".length);
      if (!archiveProjectGroups.has(slug)) {
        archiveProjectGroups.set(slug, []);
      }
      archiveProjectGroups.get(slug)!.push(artifact);
      continue;
    }

    const publicCategories = artifact.categories.filter((category) => !category.startsWith("project:"));
    const category = publicCategories[0] ?? "uncategorized";
    if (!archivePublicGroups.has(category)) {
      archivePublicGroups.set(category, []);
    }
    archivePublicGroups.get(category)!.push(artifact);
  }
  const archiveTotalCount = activeArchiveItems.length;
  const archiveKindLabel = archiveKind === "skill" ? "skill" : "plugin";
  const manageableArchiveCount = archiveKind === "skill" ? manageableSkills.length : manageablePlugins.length;

  return (
    <section className="admin-console">
      <div className="admin-header">
        <div className="admin-title">
          <p>PUBLISHING CONTROL</p>
          <h2>管理发布</h2>
        </div>
        <div className="admin-session-compact">
          <span className="session-indicator">
            <span className="session-dot"></span>
            MinIO Live Draft 已下架草稿已同步
          </span>
          <button className="session-info-btn" title={sessionTitle || "查看会话详情"}>
            <span className={`session-role-badge ${sessionName ? "named" : ""}`}>
              {sessionName || sessionRoleLabel}
            </span>
            {!sessionName && sessionShortMac ? <span className="session-id">{sessionShortMac}</span> : null}
          </button>
        </div>
      </div>

      <div className="admin-layout">
        <aside className="admin-rail">
          <button
            className={props.activeTab === "projects" ? "active" : ""}
            onClick={() => props.onActiveTab("projects")}
          >
            <FolderGit2 size={17} />
            项目治理
          </button>
          <button
            className={props.activeTab === "drafts" ? "active" : ""}
            onClick={() => props.onActiveTab("drafts")}
          >
            <FileText size={17} />
            草稿发布
          </button>
          <button
            className={props.activeTab === "archive" ? "active" : ""}
            onClick={() => props.onActiveTab("archive")}
          >
            <Archive size={17} />
            市场下架
          </button>
          {isSystem ? (
            <button
              className={props.activeTab === "audit" ? "active" : ""}
              onClick={() => props.onActiveTab("audit")}
            >
              <ScrollText size={17} />
              审计记录
            </button>
          ) : null}
        </aside>

        <div className="admin-workspace">
          {props.activeTab === "projects" ? (
            <div className="admin-panels governance">
              <section className="admin-panel governance-panel">
                <div className="section-toolbar">
                  <div>
                    <h2>项目治理</h2>
                    <p>{isSystem ? "维护市场项目和公共分类" : "维护所有市场项目"}</p>
                  </div>
                  <div className="segmented governance-tabs" aria-label="治理类型">
                    <button
                      className={activeGovernanceTab === "project" ? "active" : ""}
                      onClick={() => props.onGovernanceTab("project")}
                    >
                      项目
                    </button>
                    {isSystem ? (
                      <button
                        className={activeGovernanceTab === "general" ? "active" : ""}
                        onClick={() => props.onGovernanceTab("general")}
                      >
                        公共
                      </button>
                    ) : null}
                  </div>
                </div>

                {activeGovernanceTab === "project" ? (
                  <div className="governance-board">
                    <div className="governance-board-head">
                      <div>
                        <h3>项目</h3>
                        <span>{manageableProjects.length} 个可管理项目</span>
                      </div>
                      <button
                        className="primary-action compact"
                        onClick={() => {
                          props.onProjectDraft({ ...emptyMarketProject(), order: nextProjectOrder(props.projects) });
                          props.onGovernanceDialog({ kind: "project-create" });
                        }}
                      >
                        <Plus size={17} />
                        新增项目
                      </button>
                    </div>
                    <div className="governance-list">
                      {manageableProjects.map((project) => (
                        <article className="governance-row project-row" key={project.slug}>
                          <div>
                            <strong>{project.name}</strong>
                            <span>
                              {project.slug} · 排序 {project.order} · {project.description || "无描述"}
                            </span>
                          </div>
                          <div className="row-actions">
                            <button
                              className="icon-button"
                              onClick={() => {
                                props.onProjectDraft({ ...project });
                                props.onGovernanceDialog({ kind: "project-edit", project });
                              }}
                              title="编辑项目"
                            >
                              <Pencil size={16} />
                            </button>
                            <button
                              className="icon-button danger"
                              onClick={() => props.onGovernanceDialog({ kind: "project-delete", project })}
                              title="删除项目"
                            >
                              <Trash2 size={16} />
                            </button>
                          </div>
                        </article>
                      ))}
                      {manageableProjects.length === 0 ? (
                        <div className="empty-state compact">暂无市场项目。</div>
                      ) : null}
                    </div>
                  </div>
                ) : null}

                {isSystem && activeGovernanceTab === "general" ? (
                  <div className="governance-board">
                    <div className="governance-board-head">
                      <div>
                        <h3>公共分类</h3>
                        <span>{props.categories.length} 个公共分类</span>
                      </div>
                      <button
                        className="primary-action compact"
                        onClick={() => {
                          props.onCategoryDraft({ ...emptyMarketCategory(), order: nextCategoryOrder(props.categories) });
                          props.onGovernanceDialog({ kind: "category-create" });
                        }}
                      >
                        <Plus size={17} />
                        新增分类
                      </button>
                    </div>
                    <div className="governance-list">
                      {props.categories.map((category, index) => {
                        const categoryName = category.name.trim() || category.id.trim() || "未命名分类";
                        const categoryId = category.id.trim() || "未设置 slug";
                        return (
                          <article className="governance-row category-row" key={category.id || `${categoryName}-${index}`}>
                            <div>
                              <strong>{categoryName}</strong>
                              <span>{categoryId} · 排序 {category.order}</span>
                            </div>
                            <div className="row-actions">
                              <button
                                className="icon-button"
                                onClick={() => {
                                  props.onCategoryDraft({ ...category });
                                  props.onGovernanceDialog({ kind: "category-edit", category });
                                }}
                                title="编辑公共分类"
                              >
                                <Pencil size={16} />
                              </button>
                              <button
                                className="icon-button danger"
                                onClick={() => props.onGovernanceDialog({ kind: "category-delete", category })}
                                title="删除公共分类"
                              >
                                <Trash2 size={16} />
                              </button>
                            </div>
                          </article>
                        );
                      })}
                    </div>
                  </div>
                ) : null}
              </section>
            </div>
          ) : null}

          {props.activeTab === "drafts" ? (
            <div className="admin-panels drafts">
              <section className="admin-panel draft-browser">
                <div className="section-toolbar">
                  <div>
                    <h2>草稿区</h2>
                  </div>
                  <div className="draft-kind-controls">
                    <div className="segmented" aria-label="草稿类型">
                      <button
                        className={publishKind === "skill" ? "active" : ""}
                        onClick={() => setPublishKind("skill")}
                      >
                        Skill
                      </button>
                      <button
                        className={publishKind === "plugin" ? "active" : ""}
                        onClick={() => setPublishKind("plugin")}
                      >
                        Plugin
                      </button>
                    </div>
                    <button
                      className="icon-button"
                      onClick={publishKind === "skill" ? props.onRefreshDrafts : props.onRefreshPluginDrafts}
                      title={publishKind === "skill" ? "刷新 skill 草稿" : "刷新 plugin 草稿"}
                    >
                      <RefreshCw size={16} />
                    </button>
                  </div>
                </div>
                <div className="draft-list">
                  {publishKind === "skill" ? (
                    props.drafts.length === 0 ? (
                      <div className="empty-state compact">暂无 skill 草稿。请确认 GitLab 已同步到 MinIO 草稿前缀。</div>
                    ) : (
                      <DraftList
                        drafts={props.drafts}
                        selectedDraftPath={props.selectedDraftPath}
                        onSelectDraft={props.onSelectDraft}
                      />
                    )
                  ) : props.pluginDrafts.length === 0 ? (
                    <div className="empty-state compact">暂无 plugin 草稿。请确认 GitLab 已同步到 draft/gitlab/plugins/ 前缀。</div>
                  ) : (
                    <PluginDraftList
                      drafts={props.pluginDrafts}
                      selectedDraftPath={props.selectedPluginDraftPath}
                      onSelectDraft={props.onSelectPluginDraft}
                    />
                  )}
                </div>
              </section>

              <section className="admin-panel publish-editor">
                <div className="section-toolbar">
                  <div>
                    <h2>
                      {publishKind === "skill"
                        ? selectedDraft
                          ? draftSkillLabel(selectedDraft)
                          : "Skill 发布"
                        : selectedPluginDraft
                          ? pluginDraftLabel(selectedPluginDraft)
                          : "Plugin 发布"}
                    </h2>
                    <p>
                      {publishKind === "skill"
                        ? selectedDraft?.version
                          ? `version ${selectedDraft.version}`
                          : "选择 skill 草稿后编辑"
                        : selectedPluginDraft?.version
                          ? `version ${selectedPluginDraft.version}`
                          : "选择 plugin 草稿后编辑"}
                    </p>
                  </div>
                  {publishKind === "skill" ? (
                    <Badge>{selectedDraft?.author ?? "等待选择"}</Badge>
                  ) : (
                    <Badge strong={selectedPluginDraft?.status === "ready_to_publish"}>
                      {selectedPluginDraft ? pluginDraftStatusLabel(selectedPluginDraft.status) : "等待选择"}
                    </Badge>
                  )}
                </div>

                <div className="publish-scroll">
                  {publishKind === "skill" ? (
                    selectedDraft ? (
                      <>
                        <div className="meta-form">
                          <label className="text-field">
                            <span>skill_id（只读）</span>
                            <input value={props.meta.skillId} readOnly disabled />
                          </label>
                          <label className="text-field">
                            <span>名称</span>
                            <input value={props.meta.name} onChange={(event) => updateMeta("name", event.target.value)} />
                          </label>
                          <label className="text-field wide">
                            <span>摘要</span>
                            <input value={props.meta.summary} onChange={(event) => updateMeta("summary", event.target.value)} />
                          </label>
                          <label className="text-field">
                            <span>标签，逗号分隔</span>
                            <input
                              value={props.meta.tags.join(", ")}
                              onChange={(event) => updateMeta("tags", splitCsv(event.target.value))}
                            />
                          </label>
                          <label className="text-field">
                            <span>发布范围</span>
                            <select
                              value={props.meta.publishScope}
                              onChange={(event) => updateMeta("publishScope", event.target.value)}
                            >
                              {isSystem ? <option value="public">公共</option> : null}
                              <option value="project">项目</option>
                            </select>
                          </label>
                          {props.meta.publishScope === "project" ? (
                            <label className="text-field">
                              <span>项目</span>
                              <select
                                value={props.meta.publishProjectSlug ?? ""}
                                onChange={(event) => updateMeta("publishProjectSlug", event.target.value)}
                              >
                                <option value="">选择项目</option>
                                {projectOptions.map((project) => (
                                  <option key={project.slug} value={project.slug}>
                                    {project.name}
                                  </option>
                                ))}
                              </select>
                            </label>
                          ) : (
                            <label className="text-field">
                              <span>公共分类</span>
                              <select
                                value={props.meta.publishCategorySlug ?? ""}
                                onChange={(event) => updateMeta("publishCategorySlug", event.target.value)}
                              >
                                <option value="">选择公共分类</option>
                                {props.categories.map((category) => (
                                  <option key={category.id} value={category.id}>
                                    {category.name}
                                  </option>
                                ))}
                              </select>
                            </label>
                          )}
                          <label className="text-field wide">
                            <span>变更说明</span>
                            <input value={props.meta.changelog} onChange={(event) => updateMeta("changelog", event.target.value)} />
                          </label>
                        </div>

                        {!selectedDraft.sourceAvailable ? (
                          <div className="conflict-note warning">
                            <AlertCircle size={17} />
                            <div>
                              <strong>该草稿由市场下架生成，暂未关联 GitLab 源文件</strong>
                              <p>
                                如果需要编辑和预览，请按以下步骤操作：<br/>
                                1. 确保 GitLab 已重新同步该 skill 的 SKILL.md 文件到 MinIO 草稿区<br/>
                                2. 点击草稿区的"刷新"按钮，更新草稿列表<br/>
                                3. 源文件关联后即可预览和编辑
                              </p>
                              <p className="conflict-note-hint">
                                提示：如果只是误下架需要快速恢复，可以直接点击"快速重新上架"按钮，无需等待 GitLab 同步。市场中的 skill 包文件仍然存在，该操作只会更新目录关联。
                              </p>
                            </div>
                          </div>
                        ) : (
                          <div className={`publish-readiness ${metaIncomplete ? "warning" : "ready"}`}>
                            {metaIncomplete ? <AlertCircle size={17} /> : <CheckCircle2 size={17} />}
                            <span>
                              {metaIncomplete
                                ? `${missingMetaMessage}后再发布。`
                                : "发布元数据已具备基础信息，可以预览或发布。"}
                            </span>
                          </div>
                        )}
                      </>
                    ) : (
                      <div className="publish-empty-state">
                        <FileText size={28} />
                        <strong>等待选择 Skill 草稿</strong>
                        <span>左侧草稿载入后会显示发布元数据。</span>
                      </div>
                    )
                  ) : selectedPluginDraft ? (
                    <>
                      <div className="meta-form">
                        <label className="text-field">
                          <span>plugin_id（只读）</span>
                          <input value={props.pluginMeta.skillId} readOnly disabled />
                        </label>
                        <label className="text-field">
                          <span>名称</span>
                          <input value={props.pluginMeta.name} onChange={(event) => updatePluginMeta("name", event.target.value)} />
                        </label>
                        <label className="text-field">
                          <span>版本</span>
                          <input
                            value={props.pluginMeta.version ?? ""}
                            onChange={(event) => updatePluginMeta("version", event.target.value)}
                          />
                        </label>
                        <label className="text-field wide">
                          <span>摘要</span>
                          <input value={props.pluginMeta.summary} onChange={(event) => updatePluginMeta("summary", event.target.value)} />
                        </label>
                        <label className="text-field">
                          <span>标签，逗号分隔</span>
                          <input
                            value={props.pluginMeta.tags.join(", ")}
                            onChange={(event) => updatePluginMeta("tags", splitCsv(event.target.value))}
                          />
                        </label>
                        <label className="text-field">
                          <span>目标平台，逗号分隔</span>
                          <input
                            value={props.pluginMeta.targets.join(", ")}
                            onChange={(event) => updatePluginMeta("targets", splitCsv(event.target.value))}
                          />
                        </label>
                        <label className="text-field">
                          <span>作用域，逗号分隔</span>
                          <input
                            value={props.pluginMeta.levels.join(", ")}
                            onChange={(event) => updatePluginMeta("levels", splitCsv(event.target.value))}
                          />
                        </label>
                        <label className="text-field">
                          <span>发布范围</span>
                          <select
                            value={props.pluginMeta.publishScope}
                            onChange={(event) => updatePluginMeta("publishScope", event.target.value)}
                          >
                            {isSystem ? <option value="public">公共</option> : null}
                            <option value="project">项目</option>
                          </select>
                        </label>
                        {props.pluginMeta.publishScope === "project" ? (
                          <label className="text-field">
                            <span>项目</span>
                            <select
                              value={props.pluginMeta.publishProjectSlug ?? ""}
                              onChange={(event) => updatePluginMeta("publishProjectSlug", event.target.value)}
                            >
                              <option value="">选择项目</option>
                              {projectOptions.map((project) => (
                                <option key={project.slug} value={project.slug}>
                                  {project.name}
                                </option>
                              ))}
                            </select>
                          </label>
                        ) : (
                          <label className="text-field">
                            <span>公共分类</span>
                            <select
                              value={props.pluginMeta.publishCategorySlug ?? ""}
                              onChange={(event) => updatePluginMeta("publishCategorySlug", event.target.value)}
                            >
                              <option value="">选择公共分类</option>
                              {props.categories.map((category) => (
                                <option key={category.id} value={category.id}>
                                  {category.name}
                                </option>
                              ))}
                            </select>
                          </label>
                        )}
                        <label className="text-field wide">
                          <span>变更说明</span>
                          <input
                            value={props.pluginMeta.changelog}
                            onChange={(event) => updatePluginMeta("changelog", event.target.value)}
                          />
                        </label>
                      </div>

                      <div className="plugin-draft-summary">
                        <div>
                          <span>namespace / id</span>
                          <strong>
                            {selectedPluginDraft.namespace ?? "unknown"} / {selectedPluginDraft.pluginId ?? selectedPluginDraft.draftSlug ?? "unknown"}
                          </strong>
                        </div>
                        <div>
                          <span>GitLab 分类</span>
                          <strong>{selectedPluginDraft.gitlabCategoryPath.join(" / ") || "未分类"}</strong>
                        </div>
                        <div>
                          <span>支持平台</span>
                          <strong>{selectedPluginDraft.targets.join(" / ") || "未声明"}</strong>
                        </div>
                        <div>
                          <span>风险</span>
                          <strong>{selectedPluginDraft.riskLevel ?? "待计算"}</strong>
                        </div>
                      </div>

                      {!selectedPluginDraft.sourceAvailable ? (
                        <div className="conflict-note warning">
                          <AlertCircle size={17} />
                          <div>
                            <strong>缺少通用插件源文件，暂时无法发布</strong>
                            <p>请确认 GitLab 已同步 README、skills 或其他插件源文件；Codex 和 Claude 平台目录由发布器动态生成，不需要提交到草稿区。</p>
                          </div>
                        </div>
                      ) : (
                        <div className={`publish-readiness ${pluginMetaIncomplete ? "warning" : "ready"}`}>
                          {pluginMetaIncomplete ? <AlertCircle size={17} /> : <CheckCircle2 size={17} />}
                          <span>
                            {pluginMetaIncomplete
                              ? pluginReadmeIncomplete
                                ? "README.md 需要包含 name、description、version、author 后再发布。"
                                : `${pluginMissingMetaMessage}后再发布。`
                              : "发布元数据已具备基础信息，可以预览或发布。"}
                          </span>
                        </div>
                      )}
                    </>
                  ) : (
                    <div className="publish-empty-state">
                      <PackageCheck size={28} />
                      <strong>等待选择 Plugin 草稿</strong>
                      <span>左侧草稿载入后会显示发布元数据。</span>
                    </div>
                  )}
                </div>

                <div className="button-line publish-actions">
                  {publishKind === "skill" ? (
                    <>
                      <button className="primary-soft" onClick={props.onSaveMeta} disabled={!selectedDraft}>
                        <Save size={17} />
                        保存元数据
                      </button>
                      <button
                        className="primary-soft"
                        onClick={props.onPreview}
                        disabled={!selectedDraft || !selectedDraft.sourceAvailable}
                      >
                        <BookOpen size={17} />
                        预览草稿
                      </button>
                      {selectedDraftPublished ? (
                        <span className="publish-status-note">
                          <CheckCircle2 size={16} />
                          当前版本已发布
                        </span>
                      ) : selectedDraft && !selectedDraft.sourceAvailable && selectedDraft.status === "已下架" ? (
                        <button
                          className="primary-action compact"
                          onClick={props.onQuickRepublish}
                          title="无需 GitLab 源文件，直接重新上架已有版本"
                        >
                          <Rocket size={17} />
                          快速重新上架
                        </button>
                      ) : (
                        <button
                          className="primary-action compact"
                          onClick={props.onPublish}
                          disabled={!canPublishSelectedDraft}
                          title={publishTitle}
                        >
                          <Rocket size={17} />
                          {selectedDraft && !selectedDraft.sourceAvailable ? "重新上架（需要源文件）" : "发布到市场"}
                        </button>
                      )}
                    </>
                  ) : (
                    <>
                      <button className="primary-soft" onClick={props.onSavePluginMeta} disabled={!selectedPluginDraft}>
                        <Save size={17} />
                        保存元数据
                      </button>
                      <button
                        className="primary-soft"
                        onClick={props.onPreviewPlugin}
                        disabled={!selectedPluginDraft}
                      >
                        <BookOpen size={17} />
                        预览草稿
                      </button>
                      {selectedPluginDraftPublished ? (
                        <span className="publish-status-note">
                          <CheckCircle2 size={16} />
                          当前版本已发布
                        </span>
                      ) : (
                        <button
                          className="primary-action compact"
                          onClick={props.onPublishPlugin}
                          disabled={!canPublishSelectedPluginDraft}
                          title={pluginPublishTitle}
                        >
                          <Rocket size={17} />
                          发布到市场
                        </button>
                      )}
                    </>
                  )}
                </div>
              </section>
            </div>
          ) : null}

          {props.activeTab === "archive" ? (
            <div className="admin-panels archive">
              <section className="admin-panel archive-panel">
                <div className="section-toolbar">
                  <div>
                    <h2>市场下架</h2>
                    <p>
                      {isSystem
                        ? `按公共分类和项目查看可下架 ${archiveKindLabel}`
                        : `按项目查看可下架 ${archiveKindLabel}`}
                    </p>
                  </div>
                  <Badge strong>{archiveTotalCount} {archiveKindLabel}s</Badge>
                </div>
                <div className="archive-controls">
                  <div className="segmented" aria-label="下架类型">
                    <button
                      className={archiveKind === "skill" ? "active" : ""}
                      onClick={() => setArchiveKind("skill")}
                    >
                      Skill
                    </button>
                    <button
                      className={archiveKind === "plugin" ? "active" : ""}
                      onClick={() => setArchiveKind("plugin")}
                    >
                      Plugin
                    </button>
                  </div>
                  <label className="search-box archive-search-box">
                    <Search size={16} />
                    <input
                      value={archiveQuery}
                      onChange={(event) => setArchiveQuery(event.target.value)}
                      placeholder={`搜索 ${archiveKindLabel}、命名空间、分类或项目`}
                    />
                  </label>
                </div>
                <div className="archive-market-list">
                  {isSystem && archivePublicGroups.size > 0 ? (
                    <ArchiveScopeGroup
                      kind={archiveKind}
                      title="公共市场"
                      groups={archivePublicGroups}
                      labelForGroup={archivePublicCategoryName}
                      onArchiveSkill={props.onArchiveSkill}
                      onArchivePlugin={props.onArchivePlugin}
                    />
                  ) : null}
                  {archiveProjectGroups.size > 0 ? (
                    <ArchiveScopeGroup
                      kind={archiveKind}
                      title="项目市场"
                      groups={archiveProjectGroups}
                      labelForGroup={archiveProjectName}
                      onArchiveSkill={props.onArchiveSkill}
                      onArchivePlugin={props.onArchivePlugin}
                    />
                  ) : null}
                  {manageableArchiveCount === 0 ? (
                    <div className="empty-state compact">{`当前角色没有可下架的市场 ${archiveKindLabel}。`}</div>
                  ) : archiveTotalCount === 0 ? (
                    <div className="empty-state compact">{`没有匹配搜索条件的可下架 ${archiveKindLabel}。`}</div>
                  ) : null}
                </div>
              </section>
            </div>
          ) : null}

          {isSystem && props.activeTab === "audit" ? (
            <div className="admin-panels audit">
              <section className="admin-panel audit-panel">
                <div className="section-toolbar">
                  <div>
                    <h2>审计记录</h2>
                    <p>最近 100 条管理员写操作，按创建时间倒序显示。</p>
                  </div>
                  <button className="icon-button" onClick={props.onRefreshAuditLogs} title="刷新审计记录">
                    <RefreshCw size={16} />
                  </button>
                </div>
                <AuditLogList logs={props.auditLogs} />
              </section>
            </div>
          ) : null}
        </div>
      </div>
      {props.governanceDialog ? (
        <GovernanceDialogView
          dialog={props.governanceDialog}
          projectDraft={props.projectDraft}
          onProjectDraft={props.onProjectDraft}
          onSaveProject={props.onSaveProject}
          categoryDraft={props.categoryDraft}
          onCategoryDraft={props.onCategoryDraft}
          onSaveCategory={props.onSaveCategory}
          onDeleteProject={props.onDeleteProject}
          onDeleteCategory={props.onDeleteCategory}
          busy={props.busy}
          error={props.governanceDialogError}
          onClose={() => props.onGovernanceDialog(null)}
        />
      ) : null}
    </section>
  );
}

export function ArchiveScopeGroup(props: {
  kind: AdminArtifactKind;
  title: string;
  groups: Map<string, Array<MarketSkill | MarketPlugin>>;
  labelForGroup: (key: string) => string;
  onArchiveSkill: (skill: MarketSkill) => void;
  onArchivePlugin: (plugin: MarketPlugin) => void;
}) {
  const entries = Array.from(props.groups.entries()).sort((first, second) =>
    props
      .labelForGroup(first[0])
      .localeCompare(props.labelForGroup(second[0]), undefined, { sensitivity: "base" })
  );
  const total = entries.reduce((sum, [, skills]) => sum + skills.length, 0);

  return (
    <section className="archive-scope-group">
      <div className="archive-scope-head">
        <Layers3 size={16} />
        <strong>{props.title}</strong>
        <span>{total}</span>
      </div>
      <div className="archive-category-stack">
        {entries.map(([groupKey, skills]) => (
          <div className="archive-category-group" key={`${props.title}:${groupKey}`}>
            <div className="archive-category-header">
              <FolderGit2 size={16} />
              <strong>{props.labelForGroup(groupKey)}</strong>
              <span>{skills.length}</span>
            </div>
            <div className="archive-skill-list">
              {skills.map((artifact) => (
                <div
                  className="archive-market-row"
                  key={props.kind === "skill" ? skillKey(artifact as MarketSkill) : pluginKey(artifact as MarketPlugin)}
                >
                  <span className="archive-skill-icon" aria-hidden="true">
                    {props.kind === "skill" ? <FileText size={15} /> : <PackageCheck size={15} />}
                  </span>
                  <div className="archive-skill-main">
                    <strong>{artifact.name}</strong>
                    <span>
                      {artifact.namespace}/{artifact.id} · {artifact.latestVersion}
                    </span>
                  </div>
                  <button
                    className="archive-action-button"
                    onClick={() =>
                      props.kind === "skill"
                        ? props.onArchiveSkill(artifact as MarketSkill)
                        : props.onArchivePlugin(artifact as MarketPlugin)
                    }
                    title={`下架 ${artifact.name}`}
                  >
                    <Archive size={15} />
                    下架
                  </button>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
