import { describe, expect, it, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { GoodNewsLine } from "../GoodNewsLine";
import type { GoodNews } from "@/lib/bindings";

afterEach(cleanup);

const ZERO: GoodNews = {
  already_tidy_books: 0,
  series_shelved: 0,
  empty_folders: 0,
  duplicate_groups: 0,
  duplicate_bytes: 0,
};

describe("GoodNewsLine", () => {
  it("renders nothing when every fact is zero", () => {
    const { container } = render(<GoodNewsLine goodNews={ZERO} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("states each nonzero fact with its own unit (FD-08)", () => {
    render(
      <GoodNewsLine
        goodNews={{
          already_tidy_books: 582,
          series_shelved: 34,
          empty_folders: 20,
          duplicate_groups: 403,
          duplicate_bytes: 10.1 * 1024 ** 3,
        }}
      />,
    );
    expect(screen.getByText(/582 books already in tidy folders/)).toBeInTheDocument();
    expect(screen.getByText(/34 series shelved together/)).toBeInTheDocument();
    expect(screen.getByText(/20 empty folders ready to sweep/)).toBeInTheDocument();
    expect(screen.getByText(/10\.1 GB of duplicate copies found/)).toBeInTheDocument();
  });

  it("omits a fact whose count is zero even when others are nonzero", () => {
    render(<GoodNewsLine goodNews={{ ...ZERO, already_tidy_books: 5 }} />);
    expect(screen.getByText(/5 books already in tidy folders/)).toBeInTheDocument();
    expect(screen.queryByText(/series shelved/)).toBeNull();
    expect(screen.queryByText(/empty folders/)).toBeNull();
    expect(screen.queryByText(/duplicate copies/)).toBeNull();
  });
});
