import { cn } from "@/lib/utils";
import { STRINGS } from "@/lib/strings";
import { ROUTES, type RouteId } from "@/routes";
import { badgeForRoute, type NavCounts } from "@/hooks/useNavCounts";

export interface SidebarProps {
  active: RouteId;
  onNavigate: (route: RouteId) => void;
  counts: NavCounts;
}

// Left sidebar nav (design-system Section 4.3, AC-1, AC-3). 212px column,
// active item carries `aria-current="page"`. Badges are omitted entirely
// (not "0") when their source data is not yet available, rather than
// fabricating a number (FD-27); see useNavCounts.
export function Sidebar({ active, onNavigate, counts }: SidebarProps) {
  return (
    <nav
      aria-label="Main"
      className="flex w-[212px] flex-col border-r border-border bg-titlebar px-3.5 pb-4 pt-5"
    >
      <ul className="flex flex-col gap-0.5">
        {ROUTES.map((route) => {
          const isActive = route.id === active;
          const badge = badgeForRoute(route.id, counts);
          return (
            <li key={route.id}>
              <button
                type="button"
                aria-current={isActive ? "page" : undefined}
                onClick={() => onNavigate(route.id)}
                className={cn(
                  "flex w-full items-center gap-2 rounded px-3 py-2 text-left text-[13.5px] text-ink-2",
                  "hover:bg-surface-2 hover:text-ink",
                  isActive && "bg-surface-2 font-semibold text-ink",
                )}
              >
                <span>{route.label}</span>
                {badge !== undefined && (
                  <span className="tabular-nums ml-auto text-[11.5px] text-ink-3">{badge}</span>
                )}
              </button>
            </li>
          );
        })}
      </ul>
      <div className="mt-auto px-3 text-[11.5px] leading-relaxed text-ink-3">
        {STRINGS.sidebarFooterProvenance}
      </div>
    </nav>
  );
}
