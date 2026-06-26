import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import type { GovernanceDialog } from "../../app/viewModel";

export function GovernanceDialogView(props: {
  dialog: GovernanceDialog;
  projectDraft: MarketProject;
  onProjectDraft: (value: MarketProject) => void;
  onSaveProject: () => void;
  categoryDraft: Category;
  onCategoryDraft: (value: Category) => void;
  onSaveCategory: () => void;
  onDeleteProject: (project: MarketProject) => void;
  onDeleteCategory: (category: Category) => void;
  busy: boolean;
  error: string | null;
  onClose: () => void;
}) {
  const updateProject = <K extends keyof MarketProject>(key: K, value: MarketProject[K]) =>
    props.onProjectDraft({ ...props.projectDraft, [key]: value });
  const updateCategory = <K extends keyof Category>(key: K, value: Category[K]) =>
    props.onCategoryDraft({ ...props.categoryDraft, [key]: value });
  const editingProject = props.dialog.kind === "project-edit";
  const editingCategory = props.dialog.kind === "category-edit";
  const projectForm = props.dialog.kind === "project-create" || editingProject;

  if (props.dialog.kind === "project-delete") {
    const project = props.dialog.project;
    return (
      <div className="modal-backdrop" role="presentation">
        <section className="admin-unlock-dialog governance-dialog" role="dialog" aria-modal="true">
          <div className="preview-head">
            <div>
              <p>Project</p>
              <h2>删除项目</h2>
              <span>{project.slug}</span>
            </div>
            <button className="icon-button" onClick={props.onClose} title="关闭">
              <X size={17} />
            </button>
          </div>
          <div className="admin-unlock-body">
            {props.error ? (
              <div className="dialog-error">
                <AlertCircle size={17} />
                <span>{props.error}</span>
              </div>
            ) : null}
            <div className="delete-summary">
              <strong>{project.name}</strong>
              <span>删除前必须先下架该项目下所有 skill。</span>
            </div>
            <div className="button-line">
              <button className="primary-soft danger" onClick={() => props.onDeleteProject(project)} disabled={props.busy}>
                <Trash2 size={17} />
                确认删除
              </button>
              <button className="primary-soft" onClick={props.onClose}>
                取消
              </button>
            </div>
          </div>
        </section>
      </div>
    );
  }

  if (props.dialog.kind === "category-delete") {
    const category = props.dialog.category;
    return (
      <div className="modal-backdrop" role="presentation">
        <section className="admin-unlock-dialog governance-dialog" role="dialog" aria-modal="true">
          <div className="preview-head">
            <div>
              <p>Public</p>
              <h2>删除公共分类</h2>
              <span>{category.id}</span>
            </div>
            <button className="icon-button" onClick={props.onClose} title="关闭">
              <X size={17} />
            </button>
          </div>
          <div className="admin-unlock-body">
            {props.error ? (
              <div className="dialog-error">
                <AlertCircle size={17} />
                <span>{props.error}</span>
              </div>
            ) : null}
            <div className="delete-summary">
              <strong>{category.name}</strong>
              <span>删除前必须先下架该分类下所有 skill。</span>
            </div>
            <div className="button-line">
              <button className="primary-soft danger" onClick={() => props.onDeleteCategory(category)} disabled={props.busy}>
                <Trash2 size={17} />
                确认删除
              </button>
              <button className="primary-soft" onClick={props.onClose}>
                取消
              </button>
            </div>
          </div>
        </section>
      </div>
    );
  }

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="admin-unlock-dialog governance-dialog" role="dialog" aria-modal="true">
        <div className="preview-head">
          <div>
            <p>{projectForm ? "Project" : "Public"}</p>
            <h2>
              {projectForm
                ? editingProject
                  ? "编辑项目"
                  : "新增项目"
                : editingCategory
                  ? "编辑公共分类"
                  : "新增公共分类"}
            </h2>
            <span>
              {projectForm
                ? editingProject
                  ? "更新项目名称和描述"
                  : "创建市场项目入口"
                : editingCategory
                  ? "更新公共市场分类"
                  : "创建公共市场分类"}
            </span>
          </div>
          <button className="icon-button" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
        </div>
        <div className="admin-unlock-body">
          {props.error ? (
            <div className="dialog-error">
              <AlertCircle size={17} />
              <span>{props.error}</span>
            </div>
          ) : null}
          {projectForm ? (
            <div className="meta-form single">
              <label className="text-field">
                <span>slug</span>
                <input
                  autoFocus
                  value={props.projectDraft.slug}
                  onChange={(event) => updateProject("slug", event.target.value)}
                  placeholder="project-a"
                  readOnly={editingProject}
                />
              </label>
              <label className="text-field">
                <span>名称</span>
                <input
                  value={props.projectDraft.name}
                  onChange={(event) => updateProject("name", event.target.value)}
                  placeholder="项目 A"
                />
              </label>
              <label className="text-field">
                <span>描述</span>
                <input
                  value={props.projectDraft.description}
                  onChange={(event) => updateProject("description", event.target.value)}
                  placeholder="项目市场说明"
                />
              </label>
              <label className="text-field">
                <span>排序</span>
                <input
                  type="number"
                  value={props.projectDraft.order}
                  onChange={(event) => updateProject("order", Number(event.target.value) || 10)}
                />
              </label>
              <button className="primary-action compact" onClick={props.onSaveProject} disabled={props.busy}>
                <Save size={17} />
                {editingProject ? "保存修改" : "保存项目"}
              </button>
            </div>
          ) : (
            <div className="meta-form single">
              <label className="text-field">
                <span>分类 slug</span>
                <input
                  autoFocus
                  value={props.categoryDraft.id}
                  onChange={(event) => updateCategory("id", event.target.value)}
                  placeholder="frontend"
                  readOnly={editingCategory}
                />
              </label>
              <label className="text-field">
                <span>名称</span>
                <input
                  value={props.categoryDraft.name}
                  onChange={(event) => updateCategory("name", event.target.value)}
                  placeholder="公共分类"
                />
              </label>
              <label className="text-field">
                <span>排序</span>
                <input
                  type="number"
                  value={props.categoryDraft.order}
                  onChange={(event) => updateCategory("order", Number(event.target.value) || 10)}
                />
              </label>
              <button className="primary-action compact" onClick={props.onSaveCategory} disabled={props.busy}>
                <Save size={17} />
                {editingCategory ? "保存修改" : "保存分类"}
              </button>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
