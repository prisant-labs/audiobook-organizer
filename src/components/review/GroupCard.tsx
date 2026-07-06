import { Check, Clock, Minus } from "lucide-react";
import { cn } from "@/lib/utils";
import { formatBytes } from "@/lib/format";
import type { PlanGroupView } from "@/lib/bindings";

export interface GroupCardProps {
  group: PlanGroupView;
  selected: boolean;
  onSelect: () => void;
  onToggle: (included: boolean) => void;
}

// One campaign-group card (F-502, design-system Section 4.9 `.bundle`): a
// plain-language headline and reason, a tabular-nums count/size meta line,
// and (right column) an include/skip switch plus a status tag. Icon-plus-
// label always accompanies the status (Section 8: color is never the only
// signal). Cards are keyboard-selectable (`tabindex`, Enter selects,
// Section 7).
export function GroupCard({ group, selected, onSelect, onToggle }: GroupCardProps) {
  const disabled = group.status === "checking";
  const checked = group.status === "included";

  return (
    <div
      role="button"
      tabIndex={0}
      aria-selected={selected}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter") onSelect();
      }}
      className={cn(
        "mb-2.5 grid cursor-pointer grid-cols-[1fr_auto] gap-x-3.5 gap-y-1 rounded-[10px] border border-border bg-surface p-4",
        "hover:border-border-2",
        selected && "border-primary ring-1 ring-primary",
      )}
    >
      <h3 className="col-start-1 text-[14px] font-semibold leading-snug">{group.headline}</h3>
      <p className="col-start-1 text-[12.5px] leading-relaxed text-ink-2">{group.reason}</p>
      <div className="col-start-1 mt-1 tabular-nums text-[11.5px] text-ink-3">
        {group.op_count.toLocaleString("en-US")} {group.op_count === 1 ? "change" : "changes"}
        {group.byte_size > 0 && <> &middot; {formatBytes(group.byte_size)}</>}
        {group.warning_count > 0 && (
          <> &middot; {group.warning_count} need{group.warning_count === 1 ? "s" : ""} a look</>
        )}
      </div>
      <div className="col-start-2 row-span-3 flex flex-col items-end justify-between">
        <button
          type="button"
          role="switch"
          aria-checked={checked}
          aria-label={`Include: ${group.headline}`}
          disabled={disabled}
          onClick={(e) => {
            e.stopPropagation();
            if (!disabled) onToggle(!checked);
          }}
          className={cn(
            "relative h-[21px] w-[38px] flex-none rounded-full border border-border-2 bg-surface-2 transition-colors",
            checked && "border-primary bg-primary",
            disabled && "cursor-not-allowed opacity-45",
          )}
        >
          <span
            className={cn(
              "absolute top-0.5 left-0.5 h-[15px] w-[15px] rounded-full bg-ink-3 transition-transform",
              checked && "translate-x-[17px] bg-white",
            )}
          />
        </button>
        <StatusTag status={group.status} />
      </div>
    </div>
  );
}

function StatusTag({ status }: { status: PlanGroupView["status"] }) {
  if (status === "included") {
    return (
      <span className="inline-flex items-center gap-1 whitespace-nowrap rounded-full bg-good-bg px-2.5 py-0.5 text-[10.5px] text-good">
        <Check aria-hidden="true" className="h-[10px] w-[10px]" strokeWidth={2.5} />
        included
      </span>
    );
  }
  if (status === "checking") {
    return (
      <span className="inline-flex items-center gap-1 whitespace-nowrap rounded-full bg-warn-bg px-2.5 py-0.5 text-[10.5px] text-warn">
        <Clock aria-hidden="true" className="h-[10px] w-[10px]" strokeWidth={2.5} />
        checking
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap rounded-full bg-surface-2 px-2.5 py-0.5 text-[10.5px] text-ink-3">
      <Minus aria-hidden="true" className="h-[10px] w-[10px]" strokeWidth={2.5} />
      left out
    </span>
  );
}
