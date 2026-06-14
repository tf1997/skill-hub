# Skill Hub

Skill Hub 是一个基于 Tauri v1、Rust、React 和 SQLite 的桌面 skill 市场客户端。它从 MinIO 对象存储读取市场数据，把 skill 下载到本地缓存，并按 Codex / Claude、个人级 / 项目级安装到目标目录。

本仓库侧重桌面客户端本体：市场浏览、安装启用、本地扫描、更新、管理员发布、项目市场和审计查看。GitLab 只负责把 skill 原生文件同步到 MinIO 草稿区，正式发布由客户端管理员模式完成。

## 文档

- [操作文档](docs/operation-guide.md)：MinIO 数据准备、GitLab 草稿同步、管理员发布流程、权限配置、审计查看。
- [GitLab 草稿同步模板](docs/gitlab-draft-sync-template.yml)：复制到 skill 源码仓库作为 `.gitlab-ci.yml` 使用。
- [管理员发布设计说明](docs/admin-publishing-community-design.md)：管理员发布、项目市场和权限模型的设计记录。
- [发布说明](docs/release.md)：桌面应用发布和更新相关记录。

## 主要能力

- 浏览公共市场和项目市场，支持搜索、分类筛选、详情和预览。
- 下载 skill 到本地包缓存，并安装 / 启用到 Codex 或 Claude。
- 支持个人级和项目级作用域，同一 skill 在同一目标平台上个人级和项目级互斥。
- 扫描本机已有 Claude / Codex skill，区分 Skill Hub 托管项和本地已有项。
- 管理员模式支持项目治理、公共分类维护、GitLab 草稿发布、市场下架和审计记录。
- 管理权限分为 `system` 和 `project`：系统管理员拥有全部权限；项目管理员可管理所有项目 skill，但不能配置公共分类、发布公共 skill 或查看审计日志。

## 技术栈

- 桌面框架：Tauri v1
- 后端：Rust
- 前端：React + Vite
- 本地数据：SQLite
- 远程市场：MinIO / S3-compatible object storage
- 包格式：`package.zip`
- 校验：SHA-256

## 目录结构

```text
.
├─ fronted/                 # 前端工程，目录名按现状保留
│  ├─ src/
│  └─ package.json
├─ src-tauri/               # Tauri / Rust 后端
│  ├─ src/
│  ├─ Cargo.toml
│  └─ tauri.conf.json
├─ docs/
│  ├─ operation-guide.md
│  └─ gitlab-draft-sync-template.yml
├─ categories.v1.json       # 示例公共分类
├─ seed-minio.ps1           # 本地 MinIO 示例数据种子脚本
└─ publish-skill.ps1        # 早期脚本发布工具，保留作运维参考
```

## 环境要求

建议开发环境：

- Windows 10 / 11
- Node.js 18+
- npm 9+
- Rust stable toolchain
- Git
- WebView2 Runtime
- 可选：MinIO Server / MinIO Client `mc`

首次安装前端依赖：

```powershell
npm --prefix fronted install
```

根目录 `package.json` 提供了 workspace 级脚本，会自动转到 `fronted` 执行：

```powershell
npm run build
npm run tauri -- dev
npm run tauri -- build
```

## 配置

市场源配置在 [src-tauri/src/minio_config.rs](src-tauri/src/minio_config.rs)：

```rust
COMPILED_SOURCE_ENDPOINT = "http://192.168.1.4:9000"
COMPILED_SOURCE_BUCKET = "skill-market"
APP_UPDATE_MANIFEST_OBJECT = "updates/stable/latest.json"
```

这些值支持编译时环境变量覆盖：

```powershell
$env:SKILL_HUB_MINIO_ENDPOINT = "http://minio.example.com:9000"
$env:SKILL_HUB_MINIO_BUCKET = "skill-market"
$env:SKILL_HUB_MINIO_REGION = "us-east-1"
$env:SKILL_HUB_BUILT_IN_UPDATE_MANIFEST_URL = "http://minio.example.com:9000/skill-market/updates/stable/latest.json"
```

管理员配置在 [src-tauri/src/admin_config.rs](src-tauri/src/admin_config.rs)：

```rust
ADMIN_KEY = "skillhub-admin"
MINIO_PUBLISHER_ACCESS_KEY = "minioadmin"
MINIO_PUBLISHER_SECRET_KEY = "minioadmin"
MAC_ALLOWLIST_OBJECT_PATH = "admin/security/mac-allowlist.v1.json"
```

生产构建前需要确认这些配置已指向正式 MinIO 和正式管理员凭证。普通市场读取只需要公开读权限或只读凭证；管理员写操作使用后端中的发布凭证。

## 开发运行

启动 Tauri 开发模式：

```powershell
npm run tauri -- dev
```

当前 Tauri 配置的 `beforeDevCommand` 会先构建前端，`devPath` 指向 `fronted/dist`，不会依赖 Vite dev server。

也可以先构建前端，再直接运行 Rust 后端：

```powershell
npm run build
cd src-tauri
cargo run
```

## 构建和检查

前端构建：

```powershell
npm run build
```

Rust 编译检查：

```powershell
cd src-tauri
cargo check
```

Rust 单测：

```powershell
cd src-tauri
cargo test
```

真实 MinIO 集成测试默认标记为 ignored，运行前需要先按 [操作文档](docs/operation-guide.md) 准备 bucket、管理员白名单和 GitLab 草稿对象：

```powershell
cd src-tauri
cargo test commands::tests::live_minio_admin_publish_flow -- --ignored --exact --nocapture
```

## 打包

打包桌面应用：

```powershell
npm run tauri -- build
```

常见产物位置：

```text
src-tauri/target/release/
src-tauri/target/release/bundle/
```

打包前确认：

- 编译时 MinIO endpoint / bucket 已设置到目标环境。
- 管理员密钥和 MinIO 发布凭证已替换为目标环境配置。
- 目标 MinIO 已准备 `catalog.v1.json`、`categories.v1.json`、`projects.v1.json` 和 skill 对象。
- `admin/security/mac-allowlist.v1.json` 已包含管理员机器 MAC。
- `updates/stable/latest.json` 与发布包路径一致，或已设置 `SKILL_HUB_BUILT_IN_UPDATE_MANIFEST_URL`。

## 普通使用

普通用户打开客户端后可以：

1. 在 `市场` 页浏览公共或项目 skill。
2. 选择 Codex / Claude。
3. 选择个人级或项目级作用域。
4. 安装并启用 skill。
5. 在 `本地` 页查看缓存、预览、删除缓存或扫描本机已有 skill。
6. 在 `项目` 页绑定项目目录，供项目级安装使用。

管理员入口是隐藏入口：点击左上角 `Skill Hub` 品牌区域，输入管理员密钥并通过 MAC 白名单校验后，侧边栏才会显示 `管理` 菜单。管理员发布和权限配置见 [操作文档](docs/operation-guide.md)。
