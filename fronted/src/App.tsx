import {
  AlertCircle,
  Archive,
  Blocks,
  BookOpen,
  CheckCircle2,
  ChevronRight,
  Download,
  FolderGit2,
  FolderOpen,
  Layers3,
  PackageCheck,
  Power,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  X
} from "lucide-react";
import { open } from "@tauri-apps/api/dialog";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import type {
  AppBootstrap,
  CachedSkillPackage,
  Category,
  InstallSkillRequest,
  LocalSkill,
  MarketSkill,
  Project,
  SkillBinding,
  TargetRoot,
  UpdateCandidate,
  SkillPreview
} from "./types";

type ViewKey = "market" | "installed" | "projects" | "updates" | "settings";
type LevelChoice = "personal" | "project" | "download";
type CachedSkillItem = {
  key: string;
  package: CachedSkillPackage;
  marketSkill?: MarketSkill;
};

const emptyBootstrap: AppBootstrap = {
  sources: [],
  categories: [],
  skills: [],
  bindings: [],
  cachedPackages: [],
  localSkills: [],
  projects: [],
  targetRoots: [],
  updates: [],
  metadataSyncError: null
};

const targetLabels: Record<string, string> = {
  codex: "Codex",
  claude: "Claude"
};

const levelLabels: Record<string, string> = {
  personal: "个人",
  project: "项目"
};

function App() {
  const [view, setView] = useState<ViewKey>("market");
  const [data, setData] = useState<AppBootstrap>(emptyBootstrap);
  const [selectedCategory, setSelectedCategory] = useState("all");
  const [query, setQuery] = useState("");
  const [selectedSkillKey, setSelectedSkillKey] = useState<string | null>(null);
  const [installTarget, setInstallTarget] = useState("codex");
  const [installLevel, setInstallLevel] = useState<LevelChoice>("personal");
  const [installProjectPath, setInstallProjectPath] = useState("");
  const [updatePolicy, setUpdatePolicy] = useState("follow_latest");
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectPath, setNewProjectPath] = useState("");
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const [targetRootDrafts, setTargetRootDrafts] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<SkillPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("正在载入 Skill Hub...");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, []);

  const selectedSkill = useMemo(() => {
    return data.skills.find((skill) => skillKey(skill) === selectedSkillKey) ?? data.skills[0];
  }, [data.skills, selectedSkillKey]);

  useEffect(() => {
    if (!selectedSkillKey && data.skills.length > 0) {
      setSelectedSkillKey(skillKey(data.skills[0]));
    }
  }, [data.skills, selectedSkillKey]);

  const filteredSkills = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return data.skills.filter((skill) => {
      const categoryOk =
        selectedCategory === "all" || skill.categories.includes(selectedCategory);
      const queryOk =
        normalized.length === 0 ||
        [skill.name, skill.id, skill.namespace, skill.summary, ...skill.tags]
          .join(" ")
          .toLowerCase()
          .includes(normalized);

      return categoryOk && queryOk;
    });
  }, [data.skills, query, selectedCategory]);

  const bindingsBySkill = useMemo(() => {
    const map = new Map<string, SkillBinding[]>();
    for (const binding of data.bindings) {
      const key = `${binding.namespace}/${binding.skillId}`;
      map.set(key, [...(map.get(key) ?? []), binding]);
    }
    return map;
  }, [data.bindings]);

  const cachedSkills = useMemo<CachedSkillItem[]>(() => {
    return data.cachedPackages.map((cachedPackage) => {
      const marketSkill = data.skills.find(
        (skill) =>
          skill.sourceId === cachedPackage.sourceId &&
          skill.namespace === cachedPackage.namespace &&
          skill.id === cachedPackage.skillId
      );

      return {
        key: `${cachedPackage.sourceId ?? "local"}:${cachedPackage.namespace}/${cachedPackage.skillId}@${cachedPackage.version}`,
        package: cachedPackage,
        marketSkill
      };
    });
  }, [data.cachedPackages, data.skills]);

  useEffect(() => {
    setTargetRootDrafts(
      Object.fromEntries(data.targetRoots.map((root) => [root.target, root.personalPath]))
    );
  }, [data.targetRoots]);

  async function load() {
    setBusy(true);
    setError(null);
    try {
      const next = await api.bootstrap();
      setData(next);
      if (next.metadataSyncError) {
        setError(`市场元数据同步失败，显示本地缓存：${next.metadataSyncError}`);
        setNotice("已载入本地缓存");
      } else {
        setNotice("市场元数据已从 MinIO 同步");
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function refreshCatalog() {
    setBusy(true);
    setError(null);
    setNotice("正在从 MinIO 拉取市场元数据...");
    try {
      const next = await api.refreshCatalog();
      setData(next);
      setNotice("市场元数据已从 MinIO 同步");
    } catch (err) {
      setError(`市场索引刷新失败：${readError(err)}`);
    } finally {
      setBusy(false);
    }
  }

  async function openView(nextView: ViewKey) {
    setView(nextView);
    if (nextView === "market") {
      await refreshCatalog();
    }
  }

  async function chooseFolder(target: "project" | "root", rootTarget?: string) {
    setError(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: target === "root" ? "选择个人级 skill 目录" : "选择项目目录"
      });
      const folder = Array.isArray(selected) ? selected[0] : selected;
      if (!folder) return;
      if (target === "root" && rootTarget) {
        setTargetRootDrafts((current) => ({ ...current, [rootTarget]: folder }));
      } else {
        setNewProjectPath(folder);
        if (!newProjectName.trim()) {
          setNewProjectName(folder.split(/[\\/]/).filter(Boolean).pop() ?? "未命名项目");
        }
      }
    } catch (err) {
      setError("文件夹选择需要在 Tauri 客户端中使用，请通过 npm run tauri dev 启动。");
    }
  }

  async function installSelectedSkill() {
    if (!selectedSkill) return;
    if (installLevel === "project" && !installProjectPath) {
      setError("请先在项目菜单绑定项目，并选择一个项目");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const request: InstallSkillRequest = {
        sourceId: selectedSkill.sourceId,
        namespace: selectedSkill.namespace,
        skillId: selectedSkill.id,
        version: null,
        target: installTarget,
        level: installLevel === "download" ? "personal" : installLevel,
        projectPath: installLevel === "project" ? installProjectPath : null,
        installMode: "copy",
        updatePolicy,
        enable: installLevel !== "download"
      };
      await api.installSkill(request);
      await load();
      setNotice(`${selectedSkill.name} 已${installLevel === "download" ? "缓存" : "启用"}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewMarketSkill(skill: MarketSkill) {
    setBusy(true);
    setError(null);
    try {
      const result = await api.previewSkill({
        sourceId: skill.sourceId,
        namespace: skill.namespace,
        skillId: skill.id,
        version: null
      });
      setPreview(result);
      setNotice(`正在预览 ${skill.name}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function deleteCachedSkill(item: CachedSkillItem) {
    setBusy(true);
    setError(null);
    try {
      await api.deleteCachedSkill({
        sourceId: item.package.sourceId,
        namespace: item.package.namespace,
        skillId: item.package.skillId,
        version: item.package.version
      });
      await load();
      setNotice(`${item.package.skillName} ${item.package.version} 的本地缓存已删除`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewCachedSkill(item: CachedSkillItem) {
    setBusy(true);
    setError(null);
    try {
      const result = await api.previewSkill({
        sourceId: item.package.sourceId,
        namespace: item.package.namespace,
        skillId: item.package.skillId,
        version: item.package.version
      });
      setPreview(result);
      setNotice(`正在预览 ${item.package.skillName} ${item.package.version}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewBinding(binding: SkillBinding) {
    setBusy(true);
    setError(null);
    try {
      const result = await api.previewSkill({ bindingId: binding.id });
      setPreview(result);
      setNotice(`正在预览 ${binding.skillName}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewLocalSkill(skill: LocalSkill) {
    setBusy(true);
    setError(null);
    try {
      const result = await api.previewSkill({ path: skill.path });
      setPreview(result);
      setNotice(`正在预览 ${skill.detectedManifest ?? skill.path}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function toggleBinding(binding: SkillBinding) {
    setBusy(true);
    setError(null);
    try {
      await api.setBindingEnabled(binding.id, !binding.enabled);
      await load();
      setNotice(binding.enabled ? "已禁用绑定" : "已启用绑定");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function uninstallBinding(binding: SkillBinding) {
    setBusy(true);
    setError(null);
    try {
      await api.uninstallBinding(binding.id);
      await load();
      setNotice("绑定已移除");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function saveProject() {
    if (!newProjectPath.trim()) {
      setError("请先选择项目文件夹");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      await api.saveProject(newProjectName, newProjectPath);
      setNewProjectName("");
      setNewProjectPath("");
      await load();
      setNotice("项目已绑定");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function unbindProject(project: Project) {
    setBusy(true);
    setError(null);
    try {
      await api.unbindProject(project.id);
      if (installProjectPath === project.path) {
        setInstallProjectPath("");
      }
      await load();
      setNotice(
        "项目已解绑。此前安装到该项目目录的 skill 不会自动删除，请到项目的 .codex/skills 或 .claude/skills 中手动清理。"
      );
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function saveTargetRoot(target: string) {
    const personalPath = targetRootDrafts[target];
    if (!personalPath?.trim()) {
      setError(`请选择 ${targetLabels[target] ?? target} 的个人级 skill 目录`);
      return;
    }

    setBusy(true);
    setError(null);
    try {
      await api.saveTargetRoot(target, personalPath);
      await load();
      setNotice(`${targetLabels[target] ?? target} 个人级目录已保存`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function scanLocal() {
    setBusy(true);
    setError(null);
    try {
      const rows = await api.scanLocalSkills();
      setData((current) => ({ ...current, localSkills: rows }));
      setNotice("本地 skill 已扫描");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  const localNavCount = Math.max(data.bindings.length, data.localSkills.length);
  const navItems = [
    { key: "market" as const, label: "市场", icon: Blocks, count: data.skills.length },
    { key: "installed" as const, label: "本地", icon: PackageCheck, count: localNavCount },
    { key: "projects" as const, label: "项目", icon: FolderGit2, count: data.projects.length },
    { key: "updates" as const, label: "更新", icon: RefreshCw, count: data.updates.length },
    { key: "settings" as const, label: "设置", icon: Settings, count: data.targetRoots.length }
  ];

  return (
    <div className="app-shell" data-theme={theme}>
      <aside className="sidebar">
        <div className="brand-block">
          <div className="brand-mark">
            <Layers3 size={22} />
          </div>
          <div>
            <strong>Skill Hub</strong>
            <span>Skill Switchboard</span>
          </div>
        </div>

        <nav className="nav-stack">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.key}
                className={`nav-item ${view === item.key ? "active" : ""}`}
                onClick={() => void openView(item.key)}
              >
                <Icon size={18} />
                <span>{item.label}</span>
                <b>{item.count}</b>
              </button>
            );
          })}
        </nav>

      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <p>{viewTitle(view)}</p>
            <h1>{viewHeadline(view)}</h1>
          </div>
          <div className="top-actions">
            <StatusPill busy={busy} text={notice} />
            <button className="icon-button" onClick={() => void load()} title="重新载入">
              <RefreshCw size={18} />
            </button>
          </div>
        </header>

        {error ? (
          <div className="error-strip">
            <AlertCircle size={18} />
            <span>{error}</span>
          </div>
        ) : null}

        {view === "market" ? (
          <MarketView
            categories={data.categories}
            selectedCategory={selectedCategory}
            onSelectCategory={setSelectedCategory}
            query={query}
            onQuery={setQuery}
            skills={filteredSkills}
            marketSkillCount={data.skills.length}
            bindingsBySkill={bindingsBySkill}
            selectedSkill={selectedSkill}
            onSelectSkill={setSelectedSkillKey}
            onRefresh={refreshCatalog}
            installTarget={installTarget}
            onInstallTarget={setInstallTarget}
            installLevel={installLevel}
            onInstallLevel={setInstallLevel}
            installProjectPath={installProjectPath}
            onInstallProjectPath={setInstallProjectPath}
            targetRoots={data.targetRoots}
            projects={data.projects}
            updatePolicy={updatePolicy}
            onUpdatePolicy={setUpdatePolicy}
            onInstall={installSelectedSkill}
            onPreview={previewMarketSkill}
          />
        ) : null}

        {view === "installed" ? (
          <InstalledView
            bindings={data.bindings}
            cachedSkills={cachedSkills}
            onToggle={toggleBinding}
            onUninstall={uninstallBinding}
            localSkills={data.localSkills}
            onScan={scanLocal}
            onPreviewBinding={previewBinding}
            onPreviewLocal={previewLocalSkill}
            onPreviewCache={previewCachedSkill}
            onDeleteCache={deleteCachedSkill}
          />
        ) : null}

        {view === "projects" ? (
          <ProjectsView
            projects={data.projects}
            bindings={data.bindings}
            name={newProjectName}
            path={newProjectPath}
            onName={setNewProjectName}
            onPickPath={() => void chooseFolder("project")}
            onSave={saveProject}
            onUnbind={(project) => void unbindProject(project)}
          />
        ) : null}

        {view === "updates" ? <UpdatesView updates={data.updates} /> : null}

        {view === "settings" ? (
          <SettingsView
            targetRoots={data.targetRoots}
            targetRootDrafts={targetRootDrafts}
            onPickTargetRoot={(target) => void chooseFolder("root", target)}
            onSaveTargetRoot={(target) => void saveTargetRoot(target)}
            theme={theme}
            onTheme={setTheme}
          />
        ) : null}

        {preview ? <PreviewPanel preview={preview} onClose={() => setPreview(null)} /> : null}
      </main>
    </div>
  );
}

function MarketView(props: {
  categories: Category[];
  selectedCategory: string;
  onSelectCategory: (value: string) => void;
  query: string;
  onQuery: (value: string) => void;
  skills: MarketSkill[];
  marketSkillCount: number;
  bindingsBySkill: Map<string, SkillBinding[]>;
  selectedSkill?: MarketSkill;
  onSelectSkill: (key: string) => void;
  onRefresh: () => void;
  installTarget: string;
  onInstallTarget: (value: string) => void;
  installLevel: LevelChoice;
  onInstallLevel: (value: LevelChoice) => void;
  installProjectPath: string;
  onInstallProjectPath: (value: string) => void;
  targetRoots: TargetRoot[];
  projects: Project[];
  updatePolicy: string;
  onUpdatePolicy: (value: string) => void;
  onInstall: () => void;
  onPreview: (skill: MarketSkill) => void;
}) {
  const selectedBindings = props.selectedSkill
    ? props.bindingsBySkill.get(`${props.selectedSkill.namespace}/${props.selectedSkill.id}`) ?? []
    : [];
  const conflict = props.selectedSkill
    ? scopeConflict(selectedBindings, props.installTarget, props.installLevel)
    : null;
  const installPreview = getInstallPreview(
    props.installTarget,
    props.installLevel,
    props.installProjectPath,
    props.targetRoots
  );
  const installState = props.selectedSkill
    ? getInstallState(
        props.selectedSkill,
        selectedBindings,
        props.installTarget,
        props.installLevel,
        props.installProjectPath
      )
    : { label: "安装并启用", disabled: false, tone: "install" as const };

  return (
    <section className="market-grid">
      <div className="filter-rail">
        <div className="rail-title">
          <SlidersHorizontal size={16} />
          <span>分类</span>
        </div>
        <button
          className={`category-button ${props.selectedCategory === "all" ? "active" : ""}`}
          onClick={() => props.onSelectCategory("all")}
        >
          全部
        </button>
        {props.categories.map((category) => (
          <button
            key={category.id}
            className={`category-button ${
              props.selectedCategory === category.id ? "active" : ""
            }`}
            onClick={() => props.onSelectCategory(category.id)}
          >
            {category.name}
          </button>
        ))}
      </div>

      <div className="list-pane">
        <div className="pane-toolbar">
          <label className="search-box">
            <Search size={17} />
            <input
              value={props.query}
              onChange={(event) => props.onQuery(event.target.value)}
              placeholder="搜索 skill、标签或命名空间"
            />
          </label>
          <button className="primary-soft" onClick={props.onRefresh}>
            <RefreshCw size={17} />
            刷新
          </button>
        </div>

        <div className="skill-list">
          {props.skills.length > 0 ? (
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
                  <div className="skill-row-main">
                    <strong>{skill.name}</strong>
                    <span>{skill.summary}</span>
                  </div>
                  <div className="row-meta">
                    <Badge>{skill.latestVersion}</Badge>
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

      <aside className="detail-pane">
        {props.selectedSkill ? (
          <>
            <div className="detail-heading">
              <div>
                <p>{props.selectedSkill.namespace}</p>
                <h2>{props.selectedSkill.name}</h2>
              </div>
              <Badge strong>{props.selectedSkill.latestVersion}</Badge>
            </div>
            <p className="detail-summary">{props.selectedSkill.summary}</p>

            <div className="tag-cloud">
              {props.selectedSkill.categories.map((category) => (
                <span key={category}>{category}</span>
              ))}
              {props.selectedSkill.tags.map((tag) => (
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

              <div className="field-row">
                <span>更新</span>
                <div className="segmented">
                  <button
                    className={props.updatePolicy === "follow_latest" ? "active" : ""}
                    onClick={() => props.onUpdatePolicy("follow_latest")}
                  >
                    跟随
                  </button>
                  <button
                    className={props.updatePolicy === "pinned" ? "active" : ""}
                    onClick={() => props.onUpdatePolicy("pinned")}
                  >
                    锁定
                  </button>
                </div>
              </div>

              {conflict ? (
                <div className="conflict-note">
                  <AlertCircle size={16} />
                  <span>{conflict}</span>
                </div>
              ) : null}

              <button
                className="primary-action"
                onClick={props.onInstall}
                disabled={Boolean(conflict) || installState.disabled}
              >
                {installState.tone === "cached" || installState.tone === "installed" ? (
                  <CheckCircle2 size={18} />
                ) : (
                  <Download size={18} />
                )}
                {installState.label}
              </button>

              <button className="secondary-action" onClick={() => props.onPreview(props.selectedSkill!)}>
                <BookOpen size={18} />
                预览内容
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

function InstalledView(props: {
  bindings: SkillBinding[];
  cachedSkills: CachedSkillItem[];
  localSkills: LocalSkill[];
  onToggle: (binding: SkillBinding) => void;
  onUninstall: (binding: SkillBinding) => void;
  onScan: () => void;
  onPreviewBinding: (binding: SkillBinding) => void;
  onPreviewLocal: (skill: LocalSkill) => void;
  onPreviewCache: (item: CachedSkillItem) => void;
  onDeleteCache: (item: CachedSkillItem) => void;
}) {
  const [showCache, setShowCache] = useState(false);

  return (
    <section className={`content-stack installed-view ${showCache ? "with-cache" : ""}`}>
      <div className="section-toolbar">
        <div>
          <h2>生效矩阵</h2>
          <p>同一 skill 在同一平台下只能选择个人级或项目级之一。</p>
        </div>
        <div className="toolbar-actions">
          <button
            className={`primary-soft ${showCache ? "active" : ""}`}
            onClick={() => setShowCache((value) => !value)}
          >
            <Archive size={17} />
            本地缓存
            <Badge>{props.cachedSkills.length}</Badge>
          </button>
          <button className="primary-soft" onClick={props.onScan}>
            <ShieldCheck size={17} />
            扫描
          </button>
        </div>
      </div>

      <div className="data-table">
        <div className="table-head">
          <span>Skill</span>
          <span>平台</span>
          <span>范围</span>
          <span>版本</span>
          <span>状态</span>
          <span>操作</span>
        </div>
        {props.bindings.length > 0 ? (
          props.bindings.map((binding) => (
            <div className="table-row" key={binding.id}>
              <span>
                <strong>{binding.skillName}</strong>
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
          ))
        ) : (
          <div className="empty-state compact">还没有通过 Skill Hub 安装或启用的 skill。</div>
        )}
      </div>

      {showCache ? (
        <div className="cache-panel">
          <div className="cache-panel-head">
            <div>
              <h2>本地缓存</h2>
              <p>已下载但不一定生效的 skill 包，删除缓存不会卸载已安装目录。</p>
            </div>
            <Badge>{props.cachedSkills.length} 个版本</Badge>
          </div>

          {props.cachedSkills.length > 0 ? (
            <div className="cache-list">
              {props.cachedSkills.map((item) => (
                <div className="cache-card" key={item.key}>
                  <div className="cache-mark">
                    <Archive size={18} />
                  </div>
                  <div className="cache-main">
                    <strong>{item.package.skillName}</strong>
                    <small>{item.package.skillId}</small>
                  </div>
                  <div className="cache-meta">
                    <Badge strong={item.marketSkill ? item.package.version === item.marketSkill.latestVersion : false}>
                      {item.package.version}
                    </Badge>
                    <span>
                      {item.package.bindingCount > 0
                        ? `已安装 ${item.package.bindingCount} 处`
                        : "仅缓存"}
                    </span>
                  </div>
                  <div className="row-actions">
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
              ))}
            </div>
          ) : (
            <div className="empty-state compact">还没有下载到本地缓存的 skill。</div>
          )}
        </div>
      ) : null}

      <div className="local-scan">
        <h2>本地已有 skill</h2>
        {props.localSkills.length > 0 ? (
          props.localSkills.map((skill) => (
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
              <Badge strong={skill.managedBySkillhub && skill.status !== "missing"}>
                {localSkillStatusLabel(skill)}
              </Badge>
              <button className="icon-button" onClick={() => props.onPreviewLocal(skill)} title="预览">
                <BookOpen size={16} />
              </button>
            </div>
          ))
        ) : (
          <div className="empty-state compact">点击扫描后会显示个人级和项目级目录中包含 SKILL.md 的 skill。</div>
        )}
      </div>
    </section>
  );
}

function ProjectsView(props: {
  projects: Project[];
  bindings: SkillBinding[];
  name: string;
  path: string;
  onName: (value: string) => void;
  onPickPath: () => void;
  onSave: () => void;
  onUnbind: (project: Project) => void;
}) {
  return (
    <section className="content-stack projects-view">
      <div className="project-form">
        <label className="text-field">
          <span>项目名</span>
          <input
            value={props.name}
            onChange={(event) => props.onName(event.target.value)}
            placeholder="默认使用文件夹名称"
          />
        </label>
        <div className="path-picker grow">
          <div>
            <span>项目文件夹</span>
            <strong>{props.path || "尚未选择"}</strong>
          </div>
          <button className="icon-text-button" onClick={props.onPickPath}>
            <FolderOpen size={17} />
            选择
          </button>
        </div>
        <button className="primary-action compact" onClick={props.onSave}>
          <FolderGit2 size={17} />
          绑定项目
        </button>
      </div>

      <div className="project-grid">
        {props.projects.map((project) => {
          const count = props.bindings.filter(
            (binding) => binding.projectPath === project.path
          ).length;
          return (
            <div className="project-tile" key={project.id}>
              <div className="project-tile-main">
                <strong>{project.name}</strong>
                <span>{project.path}</span>
              </div>
              <div className="project-tile-actions">
                <Badge strong>{count} skills</Badge>
                <button
                  className="icon-button danger"
                  onClick={() => props.onUnbind(project)}
                  title="解绑项目"
                >
                  <X size={16} />
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function UpdatesView(props: { updates: UpdateCandidate[] }) {
  return (
    <section className="content-stack updates-view">
      <div className="section-toolbar">
        <div>
          <h2>更新中心</h2>
          <p>锁定版本不会自动升级，跟随版本会进入更新队列。</p>
        </div>
      </div>
      <div className="data-table">
        <div className="table-head updates-head">
          <span>Skill</span>
          <span>当前位置</span>
          <span>版本</span>
          <span>策略</span>
        </div>
        {props.updates.map((update) => (
          <div className="table-row updates-row" key={update.bindingId}>
            <span>
              <strong>{update.skillName}</strong>
              <small>{update.skillId}</small>
            </span>
            <span>
              {targetLabels[update.target] ?? update.target} /{" "}
              {update.level === "project" ? update.projectPath : "个人级"}
            </span>
            <span>
              {update.currentVersion} → {update.latestVersion}
            </span>
            <span>
              <Badge strong={!update.blockedReason}>
                {update.blockedReason ?? "可更新"}
              </Badge>
            </span>
          </div>
        ))}
        {props.updates.length === 0 ? <div className="empty-state">当前没有可更新项。</div> : null}
      </div>
    </section>
  );
}

function SettingsView(props: {
  targetRoots: TargetRoot[];
  targetRootDrafts: Record<string, string>;
  onPickTargetRoot: (target: string) => void;
  onSaveTargetRoot: (target: string) => void;
  theme: "light" | "dark";
  onTheme: (theme: "light" | "dark") => void;
}) {
  return (
    <section className="settings-grid">
      <div className="settings-form">
        <h2>界面</h2>
        <div className="field-row">
          <span>主题</span>
          <div className="segmented">
            <button
              className={props.theme === "light" ? "active" : ""}
              onClick={() => props.onTheme("light")}
            >
              白色
            </button>
            <button
              className={props.theme === "dark" ? "active" : ""}
              onClick={() => props.onTheme("dark")}
            >
              深色
            </button>
          </div>
        </div>
      </div>

      <div className="settings-stack">
        <div className="target-root-list">
          <h2>目标平台目录</h2>
          <p>市场下载后，只有启用时才写入对应平台目录。</p>
          {props.targetRoots.map((root) => (
            <div className="target-root-row" key={root.target}>
              <div>
                <strong>{targetLabels[root.target] ?? root.target}</strong>
                <span>{props.targetRootDrafts[root.target] || root.personalPath}</span>
              </div>
              <div className="row-actions">
                <button className="icon-text-button" onClick={() => props.onPickTargetRoot(root.target)}>
                  <FolderOpen size={17} />
                  选择
                </button>
                <button className="primary-soft" onClick={() => props.onSaveTargetRoot(root.target)}>
                  保存
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function PreviewPanel(props: { preview: SkillPreview; onClose: () => void }) {
  return (
    <aside className="preview-drawer">
      <div className="preview-head">
        <div>
          <p>{props.preview.origin}</p>
          <h2>{props.preview.title}</h2>
          <span>{props.preview.rootPath}</span>
        </div>
        <button className="icon-button" onClick={props.onClose} title="关闭预览">
          <X size={17} />
        </button>
      </div>

      <div className="preview-files">
        {props.preview.files.length === 0 ? (
          <div className="empty-state">没有可预览的文本内容。</div>
        ) : (
          props.preview.files.map((file) => (
            <article className="preview-file" key={file.path}>
              <header>
                <strong>{file.path}</strong>
                <Badge>{file.language}</Badge>
              </header>
              <pre>{file.content}</pre>
              {file.truncated ? <small>内容过长，已截断预览。</small> : null}
            </article>
          ))
        )}
      </div>
    </aside>
  );
}

function StatusPill(props: { busy: boolean; text: string }) {
  return (
    <div className={`status-pill ${props.busy ? "busy" : ""}`}>
      <span />
      {props.text}
    </div>
  );
}

function Badge(props: { children: React.ReactNode; strong?: boolean }) {
  return <em className={`badge ${props.strong ? "strong" : ""}`}>{props.children}</em>;
}

function BindingDots(props: { bindings: SkillBinding[] }) {
  if (props.bindings.length === 0) {
    return <span className="mini-status">未启用</span>;
  }

  return (
    <span className="binding-dots">
      {props.bindings.slice(0, 4).map((binding) => (
        <i
          key={binding.id}
          className={binding.enabled ? "enabled" : ""}
          title={`${binding.target}/${binding.level}`}
        />
      ))}
    </span>
  );
}

function isInstalledSkill(skill: MarketSkill, bindings: SkillBinding[]) {
  return bindings.some(
    (binding) =>
      binding.namespace === skill.namespace &&
      binding.skillId === skill.id &&
      binding.status === "installed"
  );
}

function marketStatusLabel(skill: MarketSkill, bindings: SkillBinding[]) {
  const related = bindings.filter(
    (binding) => binding.namespace === skill.namespace && binding.skillId === skill.id
  );
  if (related.some((binding) => binding.enabled)) return "已启用";
  if (related.length > 0) return "已安装";
  if (skill.cachedVersions.includes(skill.latestVersion)) return "已缓存";
  return "未安装";
}

function localSkillStatusLabel(skill: LocalSkill) {
  if (skill.status === "missing") return "缺失";
  if (skill.managedBySkillhub) return "Skill Hub";
  if (skill.status === "unmanaged") return "用户自建";
  return skill.status;
}

function getInstallState(
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

function skillKey(skill: MarketSkill) {
  return `${skill.sourceId ?? "local"}:${skill.namespace}/${skill.id}`;
}

function scopeConflict(bindings: SkillBinding[], target: string, level: LevelChoice) {
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

function getInstallPreview(
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

function viewTitle(view: ViewKey) {
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
  }
}

function viewHeadline(view: ViewKey) {
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
  }
}

function readError(err: unknown) {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return "操作失败";
}

export default App;
