import type { SkillBinding } from "../../types";

export function BindingDots(props: { bindings: SkillBinding[] }) {
  if (props.bindings.length === 0) {
    return <span className="mini-status">未启用</span>;
  }

  return (
    <span className="binding-dots">
      {props.bindings.slice(0, 4).map((binding) => (
        <i
          key={binding.id}
          className={binding.enabled ? "enabled" : ""}
          title={`${binding.target}/${binding.level}`}
        />
      ))}
    </span>
  );
}
