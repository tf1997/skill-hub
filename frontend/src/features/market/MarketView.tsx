import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, Loader2, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
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

export function MarketView(props: {
  artifactKind: MarketArtifactKind;
  onArtifactKind: (value: MarketArtifactKind) => void;
  mode: MarketMode;
  onMode: (value: MarketMode) => void;
  marketProjects: MarketProject[];
  selectedMarketProjectSlug: string;
  onSelectedMarketProjectSlug: (value: string) => void;
  categories: Category[];
  selectedCategory: string;
  onSelectCategory: (value: string) => void;
  query: string;
  onQuery: (value: string) => void;
  skills: MarketSkill[];
  plugins: MarketPlugin[];
  marketSkillCount: number;
  marketPluginCount: number;
  bindingsBySkill: Map<string, SkillBinding[]>;
  bindingsByPlugin: Map<string, AppBootstrap["pluginBindings"]>;
  selectedSkill?: MarketSkill;
  selectedPlugin?: MarketPlugin;
  onSelectSkill: (key: string) => void;
  onSelectPlugin: (key: string) => void;
  onRefresh: () => void;
  installTarget: string;
  onInstallTarget: (value: string) => void;
  installLevel: LevelChoice;
  onInstallLevel: (value: LevelChoice) => void;
  onUnsupportedPluginProjectScope: () => void;
  installProjectPath: string;
  onInstallProjectPath: (value: string) => void;
  installingPluginKey?: string | null;
  installingPluginStage?: string;
  targetRoots: TargetRoot[];
  projects: Project[];
  onInstall: () => void;
  onPreview: (skill: MarketSkill) => void;
  onPreviewPlugin: (plugin: MarketPlugin) => void;
}) {
  const selectedBindings = props.selectedSkill
    ? props.bindingsBySkill.get(`${props.selectedSkill.namespace}/${props.selectedSkill.id}`) ?? []
    : [];
  const selectedPluginBindings = props.selectedPlugin
    ? props.bindingsByPlugin.get(`${props.selectedPlugin.namespace}/${props.selectedPlugin.id}`) ?? []
    : [];
  const conflict = props.selectedSkill
    ? scopeConflict(selectedBindings, props.installTarget, props.installLevel)
    : null;
  const pluginConflict = props.selectedPlugin
    ? pluginScopeConflict(selectedPluginBindings, props.installTarget, props.installLevel)
    : null;
  const activeConflict = props.artifactKind === "skill" ? conflict : props.artifactKind === "plugin" ? pluginConflict : null;
  const installPreview = getInstallPreview(
    props.installTarget,
    props.installLevel,
    props.installProjectPath,
    props.targetRoots
  );
  const installState =
    props.artifactKind === "plugin"
      ? getPluginInstallState(
          props.selectedPlugin,
          selectedPluginBindings,
          props.installTarget,
          props.installLevel,
          props.installProjectPath,
          pluginConflict
        )
      : props.selectedSkill
        ? getInstallState(
            props.selectedSkill,
            selectedBindings,
            props.installTarget,
            props.installLevel,
            props.installProjectPath
          )
        : { label: "安装并启用", disabled: false, tone: "install" as const };
  const activePluginKey = props.selectedPlugin ? pluginKey(props.selectedPlugin) : null;
  const pluginInstalling =
    props.artifactKind === "plugin" && activePluginKey !== null && props.installingPluginKey === activePluginKey;
  const pluginInstallLabel = pluginInstalling ? props.installingPluginStage || "安装中" : installState.label;
  const filterProjects = normalizeProjectList(props.marketProjects);
  const codexPluginProjectUnsupported = props.artifactKind === "plugin" && props.installTarget === "codex";
  const changeMarketFilter = (value: string) => {
    if (props.mode === "project") {
      props.onSelectedMarketProjectSlug(value);
      return;
    }
    props.onSelectCategory(value);
  };

  return (
    <section className="market-grid">
      <div className="list-pane">
        <div className="pane-toolbar market-toolbar">
          <div className="market-scope-tabs" aria-label="市场范围">
            <button
              className={props.mode === "public" ? "active" : ""}
              onClick={() => props.onMode("public")}
            >
              <span>公共市场</span>
              <small>{props.artifactKind === "plugin" ? props.marketPluginCount : props.marketSkillCount}</small>
            </button>
            <button
              className={props.mode === "project" ? "active" : ""}
              onClick={() => props.onMode("project")}
            >
              <span>项目市场</span>
              <small>{filterProjects.length}</small>
            </button>
          </div>
          <div className="market-toolbar-actions">
            <div className="segmented market-artifact-switch" aria-label="对象类型">
              <button
                className={props.artifactKind === "skill" ? "active" : ""}
                onClick={() => props.onArtifactKind("skill")}
              >
                Skill
              </button>
              <button
                className={props.artifactKind === "plugin" ? "active" : ""}
                onClick={() => props.onArtifactKind("plugin")}
              >
                Plugin
              </button>
            </div>
            <label className="search-box market-search">
              <Search size={17} />
              <input
                value={props.query}
                onChange={(event) => props.onQuery(event.target.value)}
                placeholder={props.artifactKind === "plugin" ? "搜索 plugin、组件或命名空间" : "搜索 skill、标签或命名空间"}
              />
            </label>
            <button className="primary-soft icon-only" onClick={props.onRefresh} title="刷新" aria-label="刷新">
              <RefreshCw size={17} />
            </button>
          </div>
        </div>
        <div className="market-list-body">
          <nav className="market-filter-rail" aria-label={props.mode === "project" ? "项目筛选" : "分类筛选"}>
            <button
              className={`market-filter-chip ${
                props.mode === "project"
                  ? props.selectedMarketProjectSlug === ""
                    ? "active"
                    : ""
                  : props.selectedCategory === "all"
                    ? "active"
                    : ""
              }`}
              onClick={() => changeMarketFilter(props.mode === "project" ? "" : "all")}
              title={props.mode === "project" ? "全部项目" : "全部分类"}
            >
              <span>{props.mode === "project" ? "全部项目" : "全部分类"}</span>
            </button>
            {props.mode === "project"
              ? filterProjects.map((project) => (
                  <button
                    key={project.slug}
                    className={`market-filter-chip ${
                      props.selectedMarketProjectSlug === project.slug ? "active" : ""
                    }`}
                    onClick={() => changeMarketFilter(project.slug)}
                    title={project.name}
                  >
                    <span>{project.name}</span>
                  </button>
                ))
              : props.categories.map((category) => (
                  <button
                    key={category.id}
                    className={`market-filter-chip ${props.selectedCategory === category.id ? "active" : ""}`}
                    onClick={() => changeMarketFilter(category.id)}
                    title={category.name}
                  >
                    <span>{category.name}</span>
                  </button>
                ))}
          </nav>

          <div className="skill-list">
            {props.artifactKind === "plugin" ? (
              props.plugins.length > 0 ? (
                props.plugins.map((plugin) => {
                  const bindings = props.bindingsByPlugin.get(`${plugin.namespace}/${plugin.id}`) ?? [];
                  return (
                    <button
                      key={pluginKey(plugin)}
                      className={`skill-row ${
                        props.selectedPlugin && pluginKey(props.selectedPlugin) === pluginKey(plugin)
                          ? "active"
                          : ""
                      }`}
                      onClick={() => props.onSelectPlugin(pluginKey(plugin))}
                    >
                      <span className="skill-row-icon" aria-hidden="true">
                        <PackageCheck size={16} />
                      </span>
                      <div className="skill-row-main">
                        <strong>{plugin.name}</strong>
                        <small>{plugin.namespace}/{plugin.id}</small>
                      </div>
                      <div className="row-meta">
                        <Badge strong={bindings.length > 0}>
                          {bindings.length > 0 ? "已写入" : plugin.riskLevel || "plugin"}
                        </Badge>
                        <ChevronRight size={17} />
                      </div>
                    </button>
                  );
                })
              ) : (
                <div className="empty-state compact">
                  {props.marketPluginCount === 0
                    ? "还没有从 MinIO 同步到 plugin。请确认已上传 plugin-catalog.v1.json。"
                    : "没有匹配当前筛选条件的 plugin。"}
                </div>
              )
            ) : props.skills.length > 0 ? (
              props.skills.map((skill) => {
                const bindings = props.bindingsBySkill.get(`${skill.namespace}/${skill.id}`) ?? [];
                return (
                  <button
                    key={skillKey(skill)}
                    className={`skill-row ${
                      props.selectedSkill && skillKey(props.selectedSkill) === skillKey(skill)
                        ? "active"
                        : ""
                    }`}
                    onClick={() => props.onSelectSkill(skillKey(skill))}
                  >
                    <span className="skill-row-icon" aria-hidden="true">
                      <Layers3 size={16} />
                    </span>
                    <div className="skill-row-main">
                      <strong>{skill.name}</strong>
                      <small>{skill.namespace}/{skill.id}</small>
                    </div>
                    <div className="row-meta">
                      <Badge strong={isInstalledSkill(skill, bindings)}>
                        {marketStatusLabel(skill, bindings)}
                      </Badge>
                      <BindingDots bindings={bindings} />
                      <ChevronRight size={17} />
                    </div>
                  </button>
                );
              })
            ) : (
              <div className="empty-state compact">
                {props.marketSkillCount === 0
                  ? "还没有从 MinIO 同步到 skill。请确认 MinIO 可连接，并已上传 catalog.v1.json。"
                  : "没有匹配当前筛选条件的 skill。"}
              </div>
            )}
          </div>
        </div>

      </div>

      <aside className="detail-pane">
        {props.artifactKind === "plugin" && props.selectedPlugin ? (
          <>
            <div className="detail-scroll">
              <div className="detail-heading">
                <div>
                  <h2>{props.selectedPlugin.name}</h2>
                </div>
                <Badge strong>{props.selectedPlugin.latestVersion}</Badge>
              </div>
              <p className="detail-summary">{props.selectedPlugin.summary}</p>

              <div className="tag-cloud">
                {[...props.selectedPlugin.targets, ...props.selectedPlugin.scopes, ...props.selectedPlugin.components].map((tag) => (
                  <span key={tag}>{tag}</span>
                ))}
              </div>

              <div className="binding-panel">
                <h3>Marketplace</h3>
                {selectedPluginBindings.length === 0 ? (
                  <p className="muted">尚未写入本地 marketplace。</p>
                ) : (
                  selectedPluginBindings.map((binding) => (
                    <div className="binding-line" key={binding.id}>
                      <span className={binding.enabled ? "dot ok" : "dot"} />
                      <strong>{targetLabels[binding.target] ?? binding.target}</strong>
                      <span>{binding.scope}</span>
                      <small>{binding.marketplaceName}</small>
                    </div>
                  ))
                )}
              </div>

              <div className="install-box">
                <h3>安装选择</h3>
                <div className="field-row">
                  <span>平台</span>
                  <div className="segmented">
                    {(["codex", "claude"] as const).map((target) => (
                      <button
                        key={target}
                        className={props.installTarget === target ? "active" : ""}
                        onClick={() => {
                          props.onInstallTarget(target);
                          if (target === "codex" && props.installLevel === "project") {
                            props.onInstallLevel("personal");
                          }
                        }}
                        disabled={!props.selectedPlugin?.targets.includes(target)}
                      >
                        {targetLabels[target] ?? target}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="field-row">
                  <span>范围</span>
                  <div className="segmented">
                    <button
                      className={props.installLevel === "personal" ? "active" : ""}
                      onClick={() => props.onInstallLevel("personal")}
                    >
                      个人
                    </button>
                    <button
                      className={`${props.installLevel === "project" ? "active" : ""}${codexPluginProjectUnsupported ? " restricted" : ""}`}
                      onClick={() => {
                        if (codexPluginProjectUnsupported) {
                          props.onUnsupportedPluginProjectScope();
                          return;
                        }
                        props.onInstallLevel("project");
                      }}
                      aria-disabled={codexPluginProjectUnsupported}
                      title={codexPluginProjectUnsupported ? "Codex plugin 当前只支持个人级安装" : undefined}
                    >
                      项目
                    </button>
                    <button
                      className={props.installLevel === "download" ? "active" : ""}
                      onClick={() => props.onInstallLevel("download")}
                    >
                      仅缓存
                    </button>
                  </div>
                </div>
                {props.installLevel === "project" ? (
                  <label className="text-field">
                    <span>绑定项目</span>
                    <select
                      value={props.installProjectPath}
                      onChange={(event) => props.onInstallProjectPath(event.target.value)}
                    >
                      <option value="">选择已绑定项目</option>
                      {props.projects.map((project) => (
                        <option key={project.id} value={project.path}>
                          {project.name} - {project.path}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
                <div className="install-preview">
                  <span>{props.installLevel === "download" ? "仅缓存" : "Marketplace 路径"}</span>
                  <strong>{pluginInstallPreview(props.installTarget, props.installLevel, props.installProjectPath)}</strong>
                </div>
                {activeConflict ? <div className="conflict-note warning">{activeConflict}</div> : null}
              </div>
            </div>
            <div className="detail-action-bar">
              <button className="secondary-action" onClick={() => props.onPreviewPlugin(props.selectedPlugin!)}>
                <BookOpen size={18} />
                详情预览
              </button>
              <button
                className={`primary-action ${pluginInstalling ? "installing" : ""}`}
                onClick={props.onInstall}
                disabled={installState.disabled || pluginInstalling}
              >
                {pluginInstalling ? (
                  <Loader2 size={18} className="button-spinner" />
                ) : installState.tone === "cached" || installState.tone === "installed" ? (
                  <CheckCircle2 size={18} />
                ) : (
                  <Download size={18} />
                )}
                {pluginInstallLabel}
              </button>
            </div>
          </>
        ) : props.selectedSkill ? (
          <>
            <div className="detail-scroll">
            <div className="detail-heading">
              <div>
                <h2>{props.selectedSkill.name}</h2>
              </div>
              <Badge strong>{props.selectedSkill.latestVersion}</Badge>
            </div>
            <p className="detail-summary">{props.selectedSkill.summary}</p>

            <div className="tag-cloud">
              {displaySkillTags(props.selectedSkill).map((tag) => (
                <span key={tag}>{tag}</span>
              ))}
            </div>

            <div className="binding-panel">
              <h3>生效位置</h3>
              {props.selectedSkill.cachedVersions.includes(props.selectedSkill.latestVersion) ? (
                <p className="muted">当前版本已下载到本地包缓存，可在“本地”菜单管理。</p>
              ) : null}
              {selectedBindings.length === 0 ? (
                <p className="muted">尚未在本机启用。</p>
              ) : (
                selectedBindings.map((binding) => (
                  <div className="binding-line" key={binding.id}>
                    <span className={binding.enabled ? "dot ok" : "dot"} />
                    <strong>{targetLabels[binding.target] ?? binding.target}</strong>
                    <span>{levelLabels[binding.level] ?? binding.level}</span>
                    <small>{binding.projectPath ?? "个人级"}</small>
                  </div>
                ))
              )}
            </div>

            <div className="install-box">
              <h3>安装选择</h3>
              <div className="field-row">
                <span>平台</span>
                <div className="segmented">
                  {(["codex", "claude"] as const).map((target) => (
                    <button
                      key={target}
                      className={props.installTarget === target ? "active" : ""}
                      onClick={() => props.onInstallTarget(target)}
                    >
                      {targetLabels[target] ?? target}
                    </button>
                  ))}
                </div>
              </div>

              <div className="field-row">
                <span>范围</span>
                <div className="segmented">
                  <button
                    className={props.installLevel === "personal" ? "active" : ""}
                    onClick={() => props.onInstallLevel("personal")}
                  >
                    个人
                  </button>
                  <button
                    className={props.installLevel === "project" ? "active" : ""}
                    onClick={() => props.onInstallLevel("project")}
                  >
                    项目
                  </button>
                  <button
                    className={props.installLevel === "download" ? "active" : ""}
                    onClick={() => props.onInstallLevel("download")}
                  >
                    仅缓存
                  </button>
                </div>
              </div>

              {props.installLevel === "project" ? (
                <label className="text-field">
                  <span>绑定项目</span>
                  <select
                    value={props.installProjectPath}
                    onChange={(event) => props.onInstallProjectPath(event.target.value)}
                  >
                    <option value="">选择已绑定项目</option>
                    {props.projects.map((project) => (
                      <option key={project.id} value={project.path}>
                        {project.name} - {project.path}
                      </option>
                    ))}
                  </select>
                </label>
              ) : null}

              {props.installLevel === "project" && props.projects.length === 0 ? (
                <div className="conflict-note">
                  <AlertCircle size={16} />
                  <span>请先到“项目”菜单绑定项目，市场不会直接选择新目录。</span>
                </div>
              ) : null}

              {props.installLevel !== "download" ? (
                <div className="install-preview">
                  <span>{targetLabels[props.installTarget] ?? props.installTarget} 生效目录</span>
                  <strong>{installPreview}</strong>
                </div>
              ) : (
                <div className="install-preview">
                  <span>仅缓存</span>
                  <strong>下载到 Skill Hub 本地包仓库，不写入 Codex 或 Claude 目录。</strong>
                </div>
              )}

              {activeConflict ? (
                <div className="conflict-note">
                  <AlertCircle size={16} />
                  <span>{activeConflict}</span>
                </div>
              ) : null}

            </div>
            </div>
            <div className="detail-action-bar">
              <button className="secondary-action" onClick={() => props.onPreview(props.selectedSkill!)}>
                <BookOpen size={18} />
                预览内容
              </button>
              <button
                className="primary-action"
                onClick={props.onInstall}
                disabled={Boolean(activeConflict) || installState.disabled}
              >
                {installState.tone === "cached" || installState.tone === "installed" ? (
                  <CheckCircle2 size={18} />
                ) : (
                  <Download size={18} />
                )}
                {installState.label}
              </button>
            </div>
          </>
        ) : (
          <div className="empty-state">
            {props.marketSkillCount === 0
              ? "市场暂无 skill。刷新失败时请查看上方 MinIO 提示。"
              : "没有可显示的 skill。"}
          </div>
        )}
      </aside>
    </section>
  );
}
