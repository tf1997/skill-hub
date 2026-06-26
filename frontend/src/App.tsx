import {
  AlertCircle,
  Archive,
  Blocks,
  BookOpen,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Download,
  FileText,
  Folder,
  FolderGit2,
  FolderOpen,
  KeyRound,
  Layers3,
  Moon,
  PackageCheck,
  Pencil,
  Plus,
  Power,
  RefreshCw,
  Rocket,
  Save,
  Search,
  Settings,
  ShieldCheck,
  ScrollText,
  Sun,
  Trash2,
  X
} from "lucide-react";
import { message, open } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { open as openExternal } from "@tauri-apps/api/shell";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { AppContextMenu } from "./components/common/AppContextMenu";
import { Badge } from "./components/common/Badge";
import { EmptyState } from "./components/common/EmptyState";
import { SourceChip } from "./components/common/SourceChip";
import { StatusPill } from "./components/common/StatusPill";
import { ThemeSwitch } from "./components/common/ThemeSwitch";
import { AboutDialog } from "./components/dialogs/AboutDialog";
import type { AboutPayload } from "./components/dialogs/AboutDialog";
import { AdminUnlockDialog } from "./components/dialogs/AdminUnlockDialog";
import { AppUpdateDialog } from "./components/dialogs/AppUpdateDialog";
import type { AppUpdateDialogState } from "./components/dialogs/AppUpdateDialog";
import { LocalDeleteDialog } from "./components/dialogs/LocalDeleteDialog";
import { LocalInstallDialog } from "./components/dialogs/LocalInstallDialog";
import { PreviewPanel } from "./components/preview/PreviewPanel";
import {
  defaultMetaFromDraft,
  defaultMetaFromPluginDraft,
  draftCategoryLabel,
  draftPrimaryCategory,
  draftSearchText,
  draftSecondaryCategory,
  draftSkillLabel,
  draftStatusClass,
  draftStatusFilterLabels,
  draftStatusFilterOrder,
  emptyPublishMeta,
  isPublishedDraft,
  normalizeMetaForSave,
  pluginDraftCategoryPath,
  pluginDraftLabel,
  pluginDraftPrimaryCategory,
  pluginDraftSearchText,
  pluginDraftSecondaryCategory,
  pluginDraftStatusClass,
  pluginDraftStatusLabel,
  publishMetaMissingMessage,
  sortDrafts,
  sortPluginDrafts,
  splitCsv
} from "./lib/adminDrafts";
import type { AdminArtifactKind, DraftStatusFilter, DraftStatusKey } from "./lib/adminDrafts";
import {
  categoryNameFromSlug,
  emptyMarketCategory,
  emptyMarketProject,
  nextCategoryOrder,
  nextProjectOrder,
  normalizeCategoryList,
  normalizeProjectList
} from "./lib/categories";
import { readError } from "./lib/errors";
import {
  getInstallPreview,
  getInstallState,
  getPluginInstallState,
  isInstalledSkill,
  marketStatusLabel,
  pluginInstallPreview,
  pluginScopeConflict,
  scopeConflict
} from "./lib/installState";
import type { LevelChoice } from "./lib/installState";
import {
  availableLocalInstallTargets,
  bindingSourceLabel,
  bindingSourceTone,
  cachedPackageInstallSummary,
  cachedPackageInstallTargets,
  cachedPackageKey,
  canDeleteLocalSkillFromMatrix,
  displaySkillTags,
  hasAvailableLocalInstallTarget,
  hasBindingForLocalSkill,
  isLocalBinding,
  isLocalInstallTarget,
  localCachedInstallations,
  localPluginDisplayName,
  localSkillStatusLabel,
  markLocalSkillsCached,
  upsertCachedPackage
} from "./lib/localSkills";
import type { CachedSkillItem, LocalInstallDialogState, LocalInstallOptions, LocalInstallLevel, LocalInstallTarget } from "./lib/localSkills";
import { pluginBindingStatusLabel, pluginLocalStatusLabel, pluginRiskLabel, pluginScopeLabel } from "./lib/plugins";
import type {
  AdminDraftPreviewRequest,
  AdminAuditLog,
  AdminDraftPlugin,
  AdminDraftSkill,
  AdminSession,
  AppBootstrap,
  CachedSkillPackage,
  Category,
  InstallCachedSkillRequest,
  InstallPluginRequest,
  InstallSkillRequest,
  LocalSkill,
  MarketPlugin,
  MarketProject,
  MarketSkill,
  Project,
  PublishMeta,
  PluginPreviewRequest,
  DeleteCachedPluginRequest,
  SkillBinding,
  TargetRoot,
  UpdateCandidate,
  UpdateCheckResult,
  DownloadUpdateResult,
  SkillPreview,
  SkillPreviewRequest
} from "./types";

type ViewKey = "market" | "installed" | "projects" | "updates" | "settings" | "admin";type MarketMode = "public" | "project";
type MarketArtifactKind = "skill" | "plugin";
type AdminTab = "projects" | "drafts" | "archive" | "audit";type GovernanceTab = "project" | "general";
type GovernanceDialog =
  | { kind: "project-create" }
  | { kind: "project-edit"; project: MarketProject }
  | { kind: "project-delete"; project: MarketProject }
  | { kind: "category-create" }
  | { kind: "category-edit"; category: Category }
  | { kind: "category-delete"; category: Category };
type PreviewContext =
  | { kind: "skill"; request: SkillPreviewRequest }
  | { kind: "plugin"; request: PluginPreviewRequest }
  | { kind: "adminDraft"; request: AdminDraftPreviewRequest }
  | { kind: "adminPluginDraft"; request: AdminDraftPreviewRequest };type ContextMenuState = {
  open: boolean;
  x: number;
  y: number;
};type InstalledArtifactKind = "skill" | "plugin";
type UpdateArtifactKind = "skill" | "plugin";
type UpdateStatusFilter = "ready" | "blocked";type InstalledTab = "bindings" | "cache" | "local";

const emptyBootstrap: AppBootstrap = {
  sources: [],
  categories: [],
  skills: [],
  plugins: [],
  marketProjects: [],
  bindings: [],
  cachedPackages: [],
  pluginPackages: [],
  pluginBindings: [],
  localPlugins: [],
  localSkills: [],
  projects: [],
  targetRoots: [],
  updates: [],
  metadataSyncError: null
};

const canUseTauriEvents =
  typeof window !== "undefined" && typeof (window as Window & { __TAURI_IPC__?: unknown }).__TAURI_IPC__ === "function";
const ADMIN_ENTRY_CLICK_THRESHOLD = 5;
const THEME_STORAGE_KEY = "skill-hub-theme";

function initialTheme(): "light" | "dark" {
  if (typeof document !== "undefined" && document.documentElement.dataset.theme === "dark") {
    return "dark";
  }
  if (typeof window !== "undefined") {
    let stored: string | null = null;
    try {
      stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    } catch {
      stored = null;
    }
    if (stored === "dark" || stored === "light") {
      return stored;
    }
    if (window.matchMedia?.("(prefers-color-scheme: dark)").matches) {
      return "dark";
    }
  }
  return "light";
}

const targetLabels: Record<string, string> = {
  codex: "Codex",
  claude: "Claude"
};

const levelLabels: Record<string, string> = {
  personal: "个人",
  project: "项目"
};

const isProjectMarketSkill = (skill: MarketSkill) =>
  skill.categories.some((category) => category.startsWith("project:"));

const isProjectMarketPlugin = (plugin: MarketPlugin) =>
  plugin.categories.some((category) => category.startsWith("project:"));
function App() {
  const [view, setView] = useState<ViewKey>("market");
  const [data, setData] = useState<AppBootstrap>(emptyBootstrap);
  const [marketArtifactKind, setMarketArtifactKind] = useState<MarketArtifactKind>("skill");
  const [marketMode, setMarketMode] = useState<MarketMode>("public");
  const [selectedMarketProjectSlug, setSelectedMarketProjectSlug] = useState("");
  const [selectedCategory, setSelectedCategory] = useState("all");
  const [query, setQuery] = useState("");
  const [selectedSkillKey, setSelectedSkillKey] = useState<string | null>(null);
  const [selectedPluginKey, setSelectedPluginKey] = useState<string | null>(null);
  const [installTarget, setInstallTarget] = useState("codex");
  const [installLevel, setInstallLevel] = useState<LevelChoice>("personal");
  const [installProjectPath, setInstallProjectPath] = useState("");
  const [updatePolicy, setUpdatePolicy] = useState("follow_latest");
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectPath, setNewProjectPath] = useState("");
  const [theme, setTheme] = useState<"light" | "dark">(() => initialTheme());
  const [targetRootDrafts, setTargetRootDrafts] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<SkillPreview | null>(null);
  const [previewContext, setPreviewContext] = useState<PreviewContext | null>(null);
  const [localInstallDialog, setLocalInstallDialog] = useState<LocalInstallDialogState | null>(null);
  const [localInstallTarget, setLocalInstallTarget] = useState<LocalInstallTarget>("codex");
  const [localInstallLevel, setLocalInstallLevel] = useState<LocalInstallLevel>("personal");
  const [localInstallProjectPath, setLocalInstallProjectPath] = useState("");
  const [localDeleteDialog, setLocalDeleteDialog] = useState<LocalSkill | null>(null);
  const [adminVisible, setAdminVisible] = useState(false);
  const [adminUnlockOpen, setAdminUnlockOpen] = useState(false);
  const [adminKey, setAdminKey] = useState("");
  const [adminSession, setAdminSession] = useState<AdminSession | null>(null);
  const [adminDrafts, setAdminDrafts] = useState<AdminDraftSkill[]>([]);
  const [adminPluginDrafts, setAdminPluginDrafts] = useState<AdminDraftPlugin[]>([]);
  const [adminAuditLogs, setAdminAuditLogs] = useState<AdminAuditLog[]>([]);
  const [adminTab, setAdminTab] = useState<AdminTab>("projects");
  const [governanceTab, setGovernanceTab] = useState<GovernanceTab>("project");
  const [governanceDialog, setGovernanceDialog] = useState<GovernanceDialog | null>(null);
  const [governanceDialogError, setGovernanceDialogError] = useState<string | null>(null);
  const [about, setAbout] = useState<AboutPayload | null>(null);
  const [appUpdateDialog, setAppUpdateDialog] = useState<AppUpdateDialogState>({
    open: false,
    phase: "checking",
    manual: false
  });
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    open: false,
    x: 0,
    y: 0
  });
  const [selectedDraftPath, setSelectedDraftPath] = useState<string | null>(null);
  const [selectedPluginDraftPath, setSelectedPluginDraftPath] = useState<string | null>(null);
  const [draftMeta, setDraftMeta] = useState<PublishMeta>(emptyPublishMeta());
  const [pluginDraftMeta, setPluginDraftMeta] = useState<PublishMeta>(emptyPublishMeta());
  const [remoteProjectDraft, setRemoteProjectDraft] = useState<MarketProject>(emptyMarketProject());
  const [marketCategoryDraft, setMarketCategoryDraft] = useState<Category>(emptyMarketCategory());
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("正在载入 Skill Hub...");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const checkingAppUpdateRef = useRef(false);
  const adminEntryClickCountRef = useRef(0);
  const localScanInFlightRef = useRef(false);

  const openAppUpdateDialog = useCallback((manual = true) => {
    setAppUpdateDialog({
      open: true,
      phase: "checking",
      manual
    });
    void api
      .checkForUpdates()
      .then((result) => {
        setAppUpdateDialog({
          open: true,
          phase: result.available ? "available" : "current",
          result,
          manual
        });
      })
      .catch((err) => {
        setAppUpdateDialog({
          open: true,
          phase: "error",
          error: readError(err),
          manual
        });
      });
  }, []);

  const showAvailableAppUpdate = useCallback((result: UpdateCheckResult, manual = false) => {
    setAppUpdateDialog({
      open: true,
      phase: "available",
      result,
      manual
    });
  }, []);

  const downloadAppUpdate = useCallback(async () => {
    setAppUpdateDialog((current) => ({
      ...current,
      phase: "downloading",
      error: null
    }));
    try {
      const downloaded = await api.downloadUpdate();
      setAppUpdateDialog((current) => ({
        ...current,
        phase: "downloaded",
        downloaded
      }));
    } catch (err) {
      setAppUpdateDialog((current) => ({
        ...current,
        phase: "error",
        error: readError(err)
      }));
    }
  }, []);

  const restartAfterAppUpdate = useCallback(async () => {
    try {
      await api.restartAfterUpdate();
    } catch (err) {
      setAppUpdateDialog((current) => ({
        ...current,
        phase: "error",
        error: readError(err)
      }));
    }
  }, []);

  const handleBackgroundAppUpdateAvailable = useCallback(
    async (result: UpdateCheckResult) => {
      if (!result.available || checkingAppUpdateRef.current) return;
      checkingAppUpdateRef.current = true;
      try {
        showAvailableAppUpdate(result, false);
      } catch (err) {
        await message(readError(err), {
          title: "Skill Hub 更新失败",
          type: "error",
          okLabel: "确定"
        });
      } finally {
        checkingAppUpdateRef.current = false;
      }
    },
    [showAvailableAppUpdate]
  );

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // Storage can be unavailable in constrained WebView environments.
    }
  }, [theme]);

  useEffect(() => {
    if (!canUseTauriEvents) return;
    let unlistenUpdate: (() => void) | undefined;
    let unlistenAbout: (() => void) | undefined;
    let unlistenAppUpdate: (() => void) | undefined;
    void listen<UpdateCheckResult>("update-available", (event) => {
      void handleBackgroundAppUpdateAvailable(event.payload);
    }).then((fn) => {
      unlistenUpdate = fn;
    });
    void listen<AboutPayload>("show-about", (event) => {
      setAbout(event.payload);
    }).then((fn) => {
      unlistenAbout = fn;
    });
    void listen("open-app-update", () => {
      openAppUpdateDialog(true);
    }).then((fn) => {
      unlistenAppUpdate = fn;
    });
    return () => {
      unlistenUpdate?.();
      unlistenAbout?.();
      unlistenAppUpdate?.();
    };
  }, [handleBackgroundAppUpdateAvailable, openAppUpdateDialog]);

  useEffect(() => {
    if (!contextMenu.open) return;

    const closeContextMenu = () => setContextMenu((current) => ({ ...current, open: false }));
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeContextMenu();
      }
    };

    window.addEventListener("click", closeContextMenu);
    window.addEventListener("blur", closeContextMenu);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("click", closeContextMenu);
      window.removeEventListener("blur", closeContextMenu);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [contextMenu.open]);

  useEffect(() => {
    async function init() {
      try {
        await load();
      } catch (err) {
        console.error("Failed to initialize:", err);
      } finally {
        setLoading(false);
      }
    }
    void init();
  }, []);

  const publicCategories = useMemo(
    () => normalizeCategoryList(data.categories),
    [data.categories]
  );

  // 默认显示"全部"项目，不自动选择第一个项目

  const filteredSkills = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return data.skills.filter((skill) => {
      const publicScopeOk = !isProjectMarketSkill(skill);
      const categoryOk =
        marketMode === "project"
          ? selectedMarketProjectSlug === ""
            ? isProjectMarketSkill(skill) // "全部"：显示所有项目 skill
            : skill.categories.includes(`project:${selectedMarketProjectSlug}`)
          : publicScopeOk &&
            (selectedCategory === "all" ||
              skill.categories.includes(selectedCategory));
      const queryOk =
        normalized.length === 0 ||
        [skill.name, skill.id, skill.namespace, skill.summary, ...skill.tags]
          .join(" ")
          .toLowerCase()
          .includes(normalized);

      return categoryOk && queryOk;
    });
  }, [data.skills, marketMode, query, selectedCategory, selectedMarketProjectSlug]);

  const filteredPlugins = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return data.plugins.filter((plugin) => {
      const publicScopeOk = !isProjectMarketPlugin(plugin);
      const categoryOk =
        marketMode === "project"
          ? selectedMarketProjectSlug === ""
            ? isProjectMarketPlugin(plugin)
            : plugin.categories.includes(`project:${selectedMarketProjectSlug}`)
          : publicScopeOk &&
            (selectedCategory === "all" ||
              plugin.categories.includes(selectedCategory));
      const queryOk =
        normalized.length === 0 ||
        [plugin.name, plugin.id, plugin.namespace, plugin.summary, ...plugin.tags, ...plugin.components]
          .join(" ")
          .toLowerCase()
          .includes(normalized);

      return categoryOk && queryOk;
    });
  }, [data.plugins, marketMode, query, selectedCategory, selectedMarketProjectSlug]);

  const selectedSkill = useMemo(() => {
    if (filteredSkills.length === 0) {
      return undefined;
    }
    return filteredSkills.find((skill) => skillKey(skill) === selectedSkillKey) ?? filteredSkills[0];
  }, [filteredSkills, selectedSkillKey]);

  const selectedPlugin = useMemo(() => {
    if (filteredPlugins.length === 0) {
      return undefined;
    }
    return filteredPlugins.find((plugin) => pluginKey(plugin) === selectedPluginKey) ?? filteredPlugins[0];
  }, [filteredPlugins, selectedPluginKey]);

  useEffect(() => {
    if (filteredSkills.length === 0) {
      if (selectedSkillKey !== null) {
        setSelectedSkillKey(null);
      }
      return;
    }

    const selectedSkillVisible =
      selectedSkillKey !== null &&
      filteredSkills.some((skill) => skillKey(skill) === selectedSkillKey);
    if (!selectedSkillVisible) {
      setSelectedSkillKey(skillKey(filteredSkills[0]));
    }
  }, [filteredSkills, selectedSkillKey]);

  useEffect(() => {
    if (filteredPlugins.length === 0) {
      if (selectedPluginKey !== null) {
        setSelectedPluginKey(null);
      }
      return;
    }

    const selectedPluginVisible =
      selectedPluginKey !== null &&
      filteredPlugins.some((plugin) => pluginKey(plugin) === selectedPluginKey);
    if (!selectedPluginVisible) {
      setSelectedPluginKey(pluginKey(filteredPlugins[0]));
    }
  }, [filteredPlugins, selectedPluginKey]);

  useEffect(() => {
    if (marketArtifactKind === "plugin" && installTarget === "codex" && installLevel === "project") {
      setInstallLevel("personal");
    }
  }, [installLevel, installTarget, marketArtifactKind]);

  const bindingsBySkill = useMemo(() => {
    const map = new Map<string, SkillBinding[]>();
    for (const binding of data.bindings) {
      const key = `${binding.namespace}/${binding.skillId}`;
      map.set(key, [...(map.get(key) ?? []), binding]);
    }
    return map;
  }, [data.bindings]);

  const bindingsByPlugin = useMemo(() => {
    const map = new Map<string, typeof data.pluginBindings>();
    for (const binding of data.pluginBindings) {
      const key = `${binding.namespace}/${binding.pluginId}`;
      map.set(key, [...(map.get(key) ?? []), binding]);
    }
    return map;
  }, [data.pluginBindings]);

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

  const isSystemAdmin = adminSession?.role === "system";

  useEffect(() => {
    if (adminSession && !isSystemAdmin && adminTab === "audit") {
      setAdminTab("projects");
    }
  }, [adminSession, adminTab, isSystemAdmin]);

  const canManageProject = (_slug: string) =>
    Boolean(adminSession && (adminSession.role === "system" || adminSession.role === "project"));

  const canManageSkill = (skill: MarketSkill) =>
    Boolean(
      adminSession &&
        skill.categories.every((category) => {
          const projectSlug = category.startsWith("project:") ? category.slice("project:".length) : null;
          return projectSlug ? canManageProject(projectSlug) : isSystemAdmin;
        })
    );

  const canManagePlugin = (plugin: MarketPlugin) =>
    Boolean(
      adminSession &&
        plugin.categories.every((category) => {
          const projectSlug = category.startsWith("project:") ? category.slice("project:".length) : null;
          return projectSlug ? canManageProject(projectSlug) : isSystemAdmin;
        })
    );

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
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
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
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      setNotice("市场元数据已从 MinIO 同步");
    } catch (err) {
      setError(`市场索引刷新失败：${readError(err)}`);
    } finally {
      setBusy(false);
    }
  }

  async function openView(nextView: ViewKey) {
    if (nextView === "admin") {
      if (!adminSession) {
        setAdminUnlockOpen(true);
        return;
      }
      setAdminVisible(true);
    }
    setView(nextView);
    if (nextView === "market") {
      await refreshCatalog();
    } else if (nextView === "installed") {
      await scanLocal({ silent: true });
    }
  }

  function revealAdminEntry() {
    if (adminSession || adminUnlockOpen) {
      adminEntryClickCountRef.current = 0;
      return;
    }
    adminEntryClickCountRef.current += 1;
    if (adminEntryClickCountRef.current < ADMIN_ENTRY_CLICK_THRESHOLD) {
      return;
    }
    adminEntryClickCountRef.current = 0;
    setAdminUnlockOpen(true);
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

  async function installSelectedPlugin() {
    if (!selectedPlugin) return;
    if (installTarget === "codex" && installLevel === "project") {
      setError("Codex plugin 当前只支持个人级安装，不支持项目级生效");
      return;
    }
    if (installLevel === "project" && !installProjectPath) {
      setError("请先在项目菜单绑定项目，并选择一个项目");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const request: InstallPluginRequest = {
        sourceId: selectedPlugin.sourceId,
        namespace: selectedPlugin.namespace,
        pluginId: selectedPlugin.id,
        version: null,
        target: installTarget,
        scope: installLevel === "project" ? "project" : "user",
        projectPath: installLevel === "project" ? installProjectPath : null,
        installMode: "marketplace",
        updatePolicy,
        enable: installLevel !== "download"
      };
      await api.installPlugin(request);
      await load();
      const installText =
        installLevel === "download"
          ? "缓存"
          : installTarget === "codex"
            ? "写入 marketplace 并安装到 Codex"
            : installTarget === "claude"
              ? "写入 marketplace 并安装到 Claude Code，当前会话请执行 /reload-plugins"
              : "写入 marketplace";
      setNotice(`${selectedPlugin.name} 已${installText}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleUpgradeBinding(update: UpdateCandidate) {
    setBusy(true);
    setError(null);
    try {
      const result =
        update.kind === "plugin"
          ? await api.upgradePluginBinding(update.bindingId)
          : await api.upgradeSkillBinding(update.bindingId);
      setData((current) => ({ ...current, ...result, marketProjects: normalizeProjectList(result.marketProjects) }));
      setNotice(update.kind === "plugin" ? "Plugin 已升级到最新版本" : "Skill 已升级到最新版本");
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
      const request: SkillPreviewRequest = {
        sourceId: skill.sourceId,
        namespace: skill.namespace,
        skillId: skill.id,
        version: null
      };
      const result = await api.previewSkill(request);
      setPreview(result);
      setPreviewContext({ kind: "skill", request });
      setNotice(`正在预览 ${skill.name}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewMarketPlugin(plugin: MarketPlugin) {
    setBusy(true);
    setError(null);
    try {
      const target = plugin.targets.includes(installTarget) ? installTarget : plugin.targets[0] ?? "codex";
      const request: PluginPreviewRequest = {
        sourceId: plugin.sourceId,
        namespace: plugin.namespace,
        pluginId: plugin.id,
        version: null,
        target
      };
      const result = await api.previewPlugin(request);
      setPreview(result);
      setPreviewContext({ kind: "plugin", request });
      setNotice(`正在预览 ${plugin.name}`);
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

  async function deleteCachedPlugin(item: AppBootstrap["pluginPackages"][number]) {
    setBusy(true);
    setError(null);
    try {
      await api.deleteCachedPlugin({
        sourceId: item.sourceId,
        namespace: item.namespace,
        pluginId: item.pluginId,
        version: item.version,
        target: item.target
      } as DeleteCachedPluginRequest);
      await load();
      setNotice(`${item.pluginName} ${item.version} 的本地缓存已删除`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  function openLocalDeleteDialog(skill: LocalSkill) {
    setLocalDeleteDialog(skill);
    setError(null);
  }

  async function deleteLocalSkill(skill: LocalSkill) {
    setBusy(true);
    setError(null);
    try {
      const rows = await api.deleteLocalSkill({ id: skill.id });
      setData((current) => ({ ...current, localSkills: rows }));
      setNotice(`${skill.detectedManifest ?? skill.path} 已删除`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function confirmLocalDeleteSkill() {
    if (!localDeleteDialog) return;
    const skill = localDeleteDialog;
    setLocalDeleteDialog(null);
    await deleteLocalSkill(skill);
  }

  async function previewCachedSkill(item: CachedSkillItem) {
    setBusy(true);
    setError(null);
    try {
      const request: SkillPreviewRequest = {
        sourceId: item.package.sourceId,
        namespace: item.package.namespace,
        skillId: item.package.skillId,
        version: item.package.version
      };
      const result = await api.previewSkill(request);
      setPreview(result);
      setPreviewContext({ kind: "skill", request });
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
      const request: SkillPreviewRequest = { bindingId: binding.id };
      const result = await api.previewSkill(request);
      setPreview(result);
      setPreviewContext({ kind: "skill", request });
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
      const request: SkillPreviewRequest = { path: skill.path };
      const result = await api.previewSkill(request);
      setPreview(result);
      setPreviewContext({ kind: "skill", request });
      setNotice(`正在预览 ${skill.detectedManifest ?? skill.path}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewPluginBinding(binding: AppBootstrap["pluginBindings"][number]) {
    setBusy(true);
    setError(null);
    try {
      const request: PluginPreviewRequest = { bindingId: binding.id };
      const result = await api.previewPlugin(request);
      setPreview(result);
      setPreviewContext({ kind: "plugin", request });
      setNotice(`正在预览 ${binding.pluginName}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewCachedPlugin(item: AppBootstrap["pluginPackages"][number]) {
    setBusy(true);
    setError(null);
    try {
      const request: PluginPreviewRequest = {
        sourceId: item.sourceId,
        namespace: item.namespace,
        pluginId: item.pluginId,
        version: item.version,
        target: item.target,
        path: item.packagePath
      };
      const result = await api.previewPlugin(request);
      setPreview(result);
      setPreviewContext({ kind: "plugin", request });
      setNotice(`正在预览 ${item.pluginName} ${item.version}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewLocalPlugin(plugin: AppBootstrap["localPlugins"][number]) {
    setBusy(true);
    setError(null);
    try {
      const request: PluginPreviewRequest = { path: plugin.path, target: plugin.target };
      const result = await api.previewPlugin(request);
      setPreview(result);
      setPreviewContext({ kind: "plugin", request });
      setNotice(`正在预览 ${localPluginDisplayName(plugin)}`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function importLocalSkill(skill: LocalSkill, installAfterImport = false, options?: LocalInstallOptions) {
    if (!skill.canImportToCache) {
      setError("该本地 skill 已匹配市场或由 Skill Hub 管理，不能作为自建 skill 导入");
      return;
    }
    const resolvedLevel = options?.level ?? "personal";
    const resolvedProjectPath = resolvedLevel === "project" ? options?.projectPath ?? "" : null;
    if (installAfterImport && resolvedLevel === "project" && !resolvedProjectPath) {
      setError("请先在项目菜单绑定项目，并选择一个项目");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const cached = await api.importLocalSkillToCache({
        path: skill.path,
        skillId: skill.skillId ?? null,
        version: skill.version ?? null,
        overwrite: true
      });
      if (installAfterImport) {
        await installCachedPackage(cached, false, {
          target: options?.target ?? "codex",
          level: resolvedLevel,
          projectPath: resolvedProjectPath
        });
      } else {
        const localSkills = markLocalSkillsCached(await api.scanLocalSkills(), cached, skill);
        setData((current) => ({
          ...current,
          cachedPackages: upsertCachedPackage(current.cachedPackages, cached),
          localSkills
        }));
        setNotice(`${cached.skillName} 已加入本地缓存`);
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function installCachedPackage(
    item: CachedSkillItem | CachedSkillPackage,
    manageBusy = true,
    options?: LocalInstallOptions
  ) {
    const cachedPackage = "package" in item ? item.package : item;
    if (cachedPackage.origin !== "local") {
      setError("市场缓存请从市场详情页安装，避免影响市场升级链路");
      return;
    }
    const resolvedLevel = options?.level ?? "personal";
    const resolvedProjectPath = resolvedLevel === "project" ? options?.projectPath ?? "" : null;
    if (resolvedLevel === "project" && !resolvedProjectPath) {
      setError("请先在项目菜单绑定项目，并选择一个项目");
      return;
    }

    if (manageBusy) {
      setBusy(true);
      setError(null);
    }
    try {
      const request: InstallCachedSkillRequest = {
        sourceId: cachedPackage.sourceId,
        namespace: cachedPackage.namespace,
        skillId: cachedPackage.skillId,
        version: cachedPackage.version,
        target: options?.target ?? "codex",
        level: resolvedLevel,
        projectPath: resolvedProjectPath,
        installMode: "copy",
        updatePolicy: "pinned",
        enable: true
      };
      await api.installCachedSkill(request);
      await load();
      setNotice(`${cachedPackage.skillName} 已安装`);
    } catch (err) {
      setError(readError(err));
    } finally {
      if (manageBusy) {
        setBusy(false);
      }
    }
  }

  function openLocalInstallDialog(dialog: LocalInstallDialogState) {
    const availableTargets = availableLocalInstallTargets(dialog, data.bindings, data.localSkills);
    if (availableTargets.length === 0) {
      setError("该 skill 已安装到所有支持的平台");
      return;
    }
    setLocalInstallDialog(dialog);
    setLocalInstallLevel("personal");
    setLocalInstallProjectPath("");
    const localTarget = dialog.kind === "local" && isLocalInstallTarget(dialog.skill.target) ? dialog.skill.target : null;
    setLocalInstallTarget(
      localTarget && availableTargets.includes(localTarget) ? localTarget : availableTargets[0]
    );
    setError(null);
  }

  async function confirmLocalInstall() {
    if (!localInstallDialog) return;
    const availableTargets = availableLocalInstallTargets(localInstallDialog, data.bindings, data.localSkills);
    if (!availableTargets.includes(localInstallTarget)) {
      setError("该平台已安装，请选择其他平台");
      return;
    }
    const resolvedProjectPath = localInstallLevel === "project" ? localInstallProjectPath : null;
    if (localInstallLevel === "project" && !resolvedProjectPath) {
      setError("请先选择一个已绑定项目");
      return;
    }

    const options: LocalInstallOptions = {
      target: localInstallTarget,
      level: localInstallLevel,
      projectPath: resolvedProjectPath
    };
    const dialog = localInstallDialog;
    setLocalInstallDialog(null);
    if (dialog.kind === "local") {
      await importLocalSkill(dialog.skill, true, options);
      return;
    }
    await installCachedPackage(dialog.item, true, options);
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

  async function toggleLocalSkill(skill: LocalSkill) {
    if (skill.managedBySkillhub) {
      setError("Skill Hub 管理的绑定请使用市场绑定开关");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const rows = await api.setLocalSkillEnabled({ id: skill.id, enabled: !skill.enabled });
      setData((current) => ({ ...current, localSkills: rows }));
      setNotice(skill.enabled ? "已禁用自建 skill" : "已启用自建 skill");
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

  async function togglePluginBinding(binding: AppBootstrap["pluginBindings"][number]) {
    setBusy(true);
    setError(null);
    try {
      await api.setPluginBindingEnabled(binding.id, !binding.enabled);
      await scanLocal({ silent: true });
      await load();
      setNotice(
        binding.enabled
          ? `Plugin 已从 ${
              binding.target === "codex" ? "Codex 和 " : binding.target === "claude" ? "Claude Code 和 " : ""
            }marketplace 禁用`
          : `Plugin 已写回 marketplace${
              binding.target === "codex"
                ? " 并安装到 Codex"
                : binding.target === "claude"
                  ? " 并安装到 Claude Code，请在当前会话执行 /reload-plugins"
                  : ""
            }`
      );
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function uninstallPluginBinding(binding: AppBootstrap["pluginBindings"][number]) {
    setBusy(true);
    setError(null);
    try {
      await api.uninstallPlugin(binding.id);
      await scanLocal({ silent: true });
      await load();
      setNotice("Plugin 绑定已移除");
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

  async function scanLocal(options: { silent?: boolean } = {}) {
    if (localScanInFlightRef.current) return;
    localScanInFlightRef.current = true;
    setBusy(true);
    setError(null);
    try {
      const [skills, plugins] = await Promise.all([api.scanLocalSkills(), api.scanLocalPlugins()]);
      setData((current) => ({ ...current, localSkills: skills, localPlugins: plugins }));
      if (!options.silent) {
        setNotice("本地 skill / plugin 已扫描");
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      localScanInFlightRef.current = false;
      setBusy(false);
    }
  }

  async function unlockAdmin() {
    setBusy(true);
    setError(null);
    try {
      const session = await api.unlockAdminMode(adminKey);
      setAdminSession(session);
      setAdminVisible(true);
      setAdminUnlockOpen(false);
      setView("admin");
      setNotice("管理员模式已解锁");
      await refreshAdminDrafts();
      await refreshAdminPluginDrafts();
      if (session.role === "system") {
        await refreshAdminAuditLogs(false);
      } else {
        setAdminAuditLogs([]);
        if (adminTab === "audit") {
          setAdminTab("projects");
        }
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function refreshAdminDrafts() {
    if (!adminKey.trim()) {
      setError("请先输入管理员密钥");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const drafts = await api.listAdminDrafts(adminKey);
      setAdminDrafts(drafts);
      if (!selectedDraftPath && drafts.length > 0) {
        selectDraft(drafts[0]);
      }
      setNotice("草稿区已刷新");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function refreshAdminPluginDrafts() {
    if (!adminKey.trim()) {
      setError("Please enter admin key first");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const drafts = await api.listAdminPluginDrafts(adminKey);
      setAdminPluginDrafts(drafts);
      if (!selectedPluginDraftPath && drafts.length > 0) {
        selectPluginDraft(drafts[0]);
      }
      setNotice("Plugin 草稿区已刷新");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function refreshAdminAuditLogs(showBusy = true) {
    if (adminSession && adminSession.role !== "system") {
      setAdminAuditLogs([]);
      return;
    }
    if (!adminKey.trim()) {
      setError("请先输入管理员密钥");
      return;
    }
    if (showBusy) {
      setBusy(true);
    }
    setError(null);
    try {
      const logs = await api.listAdminAuditLogs(adminKey, 100);
      setAdminAuditLogs(logs);
      if (showBusy) {
        setNotice("审计记录已刷新");
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      if (showBusy) {
        setBusy(false);
      }
    }
  }

  function selectDraft(draft: AdminDraftSkill) {
    setSelectedDraftPath(draft.gitlabSourcePath);
    const nextMeta = draft.publishMeta ?? defaultMetaFromDraft(draft);
    if (adminSession?.role === "project" && nextMeta.publishScope !== "project") {
      setDraftMeta({
        ...nextMeta,
        publishScope: "project",
        publishCategorySlug: null,
        publishProjectSlug: data.marketProjects[0]?.slug ?? null
      });
      return;
    }
    setDraftMeta(nextMeta);
  }

  function selectPluginDraft(draft: AdminDraftPlugin) {
    setSelectedPluginDraftPath(draft.gitlabSourcePath);
    const nextMeta = draft.publishMeta ?? defaultMetaFromPluginDraft(draft);
    if (adminSession?.role === "project" && nextMeta.publishScope !== "project") {
      setPluginDraftMeta({
        ...nextMeta,
        publishScope: "project",
        publishCategorySlug: null,
        publishProjectSlug: data.marketProjects[0]?.slug ?? null
      });
      return;
    }
    setPluginDraftMeta(nextMeta);
  }

  async function saveDraftMeta() {
    if (!selectedDraftPath) {
      setError("请先选择草稿");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await api.savePublishMeta(adminKey, selectedDraftPath, normalizeMetaForSave(draftMeta));
      setDraftMeta(saved);
      await refreshAdminDrafts();
      await refreshAdminAuditLogs(false);
      setNotice("发布元数据已保存");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function savePluginDraftMeta() {
    if (!selectedPluginDraftPath) {
      setError("请先选择 Plugin 草稿");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await api.savePublishMeta(
        adminKey,
        selectedPluginDraftPath,
        normalizeMetaForSave(pluginDraftMeta),
        "plugin"
      );
      setPluginDraftMeta(saved);
      await refreshAdminPluginDrafts();
      await refreshAdminAuditLogs(false);
      setNotice("Plugin 发布元数据已保存");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewSelectedDraft() {
    if (!selectedDraftPath) {
      setError("请先选择草稿");
      return;
    }

    const selectedDraft = adminDrafts.find((draft) => draft.gitlabSourcePath === selectedDraftPath);
    if (selectedDraft && !selectedDraft.sourceAvailable) {
      setError("该草稿未关联 GitLab 源文件，无法预览。请等待 GitLab 重新同步 SKILL.md，或使用快速重新上架功能直接发布已有版本。");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const request: AdminDraftPreviewRequest = {
        adminKey,
        gitlabSourcePath: selectedDraftPath
      };
      const result = await api.previewAdminDraft(request);
      setPreview(result);
      setPreviewContext({ kind: "adminDraft", request });
      setNotice("草稿预览已生成");
    } catch (err) {
      const errorMsg = readError(err);
      if (errorMsg.includes("SKILL.md")) {
        setError("读取草稿源文件失败，该草稿可能未关联 GitLab 源。请刷新草稿列表或等待 GitLab 同步。");
      } else {
        setError(errorMsg);
      }
    } finally {
      setBusy(false);
    }
  }

  async function previewPluginDraft(draft: AdminDraftPlugin) {
    setBusy(true);
    setError(null);
    try {
      const request: AdminDraftPreviewRequest = {
        adminKey,
        gitlabSourcePath: draft.gitlabSourcePath
      };
      const result = await api.previewAdminPluginDraft(request);
      setPreview(result);
      setPreviewContext({ kind: "adminPluginDraft", request });
      setNotice("Plugin 草稿预览已生成");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function previewSelectedPluginDraft() {
    const draft = adminPluginDrafts.find((item) => item.gitlabSourcePath === selectedPluginDraftPath);
    if (!draft) {
      setError("请先选择 Plugin 草稿");
      return;
    }
    await previewPluginDraft(draft);
  }

  async function loadPreviewFile(filePath: string) {
    if (!previewContext) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (previewContext.kind === "adminDraft") {
        const request = { ...previewContext.request, filePath };
        const result = await api.previewAdminDraft(request);
        setPreview(result);
        setPreviewContext({ kind: "adminDraft", request });
      } else if (previewContext.kind === "adminPluginDraft") {
        const request = { ...previewContext.request, filePath };
        const result = await api.previewAdminPluginDraft(request);
        setPreview(result);
        setPreviewContext({ kind: "adminPluginDraft", request });
      } else if (previewContext.kind === "plugin") {
        const request = { ...previewContext.request, filePath };
        const result = await api.previewPlugin(request);
        setPreview(result);
        setPreviewContext({ kind: "plugin", request });
      } else {
        const request = { ...previewContext.request, filePath };
        const result = await api.previewSkill(request);
        setPreview(result);
        setPreviewContext({ kind: "skill", request });
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function publishSelectedDraft() {
    if (!selectedDraftPath) {
      setError("请先选择草稿");
      return;
    }
    const selectedDraft = adminDrafts.find((draft) => draft.gitlabSourcePath === selectedDraftPath);
    if (isPublishedDraft(selectedDraft)) {
      setError("该草稿当前版本已发布，不能重复发布到市场");
      return;
    }

    const missingMetaMessage = publishMetaMissingMessage(draftMeta);
    if (missingMetaMessage) {
      setError(`发布元数据不完整：${missingMetaMessage}`);
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const saved = await api.savePublishMeta(adminKey, selectedDraftPath, normalizeMetaForSave(draftMeta));
      setDraftMeta(saved);
      const next = await api.publishDraft(adminKey, selectedDraftPath);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      await refreshAdminDrafts();
      await refreshAdminAuditLogs(false);
      setNotice("草稿已发布到市场");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function publishSelectedPluginDraft() {
    if (!selectedPluginDraftPath) {
      setError("请先选择 Plugin 草稿");
      return;
    }
    const draft = adminPluginDrafts.find((item) => item.gitlabSourcePath === selectedPluginDraftPath);
    if (!draft) {
      setError("请先选择 Plugin 草稿");
      return;
    }
    if (draft.status === "published" && draft.publishedVersion === draft.version) {
      setError("该 Plugin 当前版本已发布");
      return;
    }
    const missingMetaMessage = publishMetaMissingMessage(pluginDraftMeta, "plugin");
    if (missingMetaMessage) {
      setError(`Plugin 发布元数据不完整：${missingMetaMessage}`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await api.savePublishMeta(
        adminKey,
        selectedPluginDraftPath,
        normalizeMetaForSave(pluginDraftMeta),
        "plugin"
      );
      setPluginDraftMeta(saved);
      const next = await api.publishPluginDraft(adminKey, selectedPluginDraftPath);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      await refreshAdminPluginDrafts();
      await refreshAdminAuditLogs(false);
      setNotice("Plugin 已发布到市场");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function quickRepublishSelectedDraft() {
    if (!selectedDraftPath) {
      setError("请先选择草稿");
      return;
    }

    const selectedDraft = adminDrafts.find((draft) => draft.gitlabSourcePath === selectedDraftPath);

    if (!selectedDraft || selectedDraft.sourceAvailable) {
      setError("该功能仅适用于未关联 GitLab 源的已下架 skill");
      return;
    }

    // 不在前端检查状态，让后端来验证（因为前端显示的是翻译后的中文状态）

    const missingMetaMessage = publishMetaMissingMessage(draftMeta);
    if (missingMetaMessage) {
      setError(`发布元数据不完整：${missingMetaMessage}`);
      return;
    }

    setBusy(true);
    setError(null);
    try {
      // 先保存元数据
      const saved = await api.savePublishMeta(adminKey, selectedDraftPath, normalizeMetaForSave(draftMeta));
      setDraftMeta(saved);

      // 快速重新上架
      const next = await api.quickRepublishArchivedSkill(adminKey, selectedDraftPath);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      await refreshAdminDrafts();
      await refreshAdminAuditLogs(false);
      setNotice("已快速重新上架到市场");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function saveRemoteProject() {
    setBusy(true);
    setError(null);
    setGovernanceDialogError(null);
    try {
      const projects = await api.saveMarketProjectRemote(adminKey, remoteProjectDraft);
      setData((current) => ({ ...current, marketProjects: normalizeProjectList(projects) }));
      setRemoteProjectDraft(emptyMarketProject());
      setGovernanceDialog(null);
      await refreshAdminAuditLogs(false);
      setNotice("市场项目已保存");
    } catch (err) {
      setGovernanceDialogError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function deleteRemoteProject(project: MarketProject) {
    setBusy(true);
    setError(null);
    setGovernanceDialogError(null);
    try {
      const next = await api.deleteMarketProjectRemote(adminKey, project.slug);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      if (selectedMarketProjectSlug === project.slug) {
        setSelectedMarketProjectSlug("");
      }
      setGovernanceDialog(null);
      await refreshAdminAuditLogs(false);
      setNotice(`${project.name} 已从远程市场项目中删除`);
    } catch (err) {
      setGovernanceDialogError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function saveMarketCategory() {
    setBusy(true);
    setError(null);
    setGovernanceDialogError(null);
    try {
      const categories = await api.saveMarketCategoryRemote(adminKey, marketCategoryDraft);
      setData((current) => ({ ...current, categories: normalizeCategoryList(categories) }));
      setMarketCategoryDraft(emptyMarketCategory());
      setGovernanceDialog(null);
      await refreshAdminAuditLogs(false);
      setNotice("公共分类已保存");
    } catch (err) {
      setGovernanceDialogError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function deleteMarketCategory(category: Category) {
    setBusy(true);
    setError(null);
    setGovernanceDialogError(null);
    try {
      const next = await api.deleteMarketCategoryRemote(adminKey, category.id);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      setGovernanceDialog(null);
      await refreshAdminAuditLogs(false);
      setNotice(`${category.name} 已删除`);
    } catch (err) {
      setGovernanceDialogError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function archiveMarketSkill(skill: MarketSkill) {
    setBusy(true);
    setError(null);
    try {
      const next = await api.archiveMarketSkill(adminKey, skill.namespace, skill.id);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      await refreshAdminDrafts();
      await refreshAdminAuditLogs(false);
      setNotice(`${skill.name} 已下架并回到草稿区`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  async function archiveMarketPlugin(plugin: MarketPlugin) {
    setBusy(true);
    setError(null);
    try {
      const next = await api.archiveMarketPlugin(adminKey, plugin.namespace, plugin.id);
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      await refreshAdminPluginDrafts();
      await refreshAdminAuditLogs(false);
      setNotice(`${plugin.name} 已下架并回到 Plugin 草稿区`);
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }

  const localSkillNavCount = Math.max(data.bindings.length, data.cachedPackages.length, data.localSkills.length);
  const localPluginNavCount = Math.max(data.pluginBindings.length, data.pluginPackages.length, data.localPlugins.length);
  const localNavCount = localSkillNavCount + localPluginNavCount;
  const navItems = [
    { key: "market" as const, label: "市场", icon: Blocks, count: data.skills.length },
    { key: "installed" as const, label: "本地", icon: PackageCheck, count: localNavCount },
    { key: "projects" as const, label: "项目", icon: FolderGit2, count: data.projects.length },
    { key: "updates" as const, label: "更新", icon: RefreshCw, count: data.updates.length },
    { key: "settings" as const, label: "设置", icon: Settings, count: data.targetRoots.length },
    ...(adminVisible || adminSession
      ? [{ key: "admin" as const, label: "管理", icon: ShieldCheck, count: adminDrafts.length }]
      : [])
  ];

  function openContextMenu(event: React.MouseEvent) {
    event.preventDefault();
    const menuWidth = 156;
    const menuHeight = 46;
    setContextMenu({
      open: true,
      x: Math.min(event.clientX, window.innerWidth - menuWidth - 8),
      y: Math.min(event.clientY, window.innerHeight - menuHeight - 8)
    });
  }

  function reloadWindow() {
    window.location.reload();
  }

  if (loading) {
    return (
      <main className="boot-screen" aria-label="Skill Hub 正在启动">
        <aside className="boot-rail">
          <div className="boot-brand">
            <div className="boot-mark" aria-hidden="true">
              <Layers3 size={22} />
            </div>
            <div>
              <strong>Skill Hub</strong>
              <span>Skill Switchboard</span>
            </div>
          </div>
          <div className="boot-nav" aria-hidden="true">
            <i></i>
            <i></i>
            <i></i>
            <i></i>
          </div>
        </aside>
        <section className="boot-workspace">
          <header className="boot-topbar">
            <div className="boot-title">
              <small>Starting workspace</small>
              <strong>正在启动</strong>
            </div>
            <div className="boot-status">
              <span className="boot-dot" aria-hidden="true"></span>
              <span>正在载入</span>
            </div>
          </header>
          <div className="boot-panel">
            <div className="boot-card">
              <div className="boot-spinner" aria-hidden="true"></div>
              <strong>准备 Skill Hub 工作区</strong>
              <span>正在连接本地配置、市场缓存与技能索引。</span>
            </div>
          </div>
        </section>
      </main>
    );
  }

  return (
    <div className="app-shell" data-theme={theme} onContextMenu={openContextMenu}>
      <aside className="sidebar">
        <button className="brand-block" onClick={revealAdminEntry} type="button" title="Skill Hub">
          <div className="brand-mark">
            <Layers3 size={22} />
          </div>
          <div>
            <strong>Skill Hub</strong>
            <span>Skill Switchboard</span>
          </div>
        </button>

        <nav className="nav-stack">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.key}
                className={`nav-item ${view === item.key ? "active" : ""}`}
                onClick={() => void openView(item.key)}
                type="button"
                aria-current={view === item.key ? "page" : undefined}
              >
                <span className="nav-icon" aria-hidden="true">
                  <Icon size={16} />
                </span>
                <span>{item.label}</span>
                <b>{item.count}</b>
              </button>
            );
          })}
        </nav>

        <ThemeSwitch theme={theme} onTheme={setTheme} />
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

        {error && !governanceDialog ? (
          <div className="error-strip">
            <AlertCircle size={18} />
            <span>{error}</span>
          </div>
        ) : null}

        {view === "market" ? (
          <MarketView
            artifactKind={marketArtifactKind}
            onArtifactKind={setMarketArtifactKind}
            mode={marketMode}
            onMode={setMarketMode}
            marketProjects={data.marketProjects}
            selectedMarketProjectSlug={selectedMarketProjectSlug}
            onSelectedMarketProjectSlug={setSelectedMarketProjectSlug}
            categories={publicCategories}
            selectedCategory={selectedCategory}
            onSelectCategory={setSelectedCategory}
            query={query}
            onQuery={setQuery}
            skills={filteredSkills}
            plugins={filteredPlugins}
            marketSkillCount={data.skills.length}
            marketPluginCount={data.plugins.length}
            bindingsBySkill={bindingsBySkill}
            bindingsByPlugin={bindingsByPlugin}
            selectedSkill={selectedSkill}
            selectedPlugin={selectedPlugin}
            onSelectSkill={setSelectedSkillKey}
            onSelectPlugin={setSelectedPluginKey}
            onRefresh={refreshCatalog}
            installTarget={installTarget}
            onInstallTarget={setInstallTarget}
            installLevel={installLevel}
            onInstallLevel={setInstallLevel}
            onUnsupportedPluginProjectScope={() => {
              setError("Codex plugin 当前只支持个人级安装，不支持项目级生效");
            }}
            installProjectPath={installProjectPath}
            onInstallProjectPath={setInstallProjectPath}
            targetRoots={data.targetRoots}
            projects={data.projects}
            onInstall={() => void (marketArtifactKind === "plugin" ? installSelectedPlugin() : installSelectedSkill())}
            onPreview={previewMarketSkill}
            onPreviewPlugin={previewMarketPlugin}
          />
        ) : null}

        {view === "installed" ? (
          <InstalledView
            bindings={data.bindings}
            cachedSkills={cachedSkills}
            pluginPackages={data.pluginPackages}
            pluginBindings={data.pluginBindings}
            localPlugins={data.localPlugins}
            onTogglePlugin={togglePluginBinding}
            onUninstallPlugin={uninstallPluginBinding}
            onPreviewPluginBinding={previewPluginBinding}
            onPreviewPluginCache={previewCachedPlugin}
            onDeletePluginCache={deleteCachedPlugin}
            onPreviewLocalPlugin={previewLocalPlugin}
            onToggle={toggleBinding}
            onToggleLocal={toggleLocalSkill}
            onUninstall={uninstallBinding}
            localSkills={data.localSkills}
            onScan={scanLocal}
            onPreviewBinding={previewBinding}
            onPreviewLocal={previewLocalSkill}
            onPreviewCache={previewCachedSkill}
            onDeleteCache={deleteCachedSkill}
            onDeleteLocal={openLocalDeleteDialog}
            onImportLocal={(skill) => void importLocalSkill(skill)}
            onInstallLocal={(skill) => openLocalInstallDialog({ kind: "local", skill })}
            onInstallCache={(item) => openLocalInstallDialog({ kind: "cache", item })}
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

        {view === "updates" ? (
          <UpdatesView updates={data.updates} onRefresh={refreshCatalog} onUpgrade={handleUpgradeBinding} busy={busy} />
        ) : null}

        {view === "settings" ? (
          <SettingsView
            targetRoots={data.targetRoots}
            targetRootDrafts={targetRootDrafts}
            onPickTargetRoot={(target) => void chooseFolder("root", target)}
            onSaveTargetRoot={(target) => void saveTargetRoot(target)}
          />
        ) : null}

        {view === "admin" && adminSession ? (
          <AdminView
            session={adminSession}
            activeTab={adminTab}
            onActiveTab={setAdminTab}
            governanceTab={governanceTab}
            onGovernanceTab={setGovernanceTab}
            governanceDialog={governanceDialog}
            governanceDialogError={governanceDialogError}
            busy={busy}
            onGovernanceDialog={(dialog) => {
              setGovernanceDialogError(null);
              setGovernanceDialog(dialog);
            }}
            drafts={adminDrafts}
            pluginDrafts={adminPluginDrafts}
            auditLogs={adminAuditLogs}
            onRefreshAuditLogs={() => void refreshAdminAuditLogs()}
            selectedDraftPath={selectedDraftPath}
            selectedPluginDraftPath={selectedPluginDraftPath}
            onRefreshDrafts={() => void refreshAdminDrafts()}
            onRefreshPluginDrafts={() => void refreshAdminPluginDrafts()}
            onSelectDraft={selectDraft}
            onSelectPluginDraft={selectPluginDraft}
            meta={draftMeta}
            onMeta={setDraftMeta}
            pluginMeta={pluginDraftMeta}
            onPluginMeta={setPluginDraftMeta}
            onSaveMeta={() => void saveDraftMeta()}
            onSavePluginMeta={() => void savePluginDraftMeta()}
            onPreview={() => void previewSelectedDraft()}
            onPreviewPlugin={() => void previewSelectedPluginDraft()}
            onPublish={() => void publishSelectedDraft()}
            onPublishPlugin={() => void publishSelectedPluginDraft()}
            onQuickRepublish={() => void quickRepublishSelectedDraft()}
            projects={data.marketProjects}
            projectDraft={remoteProjectDraft}
            onProjectDraft={setRemoteProjectDraft}
            onSaveProject={() => void saveRemoteProject()}
            onDeleteProject={(project) => void deleteRemoteProject(project)}
            categories={publicCategories}
            categoryDraft={marketCategoryDraft}
            onCategoryDraft={setMarketCategoryDraft}
            onSaveCategory={() => void saveMarketCategory()}
            onDeleteCategory={(category) => void deleteMarketCategory(category)}
            skills={data.skills}
            plugins={data.plugins}
            canManageProject={canManageProject}
            canManageSkill={canManageSkill}
            canManagePlugin={canManagePlugin}
            onArchiveSkill={(skill) => void archiveMarketSkill(skill)}
            onArchivePlugin={(plugin) => void archiveMarketPlugin(plugin)}
          />
        ) : null}

        {adminUnlockOpen && !adminSession ? (
          <AdminUnlockDialog
            adminKey={adminKey}
            onAdminKey={setAdminKey}
            busy={busy}
            onUnlock={() => void unlockAdmin()}
            onClose={() => setAdminUnlockOpen(false)}
          />
        ) : null}

        {preview ? (
          <PreviewPanel
            preview={preview}
            onSelectFile={(filePath) => void loadPreviewFile(filePath)}
            onClose={() => {
              setPreview(null);
              setPreviewContext(null);
            }}
          />
        ) : null}

        {localInstallDialog ? (
          <LocalInstallDialog
            dialog={localInstallDialog}
            target={localInstallTarget}
            onTarget={setLocalInstallTarget}
            level={localInstallLevel}
            onLevel={setLocalInstallLevel}
            projectPath={localInstallProjectPath}
            onProjectPath={setLocalInstallProjectPath}
            projects={data.projects}
            availableTargets={availableLocalInstallTargets(localInstallDialog, data.bindings, data.localSkills)}
            busy={busy}
            onConfirm={() => void confirmLocalInstall()}
            onClose={() => setLocalInstallDialog(null)}
          />
        ) : null}

        {localDeleteDialog ? (
          <LocalDeleteDialog
            skill={localDeleteDialog}
            busy={busy}
            onConfirm={() => void confirmLocalDeleteSkill()}
            onClose={() => setLocalDeleteDialog(null)}
          />
        ) : null}

        {about ? (
          <AboutDialog
            about={about}
            onOpenDocs={() => void openExternal(about.docs_url)}
            onFeedback={() => void openExternal(`mailto:${about.feedback_email}`)}
            onClose={() => setAbout(null)}
          />
        ) : null}

        {appUpdateDialog.open ? (
          <AppUpdateDialog
            state={appUpdateDialog}
            onCheck={() => openAppUpdateDialog(true)}
            onDownload={() => void downloadAppUpdate()}
            onRestart={() => void restartAfterAppUpdate()}
            onClose={() => {
              setAppUpdateDialog((current) => ({
                ...current,
                open: false
              }));
              checkingAppUpdateRef.current = false;
            }}
          />
        ) : null}
      </main>
      {contextMenu.open ? (
        <AppContextMenu x={contextMenu.x} y={contextMenu.y} onRefresh={reloadWindow} />
      ) : null}
    </div>
  );
}
function MarketView(props: {
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
                className="primary-action"
                onClick={props.onInstall}
                disabled={installState.disabled}
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

function InstalledView(props: {
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
        <button className="primary-action compact" onClick={props.onSave} disabled={!props.path.trim()}>
          <FolderGit2 size={17} />
          绑定项目
        </button>
      </div>

      <div className="project-grid">
        {props.projects.length > 0 ? props.projects.map((project) => {
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
        }) : (
          <EmptyState
            title="还没有项目绑定"
            body="选择一个项目文件夹后，Skill Hub 可以把 skill 安装到项目级目录。"
          />
        )}
      </div>
    </section>
  );
}

function UpdatesView(props: {
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

function UpdateDetailCard(props: {
  update: UpdateCandidate;
  busy: boolean;
  onUpgrade: (update: UpdateCandidate) => void;
}) {
  const isPlugin = props.update.kind === "plugin";
  const artifactLabel = isPlugin ? "Plugin" : "Skill";
  const scopeText = props.update.level === "project" ? props.update.projectPath ?? "项目级" : "个人级";
  const steps = isPlugin
    ? [
        ["下载插件包", "从市场目录获取最新通用插件源和 pluginhub.json 元数据。"],
        ["生成平台目录", "根据目标动态生成 Codex / Claude 所需 manifest 和 marketplace 结构。"],
        ["执行 CLI 同步", "自动调用对应平台安装或同步命令，CLI 缺失时给出安装引导。"],
        ["刷新生效矩阵", "写回绑定版本并重新扫描个人级、项目级插件状态。"]
      ]
    : [
        ["下载 skill 包", "从市场目录获取最新 SKILL.md、README 和资源文件。"],
        ["覆盖安装目录", "保留原有范围、项目路径、启用状态和更新策略。"],
        ["刷新本地状态", "更新缓存记录、绑定版本和更新中心计数。"]
      ];

  return (
    <aside className="update-detail-card">
      <div className="detail-heading">
        <div>
          <p>{artifactLabel} update detail</p>
          <h2>{props.update.skillName}</h2>
          {props.update.skillName !== props.update.skillId ? <small>{props.update.skillId}</small> : null}
        </div>
        <Badge strong={!props.update.blockedReason}>{props.update.blockedReason ?? "可升级"}</Badge>
      </div>

      <div className="update-detail-grid">
        <div>
          <span>平台</span>
          <strong>{targetLabels[props.update.target] ?? props.update.target}</strong>
        </div>
        <div>
          <span>范围</span>
          <strong>{scopeText}</strong>
        </div>
        <div>
          <span>版本</span>
          <strong>
            {props.update.currentVersion} → {props.update.latestVersion}
          </strong>
        </div>
        <div>
          <span>更新策略</span>
          <strong>{props.update.updatePolicy}</strong>
        </div>
      </div>

      {props.update.blockedReason ? (
        <div className="update-blocker">
          <AlertCircle size={17} />
          <div>
            <strong>需要先处理</strong>
            <span>{props.update.blockedReason}</span>
          </div>
        </div>
      ) : null}

      <div className="update-pipeline">
        <h3>{artifactLabel} 自动升级动作</h3>
        {steps.map(([title, body], index) => (
          <div className="update-step" key={title}>
            <span>{index + 1}</span>
            <div>
              <strong>{title}</strong>
              <small>{body}</small>
            </div>
          </div>
        ))}
      </div>

      <div className="update-note">
        {isPlugin
          ? "插件升级不会要求草稿区保存 codex/claude 目录；平台专用目录在安装或升级时动态生成。"
          : "Skill 升级只替换包内容，绑定范围和启用状态继续沿用当前配置。"}
      </div>

      <div className="detail-action-bar">
        <button
          className="primary-action"
          disabled={!!props.update.blockedReason || props.busy}
          onClick={() => props.onUpgrade(props.update)}
        >
          <Rocket size={17} />
          升级到最新版本
        </button>
      </div>
    </aside>
  );
}

function SettingsView(props: {
  targetRoots: TargetRoot[];
  targetRootDrafts: Record<string, string>;
  onPickTargetRoot: (target: string) => void;
  onSaveTargetRoot: (target: string) => void;
}) {
  return (
    <section className="settings-grid">
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
                  <Save size={17} />
                  保存
                </button>
              </div>
            </div>
          ))}
          {props.targetRoots.length === 0 ? (
            <EmptyState
              title="没有目标平台目录"
              body="配置 Codex 或 Claude 的个人级 skill 目录后，安装流程会显示写入位置。"
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}
function DraftList(props: {
  drafts: AdminDraftSkill[];
  selectedDraftPath: string | null;
  onSelectDraft: (draft: AdminDraftSkill) => void;
}) {
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());
  const [collapsedSubcategories, setCollapsedSubcategories] = useState<Set<string>>(new Set());
  const [draftQuery, setDraftQuery] = useState("");
  const [draftStatusFilter, setDraftStatusFilter] = useState<DraftStatusFilter>("all");

  const grouped = new Map<string, { direct: AdminDraftSkill[]; secondary: Map<string, AdminDraftSkill[]> }>();
  const normalizedQuery = draftQuery.trim().toLocaleLowerCase();
  const statusCounts = new Map<DraftStatusKey, number>();
  for (const draft of props.drafts) {
    const statusKey = draftStatusClass(draft);
    statusCounts.set(statusKey, (statusCounts.get(statusKey) ?? 0) + 1);
  }
  const activeStatusFilters = draftStatusFilterOrder.filter(
    (key) => (statusCounts.get(key) ?? 0) > 0 || draftStatusFilter === key
  );
  for (const draft of props.drafts) {
    const category = draftPrimaryCategory(draft);
    const secondary = draftSecondaryCategory(draft);
    const categoryText = draftCategoryLabel(category).toLocaleLowerCase();
    const secondaryText = secondary
      ? `${secondary} ${draftCategoryLabel(secondary)} ${category}/${secondary}`.toLocaleLowerCase()
      : "";
    const matchesQuery =
      !normalizedQuery ||
      categoryText.includes(normalizedQuery) ||
      secondaryText.includes(normalizedQuery) ||
      draftSearchText(draft).includes(normalizedQuery);
    const matchesStatus = draftStatusFilter === "all" || draftStatusClass(draft) === draftStatusFilter;
    if (!matchesQuery || !matchesStatus) {
      continue;
    }

    if (!grouped.has(category)) {
      grouped.set(category, { direct: [], secondary: new Map() });
    }
    const group = grouped.get(category)!;
    if (!secondary) {
      group.direct.push(draft);
      continue;
    }
    if (!group.secondary.has(secondary)) {
      group.secondary.set(secondary, []);
    }
    group.secondary.get(secondary)!.push(draft);
  }

  const categories = Array.from(grouped.keys()).sort();
  const secondaryKey = (category: string, secondary: string) => `${category}/${secondary}`;
  const allSecondaryKeys = categories.flatMap((category) =>
    Array.from(grouped.get(category)!.secondary.keys()).map((secondary) => secondaryKey(category, secondary))
  );
  const visibleDraftCount = categories.reduce((sum, category) => {
    const group = grouped.get(category)!;
    return (
      sum +
      group.direct.length +
      Array.from(group.secondary.values()).reduce((subtotal, drafts) => subtotal + drafts.length, 0)
    );
  }, 0);

  useEffect(() => {
    if (normalizedQuery || draftStatusFilter !== "all") {
      setCollapsedCategories(new Set());
      setCollapsedSubcategories(new Set());
    }
  }, [normalizedQuery, draftStatusFilter]);

  const toggleCategory = (category: string) => {
    const newSet = new Set(collapsedCategories);
    if (newSet.has(category)) {
      newSet.delete(category);
    } else {
      newSet.add(category);
    }
    setCollapsedCategories(newSet);
  };

  const toggleSubcategory = (key: string) => {
    const newSet = new Set(collapsedSubcategories);
    if (newSet.has(key)) {
      newSet.delete(key);
    } else {
      newSet.add(key);
    }
    setCollapsedSubcategories(newSet);
  };

  const expandAllDraftGroups = () => {
    setCollapsedCategories(new Set());
    setCollapsedSubcategories(new Set());
  };

  const collapseAllDraftGroups = () => {
    setCollapsedCategories(new Set(categories));
    setCollapsedSubcategories(new Set(allSecondaryKeys));
  };
  const allDraftGroupsCollapsed =
    categories.length > 0 && categories.every((category) => collapsedCategories.has(category));
  const toggleAllDraftGroups = () => {
    if (allDraftGroupsCollapsed) {
      expandAllDraftGroups();
    } else {
      collapseAllDraftGroups();
    }
  };

  const renderDraftRow = (draft: AdminDraftSkill, nested = false) => (
    <button
      type="button"
      key={draft.gitlabSourcePath}
      className={`draft-row ${nested ? "nested" : ""} ${props.selectedDraftPath === draft.gitlabSourcePath ? "active" : ""} ${!draft.sourceAvailable ? "no-source" : ""}`}
      onClick={() => props.onSelectDraft(draft)}
      title={!draft.sourceAvailable ? "未关联 GitLab 源，无法预览" : undefined}
    >
      <span className="draft-icon">
        <FileText size={16} />
      </span>
      <span className="draft-row-main">
        <strong>{draftSkillLabel(draft)}</strong>
      </span>
      <span className={`badge badge-status ${draftStatusClass(draft)}`}>
        {!draft.sourceAvailable && <AlertCircle size={12} className="badge-inline-icon" />}
        {draft.status}
      </span>
    </button>
  );

  return (
    <>
      <div className="draft-list-tools">
        <div className="search-box draft-search-box">
          <Search size={15} />
          <input
            value={draftQuery}
            onChange={(event) => setDraftQuery(event.target.value)}
            placeholder="搜索分类、二级分类或 skill"
            aria-label="搜索草稿分类和 skill"
          />
          {draftQuery ? (
            <button
              type="button"
              className="draft-search-clear"
              onClick={() => setDraftQuery("")}
              title="清空搜索"
              aria-label="清空搜索"
            >
              <X size={14} />
            </button>
          ) : null}
        </div>
        <div className="draft-list-actions">
          <span className="draft-list-count">
            {normalizedQuery || draftStatusFilter !== "all"
              ? `${visibleDraftCount}/${props.drafts.length}`
              : `${props.drafts.length}`}
          </span>
          <button
            type="button"
            className="draft-fold-button"
            onClick={toggleAllDraftGroups}
            disabled={categories.length === 0}
            aria-label={allDraftGroupsCollapsed ? "展开全部分类" : "折叠全部分类"}
            title={allDraftGroupsCollapsed ? "展开全部分类" : "折叠全部分类"}
          >
            {allDraftGroupsCollapsed ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        </div>
        <div className="draft-status-filter" aria-label="按状态过滤草稿">
          <button
            type="button"
            className={`draft-status-filter-button ${draftStatusFilter === "all" ? "active" : ""}`}
            onClick={() => setDraftStatusFilter("all")}
            aria-pressed={draftStatusFilter === "all"}
          >
            <span>{draftStatusFilterLabels.all}</span>
            <small>{props.drafts.length}</small>
          </button>
          {activeStatusFilters.map((key) => (
            <button
              type="button"
              key={key}
              className={`draft-status-filter-button ${draftStatusFilter === key ? "active" : ""}`}
              onClick={() => setDraftStatusFilter(key)}
              aria-pressed={draftStatusFilter === key}
            >
              <span>{draftStatusFilterLabels[key]}</span>
              <small>{statusCounts.get(key) ?? 0}</small>
            </button>
          ))}
        </div>
      </div>
      {categories.length === 0 ? (
        <div className="empty-state compact draft-empty-results">
          <strong>没有匹配的草稿</strong>
          <span>换个状态、分类、路径或 skill 名称试试。</span>
        </div>
      ) : null}
      {categories.map((category) => {
        const isCollapsed = collapsedCategories.has(category);
        const group = grouped.get(category)!;
        const secondaryCategories = Array.from(group.secondary.keys()).sort();
        const count = group.direct.length + secondaryCategories.reduce((sum, key) => sum + group.secondary.get(key)!.length, 0);
        return (
          <div key={category} className="draft-category-group">
            <button
              type="button"
              className={`draft-category-header ${isCollapsed ? "collapsed" : ""}`}
              onClick={() => toggleCategory(category)}
              aria-expanded={!isCollapsed}
            >
              <FolderGit2 size={18} />
              <strong className="draft-category-title">{draftCategoryLabel(category)}</strong>
              <span className="badge">{count}</span>
              <ChevronRight size={16} className="category-toggle" />
            </button>
            <div className={`draft-items ${isCollapsed ? "collapsed" : ""}`}>
              {group.direct.length > 0 ? (
                <div className="draft-direct-items">
                  {sortDrafts(group.direct).map((draft) => renderDraftRow(draft, true))}
                </div>
              ) : null}
              {secondaryCategories.map((secondary) => {
                const key = secondaryKey(category, secondary);
                const isSecondaryCollapsed = collapsedSubcategories.has(key);
                return (
                  <div key={key} className="draft-subcategory-group">
                    <button
                      type="button"
                      className={`draft-subcategory-label ${isSecondaryCollapsed ? "collapsed" : ""}`}
                      onClick={() => toggleSubcategory(key)}
                      aria-expanded={!isSecondaryCollapsed}
                    >
                      <FolderGit2 size={16} className="draft-subcategory-icon" />
                      <span className="draft-subcategory-title">{draftCategoryLabel(secondary)}</span>
                      <small>{group.secondary.get(secondary)!.length}</small>
                      <ChevronRight size={14} className="subcategory-toggle" />
                    </button>
                    <div className={`draft-subcategory-items ${isSecondaryCollapsed ? "collapsed" : ""}`}>
                      {sortDrafts(group.secondary.get(secondary)!).map((draft) => renderDraftRow(draft, true))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </>
  );
}
function PluginDraftList(props: {
  drafts: AdminDraftPlugin[];
  selectedDraftPath: string | null;
  onSelectDraft: (draft: AdminDraftPlugin) => void;
}) {
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());
  const [collapsedSubcategories, setCollapsedSubcategories] = useState<Set<string>>(new Set());
  const [draftQuery, setDraftQuery] = useState("");
  const [draftStatusFilter, setDraftStatusFilter] = useState<DraftStatusFilter>("all");

  const grouped = new Map<string, { direct: AdminDraftPlugin[]; secondary: Map<string, AdminDraftPlugin[]> }>();
  const normalizedQuery = draftQuery.trim().toLocaleLowerCase();
  const statusCounts = new Map<DraftStatusKey, number>();
  for (const draft of props.drafts) {
    const statusKey = pluginDraftStatusClass(draft.status);
    statusCounts.set(statusKey, (statusCounts.get(statusKey) ?? 0) + 1);
  }
  const activeStatusFilters = draftStatusFilterOrder.filter(
    (key) => (statusCounts.get(key) ?? 0) > 0 || draftStatusFilter === key
  );

  for (const draft of props.drafts) {
    const category = pluginDraftPrimaryCategory(draft);
    const secondary = pluginDraftSecondaryCategory(draft);
    const categoryText = draftCategoryLabel(category).toLocaleLowerCase();
    const secondaryText = secondary
      ? `${secondary} ${draftCategoryLabel(secondary)} ${category}/${secondary}`.toLocaleLowerCase()
      : "";
    const matchesQuery =
      !normalizedQuery ||
      categoryText.includes(normalizedQuery) ||
      secondaryText.includes(normalizedQuery) ||
      pluginDraftSearchText(draft).includes(normalizedQuery);
    const matchesStatus = draftStatusFilter === "all" || pluginDraftStatusClass(draft.status) === draftStatusFilter;
    if (!matchesQuery || !matchesStatus) {
      continue;
    }

    if (!grouped.has(category)) {
      grouped.set(category, { direct: [], secondary: new Map() });
    }
    const group = grouped.get(category)!;
    if (!secondary) {
      group.direct.push(draft);
      continue;
    }
    if (!group.secondary.has(secondary)) {
      group.secondary.set(secondary, []);
    }
    group.secondary.get(secondary)!.push(draft);
  }

  const categories = Array.from(grouped.keys()).sort();
  const secondaryKey = (category: string, secondary: string) => `${category}/${secondary}`;
  const allSecondaryKeys = categories.flatMap((category) =>
    Array.from(grouped.get(category)!.secondary.keys()).map((secondary) => secondaryKey(category, secondary))
  );
  const visibleDraftCount = categories.reduce((sum, category) => {
    const group = grouped.get(category)!;
    return (
      sum +
      group.direct.length +
      Array.from(group.secondary.values()).reduce((subtotal, drafts) => subtotal + drafts.length, 0)
    );
  }, 0);

  useEffect(() => {
    if (normalizedQuery || draftStatusFilter !== "all") {
      setCollapsedCategories(new Set());
      setCollapsedSubcategories(new Set());
    }
  }, [normalizedQuery, draftStatusFilter]);

  const toggleCategory = (category: string) => {
    const newSet = new Set(collapsedCategories);
    if (newSet.has(category)) {
      newSet.delete(category);
    } else {
      newSet.add(category);
    }
    setCollapsedCategories(newSet);
  };

  const toggleSubcategory = (key: string) => {
    const newSet = new Set(collapsedSubcategories);
    if (newSet.has(key)) {
      newSet.delete(key);
    } else {
      newSet.add(key);
    }
    setCollapsedSubcategories(newSet);
  };

  const expandAllDraftGroups = () => {
    setCollapsedCategories(new Set());
    setCollapsedSubcategories(new Set());
  };

  const collapseAllDraftGroups = () => {
    setCollapsedCategories(new Set(categories));
    setCollapsedSubcategories(new Set(allSecondaryKeys));
  };
  const allDraftGroupsCollapsed =
    categories.length > 0 && categories.every((category) => collapsedCategories.has(category));
  const toggleAllDraftGroups = () => {
    if (allDraftGroupsCollapsed) {
      expandAllDraftGroups();
    } else {
      collapseAllDraftGroups();
    }
  };

  const renderDraftRow = (draft: AdminDraftPlugin, nested = false) => (
    <button
      type="button"
      className={`draft-row plugin-draft-row ${nested ? "nested" : ""} ${props.selectedDraftPath === draft.gitlabSourcePath ? "active" : ""} ${!draft.sourceAvailable ? "no-source" : ""}`}
      key={draft.gitlabSourcePath}
      onClick={() => props.onSelectDraft(draft)}
      title={!draft.sourceAvailable ? "缺少通用插件源文件；请确认 GitLab 已同步插件目录" : undefined}
    >
      <span className="draft-icon">
        <PackageCheck size={16} />
      </span>
      <span className="draft-row-main">
        <strong>{pluginDraftLabel(draft)}</strong>
        <small>
          {draft.namespace || draft.pluginId
            ? `${draft.namespace ?? "unknown"} / ${draft.pluginId ?? draft.draftSlug ?? "unknown"}`
            : draft.gitlabSourcePath}
        </small>
      </span>
      <span className={`badge badge-status ${pluginDraftStatusClass(draft.status)}`}>
        {!draft.sourceAvailable && <AlertCircle size={12} className="badge-inline-icon" />}
        {pluginDraftStatusLabel(draft.status)}
      </span>
    </button>
  );

  return (
    <>
      <div className="draft-list-tools">
        <div className="search-box draft-search-box">
          <Search size={15} />
          <input
            value={draftQuery}
            onChange={(event) => setDraftQuery(event.target.value)}
            placeholder="搜索分类、二级分类或 plugin"
            aria-label="搜索插件草稿分类和 plugin"
          />
          {draftQuery ? (
            <button
              type="button"
              className="draft-search-clear"
              onClick={() => setDraftQuery("")}
              title="清空搜索"
              aria-label="清空搜索"
            >
              <X size={14} />
            </button>
          ) : null}
        </div>
        <div className="draft-list-actions">
          <span className="draft-list-count">
            {normalizedQuery || draftStatusFilter !== "all"
              ? `${visibleDraftCount}/${props.drafts.length}`
              : `${props.drafts.length}`}
          </span>
          <button
            type="button"
            className="draft-fold-button"
            onClick={toggleAllDraftGroups}
            disabled={categories.length === 0}
            aria-label={allDraftGroupsCollapsed ? "展开全部分类" : "折叠全部分类"}
            title={allDraftGroupsCollapsed ? "展开全部分类" : "折叠全部分类"}
          >
            {allDraftGroupsCollapsed ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        </div>
        <div className="draft-status-filter" aria-label="按状态过滤插件草稿">
          <button
            type="button"
            className={`draft-status-filter-button ${draftStatusFilter === "all" ? "active" : ""}`}
            onClick={() => setDraftStatusFilter("all")}
            aria-pressed={draftStatusFilter === "all"}
          >
            <span>{draftStatusFilterLabels.all}</span>
            <small>{props.drafts.length}</small>
          </button>
          {activeStatusFilters.map((key) => (
            <button
              type="button"
              key={key}
              className={`draft-status-filter-button ${draftStatusFilter === key ? "active" : ""}`}
              onClick={() => setDraftStatusFilter(key)}
              aria-pressed={draftStatusFilter === key}
            >
              <span>{draftStatusFilterLabels[key]}</span>
              <small>{statusCounts.get(key) ?? 0}</small>
            </button>
          ))}
        </div>
      </div>
      {categories.length === 0 ? (
        <div className="empty-state compact draft-empty-results">
          <strong>没有匹配的插件草稿</strong>
          <span>换个状态、分类、路径或 plugin 名称试试。</span>
        </div>
      ) : null}
      {categories.map((category) => {
        const isCollapsed = collapsedCategories.has(category);
        const group = grouped.get(category)!;
        const secondaryCategories = Array.from(group.secondary.keys()).sort();
        const count = group.direct.length + secondaryCategories.reduce((sum, key) => sum + group.secondary.get(key)!.length, 0);
        return (
          <div key={category} className="draft-category-group">
            <button
              type="button"
              className={`draft-category-header ${isCollapsed ? "collapsed" : ""}`}
              onClick={() => toggleCategory(category)}
              aria-expanded={!isCollapsed}
            >
              <FolderGit2 size={18} />
              <strong className="draft-category-title">{draftCategoryLabel(category)}</strong>
              <span className="badge">{count}</span>
              <ChevronRight size={16} className="category-toggle" />
            </button>
            <div className={`draft-items ${isCollapsed ? "collapsed" : ""}`}>
              {group.direct.length > 0 ? (
                <div className="draft-direct-items">
                  {sortPluginDrafts(group.direct).map((draft) => renderDraftRow(draft, true))}
                </div>
              ) : null}
              {secondaryCategories.map((secondary) => {
                const key = secondaryKey(category, secondary);
                const isSecondaryCollapsed = collapsedSubcategories.has(key);
                return (
                  <div key={key} className="draft-subcategory-group">
                    <button
                      type="button"
                      className={`draft-subcategory-label ${isSecondaryCollapsed ? "collapsed" : ""}`}
                      onClick={() => toggleSubcategory(key)}
                      aria-expanded={!isSecondaryCollapsed}
                    >
                      <FolderGit2 size={16} className="draft-subcategory-icon" />
                      <span className="draft-subcategory-title">{draftCategoryLabel(secondary)}</span>
                      <small>{group.secondary.get(secondary)!.length}</small>
                      <ChevronRight size={14} className="subcategory-toggle" />
                    </button>
                    <div className={`draft-subcategory-items ${isSecondaryCollapsed ? "collapsed" : ""}`}>
                      {sortPluginDrafts(group.secondary.get(secondary)!).map((draft) => renderDraftRow(draft, true))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </>
  );
}

function AdminView(props: {
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

function AuditLogList(props: { logs: AdminAuditLog[] }) {
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

function adminAuditActionLabel(action: string) {
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
function formatAuditTime(value: string) {
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

function GovernanceDialogView(props: {
  dialog: GovernanceDialog;
  projectDraft: MarketProject;
  onProjectDraft: (value: MarketProject) => void;
  onSaveProject: () => void;
  categoryDraft: Category;
  onCategoryDraft: (value: Category) => void;
  onSaveCategory: () => void;
  onDeleteProject: (project: MarketProject) => void;
  onDeleteCategory: (category: Category) => void;
  busy: boolean;
  error: string | null;
  onClose: () => void;
}) {
  const updateProject = <K extends keyof MarketProject>(key: K, value: MarketProject[K]) =>
    props.onProjectDraft({ ...props.projectDraft, [key]: value });
  const updateCategory = <K extends keyof Category>(key: K, value: Category[K]) =>
    props.onCategoryDraft({ ...props.categoryDraft, [key]: value });
  const editingProject = props.dialog.kind === "project-edit";
  const editingCategory = props.dialog.kind === "category-edit";
  const projectForm = props.dialog.kind === "project-create" || editingProject;

  if (props.dialog.kind === "project-delete") {
    const project = props.dialog.project;
    return (
      <div className="modal-backdrop" role="presentation">
        <section className="admin-unlock-dialog governance-dialog" role="dialog" aria-modal="true">
          <div className="preview-head">
            <div>
              <p>Project</p>
              <h2>删除项目</h2>
              <span>{project.slug}</span>
            </div>
            <button className="icon-button" onClick={props.onClose} title="关闭">
              <X size={17} />
            </button>
          </div>
          <div className="admin-unlock-body">
            {props.error ? (
              <div className="dialog-error">
                <AlertCircle size={17} />
                <span>{props.error}</span>
              </div>
            ) : null}
            <div className="delete-summary">
              <strong>{project.name}</strong>
              <span>删除前必须先下架该项目下所有 skill。</span>
            </div>
            <div className="button-line">
              <button className="primary-soft danger" onClick={() => props.onDeleteProject(project)} disabled={props.busy}>
                <Trash2 size={17} />
                确认删除
              </button>
              <button className="primary-soft" onClick={props.onClose}>
                取消
              </button>
            </div>
          </div>
        </section>
      </div>
    );
  }

  if (props.dialog.kind === "category-delete") {
    const category = props.dialog.category;
    return (
      <div className="modal-backdrop" role="presentation">
        <section className="admin-unlock-dialog governance-dialog" role="dialog" aria-modal="true">
          <div className="preview-head">
            <div>
              <p>Public</p>
              <h2>删除公共分类</h2>
              <span>{category.id}</span>
            </div>
            <button className="icon-button" onClick={props.onClose} title="关闭">
              <X size={17} />
            </button>
          </div>
          <div className="admin-unlock-body">
            {props.error ? (
              <div className="dialog-error">
                <AlertCircle size={17} />
                <span>{props.error}</span>
              </div>
            ) : null}
            <div className="delete-summary">
              <strong>{category.name}</strong>
              <span>删除前必须先下架该分类下所有 skill。</span>
            </div>
            <div className="button-line">
              <button className="primary-soft danger" onClick={() => props.onDeleteCategory(category)} disabled={props.busy}>
                <Trash2 size={17} />
                确认删除
              </button>
              <button className="primary-soft" onClick={props.onClose}>
                取消
              </button>
            </div>
          </div>
        </section>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="admin-unlock-dialog governance-dialog" role="dialog" aria-modal="true">
        <div className="preview-head">
          <div>
            <p>{projectForm ? "Project" : "Public"}</p>
            <h2>
              {projectForm
                ? editingProject
                  ? "编辑项目"
                  : "新增项目"
                : editingCategory
                  ? "编辑公共分类"
                  : "新增公共分类"}
            </h2>
            <span>
              {projectForm
                ? editingProject
                  ? "更新项目名称和描述"
                  : "创建市场项目入口"
                : editingCategory
                  ? "更新公共市场分类"
                  : "创建公共市场分类"}
            </span>
          </div>
          <button className="icon-button" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
        </div>
        <div className="admin-unlock-body">
          {props.error ? (
            <div className="dialog-error">
              <AlertCircle size={17} />
              <span>{props.error}</span>
            </div>
          ) : null}
          {projectForm ? (
            <div className="meta-form single">
              <label className="text-field">
                <span>slug</span>
                <input
                  autoFocus
                  value={props.projectDraft.slug}
                  onChange={(event) => updateProject("slug", event.target.value)}
                  placeholder="project-a"
                  readOnly={editingProject}
                />
              </label>
              <label className="text-field">
                <span>名称</span>
                <input
                  value={props.projectDraft.name}
                  onChange={(event) => updateProject("name", event.target.value)}
                  placeholder="项目 A"
                />
              </label>
              <label className="text-field">
                <span>描述</span>
                <input
                  value={props.projectDraft.description}
                  onChange={(event) => updateProject("description", event.target.value)}
                  placeholder="项目市场说明"
                />
              </label>
              <label className="text-field">
                <span>排序</span>
                <input
                  type="number"
                  value={props.projectDraft.order}
                  onChange={(event) => updateProject("order", Number(event.target.value) || 10)}
                />
              </label>
              <button className="primary-action compact" onClick={props.onSaveProject} disabled={props.busy}>
                <Save size={17} />
                {editingProject ? "保存修改" : "保存项目"}
              </button>
            </div>
          ) : (
            <div className="meta-form single">
              <label className="text-field">
                <span>分类 slug</span>
                <input
                  autoFocus
                  value={props.categoryDraft.id}
                  onChange={(event) => updateCategory("id", event.target.value)}
                  placeholder="frontend"
                  readOnly={editingCategory}
                />
              </label>
              <label className="text-field">
                <span>名称</span>
                <input
                  value={props.categoryDraft.name}
                  onChange={(event) => updateCategory("name", event.target.value)}
                  placeholder="公共分类"
                />
              </label>
              <label className="text-field">
                <span>排序</span>
                <input
                  type="number"
                  value={props.categoryDraft.order}
                  onChange={(event) => updateCategory("order", Number(event.target.value) || 10)}
                />
              </label>
              <button className="primary-action compact" onClick={props.onSaveCategory} disabled={props.busy}>
                <Save size={17} />
                {editingCategory ? "保存修改" : "保存分类"}
              </button>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
function ArchiveScopeGroup(props: {
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
function skillKey(skill: MarketSkill) {
  return `${skill.sourceId ?? "local"}:${skill.namespace}/${skill.id}`;
}

function pluginKey(plugin: MarketPlugin) {
  return `${plugin.sourceId ?? "local"}:${plugin.namespace}/${plugin.id}`;
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
    case "admin":
      return "Publishing control";
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
    case "admin":
      return "管理发布";
  }
}
export default App;
