import { describe, expect, it } from "vitest";
import { formatBytes } from "../format";

describe("formatBytes", () => {
  it("renders a whole GB figure at 100 GB and above", () => {
    expect(formatBytes(297 * 1024 ** 3)).toBe("297 GB");
    expect(formatBytes(100 * 1024 ** 3)).toBe("100 GB");
  });

  it("renders one decimal below 100 GB", () => {
    expect(formatBytes(10.1 * 1024 ** 3)).toBe("10.1 GB");
  });

  it("renders MB below 1 GB", () => {
    expect(formatBytes(50 * 1024 ** 2)).toBe("50 MB");
  });

  it("renders KB below 1 MB", () => {
    expect(formatBytes(240_000)).toBe("234 KB");
  });

  it("renders a bare byte count below 1 KB, comma-grouped at 1000+", () => {
    expect(formatBytes(500)).toBe("500 bytes");
    expect(formatBytes(1000)).toBe("1,000 bytes");
  });
});
