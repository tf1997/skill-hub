import { Trash2, X } from "lucide-react";
import type { LocalSkill } from "../../types";

export function LocalDeleteDialog(props: {
  skill: LocalSkill;
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const name = props.skill.detectedManifest ?? props.skill.skillId ?? "本地 skill";
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="admin-unlock-dialog local-delete-dialog" role="dialog" aria-modal="true" aria-labelledby="local-delete-title">
        <div className="preview-head">
          <div>
            <p>Local</p>
            <h2 id="local-delete-title">删除本地 skill</h2>
            <span>{name}</span>
          </div>
          <button className="icon-button" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
        </div>
        <div className="admin-unlock-body">
          <div className="delete-summary">
            <strong>{name}</strong>
            <span>{props.skill.path}</span>
            <span>删除后会移除该本地目录，且无法恢复。</span>
          </div>
          <div className="button-line">
            <button className="primary-soft danger" onClick={props.onConfirm} disabled={props.busy}>
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
