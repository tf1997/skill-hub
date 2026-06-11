# Skill Hub 管理发布、需求区与草稿区开发文档

## 1. 背景

当前 Skill Hub 通过客户端浏览市场、下载 skill、安装到 Codex / Claude 目录，并通过 `publish-skill.ps1` 将本地 skill 发布到 MinIO。

新增需求后，系统需要支持：

- 管理员在界面中发布 skill，不再依赖脚本发布到 MinIO。
- 普通用户继续使用 Skill Hub 客户端，不暴露发布权限。
- 用户可以在需求区发表需求。
- 任意登录用户可以评论需求。
- 发布前增加草稿区。
- 管理员可以把草稿区内容发布到市场。
- 普通用户可以使用需求区和草稿区。

这些能力涉及多人写入、评论、草稿协作、审核和权限控制，不能继续只依赖本地 SQLite 和 MinIO 静态对象完成。

## 2. 设计结论

必须引入中心化服务端：`skill-hub-server`。

MinIO 继续作为对象存储，用于保存市场 catalog、manifest、skill 版本文件、package zip、sha256、草稿附件等。需求、评论、草稿状态、用户、权限、审核记录进入服务端数据库。

客户端不再直接持有 MinIO 写权限。所有发布操作必须通过服务端完成。

```text
普通客户端 / 管理界面
        |
        | HTTP API + 登录态
        v
skill-hub-server
        |
        | 服务端持有 MinIO 写权限
        v
MinIO bucket
```

## 3. 目标

- 普通用户可以浏览市场、安装 skill、发表需求、评论需求、查看和参与草稿。
- 普通用户可以创建自己的草稿并提交审核。
- 管理员可以审核草稿、编辑发布元数据、执行发布。
- 发布流程复用现有脚本的校验、打包、上传、索引更新逻辑，但迁移到服务端。
- MinIO 写密钥只存在于服务端环境变量或服务端密钥配置中。
- 所有发布和审核操作都有审计记录。

## 4. 非目标

- 不在普通客户端内置 MinIO 写密钥。
- 不用隐藏按钮作为权限边界。
- 不让普通用户直接更新 catalog 或 manifest。
- 不执行 skill 包内任意脚本。
- 不把需求和评论存入每个用户的本地 SQLite。

## 5. 角色与权限

| 功能 | 普通用户 | 管理员 |
| --- | --- | --- |
| 浏览市场 skill | 可以 | 可以 |
| 安装 / 缓存 skill | 可以 | 可以 |
| 本地扫描 skill | 可以 | 可以 |
| 发表需求 | 可以 | 可以 |
| 评论需求 | 可以 | 可以 |
| 查看草稿区 | 可以 | 可以 |
| 评论草稿 | 可以 | 可以 |
| 创建草稿 | 可以 | 可以 |
| 编辑自己的未提交草稿 | 可以 | 可以 |
| 编辑他人草稿 | 不可以 | 可以 |
| 提交草稿审核 | 可以 | 可以 |
| 驳回草稿 | 不可以 | 可以 |
| 发布草稿到 MinIO 市场 | 不可以 | 可以 |
| 下架 / 归档 | 不可以 | 可以 |
| 管理用户和角色 | 不可以 | 可以 |

普通用户可以使用草稿区，但发布按钮和发布 API 仅对管理员开放。权限必须由服务端校验，前端显示控制只用于用户体验。

## 6. 推荐架构

### 6.1 客户端

当前 Tauri 客户端继续保留本地能力：

- 市场浏览。
- skill 安装 / 缓存。
- 本地扫描。
- 项目绑定。
- 更新中心。
- 本地设置。

新增远程协作能力：

- 需求区。
- 草稿区。
- 管理员审核入口。

客户端调用两类接口：

- Tauri command：本地文件选择、本地安装、本地扫描、本地预览。
- HTTP API：用户登录、需求、评论、草稿、审核、发布。

### 6.2 服务端

新增 `skill-hub-server`，负责：

- 用户认证。
- 角色鉴权。
- 需求区数据。
- 评论数据。
- 草稿区数据。
- 草稿文件上传。
- 草稿校验。
- 发布任务。
- MinIO 写入。
- manifest / catalog / category index / search-lite 更新。
- 审计日志。

### 6.3 数据库

MVP 可以使用服务端 SQLite。多人长期使用建议使用 Postgres。

客户端本地 SQLite 只继续保存本机状态，例如安装绑定、本地缓存、本地扫描、项目绑定、个人目录配置。

## 7. 数据模型草案

### 7.1 users

```text
id
username
display_name
email
role                  -- user / admin
password_hash         -- 如使用账号密码
status                -- active / disabled
created_at
updated_at
```

如果接入企业登录，可以用外部身份 ID 替代 `password_hash`。

### 7.2 requirements

```text
id
title
body
author_id
status                -- open / accepted / in_progress / fulfilled / rejected / archived
priority              -- low / normal / high
tags_json
linked_draft_id
created_at
updated_at
```

### 7.3 requirement_comments

```text
id
requirement_id
author_id
body
parent_id             -- 可选，支持回复
status                -- visible / deleted
created_at
updated_at
```

### 7.4 skill_drafts

```text
id
source_requirement_id
author_id
owner_id
namespace
skill_id
name
summary
version
categories_json
tags_json
levels_json
targets_json
changelog
status                -- draft / submitted / reviewing / approved / rejected / published / archived
visibility            -- public / private
validation_status     -- unknown / passed / failed
validation_report_json
draft_object_prefix
published_at
published_by
created_at
updated_at
```

MVP 中草稿默认 `public`，普通用户可查看和评论。编辑权限限制为作者和管理员。

### 7.5 draft_comments

```text
id
draft_id
author_id
body
parent_id
status                -- visible / deleted
created_at
updated_at
```

### 7.6 publish_jobs

```text
id
draft_id
requested_by
status                -- queued / validating / uploading / indexing / completed / failed
error_message
started_at
completed_at
created_at
updated_at
```

### 7.7 audit_logs

```text
id
actor_id
action                -- draft.submit / draft.approve / draft.publish / requirement.archive 等
entity_type
entity_id
payload_json
created_at
```

## 8. MinIO 对象结构

草稿对象建议使用独立前缀，不直接进入市场 catalog。

```text
drafts/{draft_id}/source.zip
drafts/{draft_id}/skill.json
drafts/{draft_id}/changelog.md
drafts/{draft_id}/validation.json
drafts/{draft_id}/preview/
```

发布成功后写入正式市场结构：

```text
skills/{namespace}/{skill_id}/versions/{version}/skill.json
skills/{namespace}/{skill_id}/versions/{version}/package.zip
skills/{namespace}/{skill_id}/versions/{version}/package.sha256
skills/{namespace}/{skill_id}/versions/{version}/changelog.md
skills/{namespace}/{skill_id}/manifest.json
catalog.v1.json
indexes/categories/{category_id}.v1.json
indexes/search-lite.v1.json
categories.v1.json
```

发布时必须最后写入 `catalog.v1.json`，避免客户端读到不完整版本。

## 9. API 草案

### 9.1 认证

```text
POST /api/auth/login
POST /api/auth/logout
GET  /api/auth/me
```

### 9.2 需求区

```text
GET    /api/requirements
POST   /api/requirements
GET    /api/requirements/{id}
PATCH  /api/requirements/{id}
POST   /api/requirements/{id}/comments
GET    /api/requirements/{id}/comments
```

普通用户可以创建需求和评论。需求状态流转由作者和管理员共同控制，管理员拥有最终管理权限。

### 9.3 草稿区

```text
GET    /api/drafts
POST   /api/drafts
GET    /api/drafts/{id}
PATCH  /api/drafts/{id}
POST   /api/drafts/{id}/upload
POST   /api/drafts/{id}/validate
POST   /api/drafts/{id}/submit
POST   /api/drafts/{id}/comments
GET    /api/drafts/{id}/comments
```

普通用户可以创建、编辑自己的未提交草稿，提交后进入审核状态。提交后的草稿默认锁定，除管理员退回或重新打开外，作者不能直接修改发布内容。

### 9.4 管理员接口

```text
GET    /api/admin/review-queue
POST   /api/admin/drafts/{id}/approve
POST   /api/admin/drafts/{id}/reject
POST   /api/admin/drafts/{id}/publish
GET    /api/admin/publish-jobs
GET    /api/admin/audit-logs
```

这些接口必须服务端校验 `role = admin`。

## 10. 发布流程

```text
1. 用户创建草稿
2. 用户上传 skill 目录压缩包或选择本地目录后由客户端打包上传
3. 服务端解包到临时工作区
4. 服务端校验：
   - 必须包含 SKILL.md
   - skill_id / namespace / version 合法
   - 分类存在
   - package 内不得包含禁止文件
   - 不执行任意脚本
5. 校验结果写入 validation_report_json
6. 用户提交审核
7. 管理员审核
8. 管理员点击发布
9. 服务端创建 publish_job
10. 服务端生成正式 skill.json、package.zip、package.sha256、changelog.md
11. 上传版本文件到 MinIO
12. 更新 skill manifest
13. 更新 category index 和 search-lite
14. 最后更新 catalog.v1.json
15. 草稿状态改为 published
16. 写入 audit_logs
17. 客户端刷新市场
```

发布操作需要支持幂等。若发布任务中断，重新执行时应能识别已经上传的对象，并继续或安全失败。

## 11. 前端页面规划

### 11.1 需求区

页面组成：

- 需求列表。
- 状态筛选。
- 标签筛选。
- 新建需求按钮。
- 需求详情。
- 评论流。
- 评论输入框。

普通用户默认入口：

```text
侧边栏 -> 需求
```

### 11.2 草稿区

页面组成：

- 草稿列表。
- 状态筛选。
- 我创建的草稿。
- 关联需求。
- 草稿详情。
- 元数据编辑。
- 文件上传 / 本地目录打包上传。
- 校验报告。
- 评论流。
- 提交审核按钮。

普通用户可见按钮：

- 新建草稿。
- 保存自己的草稿。
- 上传草稿包。
- 运行校验。
- 提交审核。
- 评论。

管理员额外可见按钮：

- 通过。
- 驳回。
- 编辑发布元数据。
- 发布到市场。
- 归档。

### 11.3 管理入口

管理员登录后显示：

```text
侧边栏 -> 管理
```

管理页包含：

- 待审核草稿。
- 发布任务。
- 发布日志。
- 审计记录。
- 用户与角色管理，后续阶段实现。

## 12. 安全要求

- MinIO 写密钥只允许存在于服务端。
- 普通客户端不得包含 MinIO 写权限。
- 发布 API 必须校验管理员角色。
- 草稿编辑 API 必须校验作者或管理员权限。
- 评论删除和需求归档必须写审计日志。
- 上传文件必须限制大小。
- 上传文件必须解压到隔离临时目录。
- 防止 zip slip 路径穿越。
- 不执行 skill 包内脚本。
- 发布前必须校验 package 内容。
- 服务端日志不得打印 access key、secret key、登录 token。

## 13. 与现有脚本的关系

`publish-skill.ps1` 作为现有发布流程的参考实现，后续应把核心逻辑迁移到服务端：

- 读取和生成 `skill.json`。
- 校验 `SKILL.md`。
- 校验分类。
- 过滤不应进入 package 的 JSON 文件。
- 生成 package zip。
- 计算 sha256。
- 上传版本文件。
- 更新 manifest。
- 更新 catalog。
- 更新 category index。
- 更新 search-lite。

迁移完成后，日常发布不再使用脚本。脚本可以保留为运维应急工具，但不作为产品发布入口。

## 14. 分阶段实施

### 阶段 1：服务端基础

- 新建 `skill-hub-server`。
- 增加用户认证。
- 增加角色模型。
- 增加数据库迁移。
- 增加客户端 HTTP API 配置。
- 前端显示登录状态和角色。

验收标准：

- 普通用户和管理员可以登录。
- 服务端能区分 `user` 和 `admin`。
- 管理员接口普通用户调用时返回 403。

### 阶段 2：需求区

- 新增需求列表。
- 新增需求详情。
- 新增创建需求。
- 新增需求评论。
- 新增状态筛选。

验收标准：

- 任意登录用户可以发表需求。
- 任意登录用户可以评论需求。
- 需求和评论在不同客户端之间同步。

### 阶段 3：草稿区

- 新增草稿列表。
- 新增草稿详情。
- 新增草稿元数据编辑。
- 新增草稿评论。
- 新增草稿上传。
- 新增草稿校验。
- 新增提交审核。

验收标准：

- 普通用户可以创建和提交自己的草稿。
- 普通用户不能编辑他人草稿。
- 管理员可以查看所有草稿。
- 校验失败时不能发布。

### 阶段 4：管理员发布

- 迁移 `publish-skill.ps1` 核心逻辑到服务端。
- 新增审核队列。
- 新增通过 / 驳回。
- 新增发布任务。
- 新增发布日志。
- 发布完成后刷新市场 catalog。

验收标准：

- 管理员可以从草稿发布 skill 到 MinIO。
- 普通用户不能调用发布接口。
- 发布成功后普通客户端刷新市场能看到新版本。
- 发布过程有审计日志。

### 阶段 5：治理和完善

- 用户管理。
- 评论删除 / 恢复。
- 需求归档。
- 草稿版本历史。
- 发布失败重试。
- 通知。
- 搜索。

## 15. 关键风险

- 如果把 MinIO 写密钥放进客户端，会导致普通用户获得发布权限。
- 如果没有服务端数据库，需求和评论难以一致同步。
- 如果 catalog 不是最后更新，客户端可能读到半发布状态。
- 如果发布没有幂等和任务状态，中断后容易产生脏数据。
- 如果没有审核和审计，管理员误操作难以追踪。

## 16. 推荐优先级

优先级从高到低：

1. 服务端和权限系统。
2. 需求区。
3. 草稿区。
4. 管理员审核。
5. 服务端发布到 MinIO。
6. 审计和治理能力。

不要先做“隐藏的管理员按钮 + 客户端直连 MinIO 写入”。这条路短期最快，但会直接破坏普通用户和管理员之间的权限边界。
