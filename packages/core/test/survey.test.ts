import { describe, expect, it } from "vitest";
import { buildSurvey, suggestedFolders } from "../src/survey";

describe("suggestedFolders", () => {
  it("returns the best guesses first, then the user's sections", () => {
    const questions = buildSurvey(
      [
        { folder: "Учеба", score: 3, reasons: [] },
        { folder: "Работа", score: 1, reasons: [] },
      ],
      "Прочее",
      ["Дом", "Учеба"],
    );
    expect(suggestedFolders(questions)).toEqual(["Учеба", "Работа", "Дом"]);
    expect(suggestedFolders(buildSurvey([], "Прочее"), 3)).toEqual(["Прочее"]);
  });
});
