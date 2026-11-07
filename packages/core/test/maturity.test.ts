import { describe, expect, it } from "vitest";
import { addGrowthLine, growthStep, nextStage, parseStage, reviewDaysAfterGrowth } from "../src/maturity";

describe("note maturity", () => {
  it("treats unknown stages as seeds", () => {
    expect(parseStage(undefined)).toBe("seed");
    expect(parseStage("oak")).toBe("seed");
    expect(parseStage("sprout")).toBe("sprout");
  });

  it("grows one stage when the step is done and stops at a tree", () => {
    expect(nextStage("seed", false)).toBe("seed");
    expect(nextStage("seed", true)).toBe("sprout");
    expect(nextStage("sprout", true)).toBe("tree");
    expect(nextStage("tree", true)).toBe("tree");
    expect(growthStep("tree")).toBeNull();
  });

  it("brings a growing note back for its next step regardless of importance", () => {
    expect(reviewDaysAfterGrowth(7, 1, "seed", "sprout")).toBe(14);
    expect(reviewDaysAfterGrowth(7, 1, "seed", "seed")).toBeNull();
    expect(reviewDaysAfterGrowth(14, 1, "sprout", "tree")).toBeNull();
    expect(reviewDaysAfterGrowth(14, 3, "sprout", "tree")).toBe(28);
  });

  const seed = growthStep("seed");
  const sprout = growthStep("sprout");
  if (!seed || !sprout) throw new Error("steps missing");
  const note = "---\nkind: idea\nstage: seed\n---\n\nПервая строка\nвторая\n\nСвязано: [[A]] · [[B]]\n";

  it("writes the gist on top, after the frontmatter", () => {
    expect(addGrowthLine(note, seed, "  суть\nв двух словах ")).toBe(
      "---\nkind: idea\nstage: seed\n---\n\n**Главное:** суть в двух словах\n\nПервая строка\nвторая\n\nСвязано: [[A]] · [[B]]\n",
    );
  });

  it("keeps a leading heading first", () => {
    expect(addGrowthLine("# Заголовок\n\nтекст\n", seed, "суть")).toBe("# Заголовок\n\n**Главное:** суть\n\nтекст\n");
  });

  it("writes the example at the end, before the links", () => {
    expect(addGrowthLine(note, sprout, "на работе")).toBe(
      "---\nkind: idea\nstage: seed\n---\n\nПервая строка\nвторая\n\n**Мой пример:** на работе\n\nСвязано: [[A]] · [[B]]\n",
    );
  });

  it("leaves the note alone for an empty answer", () => {
    expect(addGrowthLine(note, seed, "   ")).toBe(note);
  });
});
