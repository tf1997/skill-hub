import { BookOpen, Layers3, PackageCheck, RefreshCw, Rocket, ScrollText, X } from "lucide-react";

export type AboutPayload = {
  name: string;
  description: string;
  authors: string;
  version: string;
  docs_url: string;
  team: string;
  feedback_email: string;
};

export function AboutDialog(props: { about: AboutPayload; onOpenDocs: () => void; onFeedback: () => void; onClose: () => void }) {
  return (
    <div className="modal-backdrop about-backdrop" role="presentation">
      <section className="about-dialog" role="dialog" aria-modal="true" aria-labelledby="about-title">
        <div className="about-hero">
          <button className="icon-button about-close" onClick={props.onClose} title="关闭">
            <X size={17} />
          </button>
          <div className="about-brand-mark">
            <Layers3 size={32} />
          </div>
          <div className="about-title-block">
            <span>Skill Switchboard</span>
            <h2 id="about-title">Skill Hub</h2>
            <p>{props.about.description}</p>
          </div>
          <div className="about-version-pill">v{props.about.version}</div>
        </div>

        <div className="about-content">
          <div className="about-feature-grid">
            <div className="about-feature">
              <BookOpen size={18} />
              <div>
                <strong>Skill 市场</strong>
                <span>浏览团队沉淀的 skill，快速找到适合当前任务的能力。</span>
              </div>
            </div>
            <div className="about-feature">
              <PackageCheck size={18} />
              <div>
                <strong>本地安装</strong>
                <span>统一安装、启停、缓存和项目绑定，减少手动维护成本。</span>
              </div>
            </div>
            <div className="about-feature">
              <RefreshCw size={18} />
              <div>
                <strong>持续更新</strong>
                <span>检测应用版本和 skill 更新，保持工作区同步。</span>
              </div>
            </div>
            <div className="about-feature">
              <Rocket size={18} />
              <div>
                <strong>工作流加速</strong>
                <span>把常用能力带到 Codex / Claude 工作区，降低切换成本。</span>
              </div>
            </div>
          </div>

          <aside className="about-info-panel">
            <div className="about-info-row">
              <span>当前版本</span>
              <strong>v{props.about.version}</strong>
            </div>
            <div className="about-info-row">
              <span>支持目标</span>
              <strong>Codex / Claude</strong>
            </div>
            <div className="about-info-row">
              <span>开发团队</span>
              <strong>{props.about.team || props.about.authors || "Skill Hub Team"}</strong>
            </div>
            <div className="about-info-row">
              <span>问题反馈</span>
              <strong>{props.about.feedback_email}</strong>
            </div>
            <button className="primary-soft about-docs-button" onClick={props.onOpenDocs}>
              <BookOpen size={17} />
              在线文档
            </button>
            <button className="primary-soft about-feedback-button" onClick={props.onFeedback}>
              <ScrollText size={17} />
              问题反馈
            </button>
          </aside>

          <div className="about-actions">
            <button className="primary-soft" onClick={props.onClose}>
              关闭
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
