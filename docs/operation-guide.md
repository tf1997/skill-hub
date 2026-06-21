# Skill Hub 操作文档

本文档面向 Skill Hub 的日常运营人员和管理员，重点说明数据如何准备、GitLab 如何同步草稿、管理员如何从草稿区发布 skill，以及权限和审计如何配置。

## 1. 数据流概览

Skill Hub 的正式市场数据在 MinIO 中，客户端本地只缓存数据和安装状态。

```text
GitLab skill 仓库
  -> GitLab CI 校验 SKILL.md
  -> MinIO draft/gitlab/skills/{category}/{draft_slug}/
  -> Skill Hub 管理员模式补齐 publish-meta.v1.json
  -> 管理员预览并发布
  -> MinIO skills/、catalog.v1.json、indexes/、projects.v1.json、categories.v1.json
  -> 普通客户端刷新市场并安装
```

GitLab 不直接写正式市场对象；正式发布必须经过 Skill Hub 管理员模式。

## 2. MinIO 数据准备

默认配置：

```text
endpoint: http://192.168.1.4:9000
bucket:   skill-market
alias:    myminio
```

如果 `mc` 不在 PATH，可以先指定本机路径：

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
```

开发环境可以临时将整个 bucket 设为可下载，方便客户端直接读取对象：

```powershell
& $mc anonymous set download myminio/skill-market
```

生产环境不要公开整个 bucket。建议只公开普通客户端需要读取的对象和前缀，保留 `admin/`、`draft/` 私有：

```powershell
& $mc anonymous set download myminio/skill-market/catalog.v1.json
& $mc anonymous set download myminio/skill-market/categories.v1.json
& $mc anonymous set download myminio/skill-market/projects.v1.json
& $mc anonymous set download myminio/skill-market/indexes
& $mc anonymous set download myminio/skill-market/skills
```

初始化示例市场数据：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\seed-minio.ps1 `
  -McPath D:\tmp\skillhub-minio\mc.exe `
  -Alias myminio `
  -Bucket skill-market
```

检查关键对象：

```powershell
& $mc ls myminio/skill-market
& $mc cat myminio/skill-market/catalog.v1.json
& $mc cat myminio/skill-market/categories.v1.json
```

正式市场至少需要这些入口对象：

```text
catalog.v1.json
categories.v1.json
projects.v1.json
indexes/search-lite.v1.json
skills/{namespace}/{skill_id}/manifest.json
skills/{namespace}/{skill_id}/versions/{version}/skill.json
skills/{namespace}/{skill_id}/versions/{version}/package.zip
skills/{namespace}/{skill_id}/versions/{version}/package.sha256
```

## 3. 管理员权限配置

管理员入口需要同时满足：

- 输入的管理员密钥等于后端 `ADMIN_KEY`，默认是 `skillhub-admin`。
- 当前机器 MAC 命中 MinIO 中的 `admin/security/mac-allowlist.v1.json`。
- 命中的白名单条目是 active。
- 后端配置的 MinIO 发布凭证可访问管理和正式市场对象。

查看当前机器 MAC：

```powershell
getmac /FO CSV /NH
```

写入管理员白名单：

```powershell
@'
{
  "entries": [
    {
      "mac": "<SYSTEM_ADMIN_MAC>",
      "role": "system",
      "name": "系统管理员"
    },
    {
      "mac": "<PROJECT_ADMIN_MAC>",
      "role": "project",
      "name": "项目管理员"
    }
  ]
}
'@ | Set-Content -Encoding UTF8 .\mac-allowlist.v1.json

& $mc cp .\mac-allowlist.v1.json myminio/skill-market/admin/security/mac-allowlist.v1.json
```

字段说明：

| 字段 | 说明 |
| --- | --- |
| `mac` | 管理员机器 MAC，支持短横线、冒号或点分格式；文档中使用占位符，不要提交真实 MAC |
| `status` | 可选，默认 active；非 active 不允许登录 |
| `role` | `system` 或 `project`；缺省按 `project` 处理 |
| `name` | 授权显示名，右上角管理员信息显示这个名字 |
| `projects` | 兼容保留字段；当前 `project` 角色可管理所有项目 skill，不按该列表限制 |

兼容早期简单写法：

```json
{
  "macs": [
    "<SYSTEM_ADMIN_MAC>"
  ]
}
```

简单写法等价于系统管理员。

角色权限：

| 角色 | 权限 |
| --- | --- |
| `system` | 拥有所有管理权限，可维护公共分类、所有项目、公共 skill、项目 skill、审计日志 |
| `project` | 可维护所有市场项目，可发布 / 下架所有项目 skill；看不到公共分类页签，不能发布公共 skill，不能查看审计日志 |

管理员白名单属于敏感对象。生产环境不要让 `admin/security/mac-allowlist.v1.json` 对普通用户公开。

## 4. GitLab skill 仓库准备

每个 skill 源码仓库至少需要：

```text
SKILL.md
README.md           # 可选
assets/             # 可选
references/         # 可选
scripts/            # 可选
```

`SKILL.md` 必须包含 `version` 和 `author` 字段，GitLab 模板会校验这两个字段：

```markdown
---
name: Example Skill
version: 0.1.0
author: Skill Team
---

# Example Skill
```

把 [gitlab-draft-sync-template.yml](gitlab-draft-sync-template.yml) 复制到 skill 源码仓库，命名为 `.gitlab-ci.yml`。

必填 GitLab CI 变量：

| 变量 | 说明 |
| --- | --- |
| `MINIO_ENDPOINT` | MinIO endpoint，例如 `http://minio.internal:9000` |
| `MINIO_ACCESS_KEY` | 草稿写入凭证 |
| `MINIO_SECRET_KEY` | 草稿写入凭证密钥 |
| `GITLAB_CATEGORY_CODE` | GitLab 草稿分类码，例如 `product` |
| `DRAFT_SLUG` | 草稿 slug，例如 `prd-shaper` |

可选变量：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `SKILL_DIR` | `.` | skill 文件所在目录 |
| `SKILL_MARKET_BUCKET` | `skill-market` | MinIO bucket |

模板只在默认分支同步草稿。同步目标：

```text
draft/gitlab/skills/{GITLAB_CATEGORY_CODE}/{DRAFT_SLUG}/
```

同步后应能看到：

```text
draft/gitlab/skills/product/prd-shaper/SKILL.md
draft/gitlab/skills/product/prd-shaper/validation.json
```

检查草稿：

```powershell
& $mc ls --recursive myminio/skill-market/draft/gitlab/skills/product/prd-shaper
& $mc cat myminio/skill-market/draft/gitlab/skills/product/prd-shaper/validation.json
```

`validation.json.status` 必须是 `passed`、`ok` 或 `success`，否则管理员发布会被拒绝。

## 5. 市场项目和公共分类

项目市场由 `projects.v1.json` 驱动，项目对象不再使用 `active` / `archive` 状态字段。

示例：

```json
{
  "schema": "skillhub.projects.v1",
  "generatedAt": "2026-06-14T00:00:00Z",
  "projects": [
    {
      "slug": "alpha",
      "name": "Alpha 项目",
      "description": "Alpha 项目专用 skills"
    }
  ]
}
```

公共分类由 `categories.v1.json` 驱动，不应该依赖客户端内置通用分类。

示例：

```json
{
  "schema": "skillhub.categories.v1",
  "items": [
    {
      "id": "frontend",
      "name": "前端",
      "order": 20
    }
  ]
}
```

在客户端管理员模式中：

- 系统管理员可新增、编辑、删除市场项目和公共分类。
- 项目管理员可新增、编辑、删除市场项目。
- 项目管理员看不到公共分类页签。
- 删除项目或公共分类前，必须先下架关联 skill。

## 6. 管理员发布流程

进入管理员模式：

1. 打开客户端。
2. 点击左上角 `Skill Hub` 品牌区域。
3. 输入管理员密钥。
4. 通过 MAC 白名单校验后，侧边栏显示 `管理`。

发布草稿：

1. 进入 `管理`。
2. 打开 `草稿发布`。
3. 点击刷新草稿列表。
4. 选择 GitLab 同步过来的草稿。
5. 补齐发布元数据。
6. 点击 `保存元数据`。
7. 点击 `预览草稿`，确认 `SKILL.md`、文件清单和 `publish-meta.v1.json`。
8. 点击 `发布到市场`。

发布元数据字段：

| 字段 | 说明 |
| --- | --- |
| `namespace` | 市场命名空间，例如 `official`、`team-alpha` |
| `skill_id` | skill ID，只能使用安全路径片段 |
| `name` | 市场显示名 |
| `summary` | 市场摘要 |
| `tags` | 搜索标签 |
| `targets` | 支持目标，例如 `codex`、`claude` |
| `levels` | 支持作用域，例如 `personal`、`project` |
| `publish_scope` | `public` 或 `project` |
| `publish_category_slug` | 发布到公共分类时必填 |
| `publish_project_slug` | 发布到项目时必填 |
| `changelog` | 版本变更说明 |

元数据保存位置：

```text
draft/admin/gitlab/skills/{gitlab_source_path}/publish-meta.v1.json
```

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
draft/admin/gitlab/skills/{gitlab_source_path}/state.v1.json
admin/publish-jobs/{job_id}.json
admin/audit/{yyyy}/{mm}/{dd}/{action}-{id}.json
```

发布约束：

- `SKILL.md` 必须存在。
- `SKILL.md` 中的 `version` 是正式发布版本。
- `validation.json.status` 必须通过。
- 发布项目必须存在于 `projects.v1.json`。
- 发布公共分类需要系统管理员权限。
- 项目管理员只能发布到项目。
- 已发布版本重复发布会被拒绝，除非是下架草稿的快速重新上架流程。

## 7. 下架和重新上架

下架不是删除历史版本包。下架会让 skill 从 `catalog.v1.json` 和相关索引中移除，并把草稿状态标记为 archived，便于修订后重新发布。

下架流程：

1. 进入 `管理`。
2. 打开 `市场下架`。
3. 选择 skill。
4. 填写下架原因。
5. 点击下架。

权限：

- 系统管理员可下架公共和项目 skill。
- 项目管理员只能下架项目 skill。

重新上架：

1. 回到 `草稿发布`。
2. 选择下架后回到草稿区的 skill。
3. 确认或修改发布元数据。
4. 点击快速重新上架或按正常发布流程发布。

## 8. 审计日志

管理员写操作会写入 MinIO：

```text
admin/audit/{yyyy}/{mm}/{dd}/{action}-{id}.json
```

目前记录内容包括：

- 操作动作 `action`
- 操作人显示名 `actor`
- 角色 `role`
- 授权 MAC `macAddress`
- IP 字段 `ipAddress`，当前内置写入为空，预留给部署层补充
- 操作对象和 payload
- 创建时间 `createdAt`

客户端管理页中只有系统管理员能看到 `审计记录` 页。项目管理员不会显示该入口，也不能调用审计日志读取接口。

## 9. 普通用户使用流程

普通用户不需要管理员密钥。

1. 打开客户端。
2. 在 `市场` 页选择 `公共` 或 `项目`。
3. 搜索或按分类筛选 skill。
4. 查看详情和预览。
5. 选择目标平台：Codex 或 Claude。
6. 选择作用域：个人级或项目级。
7. 点击安装 / 启用。
8. 在 `本地` 页查看缓存、预览、删除缓存或扫描本地已有 skill。
9. 在 `项目` 页维护本机项目目录，用于项目级安装。

作用域规则：

```text
Codex / personal  -> ~/.codex/skills 或设置页配置的 Codex 个人目录
Claude / personal -> ~/.claude/skills 或设置页配置的 Claude 个人目录
Codex / project   -> 项目根目录/.codex/skills
Claude / project  -> 项目根目录/.claude/skills
```

同一个 `namespace + skill_id + target` 不能同时以个人级和项目级启用。Claude 和 Codex 分开判断，互不影响。

## 10. 系统发版和更新流程

Skill Hub 的桌面应用更新通过 MinIO manifest 完成。客户端启动后会做后台检查，用户也可以从原生菜单 `帮助 -> 检查更新` 手动触发检查。

默认更新 manifest 地址由后端配置拼出：

```text
{SKILL_HUB_MINIO_ENDPOINT}/{SKILL_HUB_MINIO_BUCKET}/updates/stable/latest.json
```

当前默认值：

```text
http://192.168.1.4:9000/skill-market/updates/stable/latest.json
```

也可以在构建时指定完整 manifest URL：

```powershell
$env:SKILL_HUB_BUILT_IN_UPDATE_MANIFEST_URL = "https://minio.example.com/skill-market/updates/stable/latest.json"
```

### 10.1 发版前版本号

发版前同步更新这些版本号：

```text
package.json               version
frontend/package.json       version
src-tauri/Cargo.toml       package.version
src-tauri/tauri.conf.json  package.version
```

版本按 SemVer 比较，使用 `0.2.0` 这类完整格式。`latest.json.version` 必须高于客户端当前 `CARGO_PKG_VERSION`，否则客户端会提示已是最新版本。

### 10.2 构建便携版

Windows 便携版使用脚本打包：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-windows-portable.ps1 -Build -Arch x64
```

脚本会生成：

```text
dist/portable/windows-x64/SkillHub-{version}-windows-x64-portable.zip
```

便携包内至少包含：

```text
skill-hub.exe
WebView2Loader.dll
portable.json
```

脚本输出的 `sha256` 和 `size` 要填入 `latest.json`。如果需要固定版 WebView2 Runtime，可传入 `-WebView2RuntimePath` 或设置 `WEBVIEW2_FIXED_RUNTIME_PATH`。

### 10.3 构建安装版

安装版使用 Tauri 打包：

```powershell
npm run tauri -- build
```

产物通常在：

```text
src-tauri/target/release/bundle/
```

安装版 manifest 中的包 `target` 使用 `installer`。安装包下载后不会自动执行安装器，客户端会提示用户运行安装器完成更新。

### 10.4 latest.json

manifest 示例见仓库中的 `updates/stable/latest.example.json`。

字段说明：

| 字段 | 说明 |
| --- | --- |
| `version` | 最新应用版本，必须是 SemVer |
| `channel` | 更新通道，例如 `stable` |
| `notes` | 更新说明，会展示给用户 |
| `force` | 预留字段，当前客户端不强制更新 |
| `min_supported_version` | 预留字段，当前客户端不做强制拦截 |
| `packages[].target` | `portable` 或 `installer` |
| `packages[].platform` | `windows`、`macos`、`linux` |
| `packages[].arch` | `x64` 或 `aarch64` |
| `packages[].url` | 更新包可下载 URL |
| `packages[].sha256` | 更新包 SHA-256 |
| `packages[].size` | 更新包字节数 |

示例：

```json
{
  "version": "0.2.0",
  "channel": "stable",
  "notes": "更新说明",
  "force": false,
  "min_supported_version": "0.1.0",
  "packages": [
    {
      "target": "portable",
      "platform": "windows",
      "arch": "x64",
      "url": "https://minio.example.com/skill-market/updates/stable/0.2.0/SkillHub-0.2.0-windows-x64-portable.zip",
      "sha256": "<SHA256>",
      "signature": null,
      "size": 0
    }
  ]
}
```

客户端会按当前分发形态、平台和架构选择匹配包：

```text
target:   portable | installer
platform: windows | macos | linux
arch:     x64 | aarch64
```

便携版判断依据是当前程序目录中存在 `portable.json` 或 `versions/`；也可用环境变量强制安装版判断：

```powershell
$env:SKILL_HUB_DISTRIBUTION = "installer"
```

### 10.5 上传顺序

推荐对象路径：

```text
updates/stable/{version}/SkillHub-{version}-windows-x64-portable.zip
updates/stable/{version}/SkillHub_{version}_x64-setup.exe
updates/stable/latest.json
```

上传示例：

```powershell
& $mc cp .\dist\portable\windows-x64\SkillHub-0.2.0-windows-x64-portable.zip `
  myminio/skill-market/updates/stable/0.2.0/SkillHub-0.2.0-windows-x64-portable.zip

& $mc cp .\updates\stable\latest.json `
  myminio/skill-market/updates/stable/latest.json
```

必须先上传更新包，再最后上传 `updates/stable/latest.json`。这样可以避免旧客户端读到新 manifest 后下载不到包。

### 10.6 客户端更新行为

检查更新时：

1. 客户端读取 `updates/stable/latest.json`。
2. 比较 `latest.json.version` 和当前应用版本。
3. 根据 `target + platform + arch` 选择包。
4. 下载更新包。
5. 校验 SHA-256。
6. 根据包类型执行后续动作。

便携版下载成功后：

- 解压到当前便携根目录下的 `versions/{version}/`。
- 写入 `versions/{version}/current.json` 和根目录 `current.json`。
- 用户确认后客户端启动新版本并退出旧版本。
- 下次启动时会自动跳到 `versions/` 下最高版本。

安装版下载成功后：

- 安装包保存到应用数据目录 `updates/installers/{version}/`。
- 客户端提示用户运行安装器。
- 客户端不会静默执行安装器。

### 10.7 回滚和撤回

如果新版本有问题：

1. 不删除旧版本包。
2. 将 `updates/stable/latest.json` 改回上一个稳定版本，或删除问题版本对应 package 条目。
3. 最后重新上传 `latest.json`。
4. 便携版用户如果已经下载问题版本，需要手工删除便携根目录下的 `versions/{bad_version}/` 和按需修正 `current.json`。
5. 安装版用户按安装器或系统卸载流程回退。

回滚时仍要确保 `latest.json.version`、包 URL、`sha256`、`size` 彼此一致。

### 10.8 发版检查清单

- 四处版本号已同步。
- `npm run build` 通过。
- `cargo check` 和 `cargo test` 通过。
- 便携包或安装包已生成。
- 已记录更新包 `sha256` 和 `size`。
- `latest.json` 的 `target`、`platform`、`arch` 与目标用户匹配。
- 更新包 URL 可由普通客户端访问。
- 先上传更新包，最后上传 `latest.json`。
- 用一台旧版本客户端执行 `帮助 -> 检查更新` 验证完整流程。

## 11. WebView2 离线部署

Skill Hub 依赖 Windows 的 Microsoft Edge WebView2 Runtime。若目标机器缺少该组件，应用会在启动前进入原生安装引导：先让用户确认安装，再打开一个原生进度窗口，从内网地址下载安装包到临时目录，随后启动安装程序，并在安装完成后尝试重新打开应用。

默认优先级：

1. 环境变量 `SKILL_HUB_WEBVIEW2_INSTALLER_URL`
2. 同目录文件 `webview2-installer-url.txt`
3. 编译期环境变量 `SKILL_HUB_WEBVIEW2_INSTALLER_URL`
4. 内置默认占位地址

安装参数也可配置，默认是 `/silent /install`：

```powershell
$env:SKILL_HUB_WEBVIEW2_INSTALLER_ARGS = "/silent /install"
```

推荐部署方式是把 `MicrosoftEdgeWebView2RuntimeInstallerX64.exe` 放到内网服务器，然后将安装包地址配置到环境变量或同目录文本文件中。

示例：

```powershell
$env:SKILL_HUB_WEBVIEW2_INSTALLER_URL = "http://intranet.example.com/MicrosoftEdgeWebView2RuntimeInstallerX64.exe"
```

或在程序目录旁放置 `webview2-installer-url.txt`，文件内容就是安装包完整 URL。

已有 WebView2 的开发机可以用测试开关强制进入安装引导：

```powershell
$env:SKILL_HUB_FORCE_WEBVIEW2_SETUP = "1"
$env:SKILL_HUB_WEBVIEW2_INSTALLER_URL = "http://intranet.example.com/MicrosoftEdgeWebView2RuntimeInstallerX64.exe"
.\skill-hub.exe
```

测试结束后清除：

```powershell
Remove-Item Env:\SKILL_HUB_FORCE_WEBVIEW2_SETUP
```

启动时若未检测到 WebView2 Runtime：

- 会弹出原生确认窗口，用户确认后才开始下载和安装。
- 会打开安装进度窗口；下载阶段显示百分比和已下载大小，服务器没有返回文件大小时显示连续进度。
- 会把安装包下载到 `%TEMP%\SkillHub\`。
- 会启动 WebView2 安装程序；安装阶段显示“安装中”和检测状态。
- 安装成功后会尝试重新打开 Skill Hub；如果自动重启失败，会提示用户手动重新打开。

## 12. 常见检查项

管理员无法进入：

- 确认管理员密钥是否等于后端 `ADMIN_KEY`。
- 确认本机 MAC 已写入 `admin/security/mac-allowlist.v1.json`。
- 确认白名单条目 `status` 不是 disabled。
- 确认 MinIO 发布凭证能读取 `admin/security/mac-allowlist.v1.json`。

草稿列表为空：

- 确认 GitLab pipeline 已在默认分支运行。
- 确认 `DRAFT_SLUG` 和 `GITLAB_CATEGORY_CODE` 已设置。
- 确认 MinIO 中存在 `draft/gitlab/skills/{category}/{draft_slug}/SKILL.md`。

发布按钮不可用或发布失败：

- 确认发布元数据完整。
- 确认 `validation.json.status` 是 passed / ok / success。
- 确认项目发布目标存在于 `projects.v1.json`。
- 确认项目管理员没有选择公共发布目标。
- 确认同版本没有重复发布。

普通市场看不到新 skill：

- 确认发布后 `catalog.v1.json` 已更新。
- 确认 `skills/{namespace}/{skill_id}/manifest.json` 可读取。
- 确认 `package.zip` 和 `package.sha256` 可读取。
- 确认普通客户端刷新了市场。

审计日志看不到：

- 确认当前管理员角色是 `system`。
- 确认 MinIO 中存在 `admin/audit/.../*.json`。
- 确认没有把 `admin/` 对普通用户公开。
