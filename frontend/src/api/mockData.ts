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
} from "../types";

export const mockMinioEndpoint = "http://192.168.1.4:9000";
export const mockMinioBucket = "skill-market";
export const mockUpdatedAt = "2026-06-12T16:18:08Z";

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

export const mockBootstrap: AppBootstrap = {
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

export const mockAdminSession: AdminSession = {
  enabled: true,
  endpoint: mockMinioEndpoint,
  bucket: mockMinioBucket,
  role: "system",
  projects: ["*"],
  macAddress: "C8:7F:54:5C:60:D8",
  name: "系统管理员"
};

export const mockAdminPluginDrafts: AdminDraftPlugin[] = [
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

export const mockPreviewFiles = [
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

export const mockPreviewFileList = [
  { path: "README.md", language: "markdown", previewable: true },
  { path: "SKILL.md", language: "markdown", previewable: true },
  { path: "assets/logo.png", language: "text", previewable: false },
  { path: "references/schema.json", language: "json", previewable: true },
  { path: "scripts/check.py", language: "python", previewable: true }
];

export const mockAdminDrafts: AdminDraftSkill[] = [
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

export const mockAdminAuditLogs: AdminAuditLog[] = [
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

export function upsertMockCachedPackage(packageItem: CachedSkillPackage) {
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

export function markMockLocalSkillsCached(packageItem: CachedSkillPackage, sourcePath?: string | null) {
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

export function syncMockLocalSkillsWithCache() {
  for (const packageItem of mockBootstrap.cachedPackages) {
    if (packageItem.origin === "local") {
      markMockLocalSkillsCached(packageItem, packageItem.sourcePath);
    }
  }
  return mockBootstrap.localSkills;
}

export function mockBootstrapWithSyncedLocalSkills() {
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

export function upsertMockBinding(binding: SkillBinding) {
  const index = mockBootstrap.bindings.findIndex((item) => item.id === binding.id);
  if (index >= 0) {
    mockBootstrap.bindings[index] = binding;
  } else {
    mockBootstrap.bindings.unshift(binding);
  }
}
