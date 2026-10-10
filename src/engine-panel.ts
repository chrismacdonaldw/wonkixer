import { element } from "./dom.ts";
import { STOPPED, type AudioDevice, type DevicePreferences, type EngineApi, type EngineState } from "./engine.ts";
import { describeError } from "./errors.ts";

export const VIRTUAL_CABLE_HINT = "CABLE Input";
const LOCKED_HINT = "Devices are locked while the engine is live. Stop it to switch.";
const ROUTING_HINT = "Discord, OBS and Zoom should use CABLE Output as their microphone.";

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

export interface EnginePanelOptions {
  devices?: DevicePreferences;
  autoStart?: boolean;
  onDevicesChange?: (devices: DevicePreferences) => void;
}

export interface EnginePanel {
  devices(): DevicePreferences;
}

export async function initEnginePanel(engine: EngineApi, doc: Document = document, options: EnginePanelOptions = {}): Promise<EnginePanel> {
  const inputSelect = element<HTMLSelectElement>("#input-device", doc);
  const outputSelect = element<HTMLSelectElement>("#output-device", doc);
  const toggle = element<HTMLButtonElement>("#engine-toggle", doc);
  const errorLine = element<HTMLElement>("#engine-error", doc);
  const pill = element<HTMLElement>("#engine-pill", doc);
  const pillLabel = element<HTMLElement>("#engine-pill-label", doc);
  const route = element<HTMLButtonElement>("#engine-route", doc);
  const routeText = element<HTMLElement>("#engine-route-text", doc);
  const popover = element<HTMLElement>("#devices-popover", doc);
  const hint = element<HTMLElement>("#devices-hint", doc);

  let current: EngineState = STOPPED;
  let popoverOpen = false;

  const selectedName = (select: HTMLSelectElement): string => select.selectedOptions[0]?.textContent ?? "No device";

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
    if (state.input && hasOption(inputSelect, state.input.id)) {
      inputSelect.value = state.input.id;
    }
    if (state.output && hasOption(outputSelect, state.output.id)) {
      outputSelect.value = state.output.id;
    }
    routeText.textContent =
      state.running && state.input && state.output
        ? `${state.input.name}  →  ${state.output.name}`
        : `${selectedName(inputSelect)}  →  ${selectedName(outputSelect)}`;
    hint.textContent = state.running ? LOCKED_HINT : ROUTING_HINT;
    popover.hidden = !popoverOpen;
    route.setAttribute("aria-expanded", String(popoverOpen));
    route.classList.toggle("is-open", popoverOpen);
  };

  const failed = (error: unknown): EngineState => ({ ...STOPPED, error: describeError(error) });

  const start = async (): Promise<void> => {
    toggle.disabled = true;
    try {
      render(await engine.start(inputSelect.value, outputSelect.value));
    } catch (error) {
      popoverOpen = true;
      render(failed(error));
    } finally {
      toggle.disabled = false;
    }
  };

  const toggleEngine = async (): Promise<void> => {
    if (current.running) {
      toggle.disabled = true;
      try {
        render(await engine.stop());
      } catch (error) {
        render(failed(error));
      } finally {
        toggle.disabled = false;
      }
      return;
    }
    await start();
  };

  const devices = (): DevicePreferences => ({ inputId: inputSelect.value || null, outputId: outputSelect.value || null });
  const changed = (): void => {
    render(current);
    options.onDevicesChange?.(devices());
  };

  toggle.addEventListener("click", () => void toggleEngine());
  route.addEventListener("click", () => {
    popoverOpen = !popoverOpen;
    render(current);
  });
  inputSelect.addEventListener("change", changed);
  outputSelect.addEventListener("change", changed);
  doc.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && popoverOpen) {
      popoverOpen = false;
      render(current);
    }
  });
  doc.addEventListener("pointerdown", (event) => {
    const target = event.target as Node;
    if (popoverOpen && !popover.contains(target) && !route.contains(target)) {
      popoverOpen = false;
      render(current);
    }
  });
  await engine.onState(render);

  try {
    const available = await engine.listDevices();
    populate(inputSelect, available.inputs, restore(available.inputs, options.devices?.inputId) ?? preferredInput(available.inputs));
    populate(outputSelect, available.outputs, restore(available.outputs, options.devices?.outputId) ?? preferredOutput(available.outputs));
    render(await engine.state());
    const saved = options.devices;
    const restoredBoth = saved?.inputId && saved?.outputId && inputSelect.value === saved.inputId && outputSelect.value === saved.outputId;
    if (options.autoStart && restoredBoth && !current.running) {
      await start();
    }
  } catch (error) {
    render(failed(error));
  }

  return { devices };
}

function hasOption(select: HTMLSelectElement, id: string): boolean {
  return Array.from(select.options).some((option) => option.value === id);
}

function restore(devices: AudioDevice[], id: string | null | undefined): AudioDevice | undefined {
  return id ? devices.find((device) => device.id === id) : undefined;
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
