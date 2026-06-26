import { AlertCircle, CheckCircle2, ChevronRight, Download, Power, RefreshCw, X } from "lucide-react";
import type { DownloadUpdateResult, UpdateCheckResult } from "../../types";

export type AppUpdateDialogState = {
  open: boolean;
  phase: "checking" | "current" | "available" | "downloading" | "downloaded" | "error";
  result?: UpdateCheckResult | null;
  downloaded?: DownloadUpdateResult | null;
  error?: string | null;
  manual: boolean;
};

export function AppUpdateDialog(props: {
  state: AppUpdateDialogState;
  onCheck: () => void;
  onDownload: () => void;
  onRestart: () => void;
  onClose: () => void;
}) {
  const result = props.state.result;
  const downloaded = props.state.downloaded;
  const notes = result?.notes?.trim();
  const latestVersion = result?.latest_version || downloaded?.version || result?.current_version || "";
  const hasDownloadablePackage = Boolean(result?.downloadable ?? result?.package);
  const updateMessage = result?.message?.trim();
  const title =
    props.state.phase === "current"
      ? "已是最新版本"
      : props.state.phase === "downloaded"
        ? "更新已准备就绪"
        : props.state.phase === "error"
          ? "更新检查失败"
          : props.state.phase === "available"
            ? hasDownloadablePackage
              ? "发现新版本"
              : "发现新版本，缺少更新包"
            : props.state.phase === "downloading"
              ? "正在下载更新"
              : "正在检查更新";
  const subtitle =
    props.state.phase === "current"
      ? `当前版本 ${result?.current_version ?? ""} 已可放心使用。`
      : props.state.phase === "downloaded"
        ? "下载完成后会尝试自动切换到新版本。"
        : props.state.phase === "error"
          ? props.state.error ?? "暂时无法完成更新检查。"
          : props.state.phase === "available"
            ? hasDownloadablePackage
              ? `新版本 ${latestVersion} 可用，建议在空闲时完成更新。`
              : updateMessage ?? `新版本 ${latestVersion} 已发布，但当前设备没有匹配的更新包。`
            : props.state.phase === "downloading"
              ? "正在获取更新包，请保持网络连接。"
              : "正在连接更新源并校验可用版本。";
  const statusSummary =
    props.state.phase === "available"
      ? hasDownloadablePackage
        ? "已匹配更新包"
        : "版本已更新，包未匹配"
      : props.state.phase === "current"
        ? "当前已是最新"
        : props.state.phase === "downloaded"
          ? "更新包已下载"
          : props.state.phase === "error"
            ? "检查失败"
            : "正在检查";

  return (
    <div className="modal-backdrop app-update-backdrop" role="presentation">
      <section className="app-update-dialog" role="dialog" aria-modal="true" aria-labelledby="app-update-title">
        <button className="icon-button app-update-close" onClick={props.onClose} title="关闭">
          <X size={17} />
        </button>
        <div className="app-update-hero">
          <div className={`app-update-orb ${props.state.phase}`}>
            {props.state.phase === "current" ? (
              <CheckCircle2 size={30} />
            ) : props.state.phase === "error" ? (
              <AlertCircle size={30} />
            ) : props.state.phase === "downloaded" ? (
              <Download size={30} />
            ) : (
              <RefreshCw size={30} />
            )}
          </div>
          <div className="app-update-title-block">
            <span>Skill Hub Application Update</span>
            <h2 id="app-update-title">{title}</h2>
            <p>{subtitle}</p>
          </div>
        </div>

        <div className="app-update-body">
          <div className="app-update-version-card">
            <div>
              <span>当前版本</span>
              <strong>{result?.current_version ?? "未知"}</strong>
            </div>
            <ChevronRight size={18} />
            <div>
              <span>目标版本</span>
              <strong>{latestVersion || "等待检查"}</strong>
            </div>
          </div>

          <div className="app-update-status-strip">
            <span>{statusSummary}</span>
            {result?.distribution ? <b>{result.distribution}</b> : null}
            {result?.platform ? <b>{result.platform}</b> : null}
            {result?.arch ? <b>{result.arch}</b> : null}
          </div>

          {props.state.phase === "downloading" ? (
            <div className="app-update-progress" aria-label="正在下载更新">
              <span />
            </div>
          ) : null}

          {notes ? (
            <div className="app-update-notes">
              <strong>更新说明</strong>
              <p>{notes}</p>
            </div>
          ) : null}

          {props.state.phase === "available" && !hasDownloadablePackage ? (
            <div className="app-update-manual-tip">
              <AlertCircle size={16} />
              <span>
                请联系管理员检查 manifest 中是否存在 {result?.distribution ?? "当前分发"} / {result?.platform ?? "当前平台"} /{" "}
                {result?.arch ?? "当前架构"} 的包。
              </span>
            </div>
          ) : null}

          {props.state.phase === "downloaded" ? (
            <div className="app-update-manual-tip">
              <AlertCircle size={16} />
              <span>如果应用没有自动重启，请关闭当前窗口后手动启动 Skill Hub。</span>
            </div>
          ) : null}

          <div className="app-update-actions">
            {props.state.phase === "available" && hasDownloadablePackage ? (
              <button className="primary-soft app-update-primary" onClick={props.onDownload}>
                <Download size={17} />
                下载更新
              </button>
            ) : null}
            {props.state.phase === "available" && !hasDownloadablePackage ? (
              <button className="primary-soft" onClick={props.onCheck}>
                <RefreshCw size={17} />
                重新检查
              </button>
            ) : null}
            {props.state.phase === "downloaded" && downloaded?.ready_to_restart ? (
              <button className="primary-soft app-update-primary" onClick={props.onRestart}>
                <Power size={17} />
                立即重启
              </button>
            ) : null}
            {props.state.phase === "current" || props.state.phase === "error" ? (
              <button className="primary-soft" onClick={props.onCheck}>
                <RefreshCw size={17} />
                重新检查
              </button>
            ) : null}
            <button className="primary-soft" onClick={props.onClose}>
              {props.state.phase === "downloaded" ? "稍后处理" : "关闭"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
