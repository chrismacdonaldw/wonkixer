import { element } from "./dom.ts";
import { STOPPED, type AudioDevice, type EngineApi, type EngineState } from "./engine.ts";
import { describeError } from "./errors.ts";

export const VIRTUAL_CABLE_HINT = "CABLE Input";

export function preferredInput(inputs: AudioDevice[]): AudioDevice | undefined {
  return inputs.find((device) => device.isDefault) ?? inputs[0];
}

export function preferredOutput(outputs: AudioDevice[]): AudioDevice | undefined {
  return (
    outputs.find((device) => device.name.startsWith(VIRTUAL_CABLE_HINT)) ??
    outputs.find((device) => device.isDefault) ??
    outputs[0]
  );
}

export type PillState = "live" | "stopped" | "error";

export function pillStateFor(state: EngineState): PillState {
  if (state.error) {
    return "error";
  }
  return state.running ? "live" : "stopped";
}

const PILL_LABELS: Record<PillState, string> = { live: "LIVE", stopped: "STOPPED", error: "ERROR" };

export async function initEnginePanel(engine: EngineApi, doc: Document = document): Promise<void> {
  const inputSelect = element<HTMLSelectElement>("#input-device", doc);
  const outputSelect = element<HTMLSelectElement>("#output-device", doc);
  const toggle = element<HTMLButtonElement>("#engine-toggle", doc);
  const errorLine = element<HTMLElement>("#engine-error", doc);
  const pill = element<HTMLElement>("#engine-pill", doc);
  const pillLabel = element<HTMLElement>("#engine-pill-label", doc);
  const route = element<HTMLElement>("#engine-route", doc);

  let current: EngineState = STOPPED;

  const render = (state: EngineState): void => {
    current = state;
    const pillState = pillStateFor(state);
    toggle.textContent = state.running ? "STOP" : "START";
    toggle.classList.toggle("is-running", state.running);
    inputSelect.disabled = state.running;
    outputSelect.disabled = state.running;
    errorLine.hidden = !state.error;
    errorLine.textContent = state.error ?? "";
    pill.dataset.state = pillState;
    pillLabel.textContent = PILL_LABELS[pillState];
    route.textContent = state.running && state.input && state.output ? `${state.input.name} → ${state.output.name}` : "";
    if (state.input) {
      inputSelect.value = state.input.id;
    }
    if (state.output) {
      outputSelect.value = state.output.id;
    }
  };

  const failed = (error: unknown): EngineState => ({ ...STOPPED, error: describeError(error) });

  const toggleEngine = async (): Promise<void> => {
    toggle.disabled = true;
    try {
      render(current.running ? await engine.stop() : await engine.start(inputSelect.value, outputSelect.value));
    } catch (error) {
      render(failed(error));
    } finally {
      toggle.disabled = false;
    }
  };

  toggle.addEventListener("click", () => void toggleEngine());
  await engine.onState(render);

  try {
    const devices = await engine.listDevices();
    populate(inputSelect, devices.inputs, preferredInput(devices.inputs));
    populate(outputSelect, devices.outputs, preferredOutput(devices.outputs));
    render(await engine.state());
  } catch (error) {
    render(failed(error));
  }
}

function populate(select: HTMLSelectElement, devices: AudioDevice[], selected: AudioDevice | undefined): void {
  const doc = select.ownerDocument;
  select.replaceChildren(
    ...devices.map((device) => {
      const option = doc.createElement("option");
      option.value = device.id;
      option.textContent = device.name;
      option.selected = device.id === selected?.id;
      return option;
    }),
  );
}
