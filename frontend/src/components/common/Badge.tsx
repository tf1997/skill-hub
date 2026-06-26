import type React from "react";

export function Badge(props: { children: React.ReactNode; strong?: boolean }) {
  return <em className={`badge ${props.strong ? "strong" : ""}`}>{props.children}</em>;
}
