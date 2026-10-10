import { create } from "./dom.ts";
import type { StageSpec } from "./engine.ts";
import type { PageContext, PageView } from "./page-context.ts";
import { displayedAdded, FAMILY_LABELS, type SessionState } from "./session-state.ts";

export function latencyLine(stage: StageSpec): string {
  if (stage.optional && stage.latencyMs > 0) {
    return `+${Math.round(stage.latencyMs)} ms LATENCY`;
  }
  if (stage.latencyMs > 0) {
    return `${Math.round(stage.latencyMs)} ms LOOKAHEAD`;
  }
  return "NO ADDED LATENCY";
}

export function firstSentence(text: string): string {
  return `${text.split(". ")[0].replace(/\.$/, "")}.`;
}

interface Card {
  stage: StageSpec;
  element: HTMLElement;
  status: HTMLElement;
  add: HTMLButtonElement;
  open: HTMLButtonElement;
}

export function mountLibraryPage(context: PageContext): PageView {
  const { doc, session } = context;
  const root = create(doc, "div", "library");
  const grid = create(doc, "div", "library-grid");
  const cards: Card[] = session.schema.stages
    .filter((stage) => stage.family !== "level")
    .map((stage) => {
      const element = create(doc, "div", "library-card");
      const head = create(doc, "div", "library-head");
      const status = create(doc, "span", "library-status");
      head.append(create(doc, "h2", "library-title", stage.label), status);
      const actions = create(doc, "div", "library-actions");
      const latency = create(doc, "span", "library-latency", latencyLine(stage));
      latency.classList.toggle("is-hot", stage.optional && stage.latencyMs > 0);
      const add = create(doc, "button", "button-primary", "ADD");
      add.type = "button";
      add.addEventListener("click", () => session.addStage(stage.id));
      const open = create(doc, "button", "button-ghost", "OPEN");
      open.type = "button";
      open.addEventListener("click", () => session.select(stage.id));
      actions.append(latency, add, open);
      element.append(
        head,
        create(doc, "span", "library-family", FAMILY_LABELS[stage.family]),
        create(doc, "p", "library-desc", firstSentence(stage.blurb)),
        actions,
      );
      grid.append(element);
      return { stage, element, status, add, open };
    });
  const note = create(
    doc,
    "p",
    "library-note",
    "Effects always run in the order the chain shows. A new effect slots into its place automatically, and saved presets keep working: anything a preset does not mention starts at its default.",
  );
  root.append(grid, note);

  const update = (state: SessionState): void => {
    const added = displayedAdded(session.schema, state);
    for (const card of cards) {
      const inChain = !card.stage.optional || Boolean(added[card.stage.id]);
      card.element.classList.toggle("is-available", !inChain);
      card.status.textContent = inChain ? "IN CHAIN" : "AVAILABLE";
      card.add.hidden = inChain;
      card.open.hidden = !inChain;
    }
  };

  return { element: root, update };
}
