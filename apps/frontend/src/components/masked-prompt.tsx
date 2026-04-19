import { Fragment } from "react";
import { cn } from "@/lib/utils";

const MASK_RE = /<<MASK:[^>]+:MASK>>/g;

export function MaskedPrompt({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  if (!text) return <span className={className}>—</span>;

  const parts: Array<{ kind: "text" | "mask"; value: string }> = [];
  let lastIndex = 0;
  for (const match of text.matchAll(MASK_RE)) {
    const start = match.index ?? 0;
    if (start > lastIndex) {
      parts.push({ kind: "text", value: text.slice(lastIndex, start) });
    }
    parts.push({ kind: "mask", value: match[0] });
    lastIndex = start + match[0].length;
  }
  if (lastIndex < text.length) {
    parts.push({ kind: "text", value: text.slice(lastIndex) });
  }

  return (
    <span className={cn(className)}>
      {parts.map((part, i) =>
        part.kind === "mask" ? (
          <span key={i} className="font-medium text-blue-500">
            {part.value}
          </span>
        ) : (
          <Fragment key={i}>{part.value}</Fragment>
        ),
      )}
    </span>
  );
}
