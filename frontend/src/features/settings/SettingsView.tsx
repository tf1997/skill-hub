import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import { EmptyState } from "../../components/common/EmptyState";
import { targetLabels } from "../../app/viewModel";

export function SettingsView(props: {
  targetRoots: TargetRoot[];
  targetRootDrafts: Record<string, string>;
  onPickTargetRoot: (target: string) => void;
  onSaveTargetRoot: (target: string) => void;
}) {
  return (
    <section className="settings-grid">
      <div className="settings-stack">
        <div className="target-root-list">
          <h2>目标平台目录</h2>
          <p>市场下载后，只有启用时才写入对应平台目录。</p>
          {props.targetRoots.map((root) => (
            <div className="target-root-row" key={root.target}>
              <div>
                <strong>{targetLabels[root.target] ?? root.target}</strong>
                <span>{props.targetRootDrafts[root.target] || root.personalPath}</span>
              </div>
              <div className="row-actions">
                <button className="icon-text-button" onClick={() => props.onPickTargetRoot(root.target)}>
                  <FolderOpen size={17} />
                  选择
                </button>
                <button className="primary-soft" onClick={() => props.onSaveTargetRoot(root.target)}>
                  <Save size={17} />
                  保存
                </button>
              </div>
            </div>
          ))}
          {props.targetRoots.length === 0 ? (
            <EmptyState
              title="没有目标平台目录"
              body="配置 Codex 或 Claude 的个人级 skill 目录后，安装流程会显示写入位置。"
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}
