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
//
// ARIA structure (fixed in v0.4.0 Phase 8, T-36, AC-41): the axe-core smoke
// caught real violations in the original single-`<div role="button"
// aria-selected>` wrapper. `aria-selected` is not an allowed attribute on
// `role="button"` (aria-allowed-attr); nesting the include/skip `<button
// role="switch">` inside that same interactive element was also a nested-
// interactive violation (screen readers cannot reliably reach a widget
// nested inside another widget). `aria-selected` IS valid on `role="option"`,
// but `option` in turn requires a `listbox`/`group`-of-options ancestor that
// owns ONLY option children (aria-required-children) - incompatible with the
// switch column living beside it in the same list, tried and rejected during
// this fix. The corrected pattern (this design-system Section 7 update:
// "aria-selected" -> "aria-pressed") keeps `role="button"` for the
// selectable region and conveys the same "this one is active" state via
// `aria-pressed`, a toggle-button semantic with no required-container
// constraint. The switch remains a SIBLING of the selectable region (not a
// descendant), so there is no nested-interactive violation either.
export function GroupCard({ group, selected, onSelect, onToggle }: GroupCardProps) {
  const disabled = group.status === "checking";
  const checked = group.status === "included";

  return (
    <div
      className={cn(
        "mb-2.5 grid grid-cols-[1fr_auto] gap-x-3.5 gap-y-1 rounded-[10px] border border-border bg-surface p-4",
        "hover:border-border-2",
        selected && "border-primary ring-1 ring-primary",
      )}
    >
      <div
        role="button"
        aria-pressed={selected}
        tabIndex={0}
        onClick={onSelect}
        onKeyDown={(e) => {
          if (e.key === "Enter") onSelect();
        }}
        className="col-start-1 col-span-1 row-span-3 grid cursor-pointer grid-cols-1 gap-y-1"
      >
        <h3 className="text-[14px] font-semibold leading-snug">{group.headline}</h3>
        <p className="text-[12.5px] leading-relaxed text-ink-2">{group.reason}</p>
        <div className="mt-1 tabular-nums text-[11.5px] text-ink-3">
          {group.op_count.toLocaleString("en-US")} {group.op_count === 1 ? "change" : "changes"}
          {group.byte_size > 0 && <> &middot; {formatBytes(group.byte_size)}</>}
          {group.warning_count > 0 && (
            <> &middot; {group.warning_count} need{group.warning_count === 1 ? "s" : ""} a look</>
          )}
        </div>
      </div>
      <div className="col-start-2 row-span-3 flex flex-col items-end justify-between">
        <button
          type="button"
          role="switch"
          aria-checked={checked}
          aria-label={`Include: ${group.headline}`}
          disabled={disabled}
          onClick={() => {
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
