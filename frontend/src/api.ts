import { invoke } from "@tauri-apps/api/tauri";
import type {
  AdminAuditLog,
  AdminDraftPlugin,
  AdminDraftPreviewRequest,
  AdminSession,
  AdminDraftSkill,
  AppBootstrap,
  MarketPlugin,
  PluginBinding,
  PluginPreviewRequest,
  CachedSkillPackage,
  DeleteCachedPluginRequest,
  DeleteCachedSkillRequest,
  DeleteLocalSkillRequest,
  SetLocalSkillEnabledRequest,
  ImportLocalSkillRequest,
  InstallCachedSkillRequest,
  InstallPluginRequest,
  InstallSkillRequest,
  Category,
  LocalPlugin,
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

const canUseTauri = typeof window !== "undefined" && typeof (window as Window & { __TAURI_IPC__?: unknown }).__TAURI_IPC__ === "function";
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
  plugins: [
    {
      namespace: "internal",
      id: "commit-workflow",
      name: "Commit Workflow",
      summary: "Team commit and PR workflow plugin.",
      latestVersion: "1.0.0",
      categories: ["backend"],
      tags: ["git", "pr", "workflow"],
      targets: ["codex", "claude"],
      scopes: ["user", "project"],
      components: ["skills", "agents", "hooks"],
      riskLevel: "medium",
      manifestPath: "plugins/internal/commit-workflow/manifest.json",
      updatedAt: mockUpdatedAt,
      installedBindings: [],
      cachedVersions: []
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
      bindingCount: 1,
      origin: "market",
      summary: null,
      tags: [],
      sourcePath: null
    },
    {
      sourceId: "compiled-source",
      namespace: "internal",
      skillId: "backend-release-helper",
      skillName: "Backend Release Helper",
      version: "1.2.0",
      packagePath: "C:/Users/ctf19/AppData/Local/SkillHub/cache/internal/backend-release-helper/1.2.0/package.zip",
      cachedAt: mockUpdatedAt,
      bindingCount: 1,
      origin: "market",
      summary: null,
      tags: [],
      sourcePath: null
    },
    {
      sourceId: "__local__",
      namespace: "local",
      skillId: "daily-note-helper",
      skillName: "Daily Note Helper",
      version: "0.0.0-local",
      packagePath: "C:/Users/ctf19/AppData/Local/SkillHub/packages/local.daily-note-helper/0.0.0-local",
      cachedAt: mockUpdatedAt,
      bindingCount: 0,
      origin: "local",
      summary: "User-created local skill snapshot.",
      tags: ["local"],
      sourcePath: "C:/Users/ctf19/.codex/skills/daily-note-helper"
    }
  ],
  pluginPackages: [
    {
      sourceId: "compiled-source",
      namespace: "internal",
      pluginId: "commit-workflow",
      pluginName: "Commit Workflow",
      version: "1.0.0",
      target: "codex",
      packagePath: "C:/Users/ctf19/AppData/Local/SkillHub/plugin-packages/internal/commit-workflow/1.0.0/codex",
      cachedAt: mockUpdatedAt,
      riskLevel: "medium",
      componentInventoryJson: "{}",
      bindingCount: 0
    }
  ],
  pluginBindings: [],
  localPlugins: [],
  localSkills: [
    {
      id: "local-codex-live",
      target: "codex",
      level: "personal",
      path: "C:/Users/ctf19/.codex/skills/minio-live-draft",
      detectedManifest: "MinIO Live Draft",
      managedBySkillhub: true,
      status: "installed",
      enabled: true,
      scannedAt: mockUpdatedAt,
      origin: "managed",
      skillId: "minio-live-draft",
      version: "0.1.0",
      summary: null,
      tags: [],
      matchedSourceId: "compiled-source",
      matchedNamespace: "live",
      matchedSkillId: "minio-live-draft",
      matchedVersion: "0.1.0",
      canImportToCache: false,
      canRestoreBinding: false
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
      enabled: false,
      scannedAt: mockUpdatedAt,
      origin: "managed",
      skillId: "backend-release-helper",
      version: "1.2.0",
      summary: null,
      tags: [],
      matchedSourceId: "compiled-source",
      matchedNamespace: "internal",
      matchedSkillId: "backend-release-helper",
      matchedVersion: "1.2.0",
      canImportToCache: false,
      canRestoreBinding: false
    },
    {
      id: "local-user-daily-note",
      target: "codex",
      level: "personal",
      path: "C:/Users/ctf19/.codex/skills/daily-note-helper",
      detectedManifest: "Daily Note Helper",
      managedBySkillhub: false,
      status: "local",
      enabled: true,
      scannedAt: mockUpdatedAt,
      origin: "local",
      skillId: "daily-note-helper",
      version: "0.0.0-local",
      summary: "User-created skill with minimal SKILL.md.",
      tags: [],
      matchedSourceId: null,
      matchedNamespace: null,
      matchedSkillId: null,
      matchedVersion: null,
      canImportToCache: true,
      canRestoreBinding: false
    },
    {
      id: "local-claude-daily-note",
      target: "claude",
      level: "personal",
      path: "C:/Users/ctf19/.claude/skills/daily-note-helper",
      detectedManifest: "Daily Note Helper",
      managedBySkillhub: false,
      status: "local",
      enabled: true,
      scannedAt: mockUpdatedAt,
      origin: "local",
      skillId: "daily-note-helper",
      version: "0.0.0-local",
      summary: "User-created skill with minimal SKILL.md.",
      tags: [],
      matchedSourceId: null,
      matchedNamespace: null,
      matchedSkillId: null,
      matchedVersion: null,
      canImportToCache: true,
      canRestoreBinding: false
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

const mockAdminPluginDrafts: AdminDraftPlugin[] = [
  {
    gitlabSourcePath: "backend/java/commit-workflow",
    draftSlug: "commit-workflow",
    gitlabCategoryPath: ["backend", "java"],
    sourceAvailable: true,
    namespace: "internal",
    pluginId: "commit-workflow",
    name: "Commit Workflow",
    summary: "Team commit and PR workflow plugin.",
    version: "1.0.0",
    targets: ["codex", "claude"],
    scopes: ["user", "project"],
    components: ["skills", "agents", "hooks"],
    riskLevel: "medium",
    status: "待发布",
    updatedAt: mockUpdatedAt,
    publishMeta: {
      namespace: "internal",
      skillId: "commit-workflow",
      name: "Commit Workflow",
      summary: "Team commit and PR workflow plugin.",
      tags: ["git", "pr", "workflow"],
      targets: ["codex", "claude"],
      levels: ["user", "project"],
      publishScope: "project",
      publishCategorySlug: null,
      publishProjectSlug: "live-project",
      changelog: "Initial plugin draft release."
    }
  }
];

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

function upsertMockCachedPackage(packageItem: CachedSkillPackage) {
  const index = mockBootstrap.cachedPackages.findIndex(
    (item) =>
      item.sourceId === packageItem.sourceId &&
      item.namespace === packageItem.namespace &&
      item.skillId === packageItem.skillId &&
      item.version === packageItem.version
  );
  if (index >= 0) {
    mockBootstrap.cachedPackages[index] = packageItem;
  } else {
    mockBootstrap.cachedPackages.unshift(packageItem);
  }
}

function markMockLocalSkillsCached(packageItem: CachedSkillPackage, sourcePath?: string | null) {
  const fingerprints = new Set([mockCachedLocalSkillFingerprint(packageItem)].filter(Boolean));
  const paths = new Set([sourcePath, packageItem.sourcePath].map(normalizeMockPath).filter(Boolean));
  mockBootstrap.localSkills = mockBootstrap.localSkills.map((skill) => {
    const pathMatched = paths.has(normalizeMockPath(skill.path));
    const fingerprint = mockLocalSkillFingerprint(skill);
    const fingerprintMatched = Boolean(fingerprint && fingerprints.has(fingerprint));
    if (!pathMatched && !fingerprintMatched) return skill;
    return {
      ...skill,
      status: "cached",
      origin: "local",
      canImportToCache: false
    };
  });
}

function syncMockLocalSkillsWithCache() {
  for (const packageItem of mockBootstrap.cachedPackages) {
    if (packageItem.origin === "local") {
      markMockLocalSkillsCached(packageItem, packageItem.sourcePath);
    }
  }
  return mockBootstrap.localSkills;
}

function mockBootstrapWithSyncedLocalSkills() {
  syncMockLocalSkillsWithCache();
  return mockBootstrap;
}

function mockCachedLocalSkillFingerprint(packageItem: CachedSkillPackage) {
  if (packageItem.origin !== "local") return null;
  return mockLocalFingerprint(packageItem.skillId, packageItem.version);
}

function mockLocalSkillFingerprint(skill: LocalSkill) {
  return mockLocalFingerprint(skill.skillId || mockLocalPathName(skill.path) || skill.detectedManifest || "", skill.version);
}

function mockLocalFingerprint(skillId: string, version?: string | null) {
  const normalizedSkillId = skillId
    .trim()
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .toLocaleLowerCase();
  if (!normalizedSkillId) return null;
  return `${normalizedSkillId}@${version?.trim() || "0.0.0-local"}`;
}

function normalizeMockPath(path?: string | null) {
  return path?.replace(/\\/g, "/").toLocaleLowerCase() ?? "";
}

function mockLocalPathName(path?: string | null) {
  const parts = path?.replace(/\\/g, "/").split("/").filter(Boolean) ?? [];
  return parts[parts.length - 1] ?? "";
}

function upsertMockBinding(binding: SkillBinding) {
  const index = mockBootstrap.bindings.findIndex((item) => item.id === binding.id);
  if (index >= 0) {
    mockBootstrap.bindings[index] = binding;
  } else {
    mockBootstrap.bindings.unshift(binding);
  }
}

const browserMockApi = {
  bootstrap: async () => mockBootstrapWithSyncedLocalSkills(),
  listMarketSkills: async () => mockBootstrap.skills,
  listMarketPlugins: async () => mockBootstrap.plugins,
  listSources: async () => mockBootstrap.sources,
  saveSource: async (_request: SaveSourceRequest) => mockBootstrap.sources[0],
  unlockAdminMode: async (_adminKey: string) => mockAdminSession,
  listAdminDrafts: async (_adminKey: string) => mockAdminDrafts,
  listAdminPluginDrafts: async (_adminKey: string) => mockAdminPluginDrafts,
  listAdminAuditLogs: async (_adminKey: string, _limit = 100) => mockAdminAuditLogs,
  previewAdminDraft: async (_request: AdminDraftPreviewRequest) => ({
    title: "Mock Draft",
    rootPath: "draft/gitlab/skills/mock",
    origin: "browser mock",
    files: mockPreviewFiles,
    fileList: mockPreviewFileList
  }),
  previewAdminPluginDraft: async (_request: AdminDraftPreviewRequest) => ({
    title: "Mock Plugin Draft",
    rootPath: "draft/gitlab/plugins/mock/source",
    origin: "browser mock",
    files: mockPreviewFiles,
    fileList: mockPreviewFileList
  }),
  savePublishMeta: async (_adminKey: string, _gitlabSourcePath: string, meta: PublishMeta, _artifactKind: "skill" | "plugin" = "skill") => meta,
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
  archiveMarketPlugin: async (_adminKey: string, _namespace: string, _pluginId: string, _reason?: string) => mockBootstrap,
  publishDraft: async (_adminKey: string, _gitlabSourcePath: string) => mockBootstrap,
  publishPluginDraft: async (_adminKey: string, _gitlabSourcePath: string) => mockBootstrap,
  quickRepublishArchivedSkill: async (_adminKey: string, _gitlabSourcePath: string) => mockBootstrap,
  listTargetRoots: async () => mockBootstrap.targetRoots,
  saveTargetRoot: async (target: string, personalPath: string) => ({ target, personalPath, updatedAt: new Date().toISOString() }),
  refreshCatalog: async () => mockBootstrapWithSyncedLocalSkills(),
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
  importLocalSkillToCache: async (_request: ImportLocalSkillRequest) => {
    const packageItem = mockBootstrap.cachedPackages[2];
    upsertMockCachedPackage(packageItem);
    markMockLocalSkillsCached(packageItem, _request.path);
    return packageItem;
  },
  installCachedSkill: async (_request: InstallCachedSkillRequest) => {
    const targetRoot =
      _request.level === "project"
        ? `${_request.projectPath}/${_request.target === "codex" ? ".codex" : ".claude"}/skills`
        : _request.target === "codex"
          ? "C:/Users/ctf19/.codex/skills"
          : "C:/Users/ctf19/.claude/skills";
    const binding = {
      id: `mock-local-binding-${_request.target}-${_request.level}`,
      packageId: "mock-local-package",
      sourceId: _request.sourceId,
      namespace: _request.namespace,
      skillId: _request.skillId,
      skillName: "Daily Note Helper",
      version: _request.version,
      target: _request.target,
      level: _request.level,
      projectPath: _request.projectPath,
      installPath: `${targetRoot}/${_request.skillId}`,
      enabled: true,
      installMode: "copy",
      updatePolicy: "pinned",
      status: "installed",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString()
    } as SkillBinding;
    upsertMockBinding(binding);
    return binding;
  },
  installPlugin: async (_request: InstallPluginRequest) => {
    const binding = {
      id: `plugin-binding-${Date.now()}`,
      packageId: "plugin-package-mock",
      sourceId: "compiled-source",
      namespace: "internal",
      pluginId: "commit-workflow",
      pluginName: "Commit Workflow",
      version: "1.0.0",
      target: _request.target,
      scope: _request.scope,
      projectPath: _request.projectPath ?? null,
      marketplaceId: "marketplace-mock",
      marketplaceName: "skillhub",
      platformRef: "commit-workflow@skillhub",
      enabled: _request.enable,
      installMode: _request.installMode ?? "marketplace",
      updatePolicy: _request.updatePolicy ?? "follow_latest",
      status: _request.enable ? "installed" : "cached",
      createdAt: mockUpdatedAt,
      updatedAt: mockUpdatedAt
    };
    mockBootstrap.pluginBindings.unshift(binding);
    return binding;
  },
  deleteCachedSkill: async (_request: DeleteCachedSkillRequest) => undefined,
  deleteCachedPlugin: async (request: DeleteCachedPluginRequest) => {
    mockBootstrap.pluginPackages = mockBootstrap.pluginPackages.filter(
      (item) =>
        !(
          item.sourceId === (request.sourceId ?? null) &&
          item.namespace === request.namespace &&
          item.pluginId === request.pluginId &&
          item.version === request.version &&
          item.target === request.target
        )
    );
    return undefined;
  },
  deleteLocalSkill: async (_request: DeleteLocalSkillRequest) => {
    mockBootstrap.localSkills = mockBootstrap.localSkills.filter((skill) => skill.id !== _request.id);
    return syncMockLocalSkillsWithCache();
  },
  setLocalSkillEnabled: async (_request: SetLocalSkillEnabledRequest) => {
    mockBootstrap.localSkills = mockBootstrap.localSkills.map((skill) => {
      if (skill.id !== _request.id || skill.managedBySkillhub) return skill;
      const nextPath = _request.enabled
        ? skill.path.replace(/([\\/])\.skill-hub-disabled([\\/])/, "$1")
        : skill.path.replace(/([\\/])skills([\\/])([^\\/]+)$/, "$1skills$1.skill-hub-disabled$2$3");
      return {
        ...skill,
        enabled: _request.enabled,
        status: _request.enabled ? "local" : "disabled",
        path: nextPath,
        scannedAt: new Date().toISOString()
      };
    });
    return syncMockLocalSkillsWithCache();
  },
  setBindingEnabled: async (_bindingId: string, _enabled: boolean) => browserMockApi.installSkill({} as InstallSkillRequest),
  uninstallBinding: async (_bindingId: string) => [],
  upgradeSkillBinding: async (_bindingId: string) => mockBootstrap,
  upgradePluginBinding: async (_bindingId: string) => mockBootstrap,
  listProjects: async () => mockBootstrap.projects,
  saveProject: async (name: string, path: string, id?: string) => ({
    id: id ?? "mock-project",
    name,
    path,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString()
  }),
  unbindProject: async (_projectId: string) => [],
  scanLocalSkills: async () => syncMockLocalSkillsWithCache(),
  scanLocalPlugins: async () => mockBootstrap.localPlugins,
  setPluginBindingEnabled: async (bindingId: string, enabled: boolean) => {
    mockBootstrap.pluginBindings = mockBootstrap.pluginBindings.map((binding) =>
      binding.id === bindingId ? { ...binding, enabled } : binding
    );
    return mockBootstrap.pluginBindings.find((binding) => binding.id === bindingId) ?? mockBootstrap.pluginBindings[0];
  },
  uninstallPlugin: async (bindingId: string) => {
    mockBootstrap.pluginBindings = mockBootstrap.pluginBindings.filter((binding) => binding.id !== bindingId);
    return mockBootstrap.pluginBindings;
  },
  previewSkill: async (_request: SkillPreviewRequest) => ({
    title: "MinIO Live Draft",
    rootPath: "skills/live/minio-live-draft",
    origin: "browser mock",
    files: mockPreviewFiles,
    fileList: mockPreviewFileList
  }),
  previewPlugin: async (_request: PluginPreviewRequest) => ({
    title: "Commit Workflow",
    rootPath: "plugins/internal/commit-workflow/1.0.0/codex",
    origin: "browser mock / codex",
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
  listMarketPlugins: () => invoke<MarketPlugin[]>("list_market_plugins"),
  listSources: () => invoke<Source[]>("list_sources"),
  saveSource: (request: SaveSourceRequest) => invoke<Source>("save_source", { request }),
  unlockAdminMode: (adminKey: string) =>
    invoke<AdminSession>("unlock_admin_mode", { request: { adminKey } }),
  listAdminDrafts: (adminKey: string) =>
    invoke<AdminDraftSkill[]>("list_admin_drafts", { adminKey }),
  listAdminPluginDrafts: (adminKey: string) =>
    invoke<AdminDraftPlugin[]>("list_admin_plugin_drafts", { adminKey }),
  listAdminAuditLogs: (adminKey: string, limit = 100) =>
    invoke<AdminAuditLog[]>("list_admin_audit_logs", { request: { adminKey, limit } }),
  previewAdminDraft: (request: AdminDraftPreviewRequest) =>
    invoke<SkillPreview>("preview_admin_draft", { request }),
  previewAdminPluginDraft: (request: AdminDraftPreviewRequest) =>
    invoke<SkillPreview>("preview_admin_plugin_draft", { request }),
  savePublishMeta: (adminKey: string, gitlabSourcePath: string, meta: PublishMeta, artifactKind: "skill" | "plugin" = "skill") =>
    invoke<PublishMeta>("save_publish_meta", {
      request: { adminKey, gitlabSourcePath, meta, artifactKind }
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
  archiveMarketPlugin: (adminKey: string, namespace: string, pluginId: string, reason?: string) =>
    invoke<AppBootstrap>("archive_market_plugin", {
      request: { adminKey, namespace, pluginId, reason: reason ?? null }
    }),
  publishDraft: (adminKey: string, gitlabSourcePath: string) =>
    invoke<AppBootstrap>("publish_draft", { request: { adminKey, gitlabSourcePath } }),
  publishPluginDraft: (adminKey: string, gitlabSourcePath: string) =>
    invoke<AppBootstrap>("publish_plugin_draft", { request: { adminKey, gitlabSourcePath } }),
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
  installPlugin: (request: InstallPluginRequest) =>
    invoke<PluginBinding>("install_plugin", { request }),
  importLocalSkillToCache: (request: ImportLocalSkillRequest) =>
    invoke<CachedSkillPackage>("import_local_skill_to_cache", { request }),
  installCachedSkill: (request: InstallCachedSkillRequest) =>
    invoke<SkillBinding>("install_cached_skill", { request }),
  deleteCachedSkill: (request: DeleteCachedSkillRequest) =>
    invoke<void>("delete_cached_skill", { request }),
  deleteCachedPlugin: (request: DeleteCachedPluginRequest) =>
    invoke<void>("delete_cached_plugin", { request }),
  deleteLocalSkill: (request: DeleteLocalSkillRequest) =>
    invoke<LocalSkill[]>("delete_local_skill", { request }),
  setLocalSkillEnabled: (request: SetLocalSkillEnabledRequest) =>
    invoke<LocalSkill[]>("set_local_skill_enabled", { request }),
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
  upgradePluginBinding: (bindingId: string) =>
    invoke<AppBootstrap>("upgrade_plugin_binding", {
      request: { bindingId }
    }),
  listProjects: () => invoke<Project[]>("list_projects"),
  saveProject: (name: string, path: string, id?: string) =>
    invoke<Project>("save_project", { request: { id, name, path } }),
  unbindProject: (projectId: string) =>
    invoke<Project[]>("unbind_project", { projectId }),
  scanLocalSkills: () => invoke<LocalSkill[]>("scan_local_skills"),
  scanLocalPlugins: () => invoke<LocalPlugin[]>("scan_local_plugins"),
  setPluginBindingEnabled: (bindingId: string, enabled: boolean) =>
    invoke<PluginBinding>("set_plugin_binding_enabled", {
      request: { bindingId, enabled }
    }),
  uninstallPlugin: (bindingId: string, deleteCachedPackage = false) =>
    invoke<PluginBinding[]>("uninstall_plugin", {
      request: { bindingId, deleteCachedPackage }
    }),
  previewSkill: (request: SkillPreviewRequest) =>
    invoke<SkillPreview>("preview_skill", { request }),
  previewPlugin: (request: PluginPreviewRequest) =>
    invoke<SkillPreview>("preview_plugin", { request }),
  listUpdateCandidates: () => invoke<UpdateCandidate[]>("list_update_candidates"),
  checkForUpdates: () => invoke<UpdateCheckResult>("check_for_updates_command"),
  downloadUpdate: () => invoke<DownloadUpdateResult>("download_update_command"),
  restartAfterUpdate: () => invoke<void>("restart_after_update_command")
};

export const api = useBrowserMock ? browserMockApi : tauriApi;
