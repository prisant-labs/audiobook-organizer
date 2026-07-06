import type { ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";

// Window caption buttons (design-system Section 4.1). Uses
// `@tauri-apps/api/window`'s typed Window wrapper, NOT the raw `invoke` the
// no-raw-invoke lint (FD-29, T-05) forbids: this module never imports
// `@tauri-apps/api/core`. The commands these call
// (minimize/toggleMaximize/close) require the matching `core:window:allow-*`
// permissions added to src-tauri/capabilities/default.json in this same
// change.
//
// `getCurrentWindow()` reads `window.__TAURI_INTERNALS__`, which only exists
// inside an actual Tauri webview; it throws in any other context (Vitest's
// jsdom, a future Storybook, etc.). Calling it lazily inside each handler
// (rather than once at the top of the component) means this component can
// still render outside a Tauri runtime, and only a click that needs it can
// fail there.
export function WindowControls() {
  return (
    <div className="ml-2.5 flex h-10">
      <CaptionButton label="Minimize" onClick={() => void getCurrentWindow().minimize()}>
        <Minus size={14} />
      </CaptionButton>
      <CaptionButton label="Maximize" onClick={() => void getCurrentWindow().toggleMaximize()}>
        <Square size={11} />
      </CaptionButton>
      <CaptionButton
        label="Close"
        onClick={() => void getCurrentWindow().close()}
        className="hover:bg-[#c4262e] hover:text-white"
      >
        <X size={14} />
      </CaptionButton>
    </div>
  );
}

function CaptionButton({
  label,
  onClick,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={`flex w-[46px] items-center justify-center border-0 bg-transparent text-ink-2 hover:bg-surface-2 ${className ?? ""}`}
    >
      {children}
    </button>
  );
}
