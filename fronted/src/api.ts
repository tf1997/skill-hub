import { invoke } from "@tauri-apps/api/tauri";
import type {
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
  UpdateCandidate
} from "./types";

const canUseTauri = typeof window !== "undefined" && "__TAURI_IPC__" in window;
const isDev = Boolean((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV);
const useBrowserMock = isDev && !canUseTauri;

const mockBootstrap: AppBootstrap = {
  sources: [
    {
      id: "compiled-source",
      name: "本地 MinIO",
      endpoint: "http://192.168.1.4:9000",
      bucket: "skill-market",
      enabled: true
    }
  ],
  categories: [
    { id: "public", name: "Public", order: 10 },
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
      updatedAt: "2026-06-12T16:18:08Z",
      installedBindings: [],
      cachedVersions: []
    }
  ],
  marketProjects: [
    {
      slug: "live-project",
      name: "Live Project",
      description: "Created by live MinIO integration test",
      status: "active"
    },
    {
      slug: "alpha",
      name: "Alpha",
      description: "Internal alpha workspace",
      status: "active"
    },
    {
      slug: "ops",
      name: "Ops",
      description: "Operations workflow skills",
      status: "active"
    },
    {
      slug: "research",
      name: "Research",
      description: "Research and analysis skills",
      status: "active"
    },
    {
      slug: "archive-demo",
      name: "Archive Demo",
      description: "Archived project example",
      status: "archived"
    }
  ],
  bindings: [],
  cachedPackages: [],
  localSkills: [],
  projects: [],
  targetRoots: [{ target: "codex", personalPath: "C:/Users/ctf19/.codex/skills", updatedAt: "2026-06-12T16:18:08Z" }],
  updates: []
};

const mockAdminSession: AdminSession = {
  enabled: true,
  endpoint: "http://192.168.1.4:9000",
  bucket: "skill-market",
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

const browserMockApi = {
  bootstrap: async () => mockBootstrap,
  listMarketSkills: async () => mockBootstrap.skills,
  listSources: async () => mockBootstrap.sources,
  saveSource: async (_request: SaveSourceRequest) => mockBootstrap.sources[0],
  unlockAdminMode: async (_adminKey: string) => mockAdminSession,
  listAdminDrafts: async (_adminKey: string) => [],
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
    return [project, ...next].sort((a, b) => a.name.localeCompare(b.name, "zh-Hans-CN"));
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
  listProjects: async () => [],
  saveProject: async (name: string, path: string, id?: string) => ({
    id: id ?? "mock-project",
    name,
    path,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString()
  }),
  unbindProject: async (_projectId: string) => [],
  scanLocalSkills: async () => [],
  previewSkill: async (_request: SkillPreviewRequest) => ({
    title: "MinIO Live Draft",
    rootPath: "skills/live/minio-live-draft",
    origin: "browser mock",
    files: mockPreviewFiles,
    fileList: mockPreviewFileList
  }),
  listUpdateCandidates: async () => []
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
  listUpdateCandidates: () => invoke<UpdateCandidate[]>("list_update_candidates")
};

export const api = useBrowserMock ? browserMockApi : tauriApi;
