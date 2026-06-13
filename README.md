# Skill Hub

Skill Hub 是一个 Tauri v1 + Rust + React 的桌面 skill 市场客户端。远程市场数据来自 MinIO 对象存储，本地使用 SQLite 记录缓存、项目、安装状态和 skill 生效绑定。

当前代码默认连接：

```text
MinIO endpoint: http://192.168.1.4:9000
MinIO bucket:   skill-market
mc alias:       myminio
```

固定源配置在 `src-tauri/src/db.rs`：

```rust
COMPILED_SOURCE_ENDPOINT = "http://192.168.1.4:9000"
COMPILED_SOURCE_BUCKET = "skill-market"
```

管理员配置在 `src-tauri/src/admin_config.rs`：

```rust
ADMIN_KEY = "skillhub-admin"
MINIO_PUBLISHER_ACCESS_KEY = "minioadmin"
MINIO_PUBLISHER_SECRET_KEY = "minioadmin"
MAC_ALLOWLIST_OBJECT_PATH = "admin/security/mac-allowlist.v1.json"
```

## 当前能力

- 浏览公共市场 skill，支持搜索、分类筛选、详情和预览。
- 市场有 `公共` / `项目` 两个页签，项目市场由 `projects.v1.json` 驱动。
- 下载 skill 到本地缓存，并安装 / 启用到 Codex 或 Claude。
- 支持个人级和项目级作用域，同一 skill 在同一平台上个人级和项目级互斥。
- 支持本地缓存查看、预览、删除，以及本地已有 skill 扫描。
- 管理员模式支持系统管理员 / 项目管理员两种角色、项目维护、公共分类维护、草稿列表、草稿预览、发布元数据维护、发布和 skill 下架。
- 管理员发布会生成 `skill.json`、`package.zip`、`package.sha256`、`manifest.json`、`catalog.v1.json`、分类 / 项目索引、草稿状态、publish job 和审计对象。

## 管理员入口

管理员入口是隐藏入口，普通侧边栏默认不显示“管理”。

进入方式：

1. 启动客户端。
2. 点击左上角 `Skill Hub` 品牌区域。
3. 先弹出管理员验证窗口。
4. 输入管理员密钥：`skillhub-admin`。
5. 点击 `验证并进入`。
6. 验证通过后，侧边栏才会出现 `管理` 菜单，并自动进入管理页面。

解锁成功需要同时满足：

- 管理员密钥等于 `src-tauri/src/admin_config.rs` 中的 `ADMIN_KEY`。
- MinIO 中存在 `admin/security/mac-allowlist.v1.json`。
- 当前机器 MAC 地址命中白名单，并带有允许的管理员角色。
- 后端能使用代码中写死的 MinIO Access Key / Secret Key 访问 `skill-market`。

MAC 白名单示例：

```json
{
  "entries": [
    {
      "mac": "C8-7F-54-5C-60-D8",
      "status": "active",
      "role": "system",
      "projects": ["*"],
      "name": "ops-admin"
    },
    {
      "mac": "11:22:33:44:55:66",
      "status": "active",
      "role": "project",
      "projects": ["live-project"],
      "name": "live-project-admin"
    }
  ]
}
```

也兼容早期简单写法，简单写法等价于系统管理员：

```json
{
  "macs": [
    "C8-7F-54-5C-60-D8"
  ]
}
```

后端会把 `AA-BB-CC-DD-EE-FF`、`AA:BB:CC:DD:EE:FF`、`AABB.CCDD.EEFF` 规范化后再比较。

角色说明：

- `role = system`：系统管理员，可管理公共分类、所有市场项目、公共 skill 和项目 skill 的发布 / 下架。
- `role = project`：项目管理员，只能管理 `projects` 中列出的项目，以及这些项目下 skill 的发布 / 下架。
- 项目管理员不能发布到公共分类，也不能下架公共市场 skill。

## MinIO 初始化

本机如果 `mc` 不在 PATH，可以直接使用当前测试过的路径：

```powershell
$mc = "D:\tmp\skillhub-minio\mc.exe"
```

配置 alias：

```powershell
& $mc alias set myminio http://192.168.1.4:9000 minioadmin minioadmin
```

创建 bucket：

```powershell
& $mc mb --ignore-existing myminio/skill-market
& $mc anonymous set download myminio/skill-market
```

写入管理员 MAC 白名单：

```powershell
@'
{
  "entries": [
    {
      "mac": "C8-7F-54-5C-60-D8",
      "status": "active",
      "role": "system",
      "projects": ["*"],
      "name": "ops-admin"
    }
  ]
}
'@ | Set-Content -Encoding UTF8 .\mac-allowlist.v1.json

& $mc cp .\mac-allowlist.v1.json myminio/skill-market/admin/security/mac-allowlist.v1.json
```

查看当前机器 MAC：

```powershell
getmac /FO CSV /NH
```

写入示例市场数据：

```powershell
$env:MC_CONFIG_DIR = "$env:USERPROFILE\mc"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\seed-minio.ps1 `
  -McPath D:\tmp\skillhub-minio\mc.exe `
  -Alias myminio `
  -Bucket skill-market
```

检查对象：

```powershell
& $mc ls myminio/skill-market
& $mc cat myminio/skill-market/catalog.v1.json
```

## GitLab 草稿同步

GitLab 只负责把 skill 原生内容同步到草稿前缀，不负责发布正式市场。

草稿路径约定：

```text
draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/SKILL.md
draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/...
draft/gitlab/skills/{gitlab_category_code}/{draft_slug}/validation.json
```

仓库提供模板：

```text
docs/gitlab-draft-sync-template.yml
```

使用方式：

1. 复制到 skill 源码仓库，命名为 `.gitlab-ci.yml`。
2. 配置 GitLab CI 变量：
   - `MINIO_ENDPOINT`
   - `MINIO_ACCESS_KEY`
   - `MINIO_SECRET_KEY`
   - `GITLAB_CATEGORY_CODE`
   - `DRAFT_SLUG`
   - 可选：`SKILL_DIR`
   - 可选：`SKILL_MARKET_BUCKET`
3. pipeline 会校验 `SKILL.md` 中的 `version` 和 `author`。
4. pipeline 只写 `draft/gitlab/skills/...`，不会写 catalog、manifest、projects 或正式版本对象。

手工创建一个草稿示例：

```powershell
New-Item -ItemType Directory -Force .\tmp-draft | Out-Null

@'
---
name: MinIO Live Draft
version: 0.1.0
author: Skill Hub Test
---

# MinIO Live Draft
'@ | Set-Content -Encoding UTF8 .\tmp-draft\SKILL.md

@'
{
  "schema": "skillhub.draft-validation.v1",
  "status": "passed",
  "commitSha": "local-test"
}
'@ | Set-Content -Encoding UTF8 .\tmp-draft\validation.json

& $mc mirror --overwrite .\tmp-draft myminio/skill-market/draft/gitlab/skills/product/minio-live-draft
```

## 管理员发布操作

1. 进入管理员入口并解锁。
2. 点击 `刷新草稿`。
3. 在草稿列表中选择一个草稿。
4. 补齐发布元数据：
   - `namespace`
   - `skill_id`
   - 名称
   - 摘要
   - 标签
   - 发布范围：`公共` 或 `项目`
   - 公共分类或项目
   - 变更说明
5. 点击 `保存元数据`。
6. 点击 `预览草稿`，确认 `SKILL.md`、文件清单、`publish-meta.v1.json`。
7. 点击 `发布到市场`。

发布成功后会写入：

```text
skills/{namespace}/{skill_id}/versions/{version}/skill.json
skills/{namespace}/{skill_id}/versions/{version}/package.zip
skills/{namespace}/{skill_id}/versions/{version}/package.sha256
skills/{namespace}/{skill_id}/versions/{version}/changelog.md
skills/{namespace}/{skill_id}/manifest.json
catalog.v1.json
categories.v1.json
projects.v1.json
indexes/search-lite.v1.json
indexes/market/public/{category}.v1.json
indexes/market/projects/{project}.v1.json
draft/admin/gitlab/skills/{gitlab_source_path}/publish-meta.v1.json
draft/admin/gitlab/skills/{gitlab_source_path}/state.v1.json
admin/publish-jobs/{job_id}.json
admin/audit/{yyyy}/{mm}/{dd}/publish-{id}.json
```

如果 `validation.json.status` 不是 `passed` / `ok` / `success`，发布会被拒绝。

## 构建和测试

安装前端依赖：

```powershell
npm --prefix fronted install
```

前端构建：

```powershell
npm run build
```

Rust 检查：

```powershell
cd src-tauri
cargo check
```

Rust 单测：

```powershell
cd src-tauri
cargo test
```

真实 MinIO 集成测试默认是 ignored，需要先准备：

- `src-tauri/src/db.rs` 指向 `http://192.168.1.4:9000`。
- `skill-market` bucket 存在。
- `admin/security/mac-allowlist.v1.json` 已包含当前机器 MAC。
- `draft/gitlab/skills/product/minio-live-draft/SKILL.md` 已存在。

运行：

```powershell
cd src-tauri
cargo test commands::tests::live_minio_admin_publish_flow -- --ignored --exact --nocapture
```

当前已验证通过：

```text
cargo check
cargo test
npm.cmd run build
cargo test commands::tests::live_minio_admin_publish_flow -- --ignored --exact --nocapture
```

## 开发运行

启动 Tauri 开发模式：

```powershell
npm run tauri -- dev
```

直接运行 Rust/Tauri：

```powershell
npm run build
cd src-tauri
cargo run
```

`cargo run` 会加载 `fronted/dist`，不会连接 `localhost:5173`。

## 打包部署

打包桌面应用：

```powershell
npm run build
npm run tauri -- build
```

产物通常位于：

```text
src-tauri/target/release/
src-tauri/target/release/bundle/
```

部署前确认：

- `src-tauri/src/db.rs` 中的 `COMPILED_SOURCE_ENDPOINT` 是目标 MinIO 地址。
- `src-tauri/src/admin_config.rs` 中的管理员密钥和 MinIO 发布凭证正确。
- 目标 MinIO 已有 `skill-market` bucket。
- 目标 MinIO 已有 `catalog.v1.json` 和 `categories.v1.json`。
- 管理员机器 MAC 已写入 `admin/security/mac-allowlist.v1.json`。

## 普通用户操作

1. 打开客户端。
2. 在 `市场` 页刷新市场。
3. 在 `公共` 或 `项目` 页签选择 skill。
4. 选择 Codex / Claude。
5. 选择个人级或项目级作用域。
6. 点击安装 / 启用。
7. 在 `本地` 页查看缓存、预览、删除缓存。
8. 在 `项目` 页绑定项目目录，供项目级安装使用。

## 作用域规则

市场里的 skill 不区分 Codex 和 Claude。下载 skill 只是进入 Skill Hub 本地包缓存；真正生效时，用户需要选择目标平台和作用域。

生效目录：

```text
Codex / personal  -> 设置页配置的 Codex 个人级 skill 目录，默认 ~/.codex/skills
Claude / personal -> 设置页配置的 Claude 个人级 skill 目录，默认 ~/.claude/skills
Codex / project   -> 项目根目录/.codex/skills
Claude / project  -> 项目根目录/.claude/skills
```

同一个 skill 在同一个目标平台上，个人级和项目级不能同时生效。

判断字段：

```text
namespace + skill_id + target
```

允许：

```text
Codex / personal / skill-a
Claude / project / skill-a
```

不允许：

```text
Codex / personal / skill-a
Codex / project / skill-a
```
