export type Source = {
  id: string;
  name: string;
  endpoint: string;
  bucket: string;
  region?: string | null;
  enabled: boolean;
  lastSyncAt?: string | null;
};

export type TargetRoot = {
  target: string;
  personalPath: string;
  updatedAt: string;
};

export type Category = {
  id: string;
  name: string;
  order: number;
};

export type MarketSkill = {
  namespace: string;
  id: string;
  name: string;
  summary: string;
  latestVersion: string;
  categories: string[];
  tags: string[];
  targets?: string[];
  levels: string[];
  manifestPath: string;
  updatedAt?: string | null;
  sourceId?: string | null;
  installedBindings: SkillBinding[];
  cachedVersions: string[];
};

export type MarketProject = {
  slug: string;
  name: string;
  description: string;
  order: number;
  createdAt?: string | null;
  updatedAt?: string | null;
  updatedBy?: string | null;
};

export type SkillBinding = {
  id: string;
  packageId: string;
  sourceId?: string | null;
  namespace: string;
  skillId: string;
  skillName: string;
  version: string;
  target: string;
  level: string;
  projectPath?: string | null;
  installPath: string;
  enabled: boolean;
  installMode: string;
  updatePolicy: string;
  status: string;
  createdAt: string;
  updatedAt: string;
};

export type Project = {
  id: string;
  name: string;
  path: string;
  createdAt: string;
  updatedAt: string;
};

export type LocalSkill = {
  id: string;
  target: string;
  level: string;
  projectPath?: string | null;
  path: string;
  detectedManifest?: string | null;
  managedBySkillhub: boolean;
  status: string;
  scannedAt: string;
};

export type CachedSkillPackage = {
  sourceId?: string | null;
  namespace: string;
  skillId: string;
  skillName: string;
  version: string;
  packagePath: string;
  cachedAt: string;
  bindingCount: number;
};

export type UpdateCandidate = {
  bindingId: string;
  namespace: string;
  skillId: string;
  skillName: string;
  target: string;
  level: string;
  projectPath?: string | null;
  currentVersion: string;
  latestVersion: string;
  updatePolicy: string;
  blockedReason?: string | null;
};

export type UpdatePackage = {
  target: "portable" | "installer";
  platform: string;
  arch: string;
  url: string;
  sha256: string;
  signature?: string | null;
  size?: number | null;
};

export type UpdateCheckResult = {
  current_version: string;
  latest_version?: string | null;
  available: boolean;
  downloadable: boolean;
  distribution: string;
  platform: string;
  arch: string;
  package?: UpdatePackage | null;
  notes?: string | null;
  message?: string | null;
  manifest_url?: string | null;
};

export type DownloadUpdateResult = {
  version: string;
  target: "portable" | "installer";
  path: string;
  ready_to_restart: boolean;
  message: string;
};

export type AppBootstrap = {
  sources: Source[];
  categories: Category[];
  skills: MarketSkill[];
  marketProjects: MarketProject[];
  bindings: SkillBinding[];
  cachedPackages: CachedSkillPackage[];
  localSkills: LocalSkill[];
  projects: Project[];
  targetRoots: TargetRoot[];
  updates: UpdateCandidate[];
  metadataSyncError?: string | null;
};

export type SaveSourceRequest = {
  id?: string;
  name: string;
  endpoint: string;
  bucket: string;
  region?: string | null;
  enabled: boolean;
};

export type AdminSession = {
  enabled: boolean;
  endpoint: string;
  bucket: string;
  region?: string | null;
  role: "system" | "project" | string;
  projects: string[];
  macAddress: string;
  name?: string | null;
};

export type AdminAuditLog = {
  objectPath: string;
  action: string;
  actor?: string | null;
  role?: string | null;
  macAddress?: string | null;
  ipAddress?: string | null;
  target?: string | null;
  summary: string;
  createdAt: string;
  payload: unknown;
};

export type InstallSkillRequest = {
  sourceId?: string | null;
  namespace: string;
  skillId: string;
  version?: string | null;
  target: string;
  level: string;
  projectPath?: string | null;
  installMode?: string;
  updatePolicy?: string;
  enable: boolean;
};

export type DeleteCachedSkillRequest = {
  sourceId?: string | null;
  namespace: string;
  skillId: string;
  version: string;
};

export type CommandError = {
  code: string;
  message: string;
};

export type SkillPreviewRequest = {
  sourceId?: string | null;
  namespace?: string | null;
  skillId?: string | null;
  version?: string | null;
  bindingId?: string | null;
  path?: string | null;
  filePath?: string | null;
};

export type AdminDraftPreviewRequest = {
  adminKey: string;
  gitlabSourcePath: string;
  filePath?: string | null;
};

export type SkillPreview = {
  title: string;
  rootPath: string;
  origin: string;
  files: SkillPreviewFile[];
  fileList: SkillPreviewFileEntry[];
};

export type SkillPreviewFile = {
  path: string;
  language: string;
  content: string;
  truncated: boolean;
};

export type SkillPreviewFileEntry = {
  path: string;
  language: string;
  previewable: boolean;
};

export type PublishMeta = {
  namespace: string;
  skillId: string;
  name: string;
  summary: string;
  tags: string[];
  targets: string[];
  levels: string[];
  publishScope: "public" | "project" | string;
  publishCategorySlug?: string | null;
  publishProjectSlug?: string | null;
  changelog: string;
  updatedAt?: string | null;
  updatedBy?: string | null;
};

export type AdminDraftSkill = {
  gitlabSourcePath: string;
  draftSlug?: string | null;
  gitlabCategoryCode?: string | null;
  gitlabCategoryPath?: string[] | null;
  sourceAvailable: boolean;
  version?: string | null;
  author?: string | null;
  status: string;
  validationStatus?: string | null;
  publishMeta?: PublishMeta | null;
  publishedVersion?: string | null;
  updatedAt?: string | null;
};
