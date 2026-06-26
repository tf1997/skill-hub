import type React from "react";

export function SourceChip(props: { children: React.ReactNode; tone: "market" | "local" }) {
  return <em className={`source-chip ${props.tone}`}>{props.children}</em>;
}
