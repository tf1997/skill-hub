import { useMemo, useState } from "react";
import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, Power, RefreshCw, Rocket, Save, Search, Settings, ShieldCheck, Trash2, X } from "lucide-react";
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
import { defaultMetaFromDraft, defaultMetaFromPluginDraft, draftCategoryLabel, draftPrimaryCategory, draftSearchText, draftSecondaryCategory, draftSkillLabel, draftStatusClass, draftStatusFilterLabels, draftStatusFilterOrder, normalizeMetaForSave, pluginDraftCategoryPath, pluginDraftLabel, pluginDraftPrimaryCategory, pluginDraftSearchText, pluginDraftSecondaryCategory, pluginDraftStatusClass, pluginDraftStatusLabel, publishMetaMissingMessage, sortDrafts, sortPluginDrafts, splitCsv } from "../../lib/adminDrafts";
import type { AdminArtifactKind, DraftStatusFilter, DraftStatusKey } from "../../lib/adminDrafts";
import { levelLabels, pluginKey, skillKey, targetLabels } from "../../app/viewModel";
import type { AdminTab, GovernanceDialog, GovernanceTab, InstalledArtifactKind, InstalledTab, MarketArtifactKind, MarketMode, UpdateArtifactKind, UpdateStatusFilter } from "../../app/viewModel";

export function InstalledView(props: {
  bindings: SkillBinding[];
  cachedSkills: CachedSkillItem[];
  pluginPackages: AppBootstrap["pluginPackages"];
  pluginBindings: AppBootstrap["pluginBindings"];
  localPlugins: AppBootstrap["localPlugins"];
  onTogglePlugin: (binding: AppBootstrap["pluginBindings"][number]) => void;
  onUninstallPlugin: (binding: AppBootstrap["pluginBindings"][number]) => void;
  onPreviewPluginBinding: (binding: AppBootstrap["pluginBindings"][number]) => void;
  onPreviewPluginCache: (item: AppBootstrap["pluginPackages"][number]) => void;
  onDeletePluginCache: (item: AppBootstrap["pluginPackages"][number]) => void;
  onPreviewLocalPlugin: (plugin: AppBootstrap["localPlugins"][number]) => void;
  localSkills: LocalSkill[];
  onToggle: (binding: SkillBinding) => void;
  onToggleLocal: (skill: LocalSkill) => void;
  onUninstall: (binding: SkillBinding) => void;
  onScan: () => void;
  onPreviewBinding: (binding: SkillBinding) => void;
  onPreviewLocal: (skill: LocalSkill) => void;
  onPreviewCache: (item: CachedSkillItem) => void;
  onDeleteCache: (item: CachedSkillItem) => void;
  onDeleteLocal: (skill: LocalSkill) => void;
  onImportLocal: (skill: LocalSkill) => void;
  onInstallLocal: (skill: LocalSkill) => void;
  onInstallCache: (item: CachedSkillItem) => void;
}) {
  const [activeTab, setActiveTab] = useState<InstalledTab>("bindings");
  const [artifactKind, setArtifactKind] = useState<InstalledArtifactKind>("skill");
  const inferredLocalBindings = useMemo(
    () =>
      props.cachedSkills.flatMap((item) =>
        localCachedInstallations(item.package, props.localSkills)
          .filter((skill) => !hasBindingForLocalSkill(item.package, skill, props.bindings))
          .map((skill) => ({
            key: `${item.key}:${skill.id}`,
            package: item.package,
            skill
          }))
      ),
    [props.cachedSkills, props.localSkills, props.bindings]
  );
  const bindingMatrixCount = props.bindings.length + inferredLocalBindings.length;
  const skillStateCount = bindingMatrixCount + props.cachedSkills.length + props.localSkills.length;
  const pluginStateCount = props.pluginBindings.length + props.pluginPackages.length + props.localPlugins.length;
  const bindingTabCount = artifactKind === "skill" ? bindingMatrixCount : props.pluginBindings.length;
  const cacheTabCount = artifactKind === "skill" ? props.cachedSkills.length : props.pluginPackages.length;
  const localTabCount = artifactKind === "skill" ? props.localSkills.length : props.localPlugins.length;
  const artifactLabel = artifactKind === "skill" ? "Skill" : "Plugin";
  const activeTitle =
    activeTab === "bindings"
      ? `${artifactLabel} 生效矩阵`
      : activeTab === "cache"
        ? `${artifactLabel} 本地缓存`
        : `${artifactLabel} 本地已有`;
  const activeDescription =
    activeTab === "bindings"
      ? `只展示 ${artifactLabel} 的启用状态，便于检查平台、范围、版本和冲突。`
      : activeTab === "cache"
        ? `已下载但不一定生效的 ${artifactLabel} 包，删除缓存不会卸载已安装目录。`
        : `扫描个人级和项目级目录中已有的 ${artifactLabel}。`;
  const artifactTabs = [
    { key: "skill" as const, label: "Skill", count: skillStateCount },
    { key: "plugin" as const, label: "Plugin", count: pluginStateCount }
  ];
  const tabs = [
    { key: "bindings" as const, label: "生效矩阵", count: bindingTabCount },
    { key: "cache" as const, label: "本地缓存", count: cacheTabCount },
    { key: "local" as const, label: "本地已有", count: localTabCount }
  ];
  const activeCount =
    activeTab === "bindings"
      ? bindingTabCount
      : activeTab === "cache"
        ? cacheTabCount
        : localTabCount;

  return (
    <section className="content-stack installed-view">
      <div className="section-toolbar">
        <div className="section-heading">
          <div className="section-title-line">
            <h2>{activeTitle}</h2>
            <Badge strong={activeCount > 0}>{activeCount} 项</Badge>
          </div>
          <p>{activeDescription}</p>
        </div>
        {activeTab === "local" ? (
          <div className="toolbar-actions">
            <button className="primary-soft" onClick={props.onScan}>
              <ShieldCheck size={17} />
              扫描{artifactLabel}
            </button>
          </div>
        ) : null}
      </div>

      <div className="local-filter-bar">
        <div className="segmented" role="tablist" aria-label="本地对象类型">
          {artifactTabs.map((tab) => (
            <button
              key={tab.key}
              className={artifactKind === tab.key ? "active" : ""}
              onClick={() => setArtifactKind(tab.key)}
              role="tab"
              aria-selected={artifactKind === tab.key}
            >
              {tab.label}
              <Badge>{tab.count}</Badge>
            </button>
          ))}
        </div>
      </div>

      <div className="tab-strip" role="tablist" aria-label={`${artifactLabel} 本地视图`}>
        {tabs.map((tab) => (
          <button
            key={tab.key}
            className={activeTab === tab.key ? "active" : ""}
            onClick={() => {
              setActiveTab(tab.key);
              if (tab.key === "local") {
                void props.onScan();
              }
            }}
            role="tab"
            aria-selected={activeTab === tab.key}
          >
            {tab.label}
            <Badge>{tab.count}</Badge>
          </button>
        ))}
      </div>

      {activeTab === "bindings" ? (
        <div className="data-table">
          <div className="table-head">
            <span>{artifactLabel}</span>
            <span>平台</span>
            <span>范围</span>
            <span>版本</span>
            <span>状态</span>
            <span>操作</span>
          </div>
          {bindingTabCount > 0 ? (
            <>
              {artifactKind === "skill" ? (
                <>
                  {props.bindings.map((binding) => (
                    <div className="table-row" key={binding.id}>
                      <span>
                        <strong className="skill-title-line">
                          {binding.skillName}
                          <SourceChip tone={bindingSourceTone(binding)}>{bindingSourceLabel(binding)}</SourceChip>
                        </strong>
                        <small>{binding.skillId}</small>
                      </span>
                      <span>{targetLabels[binding.target] ?? binding.target}</span>
                      <span>{binding.level === "project" ? binding.projectPath : "个人级"}</span>
                      <span>{binding.version}</span>
                      <span>
                        <Badge strong={binding.enabled}>{binding.enabled ? "启用" : "禁用"}</Badge>
                      </span>
                      <span className="row-actions">
                        <button className="icon-button" onClick={() => props.onToggle(binding)} title="启用/禁用">
                          <Power size={16} />
                        </button>
                        <button className="icon-button" onClick={() => props.onPreviewBinding(binding)} title="预览">
                          <BookOpen size={16} />
                        </button>
                        <button className="icon-button danger" onClick={() => props.onUninstall(binding)} title="卸载">
                          <Archive size={16} />
                        </button>
                      </span>
                    </div>
                  ))}
                  {inferredLocalBindings.map(({ key, package: cachedPackage, skill }) => (
                    <div className="table-row" key={key}>
                      <span>
                        <strong className="skill-title-line">
                          {skill.detectedManifest ?? cachedPackage.skillName}
                          <SourceChip tone="local">自建</SourceChip>
                        </strong>
                        <small title={skill.path}>{cachedPackage.skillId}</small>
                      </span>
                      <span>{targetLabels[skill.target] ?? skill.target}</span>
                      <span>{skill.level === "project" ? skill.projectPath ?? "项目级" : "个人级"}</span>
                      <span>{skill.version ?? cachedPackage.version}</span>
                      <span>
                        <Badge strong={skill.enabled}>{skill.enabled ? "启用" : "禁用"}</Badge>
                      </span>
                      <span className="row-actions">
                        <button className="icon-button" onClick={() => props.onToggleLocal(skill)} title={skill.enabled ? "禁用自建 skill" : "启用自建 skill"}>
                          <Power size={16} />
                        </button>
                        <button className="icon-button" onClick={() => props.onPreviewLocal(skill)} title="预览">
                          <BookOpen size={16} />
                        </button>
                        {canDeleteLocalSkillFromMatrix(skill) ? (
                          <button className="icon-button danger" onClick={() => props.onDeleteLocal(skill)} title="删除本地 skill">
                            <Trash2 size={16} />
                          </button>
                        ) : null}
                      </span>
                    </div>
                  ))}
                </>
              ) : (
                props.pluginBindings.map((binding) => (
                  <div className="table-row" key={binding.id}>
                    <span>
                      <strong className="skill-title-line">
                        {binding.pluginName}
                        <SourceChip tone="market">Plugin</SourceChip>
                      </strong>
                      <small>{binding.namespace}/{binding.pluginId}</small>
                    </span>
                    <span>{targetLabels[binding.target] ?? binding.target}</span>
                    <span>{pluginScopeLabel(binding.scope, binding.projectPath)}</span>
                    <span>{binding.version}</span>
                    <span>
                      <Badge strong={binding.enabled && binding.status === "installed"}>
                        {pluginBindingStatusLabel(binding.status, binding.enabled)}
                      </Badge>
                    </span>
                    <span className="row-actions" title={binding.platformRef}>
                      <button
                        className="icon-button"
                        onClick={() => props.onTogglePlugin(binding)}
                        title={binding.enabled ? "禁用 plugin" : "启用 plugin"}
                      >
                        <Power size={16} />
                      </button>
                      <button className="icon-button" onClick={() => props.onPreviewPluginBinding(binding)} title="预览">
                        <BookOpen size={16} />
                      </button>
                      <button
                        className="icon-button danger"
                        onClick={() => props.onUninstallPlugin(binding)}
                        title={`移除 ${binding.marketplaceName} 绑定`}
                      >
                        <Archive size={16} />
                      </button>
                    </span>
                  </div>
                ))
              )}
            </>
          ) : (
            <EmptyState
              title="还没有生效记录"
              body={`从市场安装并启用 ${artifactLabel} 后，这里会显示平台、范围、版本和启用状态。`}
            />
          )}
        </div>
      ) : null}

      {activeTab === "cache" ? (
        <div className="cache-panel">
          {cacheTabCount > 0 ? (
            <div className="cache-list">
              {artifactKind === "skill"
                ? props.cachedSkills.map((item) => (
                    <div className="cache-card" key={item.key}>
                      <div className="cache-mark">
                        <Archive size={18} />
                      </div>
                      <div className="cache-main" title={item.package.summary ?? undefined}>
                        <strong>{item.package.skillName}</strong>
                        <small>{item.package.skillId}</small>
                      </div>
                      <div className="cache-meta">
                        <Badge strong={item.marketSkill ? item.package.version === item.marketSkill.latestVersion : false}>
                          {item.package.version}
                        </Badge>
                        <Badge strong={item.package.origin === "local"}>
                          {item.package.origin === "local" ? "自建" : "市场"}
                        </Badge>
                        <span>
                          {item.package.origin === "local"
                            ? cachedPackageInstallSummary(item.package, props.bindings, props.localSkills)
                            : item.package.bindingCount > 0
                              ? `已安装 ${item.package.bindingCount} 处`
                              : "仅缓存"}
                        </span>
                      </div>
                      <div className="row-actions">
                        {item.package.origin === "local" &&
                        hasAvailableLocalInstallTarget({ kind: "cache", item }, props.bindings, props.localSkills) ? (
                          <button className="icon-button" onClick={() => props.onInstallCache(item)} title="安装自建缓存">
                            <PackageCheck size={16} />
                          </button>
                        ) : null}
                        <button className="icon-button" onClick={() => props.onPreviewCache(item)} title="预览">
                          <BookOpen size={16} />
                        </button>
                        <button
                          className="icon-button danger"
                          onClick={() => props.onDeleteCache(item)}
                          title="删除本地缓存"
                        >
                          <Archive size={16} />
                        </button>
                      </div>
                    </div>
                  ))
                : props.pluginPackages.map((item) => (
                    <div className="cache-card" key={`${item.namespace}:${item.pluginId}:${item.version}:${item.target}`}>
                      <div className="cache-mark">
                        <Blocks size={18} />
                      </div>
                      <div className="cache-main" title={item.packagePath}>
                        <strong>{item.pluginName}</strong>
                        <small>{item.namespace}/{item.pluginId}</small>
                      </div>
                      <div className="cache-meta">
                        <Badge strong>{targetLabels[item.target] ?? item.target}</Badge>
                        <Badge>{item.version}</Badge>
                        <Badge strong={item.riskLevel === "low"}>{pluginRiskLabel(item.riskLevel)}</Badge>
                        <span>{item.bindingCount > 0 ? `已写入 ${item.bindingCount} 处` : "仅缓存"}</span>
                      </div>
                      <div className="row-actions">
                        <button className="icon-button" onClick={() => props.onPreviewPluginCache(item)} title="预览">
                          <BookOpen size={16} />
                        </button>
                        {item.bindingCount === 0 ? (
                          <button
                            className="icon-button danger"
                            onClick={() => props.onDeletePluginCache(item)}
                            title="删除本地缓存"
                          >
                            <Trash2 size={16} />
                          </button>
                        ) : null}
                      </div>
                    </div>
                  ))}
            </div>
          ) : (
            <EmptyState
              title="本地缓存为空"
              body={`安装或仅缓存市场 ${artifactLabel} 后，可以在这里预览、复用或删除本地包。`}
            />
          )}
        </div>
      ) : null}

      {activeTab === "local" ? (
        <div className="local-scan">
          {localTabCount > 0 ? (
            <>
              {artifactKind === "skill"
                ? props.localSkills.map((skill) => (
                    <div className="scan-line" key={skill.id}>
                      <CheckCircle2 size={16} />
                      <span>
                        <strong>{skill.detectedManifest ?? "本地 skill"}</strong>
                        <small>
                          {targetLabels[skill.target] ?? skill.target} / {levelLabels[skill.level] ?? skill.level}
                          {skill.level === "project" && skill.projectPath ? ` · ${skill.projectPath}` : ""}
                        </small>
                        <small>{skill.path}</small>
                      </span>
                      <div className="scan-actions">
                        <Badge strong={skill.managedBySkillhub && skill.status !== "missing"}>
                          {localSkillStatusLabel(skill)}
                        </Badge>
                        <div className="row-actions">
                          {skill.canImportToCache ? (
                            <>
                              <button className="icon-button" onClick={() => props.onImportLocal(skill)} title="加入本地缓存">
                                <Download size={16} />
                              </button>
                              <button className="icon-button" onClick={() => props.onInstallLocal(skill)} title="加入缓存并安装">
                                <PackageCheck size={16} />
                              </button>
                            </>
                          ) : null}
                          {!skill.managedBySkillhub ? (
                            <button className="icon-button danger" onClick={() => props.onDeleteLocal(skill)} title="删除本地 skill">
                              <Trash2 size={16} />
                            </button>
                          ) : null}
                          <button className="icon-button" onClick={() => props.onPreviewLocal(skill)} title="预览">
                            <BookOpen size={16} />
                          </button>
                        </div>
                      </div>
                    </div>
                  ))
                : props.localPlugins.map((plugin) => (
                    <div className="scan-line" key={plugin.id}>
                      <PackageCheck size={16} />
                      <span>
                        <strong>{localPluginDisplayName(plugin)}</strong>
                        <small>
                          {targetLabels[plugin.target] ?? plugin.target} / {pluginScopeLabel(plugin.scope, plugin.projectPath)}
                        </small>
                        <small>{plugin.path}</small>
                      </span>
                      <div className="scan-actions">
                        <Badge strong={plugin.managedBySkillhub && plugin.status !== "missing"}>
                          {pluginLocalStatusLabel(plugin)}
                        </Badge>
                        <div className="row-actions">
                          <button className="icon-button" onClick={() => props.onPreviewLocalPlugin(plugin)} title="预览">
                            <BookOpen size={16} />
                          </button>
                        </div>
                      </div>
                    </div>
                  ))}
            </>
          ) : (
            <EmptyState
              title="等待扫描本地目录"
              body={`点击右上角扫描，Skill Hub 会列出个人级和项目级目录中的 ${artifactLabel}。`}
            />
          )}
        </div>
      ) : null}
    </section>
  );
}
