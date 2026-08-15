import { Component, type ErrorInfo, type ReactNode } from "react";

// The frame around one specimen in the dev-only gallery.
//
// Every specimen is wrapped in an error boundary on purpose. Some components
// reach the Tauri runtime when they render (WindowControls calls
// getCurrentWindow; BookSlot resolves a cover over IPC), and the gallery runs in
// a plain browser where that bridge does not exist. Without a boundary, one such
// component blanks the entire page and the gallery reports nothing about the
// thirty components that render fine. With one, the failure is contained,
// labelled and visible, which is more useful than a component quietly missing
// from the list.

interface BoundaryProps {
  name: string;
  children: ReactNode;
}

interface BoundaryState {
  error: Error | null;
}

class SpecimenBoundary extends Component<BoundaryProps, BoundaryState> {
  state: BoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): BoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Keep the detail in the console: the on-page card stays short so a long
    // stack does not push every other specimen off the screen.
    console.error(`[gallery] ${this.props.name} failed to render`, error, info);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="rounded border border-danger bg-danger-bg p-3 text-sm text-danger">
          <p className="font-semibold">This component needs the Tauri runtime.</p>
          <p className="mt-1 font-mono text-xs">{this.state.error.message}</p>
        </div>
      );
    }
    return this.props.children;
  }
}

export interface SpecimenProps {
  /** The component name, as it is imported. */
  name: string;
  /** Which state this instance shows, e.g. "blocked" or "no cover". */
  state?: string;
  /** Why this specimen is here, when that is not obvious from the render. */
  note?: string;
  /** Give the specimen the full row rather than a column. */
  wide?: boolean;
  children: ReactNode;
}

export function Specimen({ name, state, note, wide, children }: SpecimenProps) {
  return (
    <figure
      className={`m-0 flex flex-col gap-2 ${wide ? "col-span-full" : ""}`}
      data-specimen={state ? `${name} / ${state}` : name}
    >
      <figcaption className="flex flex-wrap items-baseline gap-2">
        <span className="font-mono text-xs font-semibold text-ink">{name}</span>
        {state ? <span className="font-mono text-xs text-ink-3">{state}</span> : null}
        {note ? <span className="text-xs text-ink-3">{note}</span> : null}
      </figcaption>
      {/* bg-bg, not bg-surface: most components sit on the page background, and
          framing them on a card would flatter contrast they do not really have. */}
      <div className="rounded border border-border bg-bg p-4">
        <SpecimenBoundary name={name}>{children}</SpecimenBoundary>
      </div>
    </figure>
  );
}

export interface SectionProps {
  title: string;
  /** What this group of components is for, in one line. */
  blurb: string;
  children: ReactNode;
}

export function Section({ title, blurb, children }: SectionProps) {
  return (
    <section className="mt-10 first:mt-0">
      <div className="mb-4 border-b border-border pb-2">
        <h2 className="font-serif text-xl font-medium text-ink">{title}</h2>
        <p className="mt-1 text-sm text-ink-3">{blurb}</p>
      </div>
      <div className="grid grid-cols-2 gap-6">{children}</div>
    </section>
  );
}
