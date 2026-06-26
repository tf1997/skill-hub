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
} from "../types";

export const tauriApi = {
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
