# Skill Hub 管理发布、市场分类与草稿区开发文档

## 1. 背景

当前 Skill Hub 通过客户端浏览市场、下载 skill、安装到 Codex / Claude 目录，并通过 `publish-skill.ps1` 将本地 skill 发布到 MinIO。

后续产品形态调整为：

- 市场增加两个一级页签：`公共` 和 `项目`。
- `公共` 页签复用现有市场页面；原先名为 `公共` 的分类改名为 `通用`。
- `项目` 页签展示各项目名，供各项目组沉淀和分发项目内 skill。
- skill 源码由 GitLab 维护，通过流水线推送到 MinIO 草稿前缀。
- GitLab 推送到草稿前缀的内容只包含 skill 原生内容；`SKILL.md` 里只有 `version` 和 `author` 是本方案依赖的必填字段。
- GitLab 不提供市场发布所需的完整元数据，例如 `namespace`、`skill_id`、`name`、`summary`、标签、目标环境和发布分类。
- 草稿发布所需的市场元数据由客户端管理员模式在草稿区补齐并写入 MinIO。
- 草稿区支持预览；正式市场 `skill.json`、package、sha256、manifest 和索引在管理员发布时生成。
- 管理员从草稿区发布 skill，发布时选择要进入市场的分类。
- 发布后草稿区中该 skill 显示为 `已发布`；只有 GitLab 再次同步更高 `version` 时，才重新进入待发布状态。
- 管理员通过客户端隐藏入口输入管理员密钥，并通过本机 MAC 地址白名单校验后进入管理模式；MAC 白名单同时声明管理员角色和项目授权范围。
- 管理员分为 `system` 系统管理员和 `project` 项目管理员：系统管理员可以治理公共市场和所有项目，项目管理员只能治理 MAC 白名单授权的项目。
- 普通用户只能浏览市场、安装 / 缓存 skill，不能新增项目、查看草稿区或发布 skill。

本版不引入额外常驻服务，也不单独开发管理端。GitLab 和客户端统一使用 `skill-market` bucket，系统依赖同一个客户端的管理员隐藏入口、MinIO 静态对象和 GitLab CI 完成草稿同步、预览和发布。

## 2. 设计结论

MinIO 的 `skill-market` bucket 是唯一共享存储：

- GitLab CI 写入草稿前缀。
- 客户端普通模式只读取正式市场前缀。
- 客户端管理员模式读取草稿前缀，写入正式市场前缀、项目配置、草稿状态和审计记录。
- MVP 阶段，管理员密钥和 MinIO 发布凭证写在 Rust 后端常量中；MAC 地址白名单放在 MinIO 管理配置对象中，使用明文 MAC，并在同一对象中维护 `role` 和项目授权范围，便于运维维护。

管理员模式启用流程：

- 客户端提供隐藏入口。
- 管理员输入管理员密钥。
- 客户端后端校验管理员密钥。
- 客户端后端使用写死的 MinIO 发布凭证读取 `admin/security/mac-allowlist.v1.json`。
- 客户端后端读取本机 MAC 地址，与 MinIO 白名单对象中的明文 MAC 比对。
- 管理员密钥和 MAC 白名单同时通过后，客户端才启用 MinIO 写能力，并把命中的角色返回给前端用于 UI 裁剪。
- 校验失败时不进入管理员模式。

写操作不能只依赖前端 UI 裁剪。每个后端管理员命令都必须重新执行管理员密钥、MAC 角色和目标范围校验。

隐藏入口只负责降低普通用户误触概率，不能单独作为权限边界。真正的写入边界由管理员密钥、MAC 白名单校验和 MinIO 凭证 / bucket policy 共同控制。

```text
GitLab CI
    |
    | 周期性写入 draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/...
    v
skill-market bucket

客户端普通模式
    |
    | 只读 catalog / indexes / skills / projects / categories
    v
skill-market bucket

客户端隐藏管理员入口
    |
    | 输入管理员密钥 + 校验 MinIO MAC 白名单
    v
客户端管理员模式
    |
    | 读取草稿，生成预览，发布正式市场对象
    v
skill-market bucket
```

客户端普通模式不得启用任何 MinIO 写能力。管理员发布能力只能在管理员模式解锁后启用，并使用单独的管理员发布凭证。

## 3. 目标

- 市场支持 `公共` / `项目` 两个一级页签。
- `公共` 页签保留现有市场体验，并将原 `公共` 分类改为 `通用`。
- `项目` 页签按项目名组织 skill，管理员可新增项目。
- GitLab 流水线可以把 skill 原生内容周期性推送到 `skill-market/draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/`。
- 客户端管理员模式从 `SKILL.md` 读取并校验 `version`、`author`。
- 客户端管理员模式负责补齐市场发布元数据，并在发布前校验这些元数据。
- 管理员可以查看草稿区、预览草稿、选择发布目标分类、执行发布。
- 正式市场产物在管理员发布时生成，而不是在 GitLab 同步时生成。
- 发布成功后，草稿状态记录的 `published_version` 与当前 `version` 一致，并显示为 `已发布`。
- GitLab 同步更高版本后，草稿重新显示为 `待发布`。
- 普通用户只能浏览市场和安装 skill，不能操作项目管理、草稿区和发布能力。
- 所有同步、发布和管理操作都有 MinIO 对象形式的审计记录。

## 4. 非目标

- 不引入额外常驻服务。
- 不使用中心数据库保存用户、草稿或发布状态。
- 不在客户端普通模式启用 MinIO 写能力。
- 不让普通用户直接更新 catalog、manifest、项目、分类或草稿状态。
- 不把 GitLab 流水线作为市场发布入口；流水线只负责写入草稿。
- 不提供普通用户创建或编辑草稿的入口。
- 不要求 GitLab 维护市场发布元数据；GitLab 只维护 skill 原生内容和 `version`、`author`。
- 不执行 skill 包内任意脚本。
- 不把隐藏入口本身作为权限边界；必须叠加管理员密钥、MAC 白名单和 MinIO 写凭证控制。

## 5. 角色与权限

管理员模式细分为两类：

```text
system   -- 系统管理员，位于项目管理员之上，可以治理公共市场、所有市场项目和所有 skill
project  -- 项目管理员，可以治理所有市场项目及项目 skill，但不能配置公共分类
```

权限边界必须以后端校验为准，前端只做展示裁剪。

| 功能 | 客户端普通模式 | 项目管理员 | 系统管理员 | GitLab CI |
| --- | --- | --- | --- | --- |
| 浏览公共市场 skill | 可以 | 可以 | 可以 | 不适用 |
| 浏览项目市场 skill | 可以 | 可以 | 可以 | 不适用 |
| 安装 / 缓存 skill | 可以 | 可以 | 可以 | 不适用 |
| 本地扫描 skill | 可以 | 可以 | 可以 | 不适用 |
| 新增 / 编辑 / 删除市场项目 | 不可以 | 可以，所有项目 | 可以，所有项目 | 不适用 |
| 新增 / 编辑 / 删除公共分类 | 不可以 | 不可以 | 可以 | 不适用 |
| 查看草稿区 | 不可以 | 可以 | 可以 | 不适用 |
| 查看草稿详情 | 不可以 | 可以 | 可以 | 不适用 |
| 生成草稿预览 | 不可以 | 可以 | 可以 | 不适用 |
| 编辑草稿发布目标 | 不可以 | 可以选择任意项目 | 可以选择公共分类或任意项目 | 不适用 |
| 发布 skill 到公共市场 | 不可以 | 不可以 | 可以 | 不可以 |
| 发布 skill 到项目市场 | 不可以 | 可以发布到任意项目 | 可以发布到任意项目 | 不可以 |
| 下架公共市场 skill | 不可以 | 不可以 | 可以 | 不适用 |
| 下架项目市场 skill | 不可以 | 可以下架任意项目 skill | 可以下架任意项目 skill | 不适用 |
| 写入 MinIO 草稿前缀 | 不可以 | 不可以 | 不可以 | 可以 |
| 写入 MinIO 正式市场前缀 | 不可以 | 可以，受后端角色限制 | 可以 | 不可以 |

MinIO 凭证建议拆为三类：

```text
market-reader       -- 客户端普通模式，只读 skill-market 正式市场对象
draft-writer        -- GitLab CI，只写 skill-market/draft/gitlab/skills/... 前缀
market-publisher    -- 客户端管理员模式，读草稿并写市场、项目、状态、审计对象
```

`market-publisher` 不应在普通模式下可用。MVP 阶段采用维护成本最低的方式：

- `src-tauri/src/admin_config.rs` 写死管理员密钥。
- `src-tauri/src/admin_config.rs` 写死 `market-publisher` 的 MinIO Access Key / Secret Key。
- MAC 白名单放在 MinIO 的 `admin/security/mac-allowlist.v1.json`，使用明文 MAC，并在条目中维护 `role`；`projects` 字段仅作为兼容旧配置的可选元数据，不再限制项目管理员范围。
- 前端只传管理员密钥；Access Key / Secret Key 不返回前端，不进入普通 UI 状态。

该方式适合内网 MVP 和小团队维护；如果客户端需要外发、多人轮换凭证或面向不可信终端，应改为安全存储 / 加密凭证对象，并收紧 MinIO policy。

## 6. 推荐架构

### 6.1 客户端普通模式

当前 Tauri 客户端继续保留本地能力：

- 市场浏览。
- skill 安装 / 缓存。
- 本地扫描。
- 项目绑定。
- 更新中心。
- 本地设置。

市场页面新增：

- `公共` 页签：复用现有市场页面。
- `公共 -> 通用` 分类：由原 `公共` 分类重命名而来。
- `项目` 页签：展示项目列表和项目下 skill。

客户端普通模式只读取正式市场对象，不读取草稿前缀，不启用写能力。

### 6.2 客户端管理员模式

管理员能力通过客户端隐藏入口启用。管理员入口负责：

- 接收管理员密钥。
- 在 Rust 后端校验管理员密钥是否等于代码常量。
- 使用写死的 MinIO 发布凭证读取 `admin/security/mac-allowlist.v1.json`。
- 在 Rust 后端读取本机 MAC 地址，校验是否命中 MinIO 白名单对象中的明文 MAC。
- 通过后启用管理员模式和 MinIO 写能力。
- 失败时不进入管理员模式。

管理员模式负责：

- 查看草稿区。
- 从 MinIO 草稿前缀读取 GitLab 来源、版本、提交记录和校验结果。
- 预览草稿内容和发布后市场卡片效果。
- 新增、编辑、归档市场项目。
- 选择发布目标分类。
- 发布 skill。
- 写入发布任务记录和审计记录。

客户端管理员模式直接使用 MinIO S3 兼容接口读写对象。MVP 阶段管理员发布凭证允许写死在 Rust 后端代码中，但不得返回前端、不得打印到日志，也不得在普通模式下调用写能力。

### 6.3 GitLab CI

GitLab CI 负责：

- 校验 `SKILL.md` 中存在 `version` 和 `author`。
- 将 skill 原生内容周期性同步到 `skill-market/draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/`。
- 可选生成 `validation.json`。

GitLab CI 不能写入正式市场前缀、catalog、manifest、项目配置或分类配置。

## 7. 市场分类模型

市场一级页签固定为：

```text
公共
项目
```

`公共` 页签使用公共分类：

```text
通用        -- 原公共分类重命名
其他公共分类 -- 继续沿用现有分类配置
```

`项目` 页签按项目名展示：

```text
项目
  - 项目 A
  - 项目 B
  - 项目 C
```

发布目标由管理员选择：

```text
公共 / 通用
公共 / {其他公共分类}
项目 / {项目名}
```

MVP 阶段项目名本身就是项目页签下的发布分类。后续如果项目内需要二级分类，可以在项目下增加项目级分类。

## 8. MinIO 对象结构

### 8.1 草稿对象

草稿对象使用独立前缀，不直接进入市场 catalog。GitLab CI 周期性写入 `skill-market/draft/gitlab/skills/{gitlab_source_path}/`。当前约定中，`{gitlab_source_path}` 通常为 `{gitlab_category_code}/{draft_slug}`，其中 `{gitlab_category_code}` 是 GitLab 侧的分类编码，`{draft_slug}` 是 GitLab 侧同步过来的 skill 目录名，例如 `xxx-skill`。

GitLab 草稿前缀只保存 skill 原生内容。客户端管理员模式补齐的市场发布元数据、发布状态和预览缓存写到独立的 `draft/admin/gitlab/skills/{gitlab_source_path}/` 前缀，避免被 GitLab 周期性同步覆盖。

`gitlab_category_code` 只表示 GitLab 侧的来源分类，和 Skill Hub 市场里的公共分类、项目分类不要求对齐，也不能自动当作发布目标。发布目标仍由客户端管理员模式在 `publish-meta.v1.json` 中选择。

示例路径：

```text
/skill-market/draft/gitlab/skills/{gitlab_category_code}/xxx-skill/SKILL.md
```

```text
draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/SKILL.md
draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/...                         -- skill 原生文件
draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/validation.json             -- 可选，CI 校验报告

draft/admin/gitlab/skills/{gitlab_category_code}/{draft_slug}/publish-meta.v1.json   -- 客户端管理员模式补齐
draft/admin/gitlab/skills/{gitlab_category_code}/{draft_slug}/state.v1.json          -- 管理员发布后写入
draft/admin/gitlab/skills/{gitlab_category_code}/{draft_slug}/preview/               -- 草稿区预览产物，可由客户端管理员模式生成
```

`gitlab_source_path` 是 `draft/gitlab/skills/` 下到 `SKILL.md` 所在目录的相对路径。客户端管理员模式写入 `draft/admin/gitlab/skills/{gitlab_source_path}/...`，确保 GitLab 来源分类编码不会和 Skill Hub 市场分类混用。

如果 GitLab 实际把分类编码和 skill 目录拼在同一个路径段中，例如 `draft/gitlab/skills/{gitlab_category_code}xxx-skill/SKILL.md`，客户端仍按 `skills/` 下的相对目录整体作为 `gitlab_source_path`，并在草稿列表中展示原始来源路径。

`SKILL.md` 至少包含：

```text
version
author
```

`publish-meta.v1.json` 由客户端管理员模式创建和维护，它就是 GitLab 草稿中缺失、需要客户端上传 / 保存的市场发布元数据。至少包含：

```text
namespace
skill_id
name
summary
tags
targets
levels
publish_scope          -- public / project
publish_category_slug  -- publish_scope = public 时使用
publish_project_slug   -- publish_scope = project 时使用
changelog
updated_at
updated_by
```

GitLab 周期性推送新内容时，`publish-meta.v1.json` 默认复用。若 `SKILL.md` 中的 `version` 发生变化，客户端管理员模式必须提示管理员重新确认市场发布元数据和预览结果。

`state.v1.json` 至少包含：

```text
gitlab_source_path
draft_slug            -- 可解析时填写
gitlab_category_code  -- 可解析时填写，仅表示 GitLab 来源分类
namespace
skill_id
published_version
published_skill_md_etag
published_skill_md_last_modified
published_source_fingerprint -- source 对象列表 + ETag 的摘要
published_at
published_by
publish_scope          -- public / project
publish_category_slug
publish_project_slug
status                 -- published / archived
last_publish_job_id
updated_at
```

草稿区状态由客户端管理员模式动态计算：

- 存在 `draft/gitlab/skills/{gitlab_source_path}/SKILL.md` 且无 `publish-meta.v1.json`：显示 `元数据待补充`。
- `publish-meta.v1.json` 缺少必要字段：显示 `元数据待补充`。
- `publish-meta.v1.json` 完整且无 `state.v1.json`：显示 `待发布`。
- `version` 高于 `published_version` 且 `publish-meta.v1.json` 完整：显示 `待发布`。
- `version` 等于 `published_version`：显示 `已发布`。
- `version` 低于 `published_version`：显示 `版本回退风险`，禁止发布。
- `validation.json` 失败或必要字段缺失：显示 `校验失败`。

`version` 建议使用 SemVer。若不使用 SemVer，至少不允许覆盖已发布版本号。

### 8.2 市场对象

发布成功后写入正式市场结构：

```text
skills/{namespace}/{skill_id}/versions/{version}/skill.json
skills/{namespace}/{skill_id}/versions/{version}/package.zip
skills/{namespace}/{skill_id}/versions/{version}/package.sha256
skills/{namespace}/{skill_id}/versions/{version}/changelog.md
skills/{namespace}/{skill_id}/manifest.json
catalog.v1.json
indexes/market/public/{category_slug}.v1.json
indexes/market/projects/{project_slug}.v1.json
indexes/search-lite.v1.json
categories.v1.json
projects.v1.json
```

发布时必须最后写入 `catalog.v1.json`，避免客户端普通模式读到不完整版本。

### 8.3 管理对象

客户端管理员模式写入管理记录：

```text
admin/publish-jobs/{job_id}.json
admin/audit/{yyyy}/{mm}/{dd}/{timestamp}-{action}.json
admin/security/mac-allowlist.v1.json
admin/backups/catalog/{timestamp}.catalog.v1.json
admin/backups/projects/{timestamp}.projects.v1.json
admin/backups/categories/{timestamp}.categories.v1.json
```

这些对象不对客户端普通模式开放。

`admin/security/mac-allowlist.v1.json` 使用明文 MAC，MVP 推荐格式：

```json
{
  "version": 1,
  "entries": [
    {
      "mac": "<SYSTEM_ADMIN_MAC>",
      "status": "active",
      "role": "system",
      "name": "ops-admin"
    },
    {
      "mac": "<PROJECT_ADMIN_MAC>",
      "status": "active",
      "role": "project",
      "name": "project-admin"
    }
  ]
}
```

为了兼容早期 MVP，也可以继续使用简单格式：

```json
{
  "version": 1,
  "macs": [
    "<SYSTEM_ADMIN_MAC>"
  ]
}
```

兼容规则：

- `entries[].status` 缺省按 `active` 处理，非 `active` 不允许进入管理员模式。
- `entries[].role` 仅支持 `system` 和 `project`，缺省按 `project` 处理。
- `entries[].projects` 为兼容旧配置保留，可省略；项目管理员默认可管理所有市场项目。
- 旧格式 `macs[]` 等价于 `role = system`、`projects = ["*"]`，避免已有内网部署升级后立即失效。
- 后端校验时会先把本机 MAC 和配置 MAC 规范化为统一格式再比对。
- 解锁成功后返回角色给前端；后端每次写操作仍必须重新读取 allowlist 并校验角色。

后端权限判断：

```text
can_manage_public(role) = role == system
can_manage_project(role, slug) = role == system || role == project
```

项目管理员没有公共分类、公共 skill 的增删和下架权限。

## 9. 市场配置对象

### 9.1 projects.v1.json

`projects.v1.json` 用于驱动市场 `项目` 页签。

```text
projects[]
  name
  slug
  description
  created_at
  updated_at
  updated_by
```

只有客户端管理员模式可以新增、编辑和归档项目。

### 9.2 categories.v1.json

公共分类中必须存在内置分类：

```text
scope = public
slug = general
name = 通用
built_in = true
```

该分类由原 `公共` 分类迁移而来。项目页签的项目列表由 `projects.v1.json` 驱动。

## 10. GitLab 草稿同步流程

```text
1. 项目组在 GitLab 中维护 skill 源码。
2. 项目组修改 SKILL.md，并在其中维护 version 和 author 字段。
3. GitLab pipeline 校验 version 和 author。
4. GitLab pipeline 周期性将 skill 原生内容同步到 draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/。
5. GitLab pipeline 可选写入 validation.json。
6. 客户端管理员模式递归扫描 draft/gitlab/skills/**/SKILL.md，草稿区出现或刷新草稿。
7. 客户端管理员模式从 SKILL.md 读取 version 和 author。
8. 客户端管理员模式记录 gitlab_source_path；如可解析 gitlab_category_code，则仅作为来源信息展示，不自动映射为市场分类。
9. 如果缺少 publish-meta.v1.json，草稿显示为元数据待补充。
10. 管理员在草稿区补齐市场发布元数据。
11. 如果 version 高于 published_version，草稿显示为待发布。
12. 如果 version 等于 published_version，草稿显示为已发布。
```

GitLab 同步不等于发布。同步只让 `draft/gitlab/skills/{gitlab_source_path}/` 中的 skill 原生内容刷新，是否进入市场、进入哪个分类、市场卡片如何展示，都由客户端管理员模式补齐元数据后决定。

## 11. 草稿预览

草稿区必须支持管理员预览。预览用于确认 GitLab 同步来的 skill 原生内容，以及管理员补齐的市场发布元数据在市场中的展示效果，但不写入正式市场 catalog。

预览内容包括：

- 市场卡片预览：名称、摘要、作者、版本、标签、目标环境。
- `SKILL.md` 内容预览。
- 文件清单预览。
- 发布目标预览：公共分类或项目。
- 校验报告预览。

预览生成规则：

- 客户端管理员模式从 `draft/gitlab/skills/{gitlab_source_path}/` 读取 `SKILL.md` 和必要文件，并从 `draft/admin/gitlab/skills/{gitlab_source_path}/` 读取 `publish-meta.v1.json`。
- 缺少 `publish-meta.v1.json` 时，只能预览 `SKILL.md` 和文件清单，并提示补齐市场元数据。
- 客户端管理员模式可以生成本地临时预览，也可以写入 `draft/admin/gitlab/skills/{gitlab_source_path}/preview/` 作为缓存。
- 预览不得生成或更新正式 `skill.json`、package、manifest、catalog 或索引。
- 如果草稿内容、`version` 或 `publish-meta.v1.json` 变化，预览缓存必须失效或重新生成。

## 12. 发布流程

```text
1. 管理员进入草稿区。
2. 管理员查看 GitLab 来源、当前 version、published_version、作者和校验报告。
3. 管理员补齐或确认 publish-meta.v1.json：
   - namespace
   - skill_id
   - name
   - summary
   - tags
   - targets
   - levels
   - changelog
4. 管理员预览草稿内容和市场展示效果。
5. 管理员选择发布目标：
   - 公共 / 通用
   - 公共 / {其他公共分类}
   - 项目 / {项目名}
6. 管理员点击发布。
7. 客户端管理员模式创建 admin/publish-jobs/{job_id}.json。
8. 客户端管理员模式从 `draft/gitlab/skills/{gitlab_source_path}/` 读取 skill 原生内容和 SKILL.md，并从 `draft/admin/gitlab/skills/{gitlab_source_path}/` 读取 publish-meta.v1.json。
9. 客户端管理员模式再次校验草稿：
   - 必须包含 SKILL.md
   - version 必须等于本次发布版本
   - author 必须存在
   - publish-meta.v1.json 必须存在
   - namespace / skill_id / name / summary 合法
   - 发布目标存在
   - package 内不得包含禁止文件
   - 不执行任意脚本
10. 客户端管理员模式生成正式 skill.json、package.zip、package.sha256、changelog.md。
11. 上传版本文件到 MinIO 正式市场前缀。
12. 更新 skill manifest。
13. 更新公共分类索引或项目索引。
14. 更新 search-lite。
15. 最后更新 catalog.v1.json。
16. 更新 draft/admin/gitlab/skills/{gitlab_source_path}/state.v1.json。
17. 写入 admin/audit/...。
18. 客户端普通模式刷新市场。
```

发布操作需要支持幂等。若发布任务中断，重新执行时应能识别已经上传的对象，并继续或安全失败。

GitLab 会不定期推送同一个草稿目录。发布时客户端管理员模式必须记录 `SKILL.md` 和关键源码对象的 ETag / Last-Modified，并在写入正式市场前复查；如果草稿内容在发布过程中变化，必须中止发布并要求管理员重新预览。

MVP 可以限制同一时间只有一个管理员执行发布。后续可增加对象锁、发布锁文件或基于对象版本号的条件写入，降低并发更新 catalog / projects / categories 的冲突风险。

## 12.1 下架流程

下架不是删除 GitLab 草稿，也不是删除历史版本包。下架只让 skill 从正式市场和索引中不可见，同时把来源草稿重新放回草稿区，便于管理员修正元数据后再次发布。

```text
1. 管理员在管理页选择正式市场中的 skill。
2. 后端读取 catalog.v1.json，定位 namespace / skill_id。
3. 后端根据 skill 当前 categories 判断权限：
   - category = project:{slug} 时，项目管理员必须被授权该 slug；系统管理员总是允许。
   - category = general 或其他公共分类时，仅系统管理员允许。
4. 后端从 catalog.v1.json 移除该 skill。
5. 后端重建 search-lite 和受影响的公共分类索引或项目索引。
6. 后端保留 skills/{namespace}/{skill_id}/manifest.json 和 versions 历史包，避免破坏已安装用户回滚或审计。
7. 如果能在 draft/admin/gitlab/skills/**/state.v1.json 中找到对应 namespace / skill_id，则把该 state 标记为 archived，并保留 gitlab_source_path。
8. 如果找不到原草稿 state，后端创建一个最小 archived state，记录 namespace / skill_id、下架人、下架原因和时间，草稿区显示为已下架待处理。
9. 写入 admin/audit/...。
10. 客户端刷新市场。
```

下架后的重新上架路径：

- 管理员在草稿区选择该草稿。
- 确认或修改 `publish-meta.v1.json`。
- 项目管理员可以选择任意项目。
- 系统管理员可以选择公共分类或任意项目。
- 点击发布后，skill 重新进入正式市场或项目分类。

MVP 可以先不物理删除版本对象；后续如需彻底删除，应单独设计回收站和保留期。

## 13. 前端页面规划

### 13.1 市场页

页面一级页签：

```text
公共 | 项目
```

`公共` 页签：

- 复用现有市场页面。
- 原 `公共` 分类显示为 `通用`。
- 继续支持搜索、分类筛选、skill 详情、安装 / 缓存。

`项目` 页签：

- 展示项目列表。
- 点击项目后展示该项目下发布的 skill。
- 普通用户可以浏览和安装项目 skill。
- 客户端管理员模式可见 `新增项目`、`编辑项目`、`归档项目` 操作。

### 13.2 草稿区

草稿区仅客户端管理员模式可见。

页面组成：

- 草稿列表。
- 状态筛选：元数据待补充 / 待发布 / 已发布 / 校验失败 / 已归档 / 版本回退风险。
- GitLab 项目、分支、路径、commit 信息。
- 当前 `version`。
- `published_version`。
- `author`。
- 市场发布元数据编辑区：namespace、skill_id、名称、摘要、标签、目标环境、级别、变更说明。
- 发布目标分类。
- 预览面板。
- 校验报告。
- 发布任务状态。
- 审计记录入口。

管理员可见按钮：

- 运行校验。
- 保存市场发布元数据。
- 生成 / 刷新预览。
- 选择发布目标。
- 发布到市场。
- 下架后重新发布。

客户端普通模式不显示草稿区入口，也没有读取草稿前缀的 MinIO 权限。

### 13.3 管理入口

管理员入口采用隐藏入口方式：

- 普通界面不显示管理入口。
- 通过约定操作打开管理员入口。
- 输入管理员密钥。
- 后端校验管理员密钥。
- 后端从 MinIO 读取 `admin/security/mac-allowlist.v1.json`。
- 后端读取本机 MAC 地址并校验 MinIO 对象中的明文白名单。
- 校验通过后切换为客户端管理员模式。
- 校验失败则保持普通模式。

管理页采用不拥挤的工作台布局，避免把项目、草稿、发布表单挤在同一列：

- 顶部权限条：显示当前角色、MinIO bucket、刷新按钮。
- 左侧工作区导航：项目治理、草稿发布、市场下架、审计记录。
- 项目治理：系统管理员可新增 / 编辑 / 删除所有项目并配置公共分类；项目管理员可新增 / 编辑 / 删除所有项目，但不显示公共分类管理。
- 草稿发布：左侧草稿列表，中间元数据表单，右侧发布目标和预览操作。
- 市场下架：列出当前正式市场 skill，按公共 / 项目过滤，显示可操作范围；不可操作项置灰并提示所需角色。
- 审计记录：MVP 可先展示最近操作入口，后续读取 `admin/audit/...`。

管理员入口密钥只在弹窗中输入。进入管理页后不再重复显示管理员密钥输入框，避免误以为可以在页面内切换权限。

管理员入口不能只依赖隐藏入口，必须同时通过管理员密钥、MAC 白名单校验，并启用独立的 MinIO 写凭证。

## 14. 安全要求

- 客户端普通模式不得启用任何 MinIO 写凭证。
- GitLab CI 只能持有受限的 MinIO 草稿前缀写权限。
- 管理员发布凭证只能写正式市场、项目配置、草稿状态和管理记录前缀。
- GitLab CI 不得写 catalog、manifest、projects、categories 或正式 skill 版本对象。
- MVP 阶段管理员发布凭证写死在 Rust 后端代码中，但不得返回前端、不得打印日志、不得在普通模式下调用写能力。
- 进入管理员模式必须输入管理员密钥。
- 管理员密钥和 MAC 白名单校验通过前不得执行写操作。
- 进入管理员模式必须校验本机 MAC 地址命中 MinIO `admin/security/mac-allowlist.v1.json` 白名单。
- MVP 阶段 MAC 地址白名单可以明文存放在 MinIO 中，便于维护。
- MAC 地址只能由客户端本地校验，MinIO 无法从 S3 请求中可靠感知客户端 MAC；它是附加限制，不能替代管理员密钥和 MinIO bucket policy。
- GitLab 推送到 MinIO 草稿前缀的对象必须限制大小。
- 管理员预览和发布时如需解压，必须解压到隔离临时目录，完成后删除临时文件。
- 防止 zip slip 路径穿越。
- 不执行 skill 包内脚本。
- 发布前必须校验 package 内容。
- 本地日志不得打印 access key、secret key 或 MinIO 对象临时签名。
- 所有同步和发布操作必须写审计对象。

## 15. 与现有脚本的关系

`publish-skill.ps1` 作为现有发布流程的参考实现，后续应把核心逻辑迁移到客户端管理员模式：

- 从 `draft/gitlab/skills/{gitlab_source_path}/` 读取 skill 原生内容和 `SKILL.md`。
- 从 `draft/admin/gitlab/skills/{gitlab_source_path}/` 读取 `publish-meta.v1.json`。
- 读取和生成 `skill.json`。
- 校验 `SKILL.md`。
- 校验 `version`。
- 校验 `author`。
- 校验客户端管理员模式补齐的市场发布元数据。
- 校验发布目标分类。
- 过滤不应进入 package 的 JSON 文件。
- 生成草稿预览。
- 生成 package zip。
- 计算 sha256。
- 上传版本文件。
- 更新 manifest。
- 更新 catalog。
- 更新公共分类索引。
- 更新项目索引。
- 更新 search-lite。
- 更新草稿状态和审计对象。

迁移完成后，日常发布不再使用脚本。脚本可以保留为运维应急工具，但不作为产品发布入口。

## 16. 分阶段实施

### 阶段 1：MinIO 权限和对象契约

- 定义 `market-reader`、`draft-writer`、`market-publisher` 三类凭证。
- 配置 bucket policy。
- 定义 `publish-meta.v1.json`、`state.v1.json`、`projects.v1.json`、`categories.v1.json`。
- 客户端接入隐藏管理员入口。
- 客户端管理员模式接入代码常量中的 MinIO 发布凭证。
- 在 Rust 后端配置管理员密钥，并在 MinIO 中维护明文 MAC 白名单。

验收标准：

- 客户端普通模式只能读取正式市场对象。
- GitLab CI 只能写草稿前缀。
- 输入管理员密钥且本机 MAC 命中白名单后，客户端进入管理员模式。
- 客户端管理员模式可以读取草稿并写正式市场对象。
- 错误凭证访问越权前缀会失败。

### 阶段 2：市场页签和项目分类

- 市场新增 `公共` / `项目` 两个页签。
- 将原 `公共` 分类迁移为 `通用`。
- 新增 `projects.v1.json`。
- 新增项目列表页面。
- 新增管理员创建项目入口。

验收标准：

- 普通用户可以浏览 `公共` 和 `项目` 页签。
- `公共` 页签中原分类显示为 `通用`。
- 管理员可以新增项目。
- 客户端普通模式不能新增、编辑或归档项目。

### 阶段 3：GitLab 同步草稿区

- GitLab pipeline 将 skill 原生内容周期性推送到 `draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/`。
- 客户端管理员模式读取草稿前缀并生成草稿列表。
- 新增 `SKILL.md` version 和 author 校验。
- 新增缺少 `publish-meta.v1.json` 时的 `元数据待补充` 状态。

验收标准：

- GitLab pipeline 可以将 skill 推送到 MinIO 草稿前缀。
- `SKILL.md` 缺少 `version` 时同步失败。
- `SKILL.md` 缺少 `author` 时同步失败。
- GitLab 同步后如未补齐市场发布元数据，草稿显示为元数据待补充。
- 市场发布元数据补齐后，新版本草稿显示为待发布。
- 已发布版本重复同步时草稿仍显示为已发布。
- 客户端普通模式不能读取草稿区。

### 阶段 4：管理员预览和发布

- 新增草稿预览。
- 新增市场发布元数据编辑和保存。
- 新增发布目标选择。
- 新增发布任务记录。
- 新增发布日志。
- 发布完成后刷新市场 catalog、分类索引和项目索引。

验收标准：

- 管理员可以从草稿区发布 skill 到 `公共 / 通用`。
- 管理员可以从草稿区发布 skill 到指定项目。
- 管理员发布前必须补齐 `namespace`、`skill_id`、`name`、`summary` 等市场发布元数据。
- 管理员发布前可以预览市场卡片、`SKILL.md` 和文件清单。
- 客户端普通模式不能调用或执行发布能力。
- 发布成功后草稿显示为已发布。
- 发布成功后客户端普通模式刷新市场能看到对应版本。
- 发布过程有审计对象。

### 阶段 5：治理和完善

- 管理员凭证轮换。
- MAC 白名单维护。
- GitLab project 与草稿前缀绑定。
- 草稿版本历史。
- 发布失败重试。
- 下架 / 恢复。
- 通知。
- 搜索优化。
- 并发发布锁或对象版本条件写入。

## 17. 关键风险

- 如果客户端普通模式可以启用 MinIO 写凭证，会导致普通用户获得发布权限。
- 如果 GitLab CI 可以写正式市场对象，会绕过管理员发布和审计。
- 如果管理员发布凭证泄露，市场对象可能被篡改。
- 如果只依赖隐藏入口或 MAC 白名单，而不校验管理员密钥和 MinIO 权限，容易被绕过。
- 如果 MAC 白名单明文存储，会暴露管理员设备信息。
- 如果 `version` 不强制校验，草稿区无法可靠判断待发布 / 已发布状态。
- 如果客户端管理员模式没有补齐市场发布元数据，草稿只能预览源码内容，不能发布。
- 如果市场发布元数据不完整，草稿预览和市场展示会不稳定。
- 如果预览生成逻辑和发布生成逻辑不一致，管理员看到的效果可能与发布结果不同。
- 如果允许版本回退覆盖，客户端可能安装到错误版本。
- 如果 catalog 不是最后更新，客户端可能读到半发布状态。
- 如果多个管理员同时发布且没有并发保护，catalog、projects 或 categories 可能互相覆盖。
- 没有中心审计库后，审计对象必须保证不可被客户端普通模式读取或修改。

## 18. 推荐优先级

优先级从高到低：

1. MinIO 权限和对象契约。
2. 市场 `公共` / `项目` 页签。
3. 项目管理和 `通用` 分类迁移。
4. GitLab 推送 MinIO 草稿前缀。
5. `SKILL.md` version 和 author 校验。
6. 市场发布元数据补齐。
7. 草稿预览。
8. 管理员发布到公共分类或项目分类。
9. 审计、凭证轮换、MAC 白名单维护和并发保护。

不要先做“隐藏入口 + 客户端普通模式可直接使用写凭证”。隐藏入口只是入口，管理员密钥、MAC 白名单和 MinIO bucket policy 必须同时落地。
