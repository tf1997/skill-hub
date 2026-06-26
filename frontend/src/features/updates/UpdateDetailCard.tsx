import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import { Badge } from "../../components/common/Badge";
import { targetLabels } from "../../app/viewModel";

export function UpdateDetailCard(props: {
  update: UpdateCandidate;
  busy: boolean;
  onUpgrade: (update: UpdateCandidate) => void;
}) {
  const isPlugin = props.update.kind === "plugin";
  const artifactLabel = isPlugin ? "Plugin" : "Skill";
  const scopeText = props.update.level === "project" ? props.update.projectPath ?? "项目级" : "个人级";
  const steps = isPlugin
    ? [
        ["下载插件包", "从市场目录获取最新通用插件源和 pluginhub.json 元数据。"],
        ["生成平台目录", "根据目标动态生成 Codex / Claude 所需 manifest 和 marketplace 结构。"],
        ["执行 CLI 同步", "自动调用对应平台安装或同步命令，CLI 缺失时给出安装引导。"],
        ["刷新生效矩阵", "写回绑定版本并重新扫描个人级、项目级插件状态。"]
      ]
    : [
        ["下载 skill 包", "从市场目录获取最新 SKILL.md、README 和资源文件。"],
        ["覆盖安装目录", "保留原有范围、项目路径、启用状态和更新策略。"],
        ["刷新本地状态", "更新缓存记录、绑定版本和更新中心计数。"]
      ];

  return (
    <aside className="update-detail-card">
      <div className="detail-heading">
        <div>
          <p>{artifactLabel} update detail</p>
          <h2>{props.update.skillName}</h2>
          {props.update.skillName !== props.update.skillId ? <small>{props.update.skillId}</small> : null}
        </div>
        <Badge strong={!props.update.blockedReason}>{props.update.blockedReason ?? "可升级"}</Badge>
      </div>

      <div className="update-detail-grid">
        <div>
          <span>平台</span>
          <strong>{targetLabels[props.update.target] ?? props.update.target}</strong>
        </div>
        <div>
          <span>范围</span>
          <strong>{scopeText}</strong>
        </div>
        <div>
          <span>版本</span>
          <strong>
            {props.update.currentVersion} → {props.update.latestVersion}
          </strong>
        </div>
        <div>
          <span>更新策略</span>
          <strong>{props.update.updatePolicy}</strong>
        </div>
      </div>

      {props.update.blockedReason ? (
        <div className="update-blocker">
          <AlertCircle size={17} />
          <div>
            <strong>需要先处理</strong>
            <span>{props.update.blockedReason}</span>
          </div>
        </div>
      ) : null}

      <div className="update-pipeline">
        <h3>{artifactLabel} 自动升级动作</h3>
        {steps.map(([title, body], index) => (
          <div className="update-step" key={title}>
            <span>{index + 1}</span>
            <div>
              <strong>{title}</strong>
              <small>{body}</small>
            </div>
          </div>
        ))}
      </div>

      <div className="update-note">
        {isPlugin
          ? "插件升级不会要求草稿区保存 codex/claude 目录；平台专用目录在安装或升级时动态生成。"
          : "Skill 升级只替换包内容，绑定范围和启用状态继续沿用当前配置。"}
      </div>

      <div className="detail-action-bar">
        <button
          className="primary-action"
          disabled={!!props.update.blockedReason || props.busy}
          onClick={() => props.onUpgrade(props.update)}
        >
          <Rocket size={17} />
          升级到最新版本
        </button>
      </div>
    </aside>
  );
}
