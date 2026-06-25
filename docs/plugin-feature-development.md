# Plugin 功能开发设计

## 1. 目标

本文件用于指导 Skill Hub 在现有 skill 市场能力之上增加 plugin 管理、发布和安装能力。

Plugin 源码仍由 GitLab 维护，GitLab CI 只负责把草稿同步到 MinIO 草稿区；正式发布、校验、打包、catalog 更新和审计仍由 Skill Hub 客户端管理员模式完成。这与现有 skill 发布链路保持一致。

目标能力：

- 浏览 skill 与 plugin 两类市场对象。
- 从 GitLab 同步 plugin 草稿到 MinIO。
- 管理员预览、校验、发布 plugin。
- 发布 Codex 和 Claude 两套平台原生 plugin 包。
- 普通用户从 Skill Hub 安装、启用、禁用、更新和卸载 plugin。
- 扫描本机已有 Claude / Codex plugin 与 marketplace 状态。
- 对 hooks、MCP、LSP、monitors、bin 等高风险组件做显式风险提示和审计。

不作为第一阶段目标：

- 接入 Claude 或 Codex 官方公共 plugin marketplace 投稿流程。
- 直接替用户绕过平台原生安装、启用或信任机制。
- 自动执行 plugin 内脚本或自动信任 hooks。
- 中心化账号、评分、评论或在线支付能力。

## 2. 官方平台约束

### 2.1 Claude Code

Claude plugin 通过 marketplace 分发。用户先添加 marketplace，再安装具体 plugin。Marketplace 可来自 GitHub 仓库、Git URL、本地路径或远程 `marketplace.json` URL。

Claude plugin 的核心约束：

- plugin 可扩展 skills、agents、hooks 和 MCP servers。
- 安装范围包括 user、project、local。
- 安装或变更后需要运行 `/reload-plugins` 以在当前会话生效。
- plugin manifest 位于 `.claude-plugin/plugin.json`。
- `.claude-plugin/` 目录中只放 `plugin.json`。
- `skills/`、`commands/`、`agents/`、`hooks/`、`.mcp.json`、`.lsp.json`、`monitors/`、`bin/`、`settings.json` 位于 plugin 根目录。
- plugin skill 会命名空间化，例如 `/my-plugin:hello`。
- project scope 写入项目共享配置，local scope 仅本机生效。

### 2.2 Codex

Codex plugin 通过 plugin marketplace 暴露给 Codex 插件目录。个人级 marketplace 可以放在 `~/.agents/plugins/marketplace.json`，仓库级 marketplace 可以放在 `$REPO_ROOT/.agents/plugins/marketplace.json`。

Codex plugin 的核心约束：

- plugin manifest 必须位于 `.codex-plugin/plugin.json`。
- `.codex-plugin/` 目录中只放 `plugin.json`。
- `skills/`、`hooks/`、`assets/`、`.mcp.json`、`.app.json` 位于 plugin 根目录。
- marketplace 的 `plugins[]` 中每个 plugin 通过 `source.path` 指向 plugin 目录，路径相对 marketplace root。
- Codex 可读取仓库级 `$REPO_ROOT/.agents/plugins/marketplace.json`、个人级 `~/.agents/plugins/marketplace.json`，以及兼容的 `$REPO_ROOT/.claude-plugin/marketplace.json`。
- Codex 安装后会把 plugin 加入自身 cache，并在配置中保存启用状态。
- plugin-bundled hooks 不会因为安装或启用 plugin 自动被信任，需要用户审查。

### 2.3 对 Skill Hub 的设计影响

- Skill Hub 不能把 plugin 当成普通 skill 目录直接复制到 `.codex/skills` 或 `.claude/skills`。
- Skill Hub 应该把 plugin 下载到本地缓存，再物化成平台可识别的本地 marketplace 或通过平台 CLI 注册。
- Skill Hub 自己的 `pluginhub.json` 只用于市场发布、治理和校验，不替代 `.codex-plugin/plugin.json` 或 `.claude-plugin/plugin.json`。
- 发布产物应区分 Codex 包和 Claude 包，避免在安装侧做复杂裁剪。

## 3. 总体产品模型

新增统一市场对象 `Artifact`，但第一阶段可以在代码中以 skill 与 plugin 两套结构分别落库，降低迁移风险。

```text
Artifact
  kind: skill | plugin
  sourceId
  namespace
  id
  name
  version
  summary
  categories
  tags
  targets: codex | claude
  scopes: user | project | local
  components
  riskLevel
```

发布治理字段不属于 GitLab 草稿源，由 `PublishMeta` 保存到 `draft/admin/...`：

```text
PublishMeta
  publishScope: public | project
  publishCategorySlug
  publishProjectSlug
  changelog
```

现有 skill 保持当前逻辑：

```text
MinIO skill package -> 本地 package cache -> 复制到 .codex/skills 或 .claude/skills
```

新增 plugin 走新逻辑：

```text
MinIO plugin package -> 本地 plugin cache -> 本地 marketplace -> 平台原生安装/启用
```

## 4. GitLab 源码规范

### 4.1 推荐目录

一个 GitLab 仓库可以维护一个 plugin，也可以维护多个 plugin。MVP 建议先支持一个仓库一个 plugin，后续通过 `pluginhub.json` 的 `sourcePath` 支持 monorepo。

```text
my-plugin/
  pluginhub.json        # 可选；缺失时由管理员发布元数据补齐
  README.md
  CHANGELOG.md
  LICENSE
  skills/
    commit-message/
      SKILL.md
  agents/
    pr-reviewer.md
  hooks/
  assets/
  .mcp.json
  .app.json
  .lsp.json
  monitors/
  bin/
  settings.json
```

说明：

- `pluginhub.json` 是可选的 Skill Hub 插件源元数据；缺失时，管理员页保存的发布元数据会生成发布所需元信息。
- `README.md` 是必需的插件源元数据入口，必须以 `---` front matter 开头，并包含 `name`、`description`、`version`、`author`。解析逻辑复用 skill front matter，兼容 `metadata.version`、`metadata.author`、`metadata.tags`。
- GitLab 与 MinIO 草稿区只保存通用 plugin 源数据，不提交 `codex/`、`claude/`、`.codex-plugin/` 或 `.claude-plugin/`。
- `.codex-plugin/plugin.json` 与 `.claude-plugin/plugin.json` 由 Skill Hub 发布器按 `targets` 动态生成。
- 如果 plugin 只支持单平台，仍复用同一套通用目录，发布器只生成对应平台包。
- `README.md` 面向市场展示；缺少上述 front matter 字段时，草稿可预览但不允许发布。
- `CHANGELOG.md` 可用于发布时生成版本 changelog。

### 4.2 pluginhub.json

示例：

```json
{
  "schema": "skillhub.plugin-source.v1",
  "namespace": "internal",
  "id": "commit-workflow",
  "name": "Commit Workflow",
  "version": "1.0.0",
  "summary": "Team commit and PR workflow plugin.",
  "categories": ["backend"],
  "tags": ["git", "pr", "workflow"],
  "targets": ["codex", "claude"],
  "scopes": ["user", "project"],
  "components": ["skills", "agents", "hooks"],
  "riskLevel": "medium"
}
```

字段要求：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `schema` | 是 | 固定为 `skillhub.plugin-source.v1` |
| `namespace` | 是 | 市场命名空间 |
| `id` | 是 | Skill Hub plugin id，建议 kebab-case |
| `name` | 是 | 市场展示名称 |
| `version` | 是 | Skill Hub 发布版本 |
| `summary` | 是 | 市场摘要 |
| `targets` | 是 | `codex`、`claude` |
| `scopes` | 是 | `user`、`project`、`local`，Codex 第一阶段只支持 user/project |
| `components` | 是 | 发布者声明组件清单，发布时由扫描结果校验 |
| `riskLevel` | 否 | 可由发布器根据组件自动计算 |

`pluginhub.json` 只描述插件自身，不描述发布到公共市场还是项目市场。发布范围、市场分类、项目归属、变更说明等治理信息与 skill 一样，由管理员页保存到 `draft/admin/gitlab/plugins/{gitlab_source_path}/publish-meta.v1.json`。

## 5. GitLab 草稿同步

新增 GitLab CI 模板 `docs/gitlab-plugin-draft-sync-template.yml`。它与现有 skill 草稿同步模板平行。

CI 只做以下事情：

1. 定位 plugin 源目录。
2. 校验存在插件通用源文件，其中 `README.md` 必须存在并包含 `name`、`description`、`version`、`author` front matter；`pluginhub.json` 可选。
3. 采集 GitLab branch、commit SHA、pipeline URL。
4. 同步源码到 MinIO 草稿区。
5. 写入 `validation.json`。

CI 不做以下事情：

- 不生成正式 `plugin.json`。
- 不生成正式 zip 包。
- 不更新 plugin catalog。
- 不覆盖正式版本。
- 不写正式 `plugins/{namespace}/{plugin_id}/` 目录。

草稿区结构：

```text
skillhub/$SKILL_MARKET_BUCKET/
  draft/
    gitlab/
      plugins/
        {gitlab_category_code1}/
          {gitlab_category_code2}/
            {draft_slug}/
              pluginhub.json
              README.md
              CHANGELOG.md
              skills/
              agents/
              validation.json
```

草稿分类路径需要和现有 skill 草稿区保持一致。当前推荐使用两级 GitLab 分类：

```text
draft/gitlab/plugins/{gitlab_category_code1}/{gitlab_category_code2}/{draft_slug}/
```

后端实现时不要把分类层级写死。应按以下规则解析：

```text
draft/gitlab/plugins/{gitlab_category_path...}/{draft_slug}/
```

其中：

- `{draft_slug}` 是插件草稿根目录，目录内至少包含一个通用插件源文件，例如 `README.md`、`skills/`、`agents/` 或 `.mcp.json`。
- `{gitlab_category_path...}` 是 `plugins/` 与 `{draft_slug}` 之间的所有路径段。
- MVP UI 可以按两级分类展示；后端和 MinIO object path 工具应支持一到多级路径。

示例：

```text
skillhub/$SKILL_MARKET_BUCKET/
  draft/
    gitlab/
      plugins/
        backend/
            java/
              {draft_slug}/
              pluginhub.json  # 可选
              README.md
              CHANGELOG.md
              skills/
              agents/
              validation.json
```

`validation.json` 示例：

```json
{
  "schema": "skillhub.plugin-validation.v1",
  "kind": "plugin",
  "commitSha": "abc123",
  "commitRef": "main",
  "pipelineId": "1",
  "pipelineUrl": "https://gitlab.example.com/ai/commit-workflow/-/pipelines/1",
  "validatedAt": "2026-06-23T10:00:00Z",
  "status": "passed"
}
```

## 6. MinIO 正式目录

正式发布区与现有 skill 正式目录平行：

```text
skill-market/
  plugin-catalog.v1.json

  indexes/
    plugin-category/
      public.json
      backend.json
    plugin-search-lite.json

  plugins/
    {namespace}/
      {plugin_id}/
        manifest.json
        versions/
          {version}/
            plugin.json
            package.codex.zip
            package.claude.zip
            package.codex.sha256
            package.claude.sha256
            component-inventory.json
            risk-report.json
            changelog.md
```

### 6.1 plugin-catalog.v1.json

`plugin-catalog.v1.json` 是 plugin 市场首页入口，只保存轻量索引。

```json
{
  "schema": "skillhub.plugin-catalog.v1",
  "generatedAt": "2026-06-23T10:00:00Z",
  "plugins": [
    {
      "namespace": "internal",
      "id": "commit-workflow",
      "name": "Commit Workflow",
      "summary": "Team commit and PR workflow plugin.",
      "latestVersion": "1.0.0",
      "categories": ["backend"],
      "tags": ["git", "pr", "workflow"],
      "targets": ["codex", "claude"],
      "scopes": ["user", "project"],
      "components": ["skills", "agents", "hooks"],
      "riskLevel": "medium",
      "manifestPath": "plugins/internal/commit-workflow/manifest.json"
    }
  ]
}
```

### 6.2 plugins/{namespace}/{plugin_id}/manifest.json

```json
{
  "schema": "skillhub.plugin-manifest.v1",
  "namespace": "internal",
  "id": "commit-workflow",
  "name": "Commit Workflow",
  "summary": "Team commit and PR workflow plugin.",
  "latestVersion": "1.0.0",
  "categories": ["backend"],
  "tags": ["git", "pr", "workflow"],
  "targets": ["codex", "claude"],
  "scopes": ["user", "project"],
  "components": ["skills", "agents", "hooks"],
  "riskLevel": "medium",
  "versions": [
    {
      "version": "1.0.0",
      "pluginPath": "plugins/internal/commit-workflow/versions/1.0.0/plugin.json",
      "packages": {
        "codex": {
          "packagePath": "plugins/internal/commit-workflow/versions/1.0.0/package.codex.zip",
          "sha256Path": "plugins/internal/commit-workflow/versions/1.0.0/package.codex.sha256"
        },
        "claude": {
          "packagePath": "plugins/internal/commit-workflow/versions/1.0.0/package.claude.zip",
          "sha256Path": "plugins/internal/commit-workflow/versions/1.0.0/package.claude.sha256"
        }
      },
      "createdAt": "2026-06-23T10:00:00Z"
    }
  ]
}
```

### 6.3 versions/{version}/plugin.json

`plugin.json` 是 Skill Hub 正式版本元数据，不是平台原生 manifest。

```json
{
  "schema": "skillhub.plugin.v1",
  "namespace": "internal",
  "id": "commit-workflow",
  "name": "Commit Workflow",
  "version": "1.0.0",
  "summary": "Team commit and PR workflow plugin.",
  "categories": ["backend"],
  "tags": ["git", "pr", "workflow"],
  "targets": ["codex", "claude"],
  "scopes": ["user", "project"],
  "components": ["skills", "agents", "hooks"],
  "riskLevel": "medium",
  "packages": {
    "codex": {
      "file": "package.codex.zip",
      "sha256": "replace-with-sha256",
      "size": 123456
    },
    "claude": {
      "file": "package.claude.zip",
      "sha256": "replace-with-sha256",
      "size": 123456
    }
  }
}
```

## 7. 管理员发布流程

管理员发布入口复用现有隐藏管理员模式，新增 Plugin 草稿列表。

发布步骤：

1. 管理员进入 `管理 -> Plugin 草稿`。
2. 后端扫描 `draft/gitlab/plugins/**/` 下的插件源目录，优先用 `pluginhub.json` 识别；缺少 `pluginhub.json` 但存在 `README.md`、`skills/` 等通用插件源文件时也列入草稿，标记为元数据待补充。
3. 后端从 object path 解析 `gitlab_category_path` 和 `draft_slug`。
4. 展示来源路径、分类路径、校验状态、草稿状态。
5. 读取草稿根目录 `README.md` front matter；缺少 `name`、`description`、`version`、`author` 时标记为元数据待补充并禁止发布。`pluginhub.json` 存在时作为可选补充；缺失时用 README 元数据与管理员保存的 `draft/admin/gitlab/plugins/{gitlab_source_path}/publish-meta.v1.json` 合成发布所需的插件源元数据。
6. 校验 Skill Hub 元数据；若 `pluginhub.json` 与管理员元数据同时存在，以管理员元数据覆盖发布范围、名称、摘要、标签、目标平台、作用域等治理字段。
7. 校验草稿中不存在 `codex/`、`claude/`、`.codex-plugin/`、`.claude-plugin/` 等平台生成目录。
8. 扫描通用组件清单；`pluginhub.json.components` 存在时可作为声明值，缺失时由源文件自动推导。
9. 生成 `component-inventory.json`。
10. 根据组件生成 `risk-report.json`。
11. 根据 `targets` 动态生成 `.codex-plugin/plugin.json` 与 `.claude-plugin/plugin.json`。
12. 生成平台包 `package.codex.zip` 与 `package.claude.zip`。
13. 计算 SHA-256。
14. 生成 Skill Hub 正式 `plugin.json`。
15. 上传版本目录。
16. 更新该 plugin 的 `manifest.json`。
17. 更新 `plugin-catalog.v1.json` 和索引。
18. 写入管理员审计日志。

发布时必须最后上传 `plugin-catalog.v1.json`，避免普通客户端看到尚未完整上传的版本。

## 8. 发布校验规则

### 8.1 基础校验

- `README.md` 必须包含 `name`、`description`、`version`、`author` front matter；如果草稿包含 `pluginhub.json`，则必须可解析；如果缺失，则必须已保存完整的管理员发布元数据。
- `namespace`、`id`、`version` 必须非空。
- `id` 建议 kebab-case。
- `targets` 至少包含一个平台。
- `categories` 必须存在于 `categories.v1.json`。
- 同版本不得静默覆盖。
- 包内路径不得包含 `..`、绝对路径或符号链接逃逸。
- 包文件数量、总大小、单文件大小必须有上限。
- 草稿源目录不得包含平台生成目录：`codex/`、`claude/`、`.codex-plugin/`、`.claude-plugin/`。

### 8.2 Codex 校验

- 发布器必须生成 `.codex-plugin/plugin.json`。
- 生成后的 `.codex-plugin/` 内只能放 `plugin.json`。
- manifest 中的组件路径必须以 `./` 开头并位于 plugin 根目录内。
- 若存在 hooks，检查 `hooks/hooks.json` 或 manifest `hooks` 字段。
- 若存在 MCP，检查 `.mcp.json`。
- 若存在 app，检查 `.app.json`。
- `skills` 指向目录时，目录下的每个 skill 应包含 `SKILL.md`。

### 8.3 Claude 校验

- 发布器必须生成 `.claude-plugin/plugin.json`。
- 生成后的 `.claude-plugin/` 内只能放 `plugin.json`。
- 组件目录必须位于 plugin 根目录。
- `skills/<name>/SKILL.md` 是推荐布局。
- 兼容旧式 `commands/`，但新 plugin 应优先使用 `skills/`。
- 若存在 `.lsp.json`、`monitors/monitors.json`、`settings.json`、`bin/`，发布器必须标记风险和安装说明。

## 9. 组件清单与风险模型

`component-inventory.json` 示例：

```json
{
  "schema": "skillhub.plugin-component-inventory.v1",
  "targets": {
    "codex": {
      "skills": ["commit"],
      "hooks": ["SessionStart"],
      "mcpServers": [],
      "apps": [],
      "assets": ["assets/icon.png"]
    },
    "claude": {
      "skills": ["commit"],
      "agents": ["pr-reviewer"],
      "hooks": ["PostToolUse"],
      "mcpServers": [],
      "lspServers": [],
      "monitors": [],
      "bin": []
    }
  }
}
```

风险等级建议：

| 等级 | 触发条件 | 安装策略 |
| --- | --- | --- |
| low | 仅 `skills/`、`assets/`、README | 普通确认 |
| medium | 包含 `agents/`、`settings.json`、`.app.json` | 显示组件清单 |
| high | 包含 `hooks/`、`.mcp.json`、`.lsp.json`、`monitors/`、`bin/` | 二次确认，审计记录必须包含组件详情 |

`risk-report.json` 示例：

```json
{
  "schema": "skillhub.plugin-risk-report.v1",
  "riskLevel": "high",
  "reasons": [
    "contains hooks",
    "contains executable bin files"
  ],
  "requiresUserReview": true,
  "notes": [
    "Codex plugin hooks require user trust review after installation.",
    "Claude plugin changes require /reload-plugins to take effect in current sessions."
  ]
}
```

## 10. 客户端本地目录

在现有 Skill Hub app data 目录下新增 plugin 子目录：

```text
SkillHub/
  plugin-packages/
    {namespace}.{plugin_id}/
      {version}/
        codex/
        claude/
  plugin-marketplaces/
    codex/
      user/
        marketplace.json
        plugins/
      projects/
        {project_hash}/
          marketplace.json
          plugins/
    claude/
      user/
        .claude-plugin/
          marketplace.json
        plugins/
      projects/
        {project_hash}/
          .claude-plugin/
            marketplace.json
          plugins/
  plugin-backups/
  logs/
```

说明：

- `plugin-packages/` 保存解压后的平台包。
- `plugin-marketplaces/` 是 Skill Hub 物化给平台读取的本地 marketplace。
- 项目级 marketplace 可以物化到项目目录，也可以先物化到 app data 后通过平台 CLI 添加。MVP 建议以平台官方期望路径为准，便于用户排查。

## 11. SQLite 数据模型

第一阶段新增表，不改动现有 `skill_packages` 和 `skill_bindings`。

```sql
CREATE TABLE plugin_packages (
  id TEXT PRIMARY KEY,
  source_id TEXT,
  namespace TEXT NOT NULL,
  plugin_id TEXT NOT NULL,
  plugin_name TEXT NOT NULL,
  version TEXT NOT NULL,
  target TEXT NOT NULL,
  package_path TEXT NOT NULL,
  sha256 TEXT,
  component_inventory_json TEXT NOT NULL,
  risk_level TEXT NOT NULL,
  cached_at TEXT NOT NULL,
  UNIQUE(source_id, namespace, plugin_id, version, target)
);

CREATE TABLE plugin_marketplaces (
  id TEXT PRIMARY KEY,
  target TEXT NOT NULL,
  scope TEXT NOT NULL,
  project_path TEXT,
  marketplace_name TEXT NOT NULL,
  root_path TEXT NOT NULL,
  marketplace_path TEXT NOT NULL,
  status TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(target, scope, project_path, marketplace_name)
);

CREATE TABLE plugin_bindings (
  id TEXT PRIMARY KEY,
  package_id TEXT NOT NULL,
  source_id TEXT,
  namespace TEXT NOT NULL,
  plugin_id TEXT NOT NULL,
  plugin_name TEXT NOT NULL,
  version TEXT NOT NULL,
  target TEXT NOT NULL,
  scope TEXT NOT NULL,
  project_path TEXT,
  marketplace_id TEXT,
  marketplace_name TEXT NOT NULL,
  platform_ref TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  install_mode TEXT NOT NULL,
  update_policy TEXT NOT NULL,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE local_plugins (
  id TEXT PRIMARY KEY,
  target TEXT NOT NULL,
  scope TEXT NOT NULL,
  project_path TEXT,
  path TEXT NOT NULL,
  marketplace_name TEXT,
  plugin_id TEXT,
  version TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL,
  component_inventory_json TEXT NOT NULL DEFAULT '{}',
  managed_by_skillhub INTEGER NOT NULL DEFAULT 0,
  scanned_at TEXT NOT NULL
);
```

## 12. 后端 Adapter 设计

新增 `PluginAdapter`，与现有 skill 安装逻辑分离。

```rust
trait PluginAdapter {
    fn target(&self) -> PluginTarget;
    fn validate_package(&self, package_dir: &Path) -> Result<ComponentInventory>;
    fn materialize_marketplace(
        &self,
        package_dir: &Path,
        scope: PluginScope,
        project_path: Option<&Path>,
    ) -> Result<MarketplaceRef>;
    fn install(&self, plugin: &PluginInstallRequest) -> Result<PluginBinding>;
    fn set_enabled(&self, binding: &PluginBinding, enabled: bool) -> Result<()>;
    fn uninstall(&self, binding: &PluginBinding) -> Result<()>;
    fn scan(&self) -> Result<Vec<LocalPlugin>>;
}
```

### 12.1 ClaudePluginAdapter

优先使用 Claude CLI：

```text
claude plugin marketplace add <local-marketplace-path> --scope user|project|local
claude plugin install <plugin>@<marketplace> --scope user|project|local
claude plugin enable <plugin>@<marketplace> --scope ...
claude plugin disable <plugin>@<marketplace> --scope ...
claude plugin uninstall <plugin>@<marketplace> --scope ...
claude plugin details <plugin>@<marketplace>
```

Skill Hub 安装 Claude plugin 时也必须自动执行平台安装动作：

- user scope 写入 Skill Hub 本地 Claude marketplace，project scope 写入 `$PROJECT/.claude-plugin/marketplace.json`。
- plugin 目录物化到 `<marketplace-root>/plugins/{namespace}.{plugin_id}`，marketplace entry 的 `source.path` 使用 `./plugins/{namespace}.{plugin_id}`。
- 安装或启用时执行 `claude plugin marketplace add <marketplace-json-path> --scope <scope>`、`claude plugin install <plugin>@<marketplace> --scope <scope>`、`claude plugin enable <plugin>@<marketplace> --scope <scope>`。
- 禁用时执行 `claude plugin disable <plugin>@<marketplace> --scope <scope>`；卸载时执行 `claude plugin uninstall <plugin>@<marketplace> --scope <scope>`。
- 已配置 marketplace、已安装或已卸载的 CLI 返回按幂等成功处理。
- Claude Code 当前会话需要 `/reload-plugins` 才能立即看到变更；新会话可直接读取。

若 Claude CLI 不可用：

- 允许只物化 marketplace。
- 安装命令返回 `PLUGIN_CLAUDE_INSTALL_FAILED`，前端提示用户安装 Claude Code CLI 后重试。Windows PowerShell 推荐 `irm https://claude.ai/install.ps1 | iex`，也可使用 `winget install Anthropic.ClaudeCode` 或 `npm install -g @anthropic-ai/claude-code`。
- 不手写 Claude 内部安装状态。

### 12.2 CodexPluginAdapter

优先使用 Codex CLI：

```text
codex plugin add <plugin>@personal
codex plugin marketplace add <local-marketplace-root>
codex plugin add <plugin>@<marketplace-name>
codex plugin remove <plugin>@<marketplace-name>
codex plugin marketplace list
codex plugin marketplace upgrade <marketplace-name>
codex plugin marketplace remove <marketplace-name>
```

Skill Hub 安装 Codex plugin 时必须自动执行平台安装动作，不能只写 marketplace 文件：

- user scope 写入 `~/.agents/plugins/marketplace.json`，marketplace 名称使用 `personal`，插件目录放在 `~/plugins/{namespace}.{plugin_id}`，随后执行 `codex plugin add <plugin>@personal`。
- project scope 写入 `$PROJECT/.agents/plugins/marketplace.json`，插件目录放在 `$PROJECT/plugins/{namespace}.{plugin_id}`，marketplace 名称使用 `skillhub-<project-path-hash>`，随后执行 `codex plugin marketplace add <project-root>` 和 `codex plugin add <plugin>@<marketplace-name>`。
- 启用时重新物化 plugin 目录和 marketplace entry，并再次执行 `codex plugin add ...`，用于刷新 Codex cache。
- 禁用或卸载时先执行 `codex plugin remove <plugin>@<marketplace-name>`，再移除 marketplace entry 和物化目录。
- 对已配置 marketplace 或已卸载 plugin 的 CLI 返回要按幂等成功处理。

若 Codex CLI 不可用：

- 个人级写入 `~/.agents/plugins/marketplace.json`，插件目录放在 marketplace root 可解析路径下。
- 项目级写入 `$PROJECT/.agents/plugins/marketplace.json`，插件目录可放在 `$PROJECT/plugins/{namespace}.{plugin_id}`。
- 前端提示用户重启 Codex 并在 Codex 插件目录中安装/启用。
- 不直接写 `~/.codex/plugins/cache`。
- 不直接修改 `~/.codex/config.toml` 的启用状态，除非后续有明确、稳定的实现和测试。

## 13. 前端交互

### 13.1 市场页

新增对象类型筛选：

```text
全部 / Skill / Plugin
```

Plugin 卡片显示：

- 名称、摘要、版本。
- 支持平台 Codex / Claude。
- 支持范围 user / project / local。
- 组件摘要。
- 风险等级。
- 安装状态。
- 同一 `namespace/plugin_id + target` 的 user/project 范围冲突状态。

### 13.2 Plugin 详情页

详情页必须展示：

- README 预览。
- 版本、changelog、GitLab commit SHA。
- 平台支持矩阵。
- 组件清单。
- 风险报告。
- 安装范围选择。
- 安装后生效提示。
- 更新策略。
- 若同一 plugin 已在同一平台的另一个范围启用，禁用安装按钮并提示先禁用或卸载冲突绑定。

对于 high risk plugin，安装按钮前必须要求二次确认。

### 13.3 本地页

本地页新增 Plugin 分组：

- 已缓存 plugin 包。
- 已注册 marketplace。
- 已安装 plugin。
- 已禁用 plugin。
- 加载错误或需要 reload 的 plugin。

## 14. 客户端安装流程

普通安装步骤：

1. 用户选择 plugin、版本、平台、scope。
2. 若 scope 是 project 或 local，用户只能选择已绑定项目。
3. 若 `enable=true`，后端检查同一 `namespace/plugin_id + target` 是否已有另一个 scope 启用。
4. user scope 和 project scope 在同一 target 内互斥；Codex 和 Claude 之间不互斥。
5. 第一阶段 project scope 之间也互斥，即同一 plugin 不允许在多个项目中同时启用。
6. 后端读取正式 plugin manifest。
7. 下载对应平台包。
8. 校验 SHA-256。
9. 解压到 `plugin-packages/`。
10. 校验平台原生 manifest。
11. 读取 `component-inventory.json` 和 `risk-report.json`。
12. 若 high risk，要求前端二次确认。
13. 物化本地 marketplace。
14. 优先调用平台 CLI 注册和安装。
15. 写入 `plugin_packages`、`plugin_marketplaces`、`plugin_bindings`。
16. 写入审计日志。
17. 前端提示是否需要重启或 reload。

失败处理：

- 下载失败删除临时文件。
- 校验失败删除包并记录安全错误。
- 解压失败删除临时目录。
- marketplace 写入失败回滚已写文件。
- 平台 CLI 安装失败保留缓存，标记 binding 为 `install_failed`。
- 范围冲突返回 `PLUGIN_SCOPE_CONFLICT`，前端提示用户先禁用或卸载冲突绑定。
- 数据库写入失败时标记为 `unknown`，下次扫描修复。

## 15. 更新和下架

更新流程：

1. 查询 `plugin_bindings`。
2. 从 `plugin-catalog.v1.json` 或 plugin manifest 查找 latestVersion。
3. 对比版本。
4. 排除 `update_policy = pinned`。
5. 下载新平台包。
6. 校验、解压、重新生成本地 marketplace。
7. 调用平台 CLI 更新或提示用户刷新 marketplace。
8. 更新绑定记录。
9. 提示 reload/restart。

下架流程：

1. 在正式 manifest 中标记 deprecated。
2. 从 `plugin-catalog.v1.json` 中移除或降权。
3. 保留历史版本包。
4. 已安装用户仍可卸载、禁用或回滚。
5. 不直接删除用户本地已安装 plugin。

## 16. 安全策略

必须执行：

- 下载后校验 SHA-256。
- 解压前检查路径逃逸。
- 解压后检查文件数量、总大小、单文件大小。
- 平台 manifest 路径必须相对 plugin 根目录。
- 禁止自动执行 plugin 内任意脚本。
- 禁止自动信任 hooks。
- high risk plugin 安装必须二次确认。
- 所有发布、安装、启用、禁用、卸载动作写入审计。
- secret、token、private key 不得进入 catalog、日志或前端持久状态。

建议执行：

- 官方源或企业源启用签名校验。
- 管理员可维护 blocklist。
- 管理员可按项目限制 high risk plugin。
- 对 `.mcp.json`、`.lsp.json`、`monitors/`、`bin/` 展示命令和参数。
- 发布器对可执行文件做扩展名和 hash 清单。

## 17. 错误码

新增错误码：

```text
PLUGIN_DRAFT_NOT_FOUND
PLUGIN_SOURCE_INVALID
PLUGIN_MANIFEST_INVALID
PLUGIN_TARGET_UNSUPPORTED
PLUGIN_PLATFORM_MANIFEST_MISSING
PLUGIN_COMPONENT_SCAN_FAILED
PLUGIN_RISK_REVIEW_REQUIRED
PLUGIN_PACKAGE_BUILD_FAILED
PLUGIN_PACKAGE_CHECKSUM_MISMATCH
PLUGIN_MARKETPLACE_WRITE_FAILED
PLUGIN_CLI_NOT_FOUND
PLUGIN_CLI_INSTALL_FAILED
PLUGIN_CLI_ENABLE_FAILED
PLUGIN_CLI_DISABLE_FAILED
PLUGIN_UNINSTALL_FAILED
PLUGIN_RELOAD_REQUIRED
PLUGIN_PUBLISH_OBJECT_EXISTS
PLUGIN_PUBLISH_CATALOG_UPDATE_FAILED
```

前端必须根据错误码显示稳定文案，不直接展示 Rust 内部错误。

## 18. 交付顺序

第一阶段，发布闭环：

1. 定义可选 `pluginhub.json` schema 与管理员发布元数据补齐规则。
2. 新增 plugin 草稿 MinIO 路径。
3. 新增 GitLab plugin 草稿同步模板。
4. 管理员页列出 plugin drafts。
5. 管理员页预览 plugin 源码和校验结果。
6. 发布生成 `package.codex.zip` 和 `package.claude.zip`。
7. 写正式 plugin manifest、catalog 和索引。

第二阶段，市场与安装：

1. 普通市场页支持 Skill / Plugin 筛选。
2. Plugin 详情页展示组件清单和风险报告。
3. 本地缓存 plugin 包。
4. Claude user scope 安装。
5. Codex user scope marketplace 物化。
6. 本地扫描已安装 plugin。

第三阶段，项目级与治理：

1. Claude project/local scope。
2. Codex project marketplace。
3. 更新中心支持 plugin。
4. 启用、禁用、卸载。
5. high risk 策略控制。
6. 签名校验。
7. 团队 marketplace allowlist/blocklist。

## 19. 测试要求

单元测试：

- `README.md` front matter 解析，以及缺失 `pluginhub.json` 时由 README 元数据与 `publish-meta.v1.json` 合成发布元数据。
- Codex manifest 路径校验。
- Claude manifest 路径校验。
- 组件扫描。
- 风险等级计算。
- MinIO object path 生成。
- plugin 草稿分类路径解析，一到多级分类都应可用。
- zip 路径逃逸检测。
- 同版本禁止覆盖。

集成测试：

- 从 MinIO 草稿区读取 plugin draft。
- 发布到正式 plugin 目录。
- 生成 `plugin-catalog.v1.json`。
- 下载并校验 `package.codex.zip`。
- 下载并校验 `package.claude.zip`。
- 物化 Codex 本地 marketplace。
- 物化 Claude 本地 marketplace。
- 平台 CLI 不存在时能进入手动安装提示状态。

前端测试：

- 市场对象类型筛选。
- Plugin 详情页组件清单。
- high risk 二次确认。
- 管理员草稿预览。
- 发布错误提示。
- 本地 plugin 状态展示。

## 20. 与现有 skill 逻辑的关系

保留：

- MinIO 作为唯一远程数据源。
- GitLab 只同步草稿。
- 客户端管理员模式负责正式发布。
- 本地 SQLite 管理缓存、绑定和审计。
- SHA-256 校验。
- 失败回滚。

新增：

- plugin 独立 catalog。
- plugin 独立 package 和 binding 表。
- plugin 平台原生 manifest 校验。
- plugin 本地 marketplace 物化。
- plugin 组件清单和风险报告。

不建议：

- 直接把 plugin 当 skill 复制到 `.codex/skills` 或 `.claude/skills`。
- 把 `.codex-plugin/plugin.json` 或 `.claude-plugin/plugin.json` 当成 Skill Hub 市场元数据。
- 让 GitLab CI 直接更新正式 catalog。
- 自动信任 hooks 或自动运行 bin。

## 21. 官方文档来源

- Claude Code 通过市场发现和安装插件：https://code.claude.com/docs/zh-CN/discover-plugins
- Claude Code 创建插件：https://code.claude.com/docs/zh-CN/plugins
- Claude Code plugin marketplace：https://code.claude.com/docs/zh-CN/plugin-marketplaces
- Codex build plugins：https://developers.openai.com/codex/plugins/build
