import { create, element } from "./dom.ts";
import type { StageSpec } from "./engine.ts";
import type { MeterState } from "./meters.ts";
import { reductionText, type PageContext, type PageView } from "./page-context.ts";
import { mountDynamicsPage } from "./page-dynamics.ts";
import { mountLevelPage } from "./page-level.ts";
import { mountLibraryPage } from "./page-library.ts";
import { mountResponsePage } from "./page-response.ts";
import { mountSections } from "./page-sections.ts";
import type { Session } from "./session.ts";
import {
  displayedBypassed,
  effectiveBypassed,
  FAMILY_LABELS,
  LIBRARY,
  padIndex,
  presetName,
  reportsGainReduction,
  resolveSelection,
  visibleStages,
  type SessionState,
} from "./session-state.ts";
import type { Store } from "./store.ts";
import { hostFromSession } from "./value-host.ts";

const DEFAULT_STAGE = "compressor";

export function mountCleanupCard(doc: Document, stage: StageSpec, schema: PageContext["session"]["schema"]): PageView {
  const card = create(doc, "div", "cleanup-card");
  const index = schema.stages.findIndex((other) => other.id === stage.id);
  const before = schema.stages[index - 1];
  card.append(
    create(doc, "span", "kicker", "NOTHING TO TUNE"),
    create(
      doc,
      "p",
      "cleanup-text",
      `This effect has no settings. Switch it off with its button in the chain, or remove it to get the ${Math.round(stage.latencyMs)} ms back.`,
    ),
  );
  const facts = create(doc, "dl", "cleanup-facts");
  const fact = (key: string, value: string, hot = false): void => {
    const row = create(doc, "div", "");
    const dd = create(doc, "dd", hot ? "is-hot" : "", value);
    row.append(create(doc, "dt", "", key), dd);
    facts.append(row);
  };
  fact("ADDED LATENCY", `+${Math.round(stage.latencyMs)} ms`, true);
  fact("FRAME", `${Math.round(stage.latencyMs)} ms`);
  fact("POSITION", before ? `AFTER ${before.label}` : "FIRST");
  card.append(facts);
  return { element: card, update: () => undefined };
}

export function initStagePage(session: Session, meters: Store<MeterState>, doc: Document): void {
  const kicker = element<HTMLElement>("#page-kicker", doc);
  const title = element<HTMLElement>("#page-title", doc);
  const blurb = element<HTMLElement>("#page-blurb", doc);
  const reduction = element<HTMLElement>("#page-gr", doc);
  const reset = element<HTMLButtonElement>("#page-reset", doc);
  const bypass = element<HTMLButtonElement>("#page-bypass", doc);
  const remove = element<HTMLButtonElement>("#page-remove", doc);
  const banner = element<HTMLElement>("#compare-banner", doc);
  const bannerText = element<HTMLElement>("#compare-text", doc);
  const bannerBack = element<HTMLButtonElement>("#compare-back", doc);
  const body = element<HTMLElement>("#page-body", doc);
  const context: PageContext = { doc, session, meters, host: hostFromSession(session) };
  const { schema } = session;
  let mountedKey = "";
  let views: PageView[] = [];
  let selection = LIBRARY;

  const selectedStage = (): StageSpec | null => schema.stages.find((stage) => stage.id === selection) ?? null;

  reset.addEventListener("click", () => {
    if (selection !== LIBRARY) {
      session.resetStage(selection);
    }
  });
  bypass.addEventListener("click", () => {
    if (selection !== LIBRARY) {
      session.toggleBypass(selection);
    }
  });
  remove.addEventListener("click", () => {
    if (selection !== LIBRARY) {
      session.removeStage(selection);
    }
  });
  bannerBack.addEventListener("click", () => session.toggleCompare());

  const mount = (): void => {
    const stage = selectedStage();
    if (!stage) {
      views = [mountLibraryPage(context)];
    } else {
      switch (stage.family) {
        case "level":
          views = [mountLevelPage(context, stage)];
          break;
        case "filter":
        case "sibilance":
        case "eq":
          views = [mountResponsePage(context, stage)];
          break;
        case "dynamics":
          views = [mountDynamicsPage(context, stage)];
          break;
        case "cleanup":
          views = [mountCleanupCard(doc, stage, schema)];
          break;
      }
      if (stage.family !== "cleanup") {
        const sections = mountSections(context, stage);
        if (sections) {
          views.push(sections);
        }
      }
    }
    body.replaceChildren(...views.map((view) => view.element));
  };

  const render = (state: SessionState, meter: MeterState): void => {
    selection = resolveSelection(schema, state, DEFAULT_STAGE);
    if (selection !== mountedKey) {
      mountedKey = selection;
      mount();
    }
    const stage = selectedStage();
    const stages = visibleStages(schema, state);
    if (!stage) {
      kicker.textContent = "EFFECTS";
      title.textContent = "Add an effect";
      blurb.textContent =
        "Every effect the engine knows about. The core effects are always in the chain; switch one off to skip it. Optional effects can be added and removed.";
      reduction.hidden = true;
      reset.hidden = true;
      bypass.hidden = true;
      remove.hidden = true;
    } else {
      const index = stages.findIndex((other) => other.id === stage.id);
      kicker.textContent = `STAGE ${padIndex(index + 1)} OF ${padIndex(stages.length)} · ${FAMILY_LABELS[stage.family]}`;
      title.textContent = stage.title;
      blurb.textContent = stage.blurb;
      const reportsReduction = reportsGainReduction(stage);
      reduction.hidden = !reportsReduction;
      if (reportsReduction) {
        const amount = meter.live ? (meter.gainReduction[stage.id] ?? 0) : 0;
        reduction.textContent = `GR ${reductionText(amount)} dB`;
        reduction.classList.toggle("is-active", amount >= 0.05);
      }
      reset.hidden = stage.family === "cleanup";
      bypass.hidden = !stage.bypassable;
      const bypassedNow = Boolean(effectiveBypassed(schema, state)[stage.id]);
      bypass.setAttribute("aria-pressed", String(Boolean(displayedBypassed(state)[stage.id])));
      bypass.classList.toggle("is-on", bypassedNow);
      remove.hidden = !stage.optional;
    }
    banner.hidden = state.compare === null;
    bannerText.textContent = `You are hearing ${presetName(state)} as saved. Your edits are kept; any change takes you back to them.`;
    for (const view of views) {
      view.update(state, meter);
    }
  };

  session.store.subscribe((state) => render(state, meters.get()));
  meters.subscribe((meter) => render(session.store.get(), meter));
  render(session.store.get(), meters.get());
}
