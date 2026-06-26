export function EmptyState(props: { title: string; body: string }) {
  return (
    <div className="empty-state compact">
      <strong>{props.title}</strong>
      <span>{props.body}</span>
    </div>
  );
}
