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
import {
  markMockLocalSkillsCached,
  mockAdminAuditLogs,
  mockAdminDrafts,
  mockAdminPluginDrafts,
  mockAdminSession,
  mockBootstrap,
  mockBootstrapWithSyncedLocalSkills,
  mockMinioBucket,
  mockMinioEndpoint,
  mockPreviewFileList,
  mockPreviewFiles,
  mockUpdatedAt,
  syncMockLocalSkillsWithCache,
  upsertMockBinding,
  upsertMockCachedPackage
} from "./mockData";

export const browserMockApi = {
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
