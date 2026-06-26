import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import { Badge } from "../../components/common/Badge";
import { EmptyState } from "../../components/common/EmptyState";

export function ProjectsView(props: {
  projects: Project[];
  bindings: SkillBinding[];
  name: string;
  path: string;
  onName: (value: string) => void;
  onPickPath: () => void;
  onSave: () => void;
  onUnbind: (project: Project) => void;
}) {
  return (
    <section className="content-stack projects-view">
      <div className="project-form">
        <label className="text-field">
          <span>项目名</span>
          <input
            value={props.name}
            onChange={(event) => props.onName(event.target.value)}
            placeholder="默认使用文件夹名称"
          />
        </label>
        <div className="path-picker grow">
          <div>
            <span>项目文件夹</span>
            <strong>{props.path || "尚未选择"}</strong>
          </div>
          <button className="icon-text-button" onClick={props.onPickPath}>
            <FolderOpen size={17} />
            选择
          </button>
        </div>
        <button className="primary-action compact" onClick={props.onSave} disabled={!props.path.trim()}>
          <FolderGit2 size={17} />
          绑定项目
        </button>
      </div>

      <div className="project-grid">
        {props.projects.length > 0 ? props.projects.map((project) => {
          const count = props.bindings.filter(
            (binding) => binding.projectPath === project.path
          ).length;
          return (
            <div className="project-tile" key={project.id}>
              <div className="project-tile-main">
                <strong>{project.name}</strong>
                <span>{project.path}</span>
              </div>
              <div className="project-tile-actions">
                <Badge strong>{count} skills</Badge>
                <button
                  className="icon-button danger"
                  onClick={() => props.onUnbind(project)}
                  title="解绑项目"
                >
                  <X size={16} />
                </button>
              </div>
            </div>
          );
        }) : (
          <EmptyState
            title="还没有项目绑定"
            body="选择一个项目文件夹后，Skill Hub 可以把 skill 安装到项目级目录。"
          />
        )}
      </div>
    </section>
  );
}
