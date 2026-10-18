import { describe, expect, it } from "vitest";
import { formatBytes, formatDelta, formatDims, recoveryFor, shortenPath, timeAgo } from "./format";

describe("format helpers", () => {
  it("formats byte sizes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1500)).toBe("1.5 KB");
    expect(formatBytes(2_483_112)).toBe("2.5 MB");
    expect(formatBytes(250_000_000)).toBe("250 MB");
    expect(formatBytes(-1)).toBe("—");
  });

  it("formats dimensions and deltas", () => {
    expect(formatDims(2400, 1600)).toBe("2400 × 1600");
    expect(formatDims(null, 4)).toBe("—");
    expect(formatDelta(1000, 580)).toBe("−42%");
    expect(formatDelta(1000, 1080)).toBe("+8%");
    expect(formatDelta(1000, 1000)).toBe("±0%");
    expect(formatDelta(0, 10)).toBe("");
  });

  it("describes relative time", () => {
    const now = 1_000_000_000;
    expect(timeAgo(now - 10_000, now)).toBe("just now");
    expect(timeAgo(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(timeAgo(now - 3 * 3_600_000, now)).toBe("3 h ago");
  });

  it("maps every error to a recovery action", () => {
    expect(recoveryFor("destination_exists")).toBe("rename");
    expect(recoveryFor("destination_not_writable")).toBe("choose_folder");
    expect(recoveryFor("insufficient_storage")).toBe("choose_folder");
    expect(recoveryFor("corrupt_file")).toBe("choose_file");
    expect(recoveryFor("cancelled")).toBe("retry");
    expect(recoveryFor("internal")).toBe("none");
  });

  it("shortens long paths from the left", () => {
    expect(shortenPath("/a/b", 10)).toBe("/a/b");
    const s = shortenPath("/very/long/path/to/some/folder/Exports", 16);
    expect(s).toHaveLength(16);
    expect(s.endsWith("folder/Exports")).toBe(true);
  });
});
