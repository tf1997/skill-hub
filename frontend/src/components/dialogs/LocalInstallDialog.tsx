import { AlertCircle, PackageCheck, X } from "lucide-react";
import type { Project } from "../../types";
import type { LocalInstallDialogState, LocalInstallLevel, LocalInstallTarget } from "../../lib/localSkills";

const targetLabels: Record<string, string> = {
  codex: "Codex",
  claude: "Claude"
};

export function LocalInstallDialog(props: {
  dialog: LocalInstallDialogState;
  target: LocalInstallTarget;
  onTarget: (value: LocalInstallTarget) => void;
  level: LocalInstallLevel;
  onLevel: (value: LocalInstallLevel) => void;
  projectPath: string;
  onProjectPath: (value: string) => void;
  projects: Project[];
  availableTargets: LocalInstallTarget[];
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const itemName =
    props.dialog.kind === "local"
      ? props.dialog.skill.detectedManifest ?? "本地 skill"
      : props.dialog.item.package.skillName;
  const itemPath =
    props.dialog.kind === "local"
      ? props.dialog.skill.path
      : `${props.dialog.item.package.skillId}@${props.dialog.item.package.version}`;
  const projectRequired = props.level === "project";
  const confirmDisabled = props.busy || props.availableTargets.length === 0 || (projectRequired && !props.projectPath);

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="admin-unlock-dialog local-install-dialog" role="dialog" aria-modal="true" aria-labelledby="local-install-title">
        <div className="preview-head">
          <div>
            <p>Install</p>
            <h2 id="local-install-title">安装自建 skill</h2>
            <span>{itemName}</span>
          </div>
          <button className="icon-button" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
        </div>
        <div className="admin-unlock-body local-install-dialog-body">
          <div className="delete-summary">
            <strong>{itemName}</strong>
            <span>{itemPath}</span>
          </div>
          <div className="field-row">
            <span>平台</span>
            <div className="segmented">
              {props.availableTargets.map((target) => (
                <button
                  key={target}
                  className={props.target === target ? "active" : ""}
                  onClick={() => props.onTarget(target)}
                >
                  {targetLabels[target] ?? target}
                </button>
              ))}
            </div>
          </div>
          <div className="field-row">
            <span>安装位置</span>
            <div className="segmented">
              <button
                className={props.level === "personal" ? "active" : ""}
                onClick={() => props.onLevel("personal")}
              >
                个人目录
              </button>
              <button
                className={props.level === "project" ? "active" : ""}
                onClick={() => props.onLevel("project")}
              >
                项目目录
              </button>
            </div>
          </div>
          {props.availableTargets.length === 0 ? (
            <div className="dialog-error">
              <AlertCircle size={16} />
              <span>已在所有支持的平台安装，无需重复安装。</span>
            </div>
          ) : null}
          {projectRequired ? (
            <label className="text-field">
              <span>绑定项目</span>
              <select value={props.projectPath} onChange={(event) => props.onProjectPath(event.target.value)}>
                <option value="">请选择项目</option>
                {props.projects.map((project) => (
                  <option key={project.id} value={project.path}>
                    {project.name} · {project.path}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          {projectRequired && props.projects.length === 0 ? (
            <div className="dialog-error">
              <AlertCircle size={16} />
              <span>请先在“项目”菜单绑定项目，再安装到项目目录。</span>
            </div>
          ) : null}
          <div className="button-line">
            <button className="primary-action compact" onClick={props.onConfirm} disabled={confirmDisabled}>
              <PackageCheck size={17} />
              安装
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
