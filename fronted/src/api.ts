import { invoke } from "@tauri-apps/api/tauri";
import type {
  AdminAuditLog,
  AdminDraftPreviewRequest,
  AdminSession,
  AdminDraftSkill,
  AppBootstrap,
  DeleteCachedSkillRequest,
  InstallSkillRequest,
  Category,
  LocalSkill,
  MarketProject,
  MarketSkill,
  Project,
  PublishMeta,
  SaveSourceRequest,
  SkillBinding,
  SkillPreview,
  SkillPreviewRequest,
  Source,
  TargetRoot,
  UpdateCandidate,
  UpdateCheckResult,
  DownloadUpdateResult
} from "./types";

const canUseTauri = typeof window !== "undefined" && "__TAURI_IPC__" in window;
const useBrowserMock = !canUseTauri;
const mockMinioEndpoint = "http://192.168.1.4:9000";
const mockMinioBucket = "skill-market";
const mockUpdatedAt = "2026-06-12T16:18:08Z";

const mockBindings: SkillBinding[] = [
  {
    id: "binding-live-codex-personal",
    packageId: "pkg-live-minio-010",
    namespace: "live",
    skillId: "minio-live-draft",
    skillName: "MinIO Live Draft",
    version: "0.1.0",
    target: "codex",
    level: "personal",
    installPath: "C:/Users/ctf19/.codex/skills/minio-live-draft",
    enabled: true,
    installMode: "copy",
    updatePolicy: "follow_latest",
    status: "enabled",
    createdAt: mockUpdatedAt,
    updatedAt: mockUpdatedAt
  },
  {
    id: "binding-backend-project",
    packageId: "pkg-backend-helper-120",
    namespace: "internal",
    skillId: "backend-release-helper",
    skillName: "Backend Release Helper",
    version: "1.2.0",
    target: "claude",
    level: "project",
    projectPath: "D:/code/skill-hub",
    installPath: "D:/code/skill-hub/.claude/skills/backend-release-helper",
    enabled: false,
    installMode: "copy",
    updatePolicy: "follow_latest",
    status: "installed",
    createdAt: mockUpdatedAt,
    updatedAt: mockUpdatedAt
  }
];

const mockBootstrap: AppBootstrap = {
  sources: [
    {
      id: "compiled-source",
      name: "本地 MinIO",
      endpoint: mockMinioEndpoint,
      bucket: mockMinioBucket,
      enabled: true
    }
  ],
  categories: [
    { id: "public", name: "公共", order: 10 },
    { id: "p", name: "p", order: 30 },
    { id: "yy", name: "yy", order: 40 },
    { id: "backend", name: "后端", order: 50 },
    { id: "w", name: "我", order: 60 }
  ],
  skills: [
    {
      namespace: "live",
      id: "minio-live-draft",
      name: "MinIO Live Draft",
      summary: "Published by live MinIO integration test.",
      latestVersion: "0.1.0",
      categories: ["project:live-project"],
      tags: ["minio", "live-test"],
      targets: ["codex"],
      levels: ["personal", "project"],
      manifestPath: "skills/live/minio-live-draft/manifest.json",
      updatedAt: mockUpdatedAt,
      installedBindings: [mockBindings[0]],
      cachedVersions: ["0.1.0"]
    },
    {
      namespace: "internal",
      id: "backend-release-helper",
      name: "Backend Release Helper",
      summary: "Prepare changelog, release notes, and validation tasks for backend skills.",
      latestVersion: "1.3.0",
      categories: ["backend", "project:ops"],
      tags: ["release", "validation", "ops"],
      targets: ["claude", "codex"],
      levels: ["project"],
      manifestPath: "skills/internal/backend-release-helper/manifest.json",
      updatedAt: mockUpdatedAt,
      installedBindings: [mockBindings[1]],
      cachedVersions: ["1.2.0"]
    }
  ],
  marketProjects: [
    {
      slug: "live-project",
      name: "Live Project",
      description: "Created by live MinIO integration test",
      order: 10
    },
    {
      slug: "alpha",
      name: "Alpha",
      description: "Internal alpha workspace",
      order: 20
    },
    {
      slug: "ops",
      name: "Ops",
      description: "Operations workflow skills",
      order: 30
    },
    {
      slug: "research",
      name: "Research",
      description: "Research and analysis skills",
      order: 40
    },
    {
      slug: "archive-demo",
      name: "Archive Demo",
      description: "Project example",
      order: 50
    }
  ],
  bindings: mockBindings,
  cachedPackages: [
    {
      sourceId: "compiled-source",
      namespace: "live",
      skillId: "minio-live-draft",
      skillName: "MinIO Live Draft",
      version: "0.1.0",
      packagePath: "C:/Users/ctf19/AppData/Local/SkillHub/cache/live/minio-live-draft/0.1.0/package.zip",
      cachedAt: mockUpdatedAt,
      bindingCount: 1
    },
    {
      sourceId: "compiled-source",
      namespace: "internal",
      skillId: "backend-release-helper",
      skillName: "Backend Release Helper",
      version: "1.2.0",
      packagePath: "C:/Users/ctf19/AppData/Local/SkillHub/cache/internal/backend-release-helper/1.2.0/package.zip",
      cachedAt: mockUpdatedAt,
      bindingCount: 1
    }
  ],
  localSkills: [
    {
      id: "local-codex-live",
      target: "codex",
      level: "personal",
      path: "C:/Users/ctf19/.codex/skills/minio-live-draft",
      detectedManifest: "MinIO Live Draft",
      managedBySkillhub: true,
      status: "installed",
      scannedAt: mockUpdatedAt
    },
    {
      id: "local-project-backend",
      target: "claude",
      level: "project",
      projectPath: "D:/code/skill-hub",
      path: "D:/code/skill-hub/.claude/skills/backend-release-helper",
      detectedManifest: "Backend Release Helper",
      managedBySkillhub: true,
      status: "missing",
      scannedAt: mockUpdatedAt
    }
  ],
  projects: [
    {
      id: "project-skill-hub",
      name: "skill-hub",
      path: "D:/code/skill-hub",
      createdAt: mockUpdatedAt,
      updatedAt: mockUpdatedAt
    },
    {
      id: "project-echo",
      name: "echo",
      path: "D:/code/echo",
      createdAt: mockUpdatedAt,
      updatedAt: mockUpdatedAt
    }
  ],
  targetRoots: [
    { target: "codex", personalPath: "C:/Users/ctf19/.codex/skills", updatedAt: mockUpdatedAt },
    { target: "claude", personalPath: "C:/Users/ctf19/.claude/skills", updatedAt: mockUpdatedAt }
  ],
  updates: [
    {
      bindingId: "binding-backend-project",
      namespace: "internal",
      skillId: "backend-release-helper",
      skillName: "Backend Release Helper",
      target: "claude",
      level: "project",
      projectPath: "D:/code/skill-hub",
      currentVersion: "1.2.0",
      latestVersion: "1.3.0",
      updatePolicy: "follow_latest",
      blockedReason: null
    }
  ]
};

const mockAdminSession: AdminSession = {
  enabled: true,
  endpoint: mockMinioEndpoint,
  bucket: mockMinioBucket,
  role: "system",
  projects: ["*"],
  macAddress: "C8:7F:54:5C:60:D8",
  name: "系统管理员"
};

const mockPreviewFiles = [
  {
    path: "SKILL.md",
    language: "markdown",
    content: "---\nname: Mock Draft\nversion: 0.1.0\nauthor: Skill Hub\n---\n\nMock draft preview.",
    truncated: false
  },
  {
    path: "references/schema.json",
    language: "json",
    content: "{\n  \"title\": \"Nested reference\"\n}",
    truncated: false
  },
  {
    path: "scripts/check.py",
    language: "python",
    content: "print('nested file preview')\n",
    truncated: false
  }
];

const mockPreviewFileList = [
  { path: "README.md", language: "markdown", previewable: true },
  { path: "SKILL.md", language: "markdown", previewable: true },
  { path: "assets/logo.png", language: "text", previewable: false },
  { path: "references/schema.json", language: "json", previewable: true },
  { path: "scripts/check.py", language: "python", previewable: true }
];

const mockAdminDrafts: AdminDraftSkill[] = [
  {
    gitlabSourcePath: "product/minio-live-draft",
    draftSlug: "minio-live-draft",
    gitlabCategoryCode: "product",
    gitlabCategoryPath: ["product"],
    sourceAvailable: true,
    version: "0.1.0",
    author: "Skill Hub Test",
    status: "待发布",
    validationStatus: "passed",
    updatedAt: mockUpdatedAt,
    publishMeta: {
      namespace: "live",
      skillId: "minio-live-draft",
      name: "MinIO Live Draft",
      summary: "Published by live MinIO integration test.",
      tags: ["backend", "minio", "live-test"],
      targets: ["codex"],
      levels: ["personal", "project"],
      publishScope: "project",
      publishCategorySlug: null,
      publishProjectSlug: "live-project",
      changelog: "Validate live MinIO draft publishing flow."
    }
  },
  {
    gitlabSourcePath: "ops/backend-release-helper",
    draftSlug: "backend-release-helper",
    gitlabCategoryCode: "ops",
    gitlabCategoryPath: ["ops"],
    sourceAvailable: false,
    version: "1.2.0",
    author: "Ops Team",
    status: "已下架",
    validationStatus: "archived",
    publishedVersion: "1.2.0",
    updatedAt: mockUpdatedAt,
    publishMeta: {
      namespace: "internal",
      skillId: "backend-release-helper",
      name: "Backend Release Helper",
      summary: "Prepare backend releases with repeatable checks and package notes.",
      tags: ["release", "validation", "ops"],
      targets: ["claude", "codex"],
      levels: ["project"],
      publishScope: "project",
      publishCategorySlug: null,
      publishProjectSlug: "live-project",
      changelog: "Restore archived package."
    }
  },
  {
    gitlabSourcePath: "general/product/prompt-audit-kit",
    draftSlug: "prompt-audit-kit",
    gitlabCategoryCode: "general/product",
    gitlabCategoryPath: ["general", "product"],
    sourceAvailable: true,
    version: "0.3.0",
    author: "Market Admin",
    status: "已发布",
    validationStatus: "passed",
    publishedVersion: "0.3.0",
    updatedAt: mockUpdatedAt,
    publishMeta: {
      namespace: "community",
      skillId: "prompt-audit-kit",
      name: "Prompt Audit Kit",
      summary: "Review prompt packs before they are shared in the market.",
      tags: ["review", "quality"],
      targets: ["codex"],
      levels: ["personal"],
      publishScope: "public",
      publishCategorySlug: "public",
      publishProjectSlug: null,
      changelog: "Current public version is already published."
    }
  }
];

const mockAdminAuditLogs: AdminAuditLog[] = [
  {
    objectPath: "admin/audit/2026/06/14/publishDraft-mock.json",
    action: "publishDraft",
    actor: "系统管理员",
    role: "system",
    macAddress: "C8:7F:54:5C:60:D8",
    ipAddress: null,
    target: "live/minio-live-draft@0.1.0",
    summary: "发布草稿: live/minio-live-draft@0.1.0",
    createdAt: "2026-06-14T10:20:30Z",
    payload: {}
  },
  {
    objectPath: "admin/audit/2026/06/14/saveMarketProject-mock.json",
    action: "saveMarketProject",
    actor: "系统管理员",
    role: "project",
    macAddress: "C8:7F:54:5C:60:D8",
    ipAddress: null,
    target: "live-project",
    summary: "保存项目: live-project",
    createdAt: "2026-06-14T09:52:12Z",
    payload: {}
  }
];

const browserMockApi = {
  bootstrap: async () => mockBootstrap,
  listMarketSkills: async () => mockBootstrap.skills,
  listSources: async () => mockBootstrap.sources,
  saveSource: async (_request: SaveSourceRequest) => mockBootstrap.sources[0],
  unlockAdminMode: async (_adminKey: string) => mockAdminSession,
  listAdminDrafts: async (_adminKey: string) => mockAdminDrafts,
  listAdminAuditLogs: async (_adminKey: string, _limit = 100) => mockAdminAuditLogs,
  previewAdminDraft: async (_request: AdminDraftPreviewRequest) => ({
    title: "Mock Draft",
    rootPath: "draft/gitlab/skills/mock",
    origin: "browser mock",
    files: mockPreviewFiles,
    fileList: mockPreviewFileList
  }),
  savePublishMeta: async (_adminKey: string, _gitlabSourcePath: string, meta: PublishMeta) => meta,
  saveMarketProjectRemote: async (_adminKey: string, project: MarketProject) => {
    const next = mockBootstrap.marketProjects.filter((item) => item.slug !== project.slug);
    return [project, ...next].sort((a, b) => a.order - b.order || a.slug.localeCompare(b.slug, "zh-Hans-CN"));
  },
  deleteMarketProjectRemote: async (_adminKey: string, _slug: string) => mockBootstrap,
  saveMarketCategoryRemote: async (_adminKey: string, category: Category) => {
    const next = mockBootstrap.categories.filter((item) => item.id !== category.id);
    return [category, ...next].sort((a, b) => a.order - b.order || a.name.localeCompare(b.name, "zh-Hans-CN"));
  },
  deleteMarketCategoryRemote: async (_adminKey: string, _categoryId: string) => mockBootstrap,
  archiveMarketSkill: async (_adminKey: string, _namespace: string, _skillId: string, _reason?: string) => mockBootstrap,
  publishDraft: async (_adminKey: string, _gitlabSourcePath: string) => mockBootstrap,
  quickRepublishArchivedSkill: async (_adminKey: string, _gitlabSourcePath: string) => mockBootstrap,
  listTargetRoots: async () => mockBootstrap.targetRoots,
  saveTargetRoot: async (target: string, personalPath: string) => ({ target, personalPath, updatedAt: new Date().toISOString() }),
  refreshCatalog: async () => mockBootstrap,
  installSkill: async (_request: InstallSkillRequest) =>
    ({
      id: "mock-binding",
      packageId: "mock-package",
      namespace: "live",
      skillId: "minio-live-draft",
      skillName: "MinIO Live Draft",
      version: "0.1.0",
      target: "codex",
      level: "personal",
      installPath: "C:/Users/ctf19/.codex/skills/minio-live-draft",
      enabled: true,
      installMode: "copy",
      updatePolicy: "follow_latest",
      status: "enabled",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString()
    }) as SkillBinding,
  deleteCachedSkill: async (_request: DeleteCachedSkillRequest) => undefined,
  setBindingEnabled: async (_bindingId: string, _enabled: boolean) => browserMockApi.installSkill({} as InstallSkillRequest),
  uninstallBinding: async (_bindingId: string) => [],
  upgradeSkillBinding: async (_bindingId: string) => mockBootstrap,
  listProjects: async () => mockBootstrap.projects,
  saveProject: async (name: string, path: string, id?: string) => ({
    id: id ?? "mock-project",
    name,
    path,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString()
  }),
  unbindProject: async (_projectId: string) => [],
  scanLocalSkills: async () => mockBootstrap.localSkills,
  previewSkill: async (_request: SkillPreviewRequest) => ({
    title: "MinIO Live Draft",
    rootPath: "skills/live/minio-live-draft",
    origin: "browser mock",
    files: mockPreviewFiles,
    fileList: mockPreviewFileList
  }),
  listUpdateCandidates: async () => mockBootstrap.updates,
  checkForUpdates: async () =>
    ({
      current_version: "0.1.0",
      latest_version: "0.1.0",
      available: false,
      downloadable: false,
      distribution: "browser",
      platform: "browser",
      arch: "browser",
      package: null,
      notes: null,
      message: "当前已是最新版本 0.1.0",
      manifest_url: `${mockMinioEndpoint}/${mockMinioBucket}/updates/stable/latest.json`
    }) as UpdateCheckResult,
  downloadUpdate: async () =>
    ({
      version: "0.1.0",
      target: "installer",
      path: "",
      ready_to_restart: false,
      message: "当前已是最新版本"
    }) as DownloadUpdateResult,
  restartAfterUpdate: async () => undefined
};

const tauriApi = {
  bootstrap: () => invoke<AppBootstrap>("bootstrap"),
  listMarketSkills: () => invoke<MarketSkill[]>("list_market_skills"),
  listSources: () => invoke<Source[]>("list_sources"),
  saveSource: (request: SaveSourceRequest) => invoke<Source>("save_source", { request }),
  unlockAdminMode: (adminKey: string) =>
    invoke<AdminSession>("unlock_admin_mode", { request: { adminKey } }),
  listAdminDrafts: (adminKey: string) =>
    invoke<AdminDraftSkill[]>("list_admin_drafts", { adminKey }),
  listAdminAuditLogs: (adminKey: string, limit = 100) =>
    invoke<AdminAuditLog[]>("list_admin_audit_logs", { request: { adminKey, limit } }),
  previewAdminDraft: (request: AdminDraftPreviewRequest) =>
    invoke<SkillPreview>("preview_admin_draft", { request }),
  savePublishMeta: (adminKey: string, gitlabSourcePath: string, meta: PublishMeta) =>
    invoke<PublishMeta>("save_publish_meta", {
      request: { adminKey, gitlabSourcePath, meta }
    }),
  saveMarketProjectRemote: (adminKey: string, project: MarketProject) =>
    invoke<MarketProject[]>("save_market_project_remote", {
      request: { adminKey, project }
    }),
  deleteMarketProjectRemote: (adminKey: string, slug: string) =>
    invoke<AppBootstrap>("delete_market_project_remote", {
      request: { adminKey, slug }
    }),
  saveMarketCategoryRemote: (adminKey: string, category: { id: string; name: string; order: number }) =>
    invoke<{ id: string; name: string; order: number }[]>("save_market_category_remote", {
      request: { adminKey, category }
    }),
  deleteMarketCategoryRemote: (adminKey: string, categoryId: string) =>
    invoke<AppBootstrap>("delete_market_category_remote", {
      request: { adminKey, categoryId }
    }),
  archiveMarketSkill: (adminKey: string, namespace: string, skillId: string, reason?: string) =>
    invoke<AppBootstrap>("archive_market_skill", {
      request: { adminKey, namespace, skillId, reason: reason ?? null }
    }),
  publishDraft: (adminKey: string, gitlabSourcePath: string) =>
    invoke<AppBootstrap>("publish_draft", { request: { adminKey, gitlabSourcePath } }),
  quickRepublishArchivedSkill: (adminKey: string, gitlabSourcePath: string) =>
    invoke<AppBootstrap>("quick_republish_archived_skill", { request: { adminKey, gitlabSourcePath } }),
  listTargetRoots: () => invoke<TargetRoot[]>("list_target_roots"),
  saveTargetRoot: (target: string, personalPath: string) =>
    invoke<TargetRoot>("save_target_root", {
      request: { target, personalPath }
    }),
  refreshCatalog: () => invoke<AppBootstrap>("refresh_catalog"),
  installSkill: (request: InstallSkillRequest) =>
    invoke<SkillBinding>("install_skill", { request }),
  deleteCachedSkill: (request: DeleteCachedSkillRequest) =>
    invoke<void>("delete_cached_skill", { request }),
  setBindingEnabled: (bindingId: string, enabled: boolean) =>
    invoke<SkillBinding>("set_binding_enabled", {
      request: { bindingId, enabled }
    }),
  uninstallBinding: (bindingId: string) =>
    invoke<SkillBinding[]>("uninstall_binding", { bindingId }),
  upgradeSkillBinding: (bindingId: string) =>
    invoke<AppBootstrap>("upgrade_skill_binding", {
      request: { bindingId }
    }),
  listProjects: () => invoke<Project[]>("list_projects"),
  saveProject: (name: string, path: string, id?: string) =>
    invoke<Project>("save_project", { request: { id, name, path } }),
  unbindProject: (projectId: string) =>
    invoke<Project[]>("unbind_project", { projectId }),
  scanLocalSkills: () => invoke<LocalSkill[]>("scan_local_skills"),
  previewSkill: (request: SkillPreviewRequest) =>
    invoke<SkillPreview>("preview_skill", { request }),
  listUpdateCandidates: () => invoke<UpdateCandidate[]>("list_update_candidates"),
  checkForUpdates: () => invoke<UpdateCheckResult>("check_for_updates_command"),
  downloadUpdate: () => invoke<DownloadUpdateResult>("download_update_command"),
  restartAfterUpdate: () => invoke<void>("restart_after_update_command")
};

export const api = useBrowserMock ? browserMockApi : tauriApi;
