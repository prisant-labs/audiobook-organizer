import {
  DERIVATION,
  GRID_RULE,
  MEASURED,
  OUT_OF_SCOPE,
  SPACING_STEPS,
  TYPE_EXCEPTIONS,
  TYPE_STEPS,
} from "./scale";
import { Section, Specimen } from "./Specimen";

// The scale proposal, rendered.
//
// WHAT THIS IS. A strawman for the one thing the design system does not govern:
// how big text is and how far apart things sit. It is derived from what the app
// already does (see `scale.ts` for the measurement and how to reproduce it), not
// from taste, so the argument to have with it is about the ROLES and the FOLD
// COST rather than about whether 13 is a nice number.
//
// WHY IT LIVES IN THE GALLERY AND NOT IN tokens.css. Writing steps into
// `src/styles/tokens.css` is a ratification. Until this is ratified it belongs
// where it can be looked at in both themes beside the components it would
// change, which is here.
//
// WHY THERE IS NO BEFORE AND AFTER PAIR. The "before" is the rest of this page:
// the real GroupCard, the real EmptyState, the real ErrorCallout, rendered from
// the real source. Hand-drawing a second copy of them at the proposed sizes would
// make exactly the artefact this gallery replaced, a drawing of the app that
// drifts from the app. Scroll down to see the before.
//
// The sizes below are set with an inline `style`, deliberately. They are the DATA
// this page is about, not design decisions taken in a class name, and the ratchet
// in scripts/check-arbitrary-values.mjs correctly counts the second kind. A
// proposal that raised the baseline it exists to lower would be a poor argument
// for itself.

/** A number with at most one decimal, so 12.5 keeps its half and 13 does not gain one. */
function px(n: number): string {
  return `${Number.isInteger(n) ? n : n.toFixed(1)}px`;
}

function signed(n: number): string {
  const rounded = Math.round(n * 10) / 10;
  if (rounded === 0) return "no change";
  return `${rounded > 0 ? "+" : ""}${rounded}px`;
}

/** The left-hand rail every row in this section shares, so the eye can scan down it. */
function StepLabel({ name, detail }: { name: string; detail: string }) {
  return (
    <div className="w-28 shrink-0">
      <div className="font-mono text-xs font-semibold text-ink">{name}</div>
      <div className="font-mono text-xs tabular-nums text-ink-3">{detail}</div>
    </div>
  );
}

function TypeScale() {
  return (
    <div className="flex flex-col gap-5">
      {TYPE_STEPS.map((step) => (
        <div key={step.name} className="flex items-start gap-4 border-b border-border pb-5 last:border-b-0 last:pb-0">
          <StepLabel name={step.name} detail={`${step.px} / ${step.linePx}`} />
          <div className="min-w-0 flex-1">
            <p
              className={`m-0 text-ink ${step.serif ? "font-serif" : ""} ${step.strong ? "font-semibold" : ""}`}
              style={{
                fontSize: step.px,
                lineHeight: `${step.linePx}px`,
                letterSpacing: step.tracking ? `${step.tracking}em` : undefined,
              }}
            >
              {step.sample}
            </p>
            <p className="mt-2 text-xs text-ink-3">{step.role}</p>
            <p className="mt-1 font-mono text-xs tabular-nums text-ink-3">
              replaces {step.folds.map((f) => `${px(f.px)} (${f.inline + f.utility})`).join(", ")}
              {step.tracking ? ` / tracking ${step.tracking}em` : ""}
            </p>
          </div>
        </div>
      ))}
    </div>
  );
}

function SpacingScale() {
  return (
    <div className="flex flex-col gap-4">
      {SPACING_STEPS.map((step) => {
        const uses = step.folds.reduce((sum, f) => sum + f.uses, 0);
        return (
          <div key={step.name} className="flex items-start gap-4">
            <StepLabel name={step.name} detail={`${step.px}px`} />
            <div className="flex w-24 shrink-0 items-center pt-1">
              <div className="h-2 rounded bg-primary" style={{ width: step.px }} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="m-0 text-xs text-ink-2">{step.role}</p>
              <p className="mt-1 font-mono text-xs tabular-nums text-ink-3">
                {uses} uses today, from {step.folds.map((f) => px(f.px)).join(", ")}
              </p>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** Every fold, most expensive first, so the cost of adopting this is legible. */
function FoldCost() {
  const rows = TYPE_STEPS.flatMap((step) =>
    step.folds.map((fold) => ({
      from: fold.px,
      to: step.px,
      step: step.name,
      uses: fold.inline + fold.utility,
      delta: step.px - fold.px,
    })),
  ).sort((a, b) => b.uses - a.uses);

  const moved = rows.filter((r) => r.delta !== 0).reduce((sum, r) => sum + r.uses, 0);
  const total = rows.reduce((sum, r) => sum + r.uses, 0);
  // Computed, never asserted. A hand-written "nothing moves more than 1.5px" sat
  // here until the render was read against the table under it, where two rows
  // move 2px. A claim about a table belongs to the table.
  const worst = rows.reduce((max, r) => Math.max(max, Math.abs(r.delta)), 0);
  const worstRows = rows.filter((r) => Math.abs(r.delta) === worst);
  const worstUses = worstRows.reduce((sum, r) => sum + r.uses, 0);

  return (
    <div>
      <p className="m-0 text-sm text-ink-2">
        Of the {total} places where the app sets a text size, {moved} change and {total - moved} stay
        exactly where they are. The largest move is {worst}px, at {worstUses}{" "}
        {worstUses === 1 ? "use" : "uses"} ({worstRows.map((r) => px(r.from)).join(", ")}).
      </p>
      <table className="mt-3 w-full border-collapse text-left">
        <thead>
          <tr className="border-b border-border">
            <th className="pb-2 pr-3 font-mono text-xs font-semibold text-ink-3">today</th>
            <th className="pb-2 pr-3 font-mono text-xs font-semibold text-ink-3">uses</th>
            <th className="pb-2 pr-3 font-mono text-xs font-semibold text-ink-3">step</th>
            <th className="pb-2 pr-3 font-mono text-xs font-semibold text-ink-3">becomes</th>
            <th className="pb-2 font-mono text-xs font-semibold text-ink-3">change</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={`${row.from}`} className="border-b border-border last:border-b-0">
              <td className="py-1 pr-3 font-mono text-xs tabular-nums text-ink">{px(row.from)}</td>
              <td className="py-1 pr-3 font-mono text-xs tabular-nums text-ink-2">{row.uses}</td>
              <td className="py-1 pr-3 font-mono text-xs text-ink-2">{row.step}</td>
              <td className="py-1 pr-3 font-mono text-xs tabular-nums text-ink">{px(row.to)}</td>
              <td
                className={`py-1 font-mono text-xs tabular-nums ${row.delta === 0 ? "text-ink-3" : "text-ink-2"}`}
              >
                {signed(row.delta)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Derivation() {
  return (
    <div className="flex flex-col gap-3 text-sm text-ink-2">
      <p className="m-0">{GRID_RULE}</p>
      <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
        {DERIVATION.map((rule) => (
          <li key={rule}>{rule}</li>
        ))}
      </ul>
      <p className="m-0 font-mono text-xs tabular-nums text-ink-3">
        Measured at {MEASURED.at}, across product source with this gallery excluded. Text:{" "}
        {MEASURED.type.inlineUses} inline uses of {MEASURED.type.inlineDistinct} sizes, plus{" "}
        {MEASURED.type.utilityUses} uses of {MEASURED.type.utilityDistinct} standard step
        {MEASURED.type.utilityDistinct === 1 ? "" : "s"}, {MEASURED.type.distinctSizes} distinct
        sizes in all. Spacing: {MEASURED.spacing.utilityUses} uses of{" "}
        {MEASURED.spacing.utilityDistinct} standard steps, plus {MEASURED.spacing.inlineUses}{" "}
        inline. Line height: {MEASURED.leading.utilityUses} uses of{" "}
        {MEASURED.leading.utilityDistinct} steps. Weight: {MEASURED.weight.utilityUses} uses of{" "}
        {MEASURED.weight.utilityDistinct}, already a scale.
      </p>
    </div>
  );
}

function Boundary() {
  return (
    <div className="flex flex-col gap-3 text-sm text-ink-2">
      <div>
        <p className="m-0 font-semibold text-ink">Outside the scale, on purpose</p>
        <ul className="mt-1 mb-0 flex list-disc flex-col gap-1 pl-5">
          {TYPE_EXCEPTIONS.map((e) => (
            <li key={e.px} className="tabular-nums">
              {px(e.px)}, {e.uses} use: {e.where}. Text inside drawn book art is scaled to the
              drawing, not to the interface.
            </li>
          ))}
        </ul>
      </div>
      <div>
        <p className="m-0 font-semibold text-ink">Not covered by this proposal</p>
        <ul className="mt-1 mb-0 flex list-disc flex-col gap-1 pl-5">
          {OUT_OF_SCOPE.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

export function ScaleProposal() {
  return (
    <Section
      title="Scale proposal, not ratified"
      blurb="Seven type steps and seven spacing steps, derived from what the app already ships. Nothing in tokens.css changed; this is here to be reacted to, and it is deleted either way."
    >
      <Specimen name="Type scale" state="7 steps" note="rendered at the proposed size and line height" wide>
        <TypeScale />
      </Specimen>
      <Specimen
        name="Spacing scale"
        state="7 steps"
        note="every step is a standard utility the app already uses, so adopting this adds no token"
        wide
      >
        <SpacingScale />
      </Specimen>
      <Specimen name="What it costs" state="type folds" note="most-used first" wide>
        <FoldCost />
      </Specimen>
      <Specimen name="How it was derived" wide>
        <Derivation />
      </Specimen>
      <Specimen name="Where it stops" wide>
        <Boundary />
      </Specimen>
    </Section>
  );
}
