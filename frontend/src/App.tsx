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
import { AdminView } from "./features/admin/AdminView";
import { InstalledView } from "./features/installed/InstalledView";
import { MarketView } from "./features/market/MarketView";
import { ProjectsView } from "./features/projects/ProjectsView";
import { SettingsView } from "./features/settings/SettingsView";
import { UpdatesView } from "./features/updates/UpdatesView";
import {
  isProjectMarketPlugin,
  isProjectMarketSkill,
  levelLabels,
  pluginKey,
  skillKey,
  targetLabels,
  viewHeadline,
  viewTitle
} from "./app/viewModel";
import type {
  AdminTab,
  GovernanceDialog,
  GovernanceTab,
  InstalledArtifactKind,
  InstalledTab,
  MarketArtifactKind,
  MarketMode,
  UpdateArtifactKind,
  UpdateStatusFilter,
  ViewKey
} from "./app/viewModel";
import { BindingDots } from "./components/common/BindingDots";
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
type PreviewContext =
  | { kind: "skill"; request: SkillPreviewRequest }
  | { kind: "plugin"; request: PluginPreviewRequest }
  | { kind: "adminDraft"; request: AdminDraftPreviewRequest }
  | { kind: "adminPluginDraft"; request: AdminDraftPreviewRequest };type ContextMenuState = {
  open: boolean;
  x: number;
  y: number;
};const emptyBootstrap: AppBootstrap = {
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
export default App;
