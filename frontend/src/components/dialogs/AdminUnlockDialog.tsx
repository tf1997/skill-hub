import { KeyRound, X } from "lucide-react";

export function AdminUnlockDialog(props: {
  adminKey: string;
  onAdminKey: (value: string) => void;
  busy: boolean;
  onUnlock: () => void;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="admin-unlock-dialog" role="dialog" aria-modal="true" aria-labelledby="admin-unlock-title">
        <div className="preview-head">
          <div>
            <p>Admin</p>
            <h2 id="admin-unlock-title">管理员验证</h2>
            <span>验证通过后才会打开管理发布页面</span>
          </div>
          <button className="icon-button" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
        </div>
        <div className="admin-unlock-body">
          <label className="text-field">
            <span>管理员密钥</span>
            <input
              autoFocus
              type="password"
              value={props.adminKey}
              onChange={(event) => props.onAdminKey(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  props.onUnlock();
                }
              }}
              placeholder="输入管理员密钥"
            />
          </label>
          <div className="button-line">
            <button className="primary-action compact" onClick={props.onUnlock} disabled={props.busy}>
              <KeyRound size={17} />
              验证并进入
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
