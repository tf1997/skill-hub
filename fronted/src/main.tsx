import React, { Component, type ErrorInfo, type ReactNode } from "react";
import ReactDOM, { type Root } from "react-dom/client";
import App from "./App";
import "./styles.css";

type StartupErrorState = {
  error: Error | null;
  detail: string;
};

let appRoot: Root | null = null;

class StartupErrorBoundary extends Component<{ children: ReactNode }, StartupErrorState> {
  state: StartupErrorState = {
    error: null,
    detail: ""
  };

  static getDerivedStateFromError(error: Error): StartupErrorState {
    return {
      error,
      detail: error.stack ?? error.message
    };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const detail = [error.stack ?? error.message, info.componentStack].filter(Boolean).join("\n\n");
    this.setState({ error, detail });
  }

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return <StartupFailure error={this.state.error} detail={this.state.detail} />;
  }
}

function StartupFailure(props: { error: Error; detail: string }) {
  return (
    <main className="startup-failure">
      <section className="startup-failure-panel">
        <span className="startup-failure-kicker">启动诊断</span>
        <h1>Skill Hub 启动失败</h1>
        <p>应用已加载窗口，但前端渲染时发生错误。请把下面的错误信息发给开发团队定位。</p>
        <div className="startup-failure-summary">
          <strong>{props.error.name || "Error"}</strong>
          <span>{props.error.message || "未知错误"}</span>
        </div>
        <pre>{props.detail || props.error.message}</pre>
      </section>
    </main>
  );
}

function renderStartupError(reason: unknown) {
  const error = reason instanceof Error ? reason : new Error(String(reason));
  getAppRoot().render(<StartupFailure error={error} detail={error.stack ?? error.message} />);
}

function getAppRoot() {
  if (appRoot) {
    return appRoot;
  }

  const rootElement = document.getElementById("root");
  if (!rootElement) {
    throw new Error("Missing #root element");
  }

  appRoot = ReactDOM.createRoot(rootElement);
  return appRoot;
}

window.addEventListener("error", (event) => {
  renderStartupError(event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  renderStartupError(event.reason);
});

try {
  getAppRoot().render(
    <React.StrictMode>
      <StartupErrorBoundary>
        <App />
      </StartupErrorBoundary>
    </React.StrictMode>
  );
} catch (error) {
  renderStartupError(error);
}
