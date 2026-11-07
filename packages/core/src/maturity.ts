import { nextReviewDays } from "./schedule";

/**
 * Note maturity ("зрелость заметки"): a filed note starts as a seed and grows with each review in which the
 * user adds something of their own: first the gist in their own words, then a personal example. The stage
 * lives in the note's frontmatter (`stage: seed | sprout | tree`), so plugin and widget agree on it.
 */
export type NoteStage = "seed" | "sprout" | "tree";

export const NOTE_STAGES: readonly NoteStage[] = ["seed", "sprout", "tree"];

export const STAGE_LABELS: Record<NoteStage, string> = {
  seed: "🌱 семечко",
  sprout: "🌿 росток",
  tree: "🌳 дерево",
};

/** Unknown or missing values (notes filed before maturity existed) count as seeds. */
export function parseStage(value: unknown): NoteStage {
  return NOTE_STAGES.includes(value as NoteStage) ? (value as NoteStage) : "seed";
}

/** The one small action a review offers to grow the note to its next stage. */
export interface GrowthStep {
  stage: NoteStage;
  /** Shown above the field in the survey. */
  prompt: string;
  placeholder: string;
  /** Bold label of the line written into the note: `**Главное:** …`. */
  label: string;
  /** Where the line goes: the gist on top of the note, the example at the end (before the "Связано:" line). */
  position: "top" | "end";
}

const STEPS: Partial<Record<NoteStage, GrowthStep>> = {
  seed: {
    stage: "seed",
    prompt: "Шаг роста: главное своими словами",
    placeholder: "В чём суть, одной строкой (можно пропустить)",
    label: "Главное",
    position: "top",
  },
  sprout: {
    stage: "sprout",
    prompt: "Шаг роста: свой пример",
    placeholder: "Где это встретилось или пригодится тебе (можно пропустить)",
    label: "Мой пример",
    position: "end",
  },
};

/** `null` for a tree: it has grown fully and the review only checks where it belongs. */
export function growthStep(stage: NoteStage): GrowthStep | null {
  return STEPS[stage] ?? null;
}

/** A note grows one stage when the user did the review's growth step, and never shrinks. */
export function nextStage(stage: NoteStage, grew: boolean): NoteStage {
  if (!grew) return stage;
  return NOTE_STAGES[Math.min(NOTE_STAGES.indexOf(stage) + 1, NOTE_STAGES.length - 1)] ?? stage;
}

/**
 * Days until the next review. A note that grew this time but is not a tree yet comes back (at a doubled
 * interval) for its next step, whatever its importance; otherwise the usual importance rule applies.
 */
export function reviewDaysAfterGrowth(
  previousDays: number,
  importance: 1 | 2 | 3,
  before: NoteStage,
  after: NoteStage,
): number | null {
  if (after !== before && after !== "tree") return Math.max(1, previousDays) * 2;
  return nextReviewDays(previousDays, importance);
}

const FRONTMATTER = /^---\n[\s\S]*?\n---\n?/;
const RELATED_LINE = /\n*Связано: [^\n]*\n?$/;

/**
 * Writes the user's growth line into the note text (frontmatter and the "Связано:" line are kept in place).
 * Line breaks in the answer are folded, so the line stays one line.
 */
export function addGrowthLine(noteText: string, step: GrowthStep, answer: string): string {
  const value = answer.replace(/\s+/g, " ").trim();
  if (!value) return noteText;
  const text = noteText.replace(/\r\n?/g, "\n");
  const frontmatter = FRONTMATTER.exec(text)?.[0] ?? "";
  let body = text.slice(frontmatter.length);
  const related = RELATED_LINE.exec(body)?.[0].trim() ?? "";
  body = body.replace(RELATED_LINE, "").trim();
  const line = `**${step.label}:** ${value}`;

  if (step.position === "top") {
    // Below a leading heading, if the note starts with one.
    const heading = /^#{1,6} [^\n]*/.exec(body)?.[0];
    const rest = heading ? body.slice(heading.length).trim() : body;
    body = [heading, line, rest].filter(Boolean).join("\n\n");
  } else {
    body = [body, line].filter(Boolean).join("\n\n");
  }
  const head = frontmatter ? `${frontmatter.replace(/\n?$/, "\n")}\n` : "";
  return `${head}${body}${related ? `\n\n${related}` : ""}\n`;
}
