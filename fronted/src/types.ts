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

export type AppBootstrap = {
  sources: Source[];
  categories: Category[];
  skills: MarketSkill[];
  bindings: SkillBinding[];
  cachedPackages: CachedSkillPackage[];
  projects: Project[];
  targetRoots: TargetRoot[];
  updates: UpdateCandidate[];
};

export type SaveSourceRequest = {
  id?: string;
  name: string;
  endpoint: string;
  bucket: string;
  region?: string | null;
  enabled: boolean;
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
};

export type SkillPreview = {
  title: string;
  rootPath: string;
  origin: string;
  files: SkillPreviewFile[];
};

export type SkillPreviewFile = {
  path: string;
  language: string;
  content: string;
  truncated: boolean;
};
