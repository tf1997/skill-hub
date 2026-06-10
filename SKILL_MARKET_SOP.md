# Skill Hub 市场 SOP

## 1. 目标

本 SOP 用于指导 Skill Hub 客户端的设计、开发、测试、发布和日常运营。软件定位为一个基于 MinIO 对象存储的 skill 市场客户端，支持 skill 浏览、下载、安装、更新、卸载、本地扫描和 Claude / Codex skill 管理。

技术架构固定使用 Tauri v1 + Rust + 前端框架。MinIO 只作为对象存储，不引入中心数据库。客户端使用本地 SQLite 维护缓存、安装状态和审计记录。

## 2. 范围

本 SOP 覆盖以下能力：

- 远程 skill 市场浏览。
- 内置 / 本地配置的 MinIO 源 catalog 同步，普通客户端 UI 不暴露源配置。
- skill 下载、校验、安装、更新、回滚和卸载。
- 公共、前端、后端、产品等分类管理。
- Claude / Codex skill 的个人级和项目级管理。
- 本地 skill 扫描、导入和状态识别。
- skill 打包、上传和索引生成。

不覆盖以下能力：

- 中心化账号系统。
- 中心化数据库服务。
- 在线支付、评分、评论等强平台能力。
- 自动执行 skill 内任意脚本。

## 3. 架构原则

- MinIO 是数据源，不是数据库。
- catalog 是索引产物，版本包和 manifest 是真实数据。
- 客户端本地 SQLite 只保存缓存、安装状态和审计记录。
- 安装逻辑通过 adapter 隔离 Claude / Codex 差异。
- 分类和安装级别都必须可扩展；目标平台只在安装或启用时选择。
- 所有下载包必须校验 hash，正式源应支持签名校验。
- 安装、更新、卸载必须支持失败回滚。
- 路径访问必须做白名单和 canonicalize 校验。

## 4. 技术架构

### 4.1 客户端技术栈

- 桌面框架：Tauri v1。
- 后端：Rust。
- 前端：React / Vue / Svelte 均可，推荐 React + Vite。
- 本地数据库：SQLite。
- 对象存储：MinIO，按 S3-compatible API 访问。
- 包格式：tar.zst 或 zip。Windows 优先兼容 zip，生产推荐 tar.zst。
- 校验：SHA-256。
- 签名：可选 minisign / cosign / age-plugin 方案，MVP 可先预留字段。

### 4.2 Tauri v1 要求

后端统一使用 Tauri v1 command 暴露能力：

```rust
#[tauri::command]
async fn refresh_catalog(...) -> Result<..., String> {
    ...
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            refresh_catalog,
            search_skills,
            install_skill,
            update_skill,
            uninstall_skill,
            scan_local_skills,
            bind_project,
            package_skill,
            publish_skill
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Tauri v1 侧的约束：

- 所有文件系统写入必须由 Rust 后端执行。
- 前端不得直接拼接任意本地路径执行安装。
- command 入参必须做 serde 结构化解析。
- command 返回值使用统一错误结构。
- 长任务使用进度事件通知前端。
- 下载、解压、安装必须放在 Rust 异步任务中执行。
- 前端不暴露 MinIO 源配置，也不保存 MinIO secret key 的明文状态。

### 4.3 模块划分

```text
src-tauri/
  src/
    commands/
      catalog.rs
      install.rs
      update.rs
      local.rs
      publish.rs
    core/
      catalog_service.rs
      source_service.rs
      package_service.rs
      update_service.rs
      audit_service.rs
    storage/
      minio_client.rs
      object_paths.rs
    db/
      mod.rs
      migrations/
    adapters/
      mod.rs
      codex.rs
      claude.rs
    models/
      skill.rs
      catalog.rs
      install.rs
      source.rs
    security/
      checksum.rs
      signature.rs
      path_guard.rs
```

前端建议结构：

```text
fronted/
  package.json
  index.html
  vite.config.ts
  src/
    pages/
      MarketPage.tsx
      SkillDetailPage.tsx
      InstalledPage.tsx
      ProjectsPage.tsx
      UpdatesPage.tsx
      SettingsPage.tsx
    components/
      SkillList.tsx
      SkillFilters.tsx
      InstallDialog.tsx
      UpdateCenter.tsx
    api/
      tauriCommands.ts
    types/
      skill.ts
```

## 5. MinIO 存储规范

### 5.1 Bucket 结构

```text
skill-market/
  catalog.v1.json
  categories.v1.json

  indexes/
    category/public.json
    category/frontend.json
    category/backend.json
    category/product.json
    search-lite.json

  skills/
    {namespace}/
      {skill_id}/
        manifest.json
        versions/
          {version}/
            skill.json
            package.zip
            package.sha256
            signature.minisig
            changelog.md
```

### 5.2 catalog.v1.json

`catalog.v1.json` 是市场首页入口，只保存轻量索引。

```json
{
  "schema": "skillhub.catalog.v1",
  "generated_at": "2026-06-09T00:00:00Z",
  "categories": ["public", "frontend", "backend", "product"],
  "skills": [
    {
      "namespace": "official",
      "id": "react-component-reviewer",
      "name": "React Component Reviewer",
      "summary": "Review React components.",
      "latest_version": "1.1.0",
      "categories": ["frontend"],
      "manifest_path": "skills/official/react-component-reviewer/manifest.json"
    }
  ]
}
```

### 5.3 categories.v1.json

分类必须可配置，不允许写死在客户端代码中。
仓库根目录提供默认 `categories.v1.json`，发布脚本默认读取该文件；也可以通过 `-CategoriesPath` 指定外部分类 JSON。`skill.json` 中引用的分类必须先在 `categories.v1.json` 的 `items` 中定义。

```json
{
  "schema": "skillhub.categories.v1",
  "items": [
    {
      "id": "public",
      "name": "公共",
      "order": 10
    },
    {
      "id": "frontend",
      "name": "前端",
      "order": 20
    },
    {
      "id": "backend",
      "name": "后端",
      "order": 30
    },
    {
      "id": "product",
      "name": "产品",
      "order": 40
    }
  ]
}
```

## 6. Skill 包规范

### 6.1 skill.json

每个版本必须在 MinIO 版本目录中包含 `skill.json`。它是市场发布和校验元数据，不打入 `package.zip`，也不安装到 Codex / Claude skill 目录。

```json
{
  "schema": "skillhub.skill.v1",
  "id": "react-component-reviewer",
  "namespace": "official",
  "name": "React Component Reviewer",
  "version": "1.1.0",
  "summary": "Review React components for UX, accessibility and maintainability.",
  "categories": ["frontend"],
  "tags": ["react", "review", "accessibility"],
  "levels": ["personal", "project"],
  "author": {
    "name": "Skill Hub"
  },
  "license": "MIT",
  "compatibility": {},
  "permissions": {
    "network": false,
    "filesystem": "project-read"
  },
  "package": {
    "file": "package.zip",
    "sha256": "replace-with-package-hash",
    "size": 123456
  }
}
```

### 6.2 包内容

```text
package.zip
  README.md
  SKILL.md
  assets/
  references/
  scripts/
```

要求：

- `package.zip` 是 skill 运行包，不得包含任何 `.json` 文件。
- `skill.json` 必须位于 MinIO 版本目录 `skills/{namespace}/{skill_id}/versions/{version}/skill.json`。
- `SKILL.md` 是 skill 主要说明文件。
- `README.md` 面向市场展示。
- `scripts/` 内文件默认不得自动执行。
- 包内路径不得包含 `..`、绝对路径或隐藏逃逸路径。
- `skill.json`、manifest、catalog 等 JSON 文件只用于市场发布和校验；本地包缓存与 Codex / Claude 目标目录均不得写入任何 `.json` 文件。

## 7. 本地数据规范

### 7.1 本地目录

```text
SkillHub/
  db/
    skillhub.sqlite
  cache/
    catalog/
    downloads/
  packages/
    {namespace}.{skill_id}/
      {version}/
  backups/
    {install_id}/
      {timestamp}/
  logs/
```

Windows 下建议放在应用数据目录，具体路径由 Tauri v1 app handle 获取，不由前端硬编码。

### 7.2 SQLite 表

```sql
CREATE TABLE sources (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  endpoint TEXT NOT NULL,
  bucket TEXT NOT NULL,
  region TEXT,
  access_key_ref TEXT,
  secret_key_ref TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_sync_at TEXT
);

CREATE TABLE catalog_cache (
  source_id TEXT NOT NULL,
  namespace TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  latest_version TEXT NOT NULL,
  manifest_path TEXT NOT NULL,
  raw_manifest TEXT NOT NULL,
  etag TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (source_id, namespace, skill_id)
);

CREATE TABLE installed_skills (
  id TEXT PRIMARY KEY,
  source_id TEXT,
  namespace TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  version TEXT NOT NULL,
  target TEXT NOT NULL,
  level TEXT NOT NULL,
  project_path TEXT,
  install_path TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL,
  managed_by_skillhub INTEGER NOT NULL DEFAULT 1,
  installed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE local_skills (
  id TEXT PRIMARY KEY,
  target TEXT NOT NULL,
  level TEXT NOT NULL,
  project_path TEXT,
  path TEXT NOT NULL,
  detected_manifest TEXT,
  managed_by_skillhub INTEGER NOT NULL DEFAULT 0,
  scanned_at TEXT NOT NULL
);

CREATE TABLE audit_logs (
  id TEXT PRIMARY KEY,
  action TEXT NOT NULL,
  skill_ref TEXT,
  result TEXT NOT NULL,
  detail TEXT,
  created_at TEXT NOT NULL
);
```

## 8. Adapter 规范

### 8.1 统一接口

```rust
trait SkillAdapter {
    fn target(&self) -> SkillTarget;
    fn discover_roots(&self) -> Vec<SkillRoot>;
    fn scan(&self, root: &SkillRoot) -> anyhow::Result<Vec<LocalSkill>>;
    fn validate_package(&self, package_dir: &Path) -> anyhow::Result<()>;
    fn install(&self, package_dir: &Path, destination: &SkillRoot) -> anyhow::Result<InstallResult>;
    fn uninstall(&self, install: &InstalledSkill) -> anyhow::Result<()>;
}
```

### 8.2 安装级别

- `personal`：安装到用户级 skill 目录。
- `project`：安装到指定项目目录下的 skill 目录。

具体路径由 adapter 负责发现和配置。客户端只保存最终安装路径，不把 Claude / Codex 的路径规则写死在 UI 中。

### 8.3 作用域互斥规则

同一个 skill 在同一个目标平台上，个人级和项目级不能同时生效。

规则：

- 如果 `Codex / personal / skill-a` 已启用，则任何项目都不能再启用 `Codex / project / skill-a`。
- 如果任意项目已启用 `Codex / project / skill-a`，则不能再启用 `Codex / personal / skill-a`。
- Claude 和 Codex 分别判断，互不影响。
- 不同版本也按同一个 skill 判断，不能通过安装不同版本绕过互斥规则。
- 仅下载到本地包缓存不受限制，只有启用或安装到目标目录时才做冲突校验。

冲突判断字段：

```text
namespace + skill_id + target
```

不把 `version` 放入冲突判断。这样可以避免同一个 skill 的个人级 `1.0.0` 和项目级 `1.1.0` 同时生效。

后端校验逻辑：

```text
准备启用 personal：
  查询是否存在 enabled = 1 且 level = project 的同 target / namespace / skill_id 记录。
  如果存在，拒绝启用，并提示用户先移除项目级绑定。

准备启用 project：
  查询是否存在 enabled = 1 且 level = personal 的同 target / namespace / skill_id 记录。
  如果存在，拒绝启用，并提示用户该 skill 已在个人级生效。
```

前端交互：

- 安装弹窗中，如果个人级已启用，则项目级选项置灰。
- 安装弹窗中，如果项目级已启用，则个人级选项置灰。
- 详情页显示冲突来源，例如“已在 Codex 个人级启用”或“已在 2 个项目中启用”。
- 用户可以先禁用旧作用域，再启用新作用域。

### 8.4 本地扫描策略

扫描顺序：

1. 读取用户配置的 Claude / Codex 根目录。
2. 自动发现常见个人级目录。
3. 扫描用户绑定的项目目录。
4. 在个人级 / 项目级 skill 目录下识别包含 `SKILL.md` 的子目录；`skill.json` 和 manifest 只属于市场发布 / 同步链路，不作为本地运行目录要求。
5. 标记是否由 Skill Hub 管理。
6. 写入 `local_skills`。

## 9. 远程同步 SOP

### 9.1 MinIO 源配置

操作步骤：

1. 安装包、本地种子或运维脚本写入 endpoint、bucket、region、access key、secret key。
2. Rust 后端校验 endpoint 格式。
3. 执行只读连通性测试。
4. 拉取 `catalog.v1.json` 和 `categories.v1.json`。
5. 校验 schema。
6. 写入 `sources`。
7. 写入 `catalog_cache`。
8. 前端显示市场同步结果，不展示源配置表单。

验收标准：

- 源不可用时显示明确错误。
- secret key 不出现在前端持久状态、UI 和日志中。
- catalog schema 错误时不得写入缓存。

### 9.2 刷新市场

操作步骤：

1. 用户点击刷新，或应用启动后按策略刷新。
2. 获取所有 enabled source。
3. 读取远程 catalog。
4. 对比 etag 或更新时间。
5. 下载变更的 manifest。
6. 更新本地缓存。
7. 发送进度事件给前端。

验收标准：

- 网络失败不影响已缓存市场展示。
- 多源 skill 冲突时按 source 显示，不静默覆盖。
- 分类来自 `categories.v1.json`，客户端可显示新增分类。

## 10. 安装 SOP

操作步骤：

1. 用户选择 skill、版本、目标平台、安装级别。
2. 如果是项目级安装，用户只能从项目菜单已绑定的项目中选择；市场不直接选择或绑定新目录。
3. 后端读取对应版本 `skill.json`。
4. 检查安装级别、项目绑定和目标目录配置是否可用。
5. 下载 package 到 `cache/downloads`。
6. 校验 SHA-256。
7. 如果启用签名策略，执行签名校验。
8. 解压到 `packages/{namespace}.{skill_id}/{version}`。
9. adapter 校验包结构。
10. 备份目标目录中的旧版本。
11. 执行安装，只复制目标平台需要的运行文件，过滤所有 `.json` 文件。
12. 写入 `installed_skills`，记录 target、level、project_path、install_path 等关系。
13. 写入 `audit_logs`。
14. 通知前端安装完成。

失败处理：

- 下载失败：删除临时下载文件。
- 校验失败：删除下载文件，记录安全错误。
- 解压失败：删除临时解压目录。
- 安装失败：恢复备份。
- 数据库写入失败：保留文件但标记为 `unknown`，下一次扫描修复。

## 11. 更新 SOP

操作步骤：

1. 查询 `installed_skills`。
2. 从 `catalog_cache` 查找 latest version。
3. 使用 semver 比较版本。
4. 排除用户锁定版本。
5. 展示 changelog。
6. 用户确认更新。
7. 按安装 SOP 下载、校验、解压。
8. 备份当前安装目录。
9. adapter 安装新版本。
10. 更新 `installed_skills.version`。
11. 写入审计日志。

验收标准：

- 支持单个 skill 更新。
- 支持批量更新。
- 支持失败回滚。
- 用户手动修改过的 skill 必须提示冲突。

## 12. 卸载 SOP

操作步骤：

1. 用户选择已安装 skill。
2. 后端确认安装记录存在。
3. adapter 删除目标安装内容。
4. 可选保留备份和包缓存。
5. 更新 `installed_skills.status = 'uninstalled'`。
6. 写入审计日志。

验收标准：

- 只删除 Skill Hub 管理的安装路径。
- 不删除用户手动创建的无关文件。
- 项目级卸载不得影响个人级安装。
- 卸载判断以 SQLite 安装记录和预期安装路径为准，不依赖目标目录内的 JSON marker。

## 13. 本地管理 SOP

### 13.1 扫描本地 skill

操作步骤：

1. 用户进入本地管理页。
2. 后端加载 Claude / Codex adapter。
3. 扫描个人级目录。
4. 扫描已绑定项目目录。
5. 识别本地 skill 结构，未托管目录只要求存在 `SKILL.md`，显示名可从 `SKILL.md` 标题或目录名推断。
6. 与 `installed_skills` 对比。
7. 显示托管、未托管、缺失、漂移四种状态。

状态定义：

- `managed`：由 Skill Hub 安装，文件存在。
- `unmanaged`：本地存在，但不由 Skill Hub 管理。
- `missing`：数据库有记录，但文件不存在。
- `modified`：文件 hash 与安装记录不一致。

状态来源：

- `skill_packages`：市场 skill 已下载到本地包缓存，前端显示“已缓存”。
- `skill_bindings`：skill 已安装到个人或项目目录，前端显示“已安装 / 已启用”。
- `local_skills`：扫描到的本地已有 skill，用户自建项只展示和预览，不自动接管。
- 本地包缓存可删除，删除时只删除 `skill_packages` 记录和缓存包目录，不影响已经安装到 Codex / Claude 的目标目录。

预览要求：

- 市场 skill 可从包缓存或临时下载后的包目录读取 `SKILL.md`、`README.md` 等文本内容预览。
- 本地已有 skill 可从扫描到的目录直接预览；本地运行目录不要求 `skill.json`。
- 预览动作不得改变安装状态；是否缓存以 `skill_packages` 记录为准。

### 13.2 导入本地 skill

操作步骤：

1. 用户选择未托管 skill。
2. 后端读取并校验结构。
3. 生成本地 manifest。
4. 写入 `installed_skills`，标记 `managed_by_skillhub = 0`。
5. 后续只提供打开目录、备份、卸载提示，不自动覆盖。

## 14. 发布 SOP

发布建议先做为本地工具或客户端高级功能。

### 14.1 使用 PowerShell 脚本发布

仓库内提供 `publish-skill.ps1`，用于把一个本地 skill 目录发布到完整 MinIO 市场结构。

前置条件：

- 已安装 MinIO Client `mc`。
- 用于发布到市场的源目录包含 `skill.json` 和 `SKILL.md`。
- `skill.json` 至少包含 `id`、`name`、`version`。

方式一，先配置 MinIO alias：

```powershell
mc alias set skillhub http://127.0.0.1:9000 minioadmin minioadmin
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Alias skillhub `
  -Bucket skill-market `
  -CreateBucket
```

方式二，由脚本临时配置 alias：

```powershell
.\publish-skill.ps1 `
  -SkillDir .\examples\frontend-reviewer `
  -Namespace official `
  -Endpoint http://127.0.0.1:9000 `
  -AccessKey minioadmin `
  -SecretKey minioadmin `
  -Bucket skill-market `
  -CreateBucket
```

脚本会自动执行：

1. 读取 `skill.json`。
2. 复制运行文件到临时目录，排除所有 `.json` 文件。
3. 读取并校验外部 `categories.v1.json`。
4. 校验 `skill.json` 中的分类均已在 `categories.v1.json` 中定义。
5. 压缩临时运行目录为 `package.zip`。
6. 计算 `package.sha256`。
7. 上传版本文件到 `skills/{namespace}/{skill_id}/versions/{version}/`。
8. 更新 `skills/{namespace}/{skill_id}/manifest.json`。
9. 上传 `categories.v1.json`。
10. 重建 `indexes/category/*.json`。
11. 重建 `indexes/search-lite.json`。
12. 最后上传 `catalog.v1.json`。

注意：`catalog.v1.json` 必须最后上传，避免客户端看到尚未完整上传的版本。

操作步骤：

1. 用户选择 skill 源目录。
2. 选择分类配置文件；默认使用仓库根目录 `categories.v1.json`，也可传入 `-CategoriesPath`。
3. 校验 `skill.json`、`SKILL.md` 和 `categories.v1.json`。
4. 校验 skill 分类已在 `categories.v1.json` 中声明。
5. 过滤 `.json` 文件后生成运行压缩包。
6. 计算 SHA-256。
7. 生成 `package.sha256`。
8. 可选生成签名。
9. 上传到 MinIO：

```text
skills/{namespace}/{skill_id}/versions/{version}/
```

10. 更新该 skill 的 `manifest.json`。
11. 上传 `categories.v1.json`。
12. 重新生成 `catalog.v1.json`。
13. 重新生成分类索引。
14. 上传索引文件。

验收标准：

- 同版本不得静默覆盖。
- catalog 更新失败时不得删除已上传版本包。
- 发布前必须预览 object paths。
- 发布成功后客户端刷新可看到新版本。

## 15. 前端页面 SOP

### 15.1 市场页

必备元素：

- 分类筛选。
- 安装状态筛选。
- 搜索框。
- skill 列表。
- 安装按钮。
- 详情入口。

### 15.2 详情页

必备元素：

- 名称、简介、版本。
- 分类、标签。
- 安装级别。
- changelog。
- 权限声明。
- 安装 / 更新 / 卸载操作。

### 15.3 本地管理页

必备元素：

- Claude / Codex 分组。
- 个人级 / 项目级分组。
- 托管状态。
- 扫描按钮。
- 导入按钮。
- 打开目录按钮。

### 15.4 更新中心

必备元素：

- 可更新列表。
- 当前版本和最新版本。
- changelog 预览。
- 批量更新。
- 版本锁定。
- 冲突提示。

## 16. 安全 SOP

必须执行：

- 下载后校验 SHA-256。
- 解压前检查压缩包路径逃逸。
- 解压后检查包大小和文件数量。
- 安装路径必须 canonicalize。
- 只允许写入配置过的 skill 根目录。
- 禁止自动执行包内脚本。
- Markdown 渲染必须 sanitize。
- secret key 不写入普通日志。
- 审计安装、更新、卸载、发布动作。

建议执行：

- 官方源启用签名校验。
- 首次添加源时展示源风险提示。
- 对远程 README 做内容安全过滤。
- 更新前保留至少一个可回滚备份。

## 17. 错误码规范

```text
SOURCE_CONNECT_FAILED
SOURCE_CATALOG_NOT_FOUND
SOURCE_SCHEMA_INVALID
SKILL_MANIFEST_INVALID
PACKAGE_DOWNLOAD_FAILED
PACKAGE_CHECKSUM_MISMATCH
PACKAGE_SIGNATURE_INVALID
PACKAGE_EXTRACT_FAILED
ADAPTER_TARGET_UNSUPPORTED
INSTALL_PATH_INVALID
INSTALL_FAILED
UPDATE_CONFLICT_LOCAL_MODIFIED
UNINSTALL_FAILED
PUBLISH_OBJECT_EXISTS
PUBLISH_CATALOG_UPDATE_FAILED
```

前端必须根据错误码显示稳定文案，不直接展示 Rust 内部错误。

## 18. MVP 交付顺序

第一阶段：

1. Tauri v1 项目骨架。
2. SQLite 初始化和 migration。
3. 默认 MinIO 源配置。
4. catalog 拉取和缓存。
5. 市场页和详情页。
6. SHA-256 下载校验。
7. Codex / Claude adapter 基础安装。
8. 个人级安装、更新、卸载。

第二阶段：

1. 项目级目录绑定。
2. 本地扫描。
3. 未托管 skill 导入。
4. 更新中心。
5. 回滚和冲突检测。

第三阶段：

1. 发布工具。
2. 分类索引生成。
3. 签名校验。
4. 多 MinIO 源。
5. 批量安装和技能集合。

## 19. 测试 SOP

### 19.1 单元测试

必须覆盖：

- semver 比较。
- SHA-256 校验。
- catalog schema 解析。
- skill.json 解析。
- object path 生成。
- 路径逃逸检测。

### 19.2 集成测试

必须覆盖：

- 从测试 MinIO 拉取 catalog。
- 下载包并校验。
- 安装到临时目录。
- 更新后旧版本备份。
- 更新失败回滚。
- 卸载不删除无关文件。

### 19.3 前端测试

必须覆盖：

- 市场列表渲染。
- 分类筛选。
- 安装弹窗。
- 更新中心。
- 错误提示。

## 20. 发布验收清单

发布前确认：

- Tauri 使用 v1。
- 应用可启动。
- 默认 MinIO 源可同步。
- catalog 可刷新。
- 分类可从远程配置加载。
- skill 可下载。
- hash 校验生效。
- Claude / Codex 个人级安装可用。
- 项目级绑定可用。
- 更新可回滚。
- 卸载不误删。
- 本地扫描可识别托管和未托管 skill。
- secret key 未出现在日志。
- 所有关键动作写入审计日志。

## 21. 运维 SOP

日常维护：

- 每次发布 skill 后重新生成 catalog。
- 定期检查 MinIO bucket 中孤儿版本包。
- 定期抽检 package hash。
- 保留旧版本，除非明确执行下架。
- 下架 skill 时更新 catalog，不直接删除用户可能依赖的旧版本。
- 分类新增只改 `categories.v1.json` 和索引生成逻辑。

下架流程：

1. 在 `manifest.json` 中标记 deprecated。
2. 从 `catalog.v1.json` 首页索引移除或降权。
3. 保留历史版本包。
4. 客户端详情页显示下架状态。
5. 已安装用户仍可卸载或回滚。

## 22. 决策记录

- 采用 Tauri v1，而不是 Tauri v2。
- 采用 MinIO 对象存储，不引入中心数据库。
- 客户端本地使用 SQLite。
- 分类由远程配置驱动。
- Claude / Codex 安装差异由 adapter 处理。
- MVP 先做 hash 校验，签名作为后续增强。
