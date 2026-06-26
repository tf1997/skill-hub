export function StatusPill(props: { busy: boolean; text: string }) {
  return (
    <div className={`status-pill ${props.busy ? "busy" : ""}`}>
      <span className="status-dot" aria-hidden="true" />
      <span className="status-text">{props.text}</span>
    </div>
  );
}
