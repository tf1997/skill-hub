# Skill Hub Release Flow

Skill Hub 的在线更新沿用 Echo 的 MinIO manifest 模式：客户端读取内置 `latest.json`，按当前分发形态、平台和 CPU 架构选择匹配包，下载后校验 SHA256。

## Manifest

示例见 `updates/stable/latest.example.json`。字段匹配值：

```text
target   portable | installer
platform windows | macos | linux
arch     x64 | aarch64
```

默认 MinIO endpoint/bucket 在 `src-tauri/src/minio_config.rs`，应用更新 manifest 默认由同一套配置拼出：

```text
http://192.168.1.4:9000/skill-market/updates/stable/latest.json
```

构建时可统一覆盖 MinIO 源：

```powershell
$env:SKILL_HUB_MINIO_ENDPOINT="https://minio.example.com"
$env:SKILL_HUB_MINIO_BUCKET="skill-market"
```

也可以只覆盖完整更新 manifest URL：

```powershell
$env:SKILL_HUB_BUILT_IN_UPDATE_MANIFEST_URL="https://minio.example.com/skill-market/updates/stable/latest.json"
```

## Version

发版前同时更新：

```text
src-tauri/Cargo.toml       package.version
src-tauri/tauri.conf.json  package.version
package.json               version
fronted/package.json       version
```

版本按 semver 比较，使用 `0.2.0` 这类完整格式。

## Windows Portable

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-windows-portable.ps1 -Build -Arch x64
```

生成的 zip 包含：

```text
skill-hub.exe
WebView2Loader.dll
portable.json
```

脚本会输出 `sha256` 和 `size`，把它们填入 `latest.json`。

## Windows Installer

安装版在 `src-tauri/tauri.conf.json` 中显式使用 `webviewInstallMode: downloadBootstrapper`。生成 MSI 时，安装器会在目标机器缺少 WebView2 Runtime 时静默下载并安装 Runtime。便携包仍使用 DLL 随包方式，确保当前 GNU 构建产物可以直接运行。

## Windows Crash Event Log

Windows 版启动时会安装崩溃上报钩子。程序崩溃后会向 Windows 事件日志写入错误事件：

```text
Log:    Application
Source: Skill Hub
Level:  Error
```

事件 ID：

```text
1001  Rust panic
1002  Tauri runtime fatal error
1003  Native unhandled Windows exception
```

如果事件查看器提示找不到 `Skill Hub` 的消息资源，这是正常现象。事件详情仍会包含崩溃摘要、版本、进程 ID、panic 位置和 backtrace。

## Publish Order

1. 更新版本号。
2. 构建安装包或便携包。
3. 生成 SHA256 和文件大小。
4. 上传包到 `updates/stable/<version>/`。
5. 最后上传 `updates/stable/latest.json`。

最后上传 `latest.json` 可以避免旧客户端看到尚未完整上传的新版本。
