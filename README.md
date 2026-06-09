# Skill Hub

Skill Hub 是一个 Tauri v1 + Rust + React 的桌面 skill 市场客户端。远程市场数据来自 MinIO 对象存储，本地使用 SQLite 记录源、缓存、项目和 skill 生效绑定。

## 当前能力

- 内置 MinIO 市场源同步。
- `catalog.v1.json` / `categories.v1.json` 读取。
- 市场 skill 浏览、搜索和分类筛选。
- 本地安装绑定管理。
- 安装时选择 Codex / Claude 目标平台。
- 个人级 / 项目级作用域选择。
- Codex / Claude 各自个人级 skill 目录配置。
- 项目级启用时自动写入项目根目录下的 `.codex/skills` 或 `.claude/skills`。
- 同一 skill 在同一平台上个人级和项目级互斥。
- 项目路径绑定。
- 更新中心列表。
- 本地扫描 Skill Hub 管理的绑定。
- 本地已有 skill 扫描与内容预览。
- 本地菜单中的包缓存查看、预览与删除。
- 完整平台结构的 PowerShell 发布脚本。

## 技术栈

- Tauri v1
- Rust
- SQLite / rusqlite
- React + Vite + TypeScript
- lucide-react
- MinIO / S3-compatible public object URL

MinIO 源由本地默认配置和运维脚本维护，不暴露在客户端 UI 页面中。

## 开发运行

前端工程位于 `fronted/`，根目录 `package.json` 只提供脚本转发。

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

`cargo run` 会加载 `fronted/dist`，不会连接 `localhost:5173`，避免被其他本地页面占用端口时加载错页面。

## MinIO 发布

发布一个本地 skill 目录到完整平台结构：

```powershell
mc alias set skillhub http://127.0.0.1:9000 minioadmin minioadmin

.\publish-skill.ps1 `
  -SkillDir .\examples\react-reviewer `
  -Namespace official `
  -Alias skillhub `
  -Bucket skill-market `
  -CreateBucket
```

脚本会读取源目录 `skill.json` 作为发布元数据，上传不含 `.json` 的 `package.zip` 运行包，更新 manifest、重建分类索引、search-lite，并最后上传 `catalog.v1.json`。

## 作用域规则

市场里的 skill 不区分 Codex 和 Claude。下载 skill 只是进入 Skill Hub 本地包缓存；真正生效时，用户需要选择目标平台和作用域。

生效目录：

```text
Codex / personal  -> 设置页配置的 Codex 个人级 skill 目录，默认 ~/.codex/skills
Claude / personal -> 设置页配置的 Claude 个人级 skill 目录，默认 ~/.claude/skills
Codex / project   -> 项目根目录/.codex/skills
Claude / project  -> 项目根目录/.claude/skills
```

项目目录只在“项目”菜单绑定。市场安装到项目级时，只能从已绑定项目中选择，不直接选择新目录。

Skill Hub 只在本地 SQLite 中记录 skill 与个人 / 项目 / 平台的关系。下载到本地包缓存和写入 Codex / Claude 目标 skill 目录时都会过滤 `*.json` 文件，不在 skill 目录放 `skillhub-binding.json`、`skill.json`、`skillhub-package.json` 等关系或元数据文件。

市场状态分三层判断：`skill_packages` 表示已下载到本地包缓存，`skill_bindings` 表示已安装或已启用，`local_skills` 表示扫描到的本地已有 skill。扫描到但不是 Skill Hub 安装的目录会显示为未托管，不会自动接管。

同一个 skill 在同一个目标平台上，个人级和项目级不能同时生效。

判断字段：

```text
namespace + skill_id + target
```

不包含版本号，因此不能用不同版本绕过互斥规则。

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

## 本地缓存

市场下载会进入 Skill Hub 本地包缓存，市场页只显示“已缓存”状态。本地缓存统一在“本地”菜单查看、预览和删除；删除缓存只移除 `skill_packages` 记录和包目录，不影响已安装到 Codex / Claude 的个人或项目 skill。
