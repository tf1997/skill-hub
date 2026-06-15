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
  SlidersHorizontal,
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
import type {
  AdminDraftPreviewRequest,
  AdminAuditLog,
  AdminDraftSkill,
  AdminSession,
  AppBootstrap,
  CachedSkillPackage,
  Category,
  InstallSkillRequest,
  LocalSkill,
  MarketProject,
  MarketSkill,
  Project,
  PublishMeta,
  SkillBinding,
  TargetRoot,
  UpdateCandidate,
  UpdateCheckResult,
  DownloadUpdateResult,
  SkillPreview,
  SkillPreviewRequest
} from "./types";

type ViewKey = "market" | "installed" | "projects" | "updates" | "settings" | "admin";
type LevelChoice = "personal" | "project" | "download";
type MarketMode = "public" | "project";
type AdminTab = "projects" | "drafts" | "archive" | "audit";
type GovernanceTab = "project" | "general";
type GovernanceDialog =
  | { kind: "project-create" }
  | { kind: "project-edit"; project: MarketProject }
  | { kind: "project-delete"; project: MarketProject }
  | { kind: "category-create" }
  | { kind: "category-edit"; category: Category }
  | { kind: "category-delete"; category: Category };
type PreviewContext =
  | { kind: "skill"; request: SkillPreviewRequest }
  | { kind: "adminDraft"; request: AdminDraftPreviewRequest };
type AboutPayload = {
  name: string;
  description: string;
  authors: string;
  version: string;
  docs_url: string;
  team: string;
  feedback_email: string;
};
type AppUpdateDialogState = {
  open: boolean;
  phase: "checking" | "current" | "available" | "downloading" | "downloaded" | "error";
  result?: UpdateCheckResult | null;
  downloaded?: DownloadUpdateResult | null;
  error?: string | null;
  manual: boolean;
};
type CachedSkillItem = {
  key: string;
  package: CachedSkillPackage;
  marketSkill?: MarketSkill;
};

const emptyBootstrap: AppBootstrap = {
  sources: [],
  categories: [],
  skills: [],
  marketProjects: [],
  bindings: [],
  cachedPackages: [],
  localSkills: [],
  projects: [],
  targetRoots: [],
  updates: [],
  metadataSyncError: null
};

const canUseTauriEvents = typeof window !== "undefined" && "__TAURI_IPC__" in window;
const ADMIN_ENTRY_CLICK_THRESHOLD = 5;

function formatUpdatePrompt(result: UpdateCheckResult) {
  const version = result.latest_version || "";
  const notes = result.notes?.trim();
  if (!notes) {
    return `发现新版本 ${version}，是否现在下载？`;
  }
  return `发现新版本 ${version}，是否现在下载？\n\n更新说明：\n${notes}`;
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

const isPublishedDraft = (draft?: AdminDraftSkill | null) => draft?.status.trim() === "已发布";

function normalizeCategoryList(categories: Category[]) {
  const byId = new Map<string, Category>();
  for (const category of categories) {
    const id = category.id.trim();
    if (!id || id.startsWith("project:")) continue;
    const name = category.name.trim() || id;
    byId.set(id, {
      id,
      name,
      order: Number.isFinite(category.order) ? category.order : 0
    });
  }

  const normalized = [...byId.values()].sort((a, b) => {
    if (a.order !== b.order) return a.order - b.order;
    return a.id.localeCompare(b.id, "en");
  });

  let nextOrder = 10;
  return normalized.map((category) => {
    const order = category.order >= nextOrder ? category.order : nextOrder;
    nextOrder = order + 10;
    return { ...category, order };
  });
}

function nextCategoryOrder(categories: Category[]) {
  return categories.reduce((max, category) => Math.max(max, category.order), 0) + 10;
}

function App() {
  const [view, setView] = useState<ViewKey>("market");
  const [data, setData] = useState<AppBootstrap>(emptyBootstrap);
  const [marketMode, setMarketMode] = useState<MarketMode>("public");
  const [selectedMarketProjectSlug, setSelectedMarketProjectSlug] = useState("");
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
  const [previewContext, setPreviewContext] = useState<PreviewContext | null>(null);
  const [adminVisible, setAdminVisible] = useState(false);
  const [adminUnlockOpen, setAdminUnlockOpen] = useState(false);
  const [adminKey, setAdminKey] = useState("");
  const [adminSession, setAdminSession] = useState<AdminSession | null>(null);
  const [adminDrafts, setAdminDrafts] = useState<AdminDraftSkill[]>([]);
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
  const [selectedDraftPath, setSelectedDraftPath] = useState<string | null>(null);
  const [draftMeta, setDraftMeta] = useState<PublishMeta>(emptyPublishMeta());
  const [remoteProjectDraft, setRemoteProjectDraft] = useState<MarketProject>(emptyMarketProject());
  const [marketCategoryDraft, setMarketCategoryDraft] = useState<Category>(emptyMarketCategory());
  const [archiveReason, setArchiveReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("正在载入 Skill Hub...");
  const [error, setError] = useState<string | null>(null);
  const checkingAppUpdateRef = useRef(false);
  const adminEntryClickCountRef = useRef(0);

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
    void load();
  }, []);

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

  const selectedSkill = useMemo(() => {
    if (filteredSkills.length === 0) {
      return undefined;
    }
    return filteredSkills.find((skill) => skillKey(skill) === selectedSkillKey) ?? filteredSkills[0];
  }, [filteredSkills, selectedSkillKey]);

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

  async function handleUpgradeBinding(bindingId: string) {
    setBusy(true);
    setError(null);
    try {
      const result = await api.upgradeSkillBinding(bindingId);
      setData(result);
      setNotice("Skill 已升级到最新版本");
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

    // 校验元数据
    if (draftMeta.publishScope === "project") {
      if (!draftMeta.publishProjectSlug) {
        setError("发布元数据不完整：请选择项目");
        return;
      }
    } else if (draftMeta.publishScope === "public") {
      if (!draftMeta.publishCategorySlug) {
        setError("发布元数据不完整：请选择公共分类");
        return;
      }
    }

    if (!draftMeta.name || !draftMeta.name.trim()) {
      setError("发布元数据不完整：请填写名称");
      return;
    }

    if (!draftMeta.summary || !draftMeta.summary.trim()) {
      setError("发布元数据不完整：请填写摘要");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const saved = await api.savePublishMeta(adminKey, selectedDraftPath, normalizeMetaForSave(draftMeta));
      setDraftMeta(saved);
      const next = await api.publishDraft(adminKey, selectedDraftPath);
      setData(next);
      await refreshAdminDrafts();
      await refreshAdminAuditLogs(false);
      setNotice("草稿已发布到市场");
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

    // 校验元数据
    if (draftMeta.publishScope === "project") {
      if (!draftMeta.publishProjectSlug) {
        setError("发布元数据不完整：请选择项目");
        return;
      }
    } else if (draftMeta.publishScope === "public") {
      if (!draftMeta.publishCategorySlug) {
        setError("发布元数据不完整：请选择公共分类");
        return;
      }
    }

    if (!draftMeta.name || !draftMeta.name.trim()) {
      setError("发布元数据不完整：请填写名称");
      return;
    }

    if (!draftMeta.summary || !draftMeta.summary.trim()) {
      setError("发布元数据不完整：请填写摘要");
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
      setData(next);
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
      setData((current) => ({ ...current, marketProjects: projects }));
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
      setData(next);
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
      setData(next);
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
      const next = await api.archiveMarketSkill(adminKey, skill.namespace, skill.id, archiveReason);
      setData(next);
      setArchiveReason("");
      await refreshAdminDrafts();
      await refreshAdminAuditLogs(false);
      setNotice(`${skill.name} 已下架并回到草稿区`);
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
    { key: "settings" as const, label: "设置", icon: Settings, count: data.targetRoots.length },
    ...(adminVisible || adminSession
      ? [{ key: "admin" as const, label: "管理", icon: ShieldCheck, count: adminDrafts.length }]
      : [])
  ];

  return (
    <div className="app-shell" data-theme={theme}>
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

        {view === "updates" ? <UpdatesView updates={data.updates} onUpgrade={handleUpgradeBinding} busy={busy} /> : null}

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
            auditLogs={adminAuditLogs}
            onRefreshAuditLogs={() => void refreshAdminAuditLogs()}
            selectedDraftPath={selectedDraftPath}
            onRefreshDrafts={() => void refreshAdminDrafts()}
            onSelectDraft={selectDraft}
            meta={draftMeta}
            onMeta={setDraftMeta}
            onSaveMeta={() => void saveDraftMeta()}
            onPreview={() => void previewSelectedDraft()}
            onPublish={() => void publishSelectedDraft()}
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
            canManageProject={canManageProject}
            canManageSkill={canManageSkill}
            archiveReason={archiveReason}
            onArchiveReason={setArchiveReason}
            onArchiveSkill={(skill) => void archiveMarketSkill(skill)}
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
    </div>
  );
}

function ThemeSwitch(props: {
  theme: "light" | "dark";
  onTheme: (theme: "light" | "dark") => void;
}) {
  const isLight = props.theme === "light";
  const Icon = isLight ? Sun : Moon;
  const nextTheme = isLight ? "dark" : "light";

  return (
    <button
      className="sidebar-theme-switch"
      onClick={() => props.onTheme(nextTheme)}
      title={isLight ? "切换到深色" : "切换到白色"}
      aria-label={isLight ? "切换到深色" : "切换到白色"}
      type="button"
    >
      <Icon size={17} />
    </button>
  );
}

function MarketView(props: {
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
        <div className="market-mode-panel">
          <div className="segmented market-mode-switch" aria-label="市场范围">
            <button
              className={props.mode === "public" ? "active" : ""}
              onClick={() => props.onMode("public")}
            >
              公共
            </button>
            <button
              className={props.mode === "project" ? "active" : ""}
              onClick={() => props.onMode("project")}
            >
              项目
            </button>
          </div>
          <p>{props.mode === "project" ? "按项目查看专属 skill" : "按公共分类筛选市场 skill"}</p>
        </div>
        <div className="rail-title">
          <SlidersHorizontal size={16} />
          <span>{props.mode === "project" ? "项目" : "分类"}</span>
          <b>
            {props.mode === "project"
              ? props.marketProjects.length + 1
              : props.categories.length + 1}
          </b>
        </div>
        {props.mode === "public" ? (
          <>
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
          </>
        ) : (
          <>
            <button
              className={`category-button ${
                props.selectedMarketProjectSlug === "" ? "active" : ""
              }`}
              onClick={() => props.onSelectedMarketProjectSlug("")}
            >
              全部
            </button>
            {props.marketProjects.map((project) => (
              <button
                key={project.slug}
                className={`category-button ${
                  props.selectedMarketProjectSlug === project.slug ? "active" : ""
                }`}
                onClick={() => props.onSelectedMarketProjectSlug(project.slug)}
              >
                {project.name}
              </button>
            ))}
            {props.marketProjects.length === 0 ? (
              <div className="empty-state compact">暂无远程市场项目。</div>
            ) : null}
          </>
        )}
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
                  <span className="skill-row-icon" aria-hidden="true">
                    <Layers3 size={16} />
                  </span>
                  <div className="skill-row-main">
                    <strong>{skill.name}</strong>
                    <small>{skill.namespace}/{skill.id}</small>
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
  const [activeTab, setActiveTab] = useState<"bindings" | "cache" | "local">("bindings");
  const activeTitle =
    activeTab === "bindings" ? "生效矩阵" : activeTab === "cache" ? "本地缓存" : "本地已有 skill";
  const activeDescription =
    activeTab === "bindings"
      ? "同一 skill 在同一平台下只能选择个人级或项目级之一。"
      : activeTab === "cache"
        ? "已下载但不一定生效的 skill 包，删除缓存不会卸载已安装目录。"
        : "扫描个人级和项目级目录中包含 SKILL.md 的 skill。";
  const tabs = [
    { key: "bindings" as const, label: "生效矩阵", count: props.bindings.length },
    { key: "cache" as const, label: "本地缓存", count: props.cachedSkills.length },
    { key: "local" as const, label: "本地已有 skill", count: props.localSkills.length }
  ];
  const activeCount =
    activeTab === "bindings"
      ? props.bindings.length
      : activeTab === "cache"
        ? props.cachedSkills.length
        : props.localSkills.length;

  return (
    <section className="content-stack installed-view">
      <div className="section-toolbar">
        <div>
          <h2>{activeTitle}</h2>
          <p>{activeDescription}</p>
        </div>
        <Badge strong={activeCount > 0}>{activeCount} 项</Badge>
        {activeTab === "local" ? (
          <div className="toolbar-actions">
            <button className="primary-soft" onClick={props.onScan}>
              <ShieldCheck size={17} />
              扫描
            </button>
          </div>
        ) : null}
      </div>

      <div className="tab-strip" role="tablist" aria-label="本地 skill 视图">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            className={activeTab === tab.key ? "active" : ""}
            onClick={() => setActiveTab(tab.key)}
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
            <EmptyState
              title="还没有生效记录"
              body="从市场安装并启用 skill 后，这里会显示平台、范围、版本和启用状态。"
            />
          )}
        </div>
      ) : null}

      {activeTab === "cache" ? (
        <div className="cache-panel">
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
            <EmptyState
              title="本地缓存为空"
              body="安装或仅缓存市场 skill 后，可以在这里预览、复用或删除本地包。"
            />
          )}
        </div>
      ) : null}

      {activeTab === "local" ? (
        <div className="local-scan">
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
            <EmptyState
              title="等待扫描本地目录"
              body="点击右上角扫描，Skill Hub 会列出个人级和项目级目录中包含 SKILL.md 的 skill。"
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
  onUpgrade: (bindingId: string) => void;
  busy: boolean;
}) {
  return (
    <section className="content-stack updates-view">
      <div className="section-toolbar">
        <div>
          <h2>更新中心</h2>
          <p>有新版本可用时会在此显示，点击升级按钮即可更新到最新版本。</p>
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
        {props.updates.length > 0 ? (
          props.updates.map((update) => (
            <div className="table-row" key={update.bindingId}>
              <span>
                <strong>{update.skillName}</strong>
                {update.skillName !== update.skillId ? <small>{update.skillId}</small> : null}
              </span>
              <span>{targetLabels[update.target] ?? update.target}</span>
              <span>{update.level === "project" ? update.projectPath : "个人级"}</span>
              <span className="version-upgrade">
                {update.currentVersion} → {update.latestVersion}
              </span>
              <span>
                <Badge strong={!update.blockedReason}>
                  {update.blockedReason ?? "可更新"}
                </Badge>
              </span>
              <span className="row-actions">
                <button
                  className="icon-button"
                  disabled={!!update.blockedReason || props.busy}
                  onClick={() => props.onUpgrade(update.bindingId)}
                  title="升级到最新版本"
                >
                  <Rocket size={16} />
                </button>
              </span>
            </div>
          ))
        ) : (
          <EmptyState
            title="所有 skill 都是最新"
            body="当本地绑定落后于市场版本时，更新项会显示在这里。"
          />
        )}
      </div>
    </section>
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

function EmptyState(props: { title: string; body: string }) {
  return (
    <div className="empty-state compact">
      <strong>{props.title}</strong>
      <span>{props.body}</span>
    </div>
  );
}

function draftCategoryPath(draft: AdminDraftSkill) {
  const path = draft.gitlabCategoryPath?.map((item) => item.trim()).filter(Boolean) ?? [];
  if (path.length > 0) {
    return path;
  }
  return (draft.gitlabCategoryCode ?? "")
    .split("/")
    .map((item) => item.trim())
    .filter(Boolean);
}

function draftPrimaryCategory(draft: AdminDraftSkill) {
  return draftCategoryPath(draft)[0] ?? "未分类";
}

function draftSecondaryCategory(draft: AdminDraftSkill) {
  const path = draftCategoryPath(draft);
  return path.length > 1 ? path.slice(1).join("/") : null;
}

function draftCategoryLabel(category: string) {
  return category.includes("/") ? category.split("/").join(" / ") : category;
}

function draftSkillLabel(draft: AdminDraftSkill) {
  return draft.draftSlug ?? draft.gitlabSourcePath.split("/").pop() ?? draft.gitlabSourcePath;
}

function draftSearchText(draft: AdminDraftSkill) {
  return [
    draftSkillLabel(draft),
    draft.gitlabSourcePath,
    draftCategoryPath(draft).join("/"),
    draftCategoryPath(draft).map(draftCategoryLabel).join(" "),
    draft.status,
    draft.validationStatus ?? "",
    draft.version ?? "",
    draft.author ?? "",
    draft.publishMeta?.name ?? "",
    draft.publishMeta?.summary ?? "",
    draft.publishMeta?.tags.join(" ") ?? ""
  ]
    .join(" ")
    .toLocaleLowerCase();
}

function draftStatusClass(draft: AdminDraftSkill) {
  const status = draft.status.trim();
  const validation = (draft.validationStatus ?? "").trim().toLocaleLowerCase();
  const failedValidation = ["failed", "failure", "error", "invalid"].some((keyword) => validation.includes(keyword));
  const incompleteValidation = ["incomplete", "missing", "metadata"].some((keyword) => validation.includes(keyword));

  if (status === "已下架") {
    return "archived";
  }
  if (status.includes("校验失败") || failedValidation) {
    return "failed";
  }
  if (status.includes("元数据待补充") || status.includes("待补充") || incompleteValidation) {
    return "incomplete";
  }
  if (status.includes("版本回退风险") || status.includes("风险")) {
    return "risk";
  }
  if (!draft.sourceAvailable) {
    return "missing-source";
  }
  if (status === "已发布") {
    return "published";
  }
  if (status === "可升级") {
    return "upgradable";
  }
  return "draft";
}

function sortDrafts(drafts: AdminDraftSkill[]) {
  return [...drafts].sort((first, second) =>
    draftSkillLabel(first).localeCompare(draftSkillLabel(second), undefined, {
      numeric: true,
      sensitivity: "base"
    })
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

  const grouped = new Map<string, { direct: AdminDraftSkill[]; secondary: Map<string, AdminDraftSkill[]> }>();
  const normalizedQuery = draftQuery.trim().toLocaleLowerCase();
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
    if (!matchesQuery) {
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
    if (normalizedQuery) {
      setCollapsedCategories(new Set());
      setCollapsedSubcategories(new Set());
    }
  }, [normalizedQuery]);

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
            {normalizedQuery ? `${visibleDraftCount}/${props.drafts.length}` : `${props.drafts.length}`}
          </span>
          <button
            type="button"
            className="draft-fold-button"
            onClick={toggleAllDraftGroups}
            disabled={categories.length === 0}
          >
            {allDraftGroupsCollapsed ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            {allDraftGroupsCollapsed ? "展开" : "折叠"}
          </button>
        </div>
      </div>
      {categories.length === 0 ? (
        <div className="empty-state compact draft-empty-results">
          <strong>没有匹配的草稿</strong>
          <span>换个分类、路径或 skill 名称试试。</span>
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
  auditLogs: AdminAuditLog[];
  onRefreshAuditLogs: () => void;
  selectedDraftPath: string | null;
  onRefreshDrafts: () => void;
  onSelectDraft: (draft: AdminDraftSkill) => void;
  meta: PublishMeta;
  onMeta: (value: PublishMeta) => void;
  onSaveMeta: () => void;
  onPreview: () => void;
  onPublish: () => void;
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
  canManageProject: (slug: string) => boolean;
  canManageSkill: (skill: MarketSkill) => boolean;
  archiveReason: string;
  onArchiveReason: (value: string) => void;
  onArchiveSkill: (skill: MarketSkill) => void;
}) {
  const selectedDraft = props.drafts.find((draft) => draft.gitlabSourcePath === props.selectedDraftPath);
  const isSystem = props.session?.role === "system";
  const manageableProjects = props.projects.filter((project) => props.canManageProject(project.slug));
  const manageableSkills = props.skills.filter((skill) => props.canManageSkill(skill));
  const updateMeta = <K extends keyof PublishMeta>(key: K, value: PublishMeta[K]) =>
    props.onMeta({ ...props.meta, [key]: value });
  const projectOptions = manageableProjects;
  const activeGovernanceTab: GovernanceTab = isSystem ? props.governanceTab : "project";
  const selectedDraftPublished = isPublishedDraft(selectedDraft);
  const selectedDraftNeedsSource = Boolean(selectedDraft && !selectedDraft.sourceAvailable);
  const publishTargetMissing =
    props.meta.publishScope === "project" ? !props.meta.publishProjectSlug : !props.meta.publishCategorySlug;
  const metaIncomplete = Boolean(
    selectedDraft && (!props.meta.name.trim() || !props.meta.summary.trim() || publishTargetMissing)
  );
  const canPublishSelectedDraft = Boolean(
    selectedDraft && selectedDraft.sourceAvailable && !selectedDraftPublished && !metaIncomplete
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
          ? "请补齐名称、摘要和发布目标"
          : "发布到市场";

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
                          props.onProjectDraft(emptyMarketProject());
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
                              {project.slug} · {project.description || "无描述"}
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
                  <button className="icon-button" onClick={props.onRefreshDrafts} title="刷新草稿列表">
                    <RefreshCw size={16} />
                  </button>
                </div>
                <div className="draft-list">
                  {props.drafts.length === 0 ? (
                    <div className="empty-state compact">暂无草稿。请确认 GitLab 已同步到 MinIO 草稿前缀。</div>
                  ) : (
                    <DraftList
                      drafts={props.drafts}
                      selectedDraftPath={props.selectedDraftPath}
                      onSelectDraft={props.onSelectDraft}
                    />
                  )}
                </div>
              </section>

              <section className="admin-panel publish-editor">
                <div className="section-toolbar">
                  <div>
                    <h2>{selectedDraft ? draftSkillLabel(selectedDraft) : "发布元数据"}</h2>
                    <p>{selectedDraft?.version ? `version ${selectedDraft.version}` : "选择草稿后编辑"}</p>
                  </div>
                  <Badge>{selectedDraft?.author ?? "等待选择"}</Badge>
                </div>

                <div className="publish-scroll">
                  {selectedDraft ? (
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
                              ? "请补齐名称、摘要和发布目标后再发布。"
                              : "发布元数据已具备基础信息，可以预览或发布。"}
                          </span>
                        </div>
                      )}
                    </>
                  ) : (
                    <div className="publish-empty-state">
                      <FileText size={28} />
                      <strong>等待选择草稿</strong>
                      <span>左侧草稿载入后会显示发布元数据。</span>
                    </div>
                  )}
                </div>

                <div className="button-line publish-actions">
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
                </div>
              </section>
            </div>
          ) : null}

          {props.activeTab === "archive" ? (
            <div className="admin-panels archive">
              <section className="admin-panel">
                <div className="section-toolbar">
                  <div>
                    <h2>市场下架</h2>
                    <p>{isSystem ? "可下架公共和项目 skill" : "可下架所有项目 skill"}</p>
                  </div>
                </div>
                <label className="text-field">
                  <span>下架原因</span>
                  <input
                    value={props.archiveReason}
                    onChange={(event) => props.onArchiveReason(event.target.value)}
                    placeholder="例如：版本过期、迁移到新 skill、内容需修订"
                  />
                </label>
                <div className="archive-skill-list">
                  {props.skills.map((skill) => {
                    const allowed = props.canManageSkill(skill);
                    return (
                      <article className={`archive-skill-row ${allowed ? "" : "disabled"}`} key={`${skill.namespace}/${skill.id}`}>
                        <div>
                          <strong>{skill.name}</strong>
                          <span>{skill.namespace}/{skill.id} · {skill.latestVersion}</span>
                          <small>{skill.categories.join(", ") || "无分类"}</small>
                        </div>
                        <button
                          className="primary-soft danger"
                          onClick={() => props.onArchiveSkill(skill)}
                          disabled={!allowed}
                        >
                          <Archive size={16} />
                          下架
                        </button>
                      </article>
                    );
                  })}
                  {manageableSkills.length === 0 ? (
                    <div className="empty-state compact">当前角色没有可下架的市场 skill。</div>
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
    quickRepublishArchivedSkill: "快速重新上架",
    archiveMarketSkill: "下架 skill"
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

function AdminUnlockDialog(props: {
  adminKey: string;
  onAdminKey: (value: string) => void;
  busy: boolean;
  onUnlock: () => void;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="admin-unlock-dialog" role="dialog" aria-modal="true" aria-labelledby="admin-unlock-title">
        <div className="preview-head">
          <div>
            <p>Admin</p>
            <h2 id="admin-unlock-title">管理员验证</h2>
            <span>验证通过后才会打开管理发布页面</span>
          </div>
          <button className="icon-button" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
        </div>
        <div className="admin-unlock-body">
          <label className="text-field">
            <span>管理员密钥</span>
            <input
              autoFocus
              type="password"
              value={props.adminKey}
              onChange={(event) => props.onAdminKey(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  props.onUnlock();
                }
              }}
              placeholder="输入管理员密钥"
            />
          </label>
          <div className="button-line">
            <button className="primary-action compact" onClick={props.onUnlock} disabled={props.busy}>
              <KeyRound size={17} />
              验证并进入
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

function AppUpdateDialog(props: {
  state: AppUpdateDialogState;
  onCheck: () => void;
  onDownload: () => void;
  onRestart: () => void;
  onClose: () => void;
}) {
  const result = props.state.result;
  const downloaded = props.state.downloaded;
  const notes = result?.notes?.trim();
  const latestVersion = result?.latest_version || downloaded?.version || result?.current_version || "";
  const title =
    props.state.phase === "current"
      ? "已是最新版本"
      : props.state.phase === "downloaded"
        ? "更新已准备就绪"
        : props.state.phase === "error"
          ? "更新检查失败"
          : props.state.phase === "available"
            ? "发现新版本"
            : props.state.phase === "downloading"
              ? "正在下载更新"
              : "正在检查更新";
  const subtitle =
    props.state.phase === "current"
      ? `当前版本 ${result?.current_version ?? ""} 已可放心使用。`
      : props.state.phase === "downloaded"
        ? "下载完成后会尝试自动切换到新版本。"
        : props.state.phase === "error"
          ? props.state.error ?? "暂时无法完成更新检查。"
          : props.state.phase === "available"
            ? `新版本 ${latestVersion} 可用，建议在空闲时完成更新。`
            : props.state.phase === "downloading"
              ? "正在获取更新包，请保持网络连接。"
              : "正在连接更新源并校验可用版本。";

  return (
    <div className="modal-backdrop app-update-backdrop" role="presentation">
      <section className="app-update-dialog" role="dialog" aria-modal="true" aria-labelledby="app-update-title">
        <button className="icon-button app-update-close" onClick={props.onClose} title="关闭">
          <X size={17} />
        </button>
        <div className="app-update-hero">
          <div className={`app-update-orb ${props.state.phase}`}>
            {props.state.phase === "current" ? (
              <CheckCircle2 size={30} />
            ) : props.state.phase === "error" ? (
              <AlertCircle size={30} />
            ) : props.state.phase === "downloaded" ? (
              <Download size={30} />
            ) : (
              <RefreshCw size={30} />
            )}
          </div>
          <div className="app-update-title-block">
            <span>Skill Hub Application Update</span>
            <h2 id="app-update-title">{title}</h2>
            <p>{subtitle}</p>
          </div>
        </div>

        <div className="app-update-body">
          <div className="app-update-version-card">
            <div>
              <span>当前版本</span>
              <strong>{result?.current_version ?? "未知"}</strong>
            </div>
            <ChevronRight size={18} />
            <div>
              <span>目标版本</span>
              <strong>{latestVersion || "等待检查"}</strong>
            </div>
          </div>

          {props.state.phase === "downloading" ? (
            <div className="app-update-progress" aria-label="正在下载更新">
              <span />
            </div>
          ) : null}

          {notes ? (
            <div className="app-update-notes">
              <strong>更新说明</strong>
              <p>{notes}</p>
            </div>
          ) : null}

          {props.state.phase === "downloaded" ? (
            <div className="app-update-manual-tip">
              <AlertCircle size={16} />
              <span>如果应用没有自动重启，请关闭当前窗口后手动启动 Skill Hub。</span>
            </div>
          ) : null}

          <div className="app-update-actions">
            {props.state.phase === "available" ? (
              <button className="primary-soft app-update-primary" onClick={props.onDownload}>
                <Download size={17} />
                下载更新
              </button>
            ) : null}
            {props.state.phase === "downloaded" && downloaded?.ready_to_restart ? (
              <button className="primary-soft app-update-primary" onClick={props.onRestart}>
                <Power size={17} />
                立即重启
              </button>
            ) : null}
            {props.state.phase === "current" || props.state.phase === "error" ? (
              <button className="primary-soft" onClick={props.onCheck}>
                <RefreshCw size={17} />
                重新检查
              </button>
            ) : null}
            <button className="primary-soft" onClick={props.onClose}>
              {props.state.phase === "downloaded" ? "稍后处理" : "关闭"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}

function AboutDialog(props: { about: AboutPayload; onOpenDocs: () => void; onFeedback: () => void; onClose: () => void }) {
  return (
    <div className="modal-backdrop about-backdrop" role="presentation">
      <section className="about-dialog" role="dialog" aria-modal="true" aria-labelledby="about-title">
        <div className="about-hero">
          <button className="icon-button about-close" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
          <div className="about-brand-mark">
            <Layers3 size={32} />
          </div>
          <div className="about-title-block">
            <span>Skill Switchboard</span>
            <h2 id="about-title">Skill Hub</h2>
            <p>{props.about.description}</p>
          </div>
          <div className="about-version-pill">v{props.about.version}</div>
        </div>

        <div className="about-content">
          <div className="about-feature-grid">
            <div className="about-feature">
              <BookOpen size={18} />
              <div>
                <strong>Skill 市场</strong>
                <span>浏览团队沉淀的 skill，快速找到适合当前任务的能力。</span>
              </div>
            </div>
            <div className="about-feature">
              <PackageCheck size={18} />
              <div>
                <strong>本地安装</strong>
                <span>统一安装、启停、缓存和项目绑定，减少手动维护成本。</span>
              </div>
            </div>
            <div className="about-feature">
              <RefreshCw size={18} />
              <div>
                <strong>持续更新</strong>
                <span>检测应用版本和 skill 更新，保持工作区同步。</span>
              </div>
            </div>
            <div className="about-feature">
              <Rocket size={18} />
              <div>
                <strong>工作流加速</strong>
                <span>把常用能力带到 Codex / Claude 工作区，降低切换成本。</span>
              </div>
            </div>
          </div>

          <aside className="about-info-panel">
            <div className="about-info-row">
              <span>当前版本</span>
              <strong>v{props.about.version}</strong>
            </div>
            <div className="about-info-row">
              <span>支持目标</span>
              <strong>Codex / Claude</strong>
            </div>
            <div className="about-info-row">
              <span>开发团队</span>
              <strong>{props.about.team || props.about.authors || "Skill Hub Team"}</strong>
            </div>
            <div className="about-info-row">
              <span>问题反馈</span>
              <strong>{props.about.feedback_email}</strong>
            </div>
            <button className="primary-soft about-docs-button" onClick={props.onOpenDocs}>
              <BookOpen size={17} />
              在线文档
            </button>
            <button className="primary-soft about-feedback-button" onClick={props.onFeedback}>
              <ScrollText size={17} />
              问题反馈
            </button>
          </aside>

          <div className="about-actions">
            <button className="primary-soft" onClick={props.onClose}>
              关闭
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}

function PreviewPanel(props: { preview: SkillPreview; onSelectFile: (filePath: string) => void; onClose: () => void }) {
  const entries = useMemo(
    () =>
      props.preview.fileList?.length
        ? props.preview.fileList
        : props.preview.files.map((file) => ({
            path: file.path,
            language: file.language,
            previewable: true
          })),
    [props.preview]
  );
  const loadedFiles = useMemo(
    () => new Map(props.preview.files.map((file) => [file.path, file])),
    [props.preview.files]
  );
  const defaultPath = props.preview.files[0]?.path ?? entries[0]?.path ?? "";
  const [selectedPath, setSelectedPath] = useState(defaultPath);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());

  // 构建文件夹树：提取所有唯一的文件夹
  const folders = useMemo(() => {
    const folderSet = new Set<string>();
    entries.forEach((entry) => {
      const parts = entry.path.split("/");
      if (parts.length > 1) {
        // 添加所有父文件夹路径
        for (let i = 1; i < parts.length; i++) {
          folderSet.add(parts.slice(0, i).join("/"));
        }
      }
    });
    return Array.from(folderSet).sort();
  }, [entries]);

  // 获取文件夹的直接子文件夹
  function getChildFolders(parentPath: string): string[] {
    const prefix = parentPath ? parentPath + "/" : "";
    const depth = parentPath ? parentPath.split("/").length : 0;
    return folders.filter((f) => {
      if (parentPath && !f.startsWith(prefix)) return false;
      if (!parentPath && f.includes("/")) return false;
      const parts = f.split("/");
      return parts.length === depth + 1;
    });
  }

  // 获取文件夹的直接子文件
  function getChildFiles(parentPath: string): typeof entries {
    const prefix = parentPath ? parentPath + "/" : "";
    return entries.filter((entry) => {
      if (parentPath) {
        // 必须以父路径开头
        if (!entry.path.startsWith(prefix)) return false;
        // 去掉前缀后不能包含 /（即是直接子文件）
        const relativePath = entry.path.substring(prefix.length);
        return !relativePath.includes("/");
      } else {
        // 根目录：不包含 / 的文件
        return !entry.path.includes("/");
      }
    });
  }

  useEffect(() => {
    if (!selectedPath || !entries.some((entry) => entry.path === selectedPath)) {
      setSelectedPath(defaultPath);
    }
  }, [defaultPath, entries, selectedPath]);

  const selectedEntry = entries.find((entry) => entry.path === selectedPath) ?? entries[0];
  const selectedFile = selectedEntry ? loadedFiles.get(selectedEntry.path) : undefined;

  function selectEntry(path: string, previewable: boolean) {
    setSelectedPath(path);
    if (previewable && !loadedFiles.has(path)) {
      props.onSelectFile(path);
    }
  }

  function toggleFolder(folderPath: string) {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  }

  function renderTree(folderPath: string, depth: number): React.ReactNode {
    const childFolders = getChildFolders(folderPath);
    const childFiles = getChildFiles(folderPath);
    const isExpanded = expandedFolders.has(folderPath) || !folderPath; // 根目录默认展开
    const folderName = folderPath ? folderPath.split("/").pop() : "";

    return (
      <React.Fragment key={folderPath || "root"}>
        {folderPath && (
          <button
            className="preview-tree-folder"
            style={{ paddingLeft: `${12 + depth * 14}px` }}
            onClick={() => toggleFolder(folderPath)}
          >
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            {isExpanded ? <FolderOpen size={15} /> : <Folder size={15} />}
            <span>{folderName}</span>
            <small className="muted">{childFiles.length} 个</small>
          </button>
        )}
        {isExpanded && (
          <>
            {childFolders.map((childFolder) => renderTree(childFolder, depth + 1))}
            {childFiles.map((entry) => {
              const name = entry.path.split("/").pop() || entry.path;
              return (
                <button
                  key={entry.path}
                  className={`preview-tree-item ${selectedEntry?.path === entry.path ? "active" : ""}`}
                  style={{ paddingLeft: `${26 + (depth + 1) * 14}px` }}
                  onClick={() => selectEntry(entry.path, entry.previewable)}
                  title={entry.path}
                >
                  <FileText size={15} />
                  <span>{name}</span>
                  <Badge>{entry.previewable ? entry.language : "file"}</Badge>
                </button>
              );
            })}
          </>
        )}
      </React.Fragment>
    );
  }

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

      <div className="preview-browser">
        {entries.length === 0 ? (
          <div className="empty-state">没有可预览的文件。</div>
        ) : (
          <>
            <aside className="preview-tree" aria-label="预览文件列表">
              <div className="preview-tree-summary">
                <FolderOpen size={16} />
                <strong>{entries.length} 个文件</strong>
              </div>
              <div className="preview-tree-list">
                {renderTree("", 0)}
              </div>
            </aside>

            <article className="preview-file">
              {selectedEntry ? (
                <>
                  <header>
                    <strong>{selectedEntry.path}</strong>
                    <Badge>{selectedEntry.language}</Badge>
                  </header>
                  {selectedFile ? (
                    <>
                      <pre>{selectedFile.content}</pre>
                      {selectedFile.truncated ? <small>内容过长，已截断预览。</small> : null}
                    </>
                  ) : (
                    <div className="preview-file-empty">
                      {selectedEntry.previewable ? "正在准备预览内容。" : "该文件不是文本内容。"}
                    </div>
                  )}
                </>
              ) : (
                <div className="preview-file-empty">没有可预览的文本内容。</div>
              )}
            </article>
          </>
        )}
      </div>
    </aside>
  );
}

function StatusPill(props: { busy: boolean; text: string }) {
  return (
    <div className={`status-pill ${props.busy ? "busy" : ""}`}>
      <span className="status-dot" aria-hidden="true" />
      <span className="status-text">{props.text}</span>
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

function emptyPublishMeta(): PublishMeta {
  return {
    namespace: "community",
    skillId: "",
    name: "",
    summary: "",
    tags: [],
    targets: [],
    levels: ["personal", "project"],
    publishScope: "public",
    publishCategorySlug: null,
    publishProjectSlug: null,
    changelog: ""
  };
}

function emptyMarketProject(): MarketProject {
  return {
    slug: "",
    name: "",
    description: ""
  };
}

function emptyMarketCategory(): Category {
  return {
    id: "",
    name: "",
    order: 10
  };
}

function defaultMetaFromDraft(draft: AdminDraftSkill): PublishMeta {
  const slug = draftSkillLabel(draft);
  return {
    ...emptyPublishMeta(),
    skillId: slug,
    name: slug,
    summary: draft.author ? `由 ${draft.author} 维护的 skill` : ""
  };
}

function normalizeMetaForSave(meta: PublishMeta): PublishMeta {
  return {
    ...meta,
    publishCategorySlug: meta.publishScope === "project" ? null : meta.publishCategorySlug || null,
    publishProjectSlug: meta.publishScope === "project" ? meta.publishProjectSlug : null
  };
}

function splitCsv(value: string) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
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

function readError(err: unknown) {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return "操作失败";
}

export default App;
