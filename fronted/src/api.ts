import { invoke } from "@tauri-apps/api/tauri";
import type {
  AppBootstrap,
  DeleteCachedSkillRequest,
  InstallSkillRequest,
  LocalSkill,
  MarketSkill,
  Project,
  SaveSourceRequest,
  SkillBinding,
  SkillPreview,
  SkillPreviewRequest,
  Source,
  TargetRoot,
  UpdateCandidate
} from "./types";

export const api = {
  bootstrap: () => invoke<AppBootstrap>("bootstrap"),
  listMarketSkills: () => invoke<MarketSkill[]>("list_market_skills"),
  listSources: () => invoke<Source[]>("list_sources"),
  saveSource: (request: SaveSourceRequest) => invoke<Source>("save_source", { request }),
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
