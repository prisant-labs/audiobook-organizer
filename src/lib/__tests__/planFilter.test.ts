import { describe, expect, it } from "vitest";
import { DEFAULT_PLAN_FILTER, filterPlanOps, isPlanFilterActive } from "../planFilter";
import type { PlanOpView } from "@/lib/bindings";

function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 1,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\lib\\Sapiens by Yuval Noah Harari.m4b",
    target_path: "E:\\lib\\Yuval Noah Harari\\Sapiens\\Sapiens by Yuval Noah Harari.m4b",
    rationale: "Move this loose book into its own folder.",
    confidence: "high",
    byte_size: 1000,
    validation: "valid",
    validation_reason: null,
    warning_text: null,
    approval: "pending",
    matched_pattern: null,
    extracted_fields: [],
    stripped_noise: null,
    ...overrides,
  };
}

describe("isPlanFilterActive", () => {
  it("is false for the default filter", () => {
    expect(isPlanFilterActive(DEFAULT_PLAN_FILTER)).toBe(false);
  });

  it("is true when any field departs from its default", () => {
    expect(isPlanFilterActive({ ...DEFAULT_PLAN_FILTER, text: "sapiens" })).toBe(true);
    expect(isPlanFilterActive({ ...DEFAULT_PLAN_FILTER, group: "bundles" })).toBe(true);
    expect(isPlanFilterActive({ ...DEFAULT_PLAN_FILTER, confidence: "low" })).toBe(true);
    expect(isPlanFilterActive({ ...DEFAULT_PLAN_FILTER, status: "blocked" })).toBe(true);
  });
});

describe("filterPlanOps", () => {
  const ops = [
    op({ id: 1, group: "loose-books", confidence: "high", validation: "valid", source_path: "E:\\lib\\Sapiens.m4b" }),
    op({ id: 2, group: "bundles", confidence: "medium", validation: "warning", source_path: "E:\\lib\\Pack\\Dune.m4b" }),
    op({ id: 3, group: "loose-books", confidence: "low", validation: "blocked", source_path: "E:\\lib\\Gone.m4b" }),
  ];

  it("returns every op unchanged for the default filter", () => {
    expect(filterPlanOps(ops, DEFAULT_PLAN_FILTER)).toHaveLength(3);
  });

  it("narrows by free text over source/target/rationale, case-insensitively", () => {
    const result = filterPlanOps(ops, { ...DEFAULT_PLAN_FILTER, text: "dune" });
    expect(result.map((o) => o.id)).toEqual([2]);
  });

  it("narrows by group facet", () => {
    const result = filterPlanOps(ops, { ...DEFAULT_PLAN_FILTER, group: "bundles" });
    expect(result.map((o) => o.id)).toEqual([2]);
  });

  it("narrows by confidence facet", () => {
    const result = filterPlanOps(ops, { ...DEFAULT_PLAN_FILTER, confidence: "low" });
    expect(result.map((o) => o.id)).toEqual([3]);
  });

  it("narrows by warning-type (validation) facet", () => {
    const result = filterPlanOps(ops, { ...DEFAULT_PLAN_FILTER, status: "blocked" });
    expect(result.map((o) => o.id)).toEqual([3]);
  });

  it("combines facets and text (AND semantics)", () => {
    const result = filterPlanOps(ops, { ...DEFAULT_PLAN_FILTER, group: "loose-books", text: "gone" });
    expect(result.map((o) => o.id)).toEqual([3]);
  });

  it("returns everything again once cleared (AC-16)", () => {
    const narrowed = filterPlanOps(ops, { ...DEFAULT_PLAN_FILTER, group: "bundles" });
    expect(narrowed).toHaveLength(1);
    const cleared = filterPlanOps(ops, DEFAULT_PLAN_FILTER);
    expect(cleared).toHaveLength(3);
  });
});
