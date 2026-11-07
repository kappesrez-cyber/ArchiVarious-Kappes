import { describe, expect, it } from "vitest";
import {
  buildCaptureUri,
  captureFileName,
  dueCaptures,
  newCapture,
  parseCapture,
  serializeCapture,
} from "../src/inbox";

describe("inbox protocol", () => {
  const now = new Date("2026-03-04T05:06:07.000Z");
  const capture = newCapture('  remember: "quotes" & colons: ok #idea\nhttps://example.com', "widget-desktop", now);

  it("creates a lightly formatted capture with a scheduled survey", () => {
    expect(capture.title).toBe('Remember: "quotes" & colons: ok');
    expect(capture.tags).toEqual(["idea"]);
    expect(capture.status).toBe("inbox");
    expect(new Date(capture.surveyAt).getTime()).toBeGreaterThan(now.getTime());
  });

  it("round-trips through markdown", () => {
    expect(parseCapture(serializeCapture(capture))).toEqual(capture);
  });

  it("ignores files that are not captures", () => {
    expect(parseCapture("# Just a note")).toBeNull();
    expect(parseCapture("---\ntitle: x\n---\n\nbody")).toBeNull();
  });

  it("builds sortable file names", () => {
    expect(captureFileName(capture)).toBe(`20260304-050607-${capture.id}.md`);
  });

  it("builds obsidian:// capture URIs", () => {
    const uri = buildCaptureUri("hello world", "widget-mobile", "Brain");
    expect(uri).toBe("obsidian://archivarious?text=hello%20world&source=widget-mobile&vault=Brain");
  });
});

describe("dueCaptures", () => {
  it("keeps due, unfiled captures, longest-waiting first", () => {
    const now = new Date("2026-05-01T12:00:00.000Z");
    const make = (surveyAt: string, status: "inbox" | "filed" = "inbox") => ({
      capture: { ...newCapture("x", "plugin", now), surveyAt, status },
    });
    const late = make("2026-05-01T11:00:00.000Z");
    const early = make("2026-05-01T09:00:00.000Z");
    const filed = make("2026-05-01T08:00:00.000Z", "filed");
    const future = make("2026-05-01T13:00:00.000Z");
    expect(dueCaptures([late, filed, future, early], now)).toEqual([early, late]);
  });
});
