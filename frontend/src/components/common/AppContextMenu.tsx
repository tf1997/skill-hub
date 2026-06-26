import { RefreshCw } from "lucide-react";

export function AppContextMenu(props: { x: number; y: number; onRefresh: () => void }) {
  return (
    <div className="app-context-menu" style={{ left: props.x, top: props.y }} role="menu">
      <button type="button" onClick={props.onRefresh} role="menuitem">
        <RefreshCw size={15} />
        刷新
      </button>
    </div>
  );
}
