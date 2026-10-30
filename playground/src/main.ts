import {
  PlaygroundAdaptationClock,
  PlaygroundBakedSurface,
  PlaygroundFieldGridHistory,
  PlaygroundFramePacer,
  PlaygroundResolutionController,
  PlaygroundTerrainCache,
  adapting_field_blur_shader_source,
  adapting_field_texels,
  baked_surface_shader_source,
  environment_field_shader_source,
  environment_field_texture_size,
  graphics_quality_scene_constants,
  landform_horizon_layer,
  landform_horizon_shader_source,
  occluder_field_shader_source,
  occluder_field_texture_size,
  playground_baked_surface_fill_bytes,
  playground_baked_surface_shader_source,
  playground_diagnostic_shader_source,
  playground_direct_scene_shader_source,
  playground_landform_horizon_shader_source,
  playground_occluder_field_shader_source,
  playground_scene_render_size,
  playground_scene_shader_source,
  playground_scene_uniform_bytes,
  playground_scene_uniforms,
  playground_startup_shader_source,
  playground_terrain_cache_fill_bytes,
  production_scene_shader_source,
  sample_playground_environment,
  sample_playground_view_cloud_state,
  sample_playground_world_geometry,
  sample_playground_world_phases,
  terrain_cache_shader_source,
  terrain_cache_texture_size,
  upscale_shader_source,
} from "browser/playground";
import {
  advanceFixedClock,
  fixedClockFraction,
  fixedTickSeconds,
  fixedTicksPerSecond,
  type FixedClock,
} from "./fixed-clock";
import { deriveIntermediateVisibility } from "./feature-visibility";
import { CloudMotion } from "./cloud-motion";
import { boundedCanvasDimensions } from "./canvas-dimensions";
import { startRenderLoop } from "./render-loop";
import {
  advanceProceduralTimeSeconds,
  maximumWorldYear,
  worldDateTimeSeconds,
  worldDaySeconds,
  worldYearDays,
} from "./world-date";
import {
  diagnosticGroups,
  diagnosticShaders,
  type DiagnosticSelection,
} from "./diagnostics";
import {
  benchSettings,
  summarizeScenario,
  type BenchScenario,
  type FrameSample,
  type PassTimes,
  type ScenarioSummary,
} from "./bench";
import benchScenarios from "./bench-scenarios.json" with { type: "json" };
import "./style.css";

const bench = benchSettings(location.search);
// The bench times the shipped shaders, which leave group 1 empty.
const productionShaders = Boolean(bench);

let directReference = false;
let occluderFieldInspection = 0;
let occluderFieldInfoElement: HTMLElement | undefined;
const occluderFieldLayers = [
  { name: "Sun cloud shadow", color: "#ff9e38" },
  { name: "Moon cloud shadow", color: "#8cb8ff" },
  { name: "Landform", color: "#33bf4d" },
  { name: "Sun landform horizon", color: "#ffd84d" },
  { name: "Moon landform horizon", color: "#7f8cff" },
] as const;
const landformHorizonLayer = landform_horizon_layer();

type State = {
  seed: string;
  cameraX: number;
  cameraY: number;
  worldWidthM: number;
  worldHeightM: number;
  viewHeight: number;
  worldTimeScale: number;
  worldTimeSeconds: number;
  sunLightVisible: number;
  moonLightVisible: number;
  skyLightVisible: number;
  canopyLightVisible: number;
  grassColorVisible: number;
  forestColorVisible: number;
  intertidalColorVisible: number;
  beachColorVisible: number;
  rockColorVisible: number;
  snowColorVisible: number;
  landIceColorVisible: number;
  waterColorVisible: number;
  bottomColorVisible: number;
  seaIceColorVisible: number;
  cloudShadowsVisible: number;
  legacyShadowLook: number;
  waterAttenuationLength: number;
  landformReliefVisible: number;
  landformShadowsVisible: number;
  landformShadowReach: number;
  renderScale: number;
  automaticResolution: number;
  maximumPixelRatio: number;
  frameRateCap: number;
  graphicsQuality: number;
  diagnostic: DiagnosticSelection;
};

type NumericStateKey = Exclude<
  keyof State,
  "seed" | "diagnostic"
>;
const defaults: State = {
  seed: "0x5eed",
  cameraX: 0,
  cameraY: 0,
  worldWidthM: 64_000,
  worldHeightM: 64_000,
  viewHeight: 480,
  worldTimeScale: 1,
  worldTimeSeconds: 37,
  sunLightVisible: 1,
  moonLightVisible: 1,
  skyLightVisible: 1,
  canopyLightVisible: 1,
  grassColorVisible: 1,
  forestColorVisible: 1,
  intertidalColorVisible: 1,
  beachColorVisible: 1,
  rockColorVisible: 1,
  snowColorVisible: 1,
  landIceColorVisible: 1,
  waterColorVisible: 1,
  bottomColorVisible: 1,
  seaIceColorVisible: 1,
  cloudShadowsVisible: 1,
  legacyShadowLook: 0,
  waterAttenuationLength: 0.5,
  landformReliefVisible: 1,
  landformShadowsVisible: 1,
  landformShadowReach: 320,
  renderScale: 1,
  automaticResolution: 0,
  maximumPixelRatio: 2,
  frameRateCap: 0,
  graphicsQuality: 2,
  diagnostic: "scene",
};
const graphicsQualities = ["low", "medium", "high"] as const;

const lunarMonthDays = 28;
const firstBloodMoonSeconds = 91 * worldDaySeconds - 4 * 60;
const celestialEventPresets = {
  "solar-eclipse": 105 * worldDaySeconds,
  "lunar-eclipse": firstBloodMoonSeconds - 60 * 60,
  "blood-moon": firstBloodMoonSeconds,
} as const;

function worldPhases(worldTimeSeconds: number): [number, number, number, number] {
  const phases = sample_playground_world_phases(worldTimeSeconds);
  return [phases[0]!, phases[1]!, phases[2]!, phases[3]!];
}
const lunarEpochDays = worldPhases(0)[3] * lunarMonthDays;

const state: State = { ...defaults };

const canvas = document.querySelector<HTMLCanvasElement>("#preview");
const controls = document.querySelector<HTMLElement>("#controls");
const tuningControls = document.querySelector<HTMLElement>("#tuning-controls");
const timeToolbar = document.querySelector<HTMLElement>("#time-toolbar");
const worldToolbar = document.querySelector<HTMLElement>("#world-toolbar");
const diagnosticToolbar = document.querySelector<HTMLElement>("#diagnostic-toolbar");
const status = document.querySelector<HTMLElement>("#status");
const scaleBar = document.querySelector<HTMLElement>(".scale-bar");
const scaleLabel = document.querySelector<HTMLElement>("#scale-label");
const resetTuningButton = document.querySelector<HTMLButtonElement>("#reset-tuning");
const playgroundMain = document.querySelector<HTMLElement>("main");
const sidebarResizer = document.querySelector<HTMLElement>("#sidebar-resizer");
if (!canvas || !controls || !tuningControls || !timeToolbar || !worldToolbar || !diagnosticToolbar || !status || !scaleBar || !scaleLabel || !resetTuningButton || !playgroundMain || !sidebarResizer) {
  throw new Error("Procedural lab page is incomplete");
}
const previewCanvas = canvas;
const controlsElement = controls;
const tuningControlsElement = tuningControls;
const timeToolbarElement = timeToolbar;
const worldToolbarElement = worldToolbar;
const diagnosticToolbarElement = diagnosticToolbar;
const statusElement = status;
const scaleBarElement = scaleBar;
const scaleLabelElement = scaleLabel;
const resetTuningButtonElement = resetTuningButton;
const playgroundMainElement = playgroundMain;
const sidebarResizerElement = sidebarResizer;

function setSidebarWidth(clientX: number): void {
  const rootFontSize = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  const minimumWidth = 18 * rootFontSize;
  const maximumWidth = Math.max(minimumWidth, playgroundMainElement.clientWidth - 20 * rootFontSize);
  const mainLeft = playgroundMainElement.getBoundingClientRect().left;
  const width = Math.min(maximumWidth, Math.max(minimumWidth, clientX - mainLeft));
  document.documentElement.style.setProperty("--panel-width", `${width}px`);
  sidebarResizerElement.setAttribute("aria-valuemin", String(Math.round(minimumWidth)));
  sidebarResizerElement.setAttribute("aria-valuemax", String(Math.round(maximumWidth)));
  sidebarResizerElement.setAttribute("aria-valuenow", String(Math.round(width)));
}

sidebarResizerElement.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  event.preventDefault();
  sidebarResizerElement.setPointerCapture(event.pointerId);
  document.body.classList.add("sidebar-resizing");
});
sidebarResizerElement.addEventListener("pointermove", (event) => {
  if (sidebarResizerElement.hasPointerCapture(event.pointerId)) setSidebarWidth(event.clientX);
});
const finishSidebarResize = (event: PointerEvent): void => {
  if (sidebarResizerElement.hasPointerCapture(event.pointerId)) {
    sidebarResizerElement.releasePointerCapture(event.pointerId);
  }
  document.body.classList.remove("sidebar-resizing");
};
sidebarResizerElement.addEventListener("pointerup", finishSidebarResize);
sidebarResizerElement.addEventListener("pointercancel", finishSidebarResize);
sidebarResizerElement.addEventListener("keydown", (event) => {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  event.preventDefault();
  const mainLeft = playgroundMainElement.getBoundingClientRect().left;
  const resizerBounds = sidebarResizerElement.getBoundingClientRect();
  const currentWidth = resizerBounds.left + resizerBounds.width / 2 - mainLeft;
  setSidebarWidth(mainLeft + currentWidth + (event.key === "ArrowLeft" ? -16 : 16));
});

let renderedCameraX = 0;
let renderedCameraY = 0;
const cloudMotion = new CloudMotion();
let adaptationClock = new PlaygroundAdaptationClock();

function format(value: number, step: number): string {
  return value.toFixed(step < 0.001 ? 4 : step < 0.01 ? 3 : step < 1 ? 2 : 0);
}

function renderControls(): void {
  controlsElement.replaceChildren();
  const diagnosticSection = document.createElement("section");
  diagnosticSection.className = "control-category diagnostics";
  diagnosticSection.innerHTML = "<h2>Diagnostics</h2>";
  const diagnosticLabel = document.createElement("div");
  diagnosticLabel.className = "select-control";
  const diagnosticHeading = document.createElement("label");
  diagnosticHeading.className = "control-name";
  diagnosticHeading.textContent = "Output";
  const diagnosticSelect = document.createElement("select");
  diagnosticSelect.id = "diagnostic-output";
  diagnosticHeading.htmlFor = diagnosticSelect.id;
  for (const domain of ["weather", "landscape", "water"] as const) {
    for (const group of diagnosticGroups[domain]) {
      if (group.label === "Overview" && domain !== "weather") continue;
      const optionGroup = document.createElement("optgroup");
      optionGroup.label = group.label === "Overview"
        ? group.label
        : `${domain.charAt(0).toUpperCase()}${domain.slice(1)} · ${group.label}`;
      for (const [value, name] of group.options) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = name;
        option.selected = state.diagnostic === value;
        optionGroup.append(option);
      }
      diagnosticSelect.append(optionGroup);
    }
  }
  const diagnosticReset = document.createElement("button");
  diagnosticReset.type = "button";
  diagnosticReset.className = "reset-value";
  diagnosticReset.textContent = "↺";
  diagnosticReset.title = "Reset diagnostic output to default";
  diagnosticReset.setAttribute("aria-label", diagnosticReset.title);
  const updateDiagnosticUI = () => {
    diagnosticReset.hidden = state.diagnostic === "scene";
  };
  diagnosticSelect.addEventListener("change", () => {
    state.diagnostic = diagnosticSelect.value as DiagnosticSelection;
    if (state.diagnostic !== "scene") {
      occluderFieldInspection = 0;
      fieldViewSelect.value = "0";
    }
    updateDiagnosticUI();
  });
  diagnosticReset.addEventListener("click", () => {
    state.diagnostic = "scene";
    diagnosticSelect.value = state.diagnostic;
    updateDiagnosticUI();
  });
  updateDiagnosticUI();
  diagnosticLabel.append(
    diagnosticHeading,
    diagnosticSelect,
    diagnosticReset,
  );
  diagnosticSection.append(diagnosticLabel);
  const directLabel = document.createElement("div");
  directLabel.className = "control toggle-control no-reset";
  const directInput = document.createElement("input");
  directInput.id = "direct-reference";
  directInput.type = "checkbox";
  directInput.checked = directReference;
  const directHeading = document.createElement("label");
  directHeading.className = "control-name";
  directHeading.htmlFor = directInput.id;
  directHeading.textContent = "Direct reference";
  directInput.addEventListener("change", () => {
    directReference = directInput.checked;
    if (directReference) {
      occluderFieldInspection = 0;
      fieldViewSelect.value = "0";
    }
  });
  directLabel.append(directInput, directHeading);
  const fieldViewLabel = document.createElement("div");
  fieldViewLabel.className = "select-control field-control";
  const fieldViewHeading = document.createElement("label");
  fieldViewHeading.className = "control-name";
  fieldViewHeading.textContent = "Field view";
  const fieldViewSelect = document.createElement("select");
  fieldViewSelect.id = "occluder-field-view";
  fieldViewHeading.htmlFor = fieldViewSelect.id;
  for (const [value, label] of [
    ["0", "Scene"],
    ["1", "Grids"],
    ["2", "Coverage"],
  ] as const) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    fieldViewSelect.append(option);
  }
  fieldViewSelect.value = String(occluderFieldInspection);
  fieldViewSelect.addEventListener("change", () => {
    occluderFieldInspection = Number(fieldViewSelect.value);
    if (occluderFieldInspection !== 0) {
      directReference = false;
      directInput.checked = false;
      state.diagnostic = "scene";
      diagnosticSelect.value = state.diagnostic;
      updateDiagnosticUI();
    }
  });
  fieldViewLabel.append(fieldViewHeading, fieldViewSelect);
  occluderFieldInfoElement = document.createElement("div");
  occluderFieldInfoElement.className = "field-info";
  diagnosticSection.append(directLabel, fieldViewLabel, occluderFieldInfoElement);
  controlsElement.append(diagnosticSection);
}

type TuningControl =
  | { type: "toggle"; key: NumericStateKey; label: string }
  | {
      type: "range";
      key: NumericStateKey;
      label: string;
      min: number;
      max: number;
      step: number;
    };

type TuningGroup = {
  label: string;
  open?: boolean;
  controls: readonly TuningControl[];
};

const tuningGroups: readonly TuningGroup[] = [
  {
    label: "Lighting",
    open: true,
    controls: [
      { type: "toggle", key: "sunLightVisible", label: "Direct Sun" },
      { type: "toggle", key: "moonLightVisible", label: "Direct Moon" },
      { type: "toggle", key: "skyLightVisible", label: "Clear-sky ambient" },
      { type: "toggle", key: "canopyLightVisible", label: "Canopy ambient" },
      { type: "toggle", key: "legacyShadowLook", label: "Legacy shadow look" },
    ],
  },
  {
    label: "Land",
    controls: [
      { type: "toggle", key: "landformReliefVisible", label: "Relief shading" },
      { type: "toggle", key: "landformShadowsVisible", label: "Cast shadows" },
      { type: "range", key: "landformShadowReach", label: "High-light reach · m", min: 40, max: 1200, step: 10 },
      { type: "toggle", key: "grassColorVisible", label: "Grass" },
      { type: "toggle", key: "forestColorVisible", label: "Forest" },
      { type: "toggle", key: "intertidalColorVisible", label: "Intertidal ground" },
      { type: "toggle", key: "beachColorVisible", label: "Beach sediment" },
      { type: "toggle", key: "rockColorVisible", label: "Rock" },
      { type: "toggle", key: "snowColorVisible", label: "Snow" },
    ],
  },
  {
    label: "Water",
    controls: [
      { type: "toggle", key: "waterColorVisible", label: "Water body" },
      { type: "toggle", key: "bottomColorVisible", label: "Visible bottom" },
      { type: "range", key: "waterAttenuationLength", label: "Bottom attenuation · m", min: 0.1, max: 4, step: 0.05 },
    ],
  },
  {
    label: "Ice",
    controls: [
      { type: "toggle", key: "landIceColorVisible", label: "Land ice" },
      { type: "toggle", key: "seaIceColorVisible", label: "Sea ice" },
    ],
  },
  {
    label: "Clouds",
    controls: [
      { type: "toggle", key: "cloudShadowsVisible", label: "Cloud shadows" },
    ],
  },
  {
    label: "Rendering",
    controls: [
      { type: "range", key: "renderScale", label: "Scene scale", min: 0.5, max: 1, step: 0.05 },
      { type: "toggle", key: "automaticResolution", label: "Automatic scale" },
      { type: "range", key: "maximumPixelRatio", label: "Pixel-ratio cap", min: 1, max: 4, step: 0.25 },
      { type: "range", key: "frameRateCap", label: "FPS cap · 0 off", min: 0, max: 240, step: 5 },
      { type: "range", key: "graphicsQuality", label: "Tier · 0 low, 2 high", min: 0, max: 2, step: 1 },
    ],
  },
];

const tuningControlSyncs: Array<() => void> = [];

function renderTuningControls(): void {
  const note = document.createElement("p");
  note.className = "composition-note";
  note.innerHTML = "<strong>Color layers × lighting</strong><span>These switches are orthogonal: enabled rows render through every enabled light.</span>";
  tuningControlsElement.append(note);
  for (const group of tuningGroups) {
    const details = document.createElement("details");
    details.open = group.open ?? false;
    const summary = document.createElement("summary");
    summary.textContent = group.label;
    const body = document.createElement("div");
    body.className = "tuning-group";

    for (const control of group.controls) {
      const reset = document.createElement("button");
      reset.type = "button";
      reset.className = "reset-value";
      reset.textContent = "↺";
      reset.title = `Reset ${control.label}`;
      reset.setAttribute("aria-label", reset.title);
      const syncReset = () => {
        const atDefault = state[control.key] === defaults[control.key];
        reset.classList.toggle("reset-value-hidden", atDefault);
        reset.disabled = atDefault;
      };

      if (control.type === "toggle") {
        const row = document.createElement("div");
        row.className = "control toggle-control";
        const input = document.createElement("input");
        input.type = "checkbox";
        input.id = `tuning-${control.key}`;
        const label = document.createElement("label");
        label.className = "control-name";
        label.htmlFor = input.id;
        label.textContent = control.label;
        const sync = () => {
          input.checked = state[control.key] > 0.5;
          syncReset();
        };
        input.addEventListener("change", () => {
          state[control.key] = Number(input.checked);
          syncReset();
        });
        reset.addEventListener("click", () => {
          state[control.key] = defaults[control.key];
          sync();
        });
        sync();
        tuningControlSyncs.push(sync);
        row.append(input, label, reset);
        body.append(row);
        continue;
      }

      const row = document.createElement("div");
      row.className = "control range-control";
      const label = document.createElement("label");
      label.className = "control-name";
      const rangeInput = document.createElement("input");
      rangeInput.type = "range";
      rangeInput.id = `tuning-${control.key}`;
      rangeInput.min = String(control.min);
      rangeInput.max = String(control.max);
      rangeInput.step = String(control.step);
      label.htmlFor = rangeInput.id;
      label.textContent = control.label;
      const valueInput = document.createElement("input");
      valueInput.type = "number";
      valueInput.className = "range-value";
      valueInput.min = rangeInput.min;
      valueInput.max = rangeInput.max;
      valueInput.step = rangeInput.step;
      const sync = () => {
        rangeInput.value = String(state[control.key]);
        valueInput.value = format(state[control.key], control.step);
        syncReset();
      };
      const setValue = (value: number) => {
        state[control.key] = Math.min(control.max, Math.max(control.min, value));
        sync();
      };
      rangeInput.addEventListener("input", () => setValue(rangeInput.valueAsNumber));
      valueInput.addEventListener("input", () => {
        if (Number.isFinite(valueInput.valueAsNumber)) setValue(valueInput.valueAsNumber);
      });
      valueInput.addEventListener("change", sync);
      reset.addEventListener("click", () => {
        state[control.key] = defaults[control.key];
        sync();
      });
      sync();
      tuningControlSyncs.push(sync);
      row.append(label, rangeInput, valueInput, reset);
      body.append(row);
    }

    details.append(summary, body);
    tuningControlsElement.append(details);
  }
}

renderTuningControls();
resetTuningButtonElement.addEventListener("click", () => {
  for (const group of tuningGroups) {
    for (const control of group.controls) state[control.key] = defaults[control.key];
  }
  for (const sync of tuningControlSyncs) sync();
});

const minimumTimeScaleSlider = -2;
const maximumTimeScale = 100000;
function timeScaleFromSlider(value: number): number {
  return value <= minimumTimeScaleSlider ? 0 : Number((10 ** value).toPrecision(3));
}
function timeScaleToSlider(value: number): number {
  return value <= 0 ? minimumTimeScaleSlider : Math.log10(value);
}

worldToolbarElement.innerHTML = `
  <details open>
    <summary>World definition</summary>
    <section class="world-toolbar-section"><h3>World definition</h3>
      <div class="text-control no-reset"><label class="control-name" for="seed">Seed</label><input id="seed" type="text" value="${state.seed}"></div>
      <div class="control range-control no-reset"><label class="control-name" for="world-width">Longitude width</label><input id="world-width" type="range" min="8" max="256" step="1" value="${state.worldWidthM / 1000}"><input id="world-width-value" class="range-value" type="number" min="8" max="256" step="1" value="${state.worldWidthM / 1000}"></div>
      <div class="control range-control no-reset"><label class="control-name" for="world-height">Pole-to-pole span</label><input id="world-height" type="range" min="8" max="256" step="1" value="${state.worldHeightM / 1000}"><input id="world-height-value" class="range-value" type="number" min="8" max="256" step="1" value="${state.worldHeightM / 1000}"></div>
    </section>
  </details>`;
timeToolbarElement.innerHTML = `
  <details open>
    <summary>Date &amp; time</summary>
    <section class="world-toolbar-section"><h3>World calendar</h3>
      <div class="world-datetime-grid">
        <label><span>Solar year</span><input id="world-year" type="number" min="1" max="${maximumWorldYear}" step="1"></label>
        <label><span>Year day</span><input id="world-year-day" type="number" min="1" max="365" step="1"></label>
        <label><span>Lunar month</span><input id="world-lunar-month" type="number" readonly></label>
        <label><span>Lunar day</span><input id="world-lunar-day" type="number" readonly></label>
        <label><span>Time</span><input id="world-time" type="time" step="1"></label>
      </div>
    </section>
    <section class="world-toolbar-section"><h3>Celestial events</h3>
      <nav class="category-presets celestial-event-presets" aria-label="Celestial event presets"><button id="solar-eclipse-preset" type="button">Solar eclipse</button><button id="lunar-eclipse-preset" type="button">Lunar eclipse</button><button id="blood-moon-preset" type="button">Blood moon</button></nav>
    </section>
  </details>
  <details open>
    <summary>Playback</summary>
    <section class="world-toolbar-section"><h3>Playback</h3>
      <div class="control range-control no-reset"><label class="control-name" for="world-time-scale">World clock · ×</label><input id="world-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.worldTimeScale)}"><input id="world-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.worldTimeScale}"></div>
    </section>
  </details>`;
diagnosticToolbarElement.innerHTML = `
  <details>
    <summary>View-center diagnostics</summary>
    <div class="view-center-diagnostics">
      <section><h3>Sample position</h3><div id="position-measurements" class="derived-grid"></div></section>
      <section><h3 id="derived-output-heading">Derived atmosphere</h3><div id="derived-output" class="derived-grid"></div></section>
    </div>
  </details>`;
const seedInput = worldToolbarElement.querySelector<HTMLInputElement>("#seed")!;
const worldWidthInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-width")!;
const worldWidthValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-width-value")!;
const worldHeightInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-height")!;
const worldHeightValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-height-value")!;
const worldYearInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-year")!;
const worldYearDayInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-year-day")!;
const worldLunarMonthInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-lunar-month")!;
const worldLunarDayInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-lunar-day")!;
const worldTimeInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-time")!;
const positionMeasurementsElement = diagnosticToolbarElement.querySelector<HTMLElement>("#position-measurements")!;
const derivedOutputHeadingElement = diagnosticToolbarElement.querySelector<HTMLElement>("#derived-output-heading")!;
const derivedOutputElement = diagnosticToolbarElement.querySelector<HTMLElement>("#derived-output")!;
const diagnosticsDetailsElement = positionMeasurementsElement.closest<HTMLDetailsElement>("details")!;
const worldTimeScaleInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-time-scale")!;
const worldTimeValueInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-time-value")!;

function syncWorldDateInputs(): void {
  const totalSeconds = Math.max(0, Math.floor(state.worldTimeSeconds));
  const totalDays = Math.floor(totalSeconds / worldDaySeconds);
  const dayOfYear = totalDays % worldYearDays;
  const lunarDays = Math.floor(totalSeconds / worldDaySeconds + lunarEpochDays);
  const secondsOfDay = totalSeconds % worldDaySeconds;
  const hours = Math.floor(secondsOfDay / 3600);
  const minutes = Math.floor(secondsOfDay % 3600 / 60);
  const seconds = secondsOfDay % 60;
  const values: Array<[HTMLInputElement, string]> = [
    [worldYearInput, String(Math.floor(totalDays / worldYearDays) + 1)],
    [worldYearDayInput, String(dayOfYear + 1)],
    [worldLunarMonthInput, String(Math.floor(lunarDays / lunarMonthDays) + 1)],
    [worldLunarDayInput, String(lunarDays % lunarMonthDays + 1)],
    [worldTimeInput, [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":")],
  ];
  for (const [input, value] of values) {
    if (document.activeElement !== input && input.value !== value) input.value = value;
  }
}

function setWorldDateFromInputs(): void {
  const year = worldYearInput.valueAsNumber || 1;
  const day = worldYearDayInput.valueAsNumber || 1;
  const [hours = 0, minutes = 0, seconds = 0] = worldTimeInput.value
    .split(":")
    .map((value) => Number.parseInt(value, 10) || 0);
  state.worldTimeSeconds = worldDateTimeSeconds(year, day, hours, minutes, seconds);
  syncWorldDateInputs();
}

for (const input of [worldYearInput, worldYearDayInput, worldTimeInput]) {
  input.addEventListener("change", setWorldDateFromInputs);
}

function applyCelestialEventPreset(event: keyof typeof celestialEventPresets): void {
  state.worldTimeSeconds = celestialEventPresets[event];
  syncWorldDateInputs();
}
timeToolbarElement.querySelector<HTMLButtonElement>("#solar-eclipse-preset")!
  .addEventListener("click", () => applyCelestialEventPreset("solar-eclipse"));
timeToolbarElement.querySelector<HTMLButtonElement>("#lunar-eclipse-preset")!
  .addEventListener("click", () => applyCelestialEventPreset("lunar-eclipse"));
timeToolbarElement.querySelector<HTMLButtonElement>("#blood-moon-preset")!
  .addEventListener("click", () => applyCelestialEventPreset("blood-moon"));
seedInput.addEventListener("input", () => {
  state.seed = seedInput.value;
});
function setWorldDimension(
  key: "worldWidthM" | "worldHeightM",
  valueKm: number,
  rangeInput: HTMLInputElement,
  valueInput: HTMLInputElement,
): void {
  const clampedKm = Math.min(256, Math.max(8, valueKm));
  state[key] = clampedKm * 1000;
  rangeInput.value = String(clampedKm);
  valueInput.value = String(clampedKm);
}
worldWidthInput.addEventListener("input", () => {
  setWorldDimension("worldWidthM", worldWidthInput.valueAsNumber, worldWidthInput, worldWidthValueInput);
});
worldWidthValueInput.addEventListener("input", () => {
  if (Number.isFinite(worldWidthValueInput.valueAsNumber)) {
    setWorldDimension("worldWidthM", worldWidthValueInput.valueAsNumber, worldWidthInput, worldWidthValueInput);
  }
});
worldWidthValueInput.addEventListener("change", () => {
  setWorldDimension("worldWidthM", state.worldWidthM / 1000, worldWidthInput, worldWidthValueInput);
});
worldHeightInput.addEventListener("input", () => {
  setWorldDimension("worldHeightM", worldHeightInput.valueAsNumber, worldHeightInput, worldHeightValueInput);
});
worldHeightValueInput.addEventListener("input", () => {
  if (Number.isFinite(worldHeightValueInput.valueAsNumber)) {
    setWorldDimension("worldHeightM", worldHeightValueInput.valueAsNumber, worldHeightInput, worldHeightValueInput);
  }
});
worldHeightValueInput.addEventListener("change", () => {
  setWorldDimension("worldHeightM", state.worldHeightM / 1000, worldHeightInput, worldHeightValueInput);
});
function syncTimeScaleInputs(): void {
  worldTimeScaleInput.value = String(timeScaleToSlider(state.worldTimeScale));
  worldTimeValueInput.value = String(state.worldTimeScale);
}
function setTimeScale(value: number): void {
  state.worldTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  syncTimeScaleInputs();
}
worldTimeScaleInput.addEventListener("input", () => {
  setTimeScale(timeScaleFromSlider(worldTimeScaleInput.valueAsNumber));
});
worldTimeValueInput.addEventListener("input", () => {
  if (Number.isFinite(worldTimeValueInput.valueAsNumber)) {
    setTimeScale(worldTimeValueInput.valueAsNumber);
  }
});
worldTimeValueInput.addEventListener("change", syncTimeScaleInputs);
syncTimeScaleInputs();
syncWorldDateInputs();

renderControls();

let dragPointer: number | undefined;
let dragX = 0;
let dragY = 0;

function canvasViewPoint(clientX: number, clientY: number, rect: DOMRect): [number, number] {
  return [
    (clientX - rect.left) / rect.width - 0.5,
    0.5 - (clientY - rect.top) / rect.height,
  ];
}

previewCanvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  dragPointer = event.pointerId;
  [dragX, dragY] = canvasViewPoint(
    event.clientX,
    event.clientY,
    previewCanvas.getBoundingClientRect(),
  );
  previewCanvas.setPointerCapture(event.pointerId);
  previewCanvas.classList.add("dragging");
});
previewCanvas.addEventListener("pointermove", (event) => {
  if (event.pointerId !== dragPointer) return;
  const [viewX, viewY] = canvasViewPoint(
    event.clientX,
    event.clientY,
    previewCanvas.getBoundingClientRect(),
  );
  const aspect = previewCanvas.clientWidth / previewCanvas.clientHeight;
  state.cameraX -= (viewX - dragX) * state.viewHeight * aspect;
  state.cameraY -= (viewY - dragY) * state.viewHeight;
  dragX = viewX;
  dragY = viewY;
});
function finishDrag(event: PointerEvent): void {
  if (event.pointerId !== dragPointer) return;
  dragPointer = undefined;
  previewCanvas.classList.remove("dragging");
}
previewCanvas.addEventListener("pointerup", finishDrag);
previewCanvas.addEventListener("pointercancel", finishDrag);
previewCanvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  const rect = previewCanvas.getBoundingClientRect();
  const [pointerX, pointerY] = canvasViewPoint(event.clientX, event.clientY, rect);
  const oldHeight = state.viewHeight;
  const oldWidth = oldHeight * rect.width / rect.height;
  const deltaScale = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? rect.height : 1;
  const newHeight = Math.min(10000000, Math.max(0.1, oldHeight * Math.exp(event.deltaY * deltaScale * 0.001)));
  const newWidth = newHeight * rect.width / rect.height;
  state.cameraX += pointerX * (oldWidth - newWidth);
  state.cameraY += pointerY * (oldHeight - newHeight);
  state.viewHeight = newHeight;
}, { passive: false });

async function showStartupPhase(message: string): Promise<void> {
  statusElement.textContent = message;
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
}

await showStartupPhase("Requesting WebGPU adapter…");
const gpu = navigator.gpu;
if (!gpu) throw new Error("WebGPU is unavailable");
const adapter = await gpu.requestAdapter({ powerPreference: "high-performance" });
if (!adapter) throw new Error("No WebGPU adapter available");
const adapterName = adapter.info?.description
  || adapter.info?.device
  || adapter.info?.architecture
  || adapter.info?.vendor
  || "WebGPU adapter";
await showStartupPhase("Creating WebGPU device…");
const timestampQueriesSupported = adapter.features.has("timestamp-query");
const device = await adapter.requestDevice({
  requiredFeatures: timestampQueriesSupported ? ["timestamp-query"] : [],
});
let renderLoopActive = true;
function stopRendering(message: string): void {
  if (!renderLoopActive) return;
  renderLoopActive = false;
  statusElement.textContent = message;
}
function endGpuSession(message: string): void {
  stopRendering(message);
  device.destroy();
}
if (import.meta.hot) {
  import.meta.hot.dispose(() => endGpuSession("WebGPU session ended"));
}
window.addEventListener(
  "pagehide",
  () => endGpuSession("WebGPU session ended; reload to resume"),
  { once: true },
);
void device.lost.then(() => {
  stopRendering("WebGPU device lost; reload to resume");
});
const context = previewCanvas.getContext("webgpu") as GPUCanvasContext;
const formatName = gpu.getPreferredCanvasFormat();
const sdrViewFormat = `${formatName}-srgb` as GPUTextureFormat;
let hdrSupported = false;
if (window.matchMedia("(dynamic-range: high)").matches) {
  try {
    context.configure({
      device, format: "rgba16float", alphaMode: "opaque",
      toneMapping: { mode: "extended" },
    });
    hdrSupported = context.getConfiguration?.()?.toneMapping?.mode === "extended";
  } catch {
    hdrSupported = false;
  }
}
let canvasHdr = hdrSupported;
function configureCanvas(hdr: boolean): void {
  context.configure(hdr ? {
    device, format: "rgba16float", alphaMode: "opaque",
    toneMapping: { mode: "extended" },
  } : {
    device, format: formatName, viewFormats: [sdrViewFormat], alphaMode: "opaque",
  });
  canvasHdr = hdr;
}
configureCanvas(hdrSupported);

await showStartupPhase("Creating startup shader…");
const startupShader = device.createShaderModule({ code: playground_startup_shader_source() });
function createPlaygroundPipeline(
  shader: GPUShaderModule,
  entryPoint: string,
  constants?: Record<string, number>,
  format?: GPUTextureFormat,
): Promise<GPURenderPipeline> {
  return device.createRenderPipelineAsync({
    layout: "auto",
    vertex: {
      module: shader,
      entryPoint: "vertex",
      buffers: [{
        arrayStride: 24,
        attributes: [
          { shaderLocation: 0, offset: 0, format: "float32x2" },
          { shaderLocation: 1, offset: 8, format: "float32x3" },
          { shaderLocation: 2, offset: 20, format: "float32" },
        ],
      }],
    },
    fragment: {
      module: shader,
      entryPoint,
      ...(constants ? { constants } : {}),
      targets: [{ format: format ?? (hdrSupported && (
        entryPoint === "fragment_playground_scene"
          || entryPoint === "fragment_playground_startup"
          || entryPoint === "fragment"
      ) ? "rgba16float" : sdrViewFormat) }],
    },
    primitive: { topology: "triangle-list" },
  });
}
await showStartupPhase("Compiling startup pipeline…");
const startupPipeline = await createPlaygroundPipeline(
  startupShader,
  "fragment_playground_startup",
);

const vertices = new Float32Array([
  -1, -1, 0, 0, 0, 1, 1, -1, 0, 0, 0, 1, -1, 1, 0, 0, 0, 1,
  -1, 1, 0, 0, 0, 1, 1, -1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1,
]);
const vertexBuffer = device.createBuffer({ size: vertices.byteLength, usage: 0x20 | 0x08 });
device.queue.writeBuffer(vertexBuffer, 0, vertices);
const upscaleShader = device.createShaderModule({ code: upscale_shader_source() });
type UpscalePipelines = { easu: GPURenderPipeline; rcas: GPURenderPipeline };
const upscalePipelines = new Map<GPUTextureFormat, UpscalePipelines>();
type UpscaleTargets = {
  key: string;
  textures: GPUTexture[];
  pipelines: UpscalePipelines;
  sceneView: GPUTextureView;
  easuInput: GPUBindGroup;
  easuOutput: GPUTextureView;
  rcasInput: GPUBindGroup;
};
let upscaleTargets: UpscaleTargets | undefined;
// Both filters read and write display-encoded values through non-sRGB views.
function prepareUpscale(sceneWidth: number, sceneHeight: number): UpscaleTargets {
  const format: GPUTextureFormat = canvasHdr ? "rgba16float" : formatName;
  const key = [sceneWidth, sceneHeight, previewCanvas.width, previewCanvas.height, format].join();
  if (upscaleTargets?.key === key) return upscaleTargets;
  for (const texture of upscaleTargets?.textures ?? []) texture.destroy();
  let pipelines = upscalePipelines.get(format);
  if (!pipelines) {
    const pipeline = (entryPoint: string) => device.createRenderPipeline({
      layout: "auto",
      vertex: { module: upscaleShader, entryPoint: "fullscreen_vertex" },
      fragment: { module: upscaleShader, entryPoint, targets: [{ format }] },
      primitive: { topology: "triangle-list" },
    });
    pipelines = { easu: pipeline("easu"), rcas: pipeline("rcas") };
    upscalePipelines.set(format, pipelines);
  }
  const texture = (width: number, height: number, viewFormats: GPUTextureFormat[]) =>
    device.createTexture({ size: [width, height], format, usage: 0x10 | 0x04, viewFormats });
  const scene = texture(sceneWidth, sceneHeight, canvasHdr ? [] : [sdrViewFormat]);
  const easuOutput = texture(previewCanvas.width, previewCanvas.height, []);
  const input = (pipeline: GPURenderPipeline, source: GPUTexture) => device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: source.createView() }],
  });
  upscaleTargets = {
    key,
    textures: [scene, easuOutput],
    pipelines,
    sceneView: scene.createView({ format: canvasHdr ? "rgba16float" : sdrViewFormat }),
    easuInput: input(pipelines.easu, scene),
    easuOutput: easuOutput.createView(),
    rcasInput: input(pipelines.rcas, easuOutput),
  };
  return upscaleTargets;
}
const framePacer = new PlaygroundFramePacer();
let renderDue = true;
let lastCompletionAt = 0;
let resolutionController: PlaygroundResolutionController | undefined;
let automaticRenderScale: number | undefined;
let sceneSize: Uint32Array = new Uint32Array([1, 1]);
function frameRateCap(): number | undefined {
  return state.frameRateCap > 0 ? state.frameRateCap : undefined;
}
let sceneUniforms: Uint8Array = new Uint8Array(playground_scene_uniform_bytes());
const tuningFloats = new Float32Array(52);
const tuningUints = new Uint32Array(tuningFloats.buffer);
const sceneBuffer = device.createBuffer({ size: sceneUniforms.byteLength, usage: 0x40 | 0x08 });
const playgroundBuffer = device.createBuffer({ size: tuningFloats.byteLength, usage: 0x40 | 0x08 });
// The scene's illumination over the view, and the eye's adapting field blurred
// from it, which scene compositions read at binding 7.
const [illuminationTexels, adaptingFieldTexels] = adapting_field_texels();
const illuminationView = device.createTexture({
  size: [illuminationTexels!, illuminationTexels!],
  format: "rgba32float",
  usage: 0x10 | 0x04,
}).createView();
const adaptingField = device.createTexture({
  size: [adaptingFieldTexels!, adaptingFieldTexels!],
  format: "rgba32float",
  usage: 0x10 | 0x04 | 0x01,
});
const adaptingFieldView = adaptingField.createView();
const previousAdaptingField = device.createTexture({
  size: [adaptingFieldTexels!, adaptingFieldTexels!],
  format: "rgba32float",
  usage: 0x04 | 0x02,
});
const adaptingFieldShader = device.createShaderModule({ code: adapting_field_blur_shader_source() });
const adaptingFieldPipeline = device.createRenderPipeline({
  layout: "auto",
  vertex: { module: adaptingFieldShader, entryPoint: "fullscreen_vertex" },
  fragment: {
    module: adaptingFieldShader,
    entryPoint: "blur_adapting_field",
    targets: [{ format: "rgba32float" }],
  },
  primitive: { topology: "triangle-list" },
});
const adaptingFieldBindGroups = [
  device.createBindGroup({
    layout: adaptingFieldPipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
  }),
  device.createBindGroup({
    layout: adaptingFieldPipeline.getBindGroupLayout(1),
    entries: [
      { binding: 0, resource: illuminationView },
      { binding: 1, resource: previousAdaptingField.createView() },
    ],
  }),
];
const environmentFieldView = device.createTexture({
  size: [...environment_field_texture_size()],
  format: "rgba32float",
  usage: 0x04 | 0x08,
}).createView({ dimension: "2d-array" });
const occluderFieldTexture = device.createTexture({
  size: [...occluder_field_texture_size()],
  format: "rgba16float",
  usage: 0x04 | 0x08,
});
const occluderFieldView = occluderFieldTexture.createView({ dimension: "2d-array" });
const occluderFieldLayersView = (first: number, count: number) =>
  occluderFieldTexture.createView({
    dimension: "2d-array",
    baseArrayLayer: first,
    arrayLayerCount: count,
  });
const occluderFieldSampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
const [terrainCacheTexels, terrainCacheLayers] = (() => {
  const [texels, , layers] = terrain_cache_texture_size();
  return [texels!, layers!];
})();
const terrainCacheView = device.createTexture({
  size: [terrainCacheTexels, terrainCacheTexels, terrainCacheLayers],
  format: "rgba32float",
  usage: 0x04 | 0x08,
}).createView({ dimension: "2d-array" });
const terrainCacheFillBuffer = device.createBuffer({
  size: playground_terrain_cache_fill_bytes(),
  usage: 0x40 | 0x08,
});
// The fill schedule advances only when a frame encodes its fill pass, so frames
// that are skipped or rendered before the pass compiles leave nothing stale.
const terrainCache = new PlaygroundTerrainCache();
const fieldGridHistory = new PlaygroundFieldGridHistory();
let terrainCacheFill: Uint8Array = new Uint8Array(0);
let terrainCacheWorkgroups = 0;
type TerrainCachePass = {
  pipeline: GPUComputePipeline;
  sceneBindGroup: GPUBindGroup;
  fillBindGroup: GPUBindGroup;
};
let terrainCachePass: TerrainCachePass | undefined;
// Sized for the largest scene the screen shows; a larger one bakes coarser.
const bakedSurface = new PlaygroundBakedSurface(
  Math.ceil(screen.width * devicePixelRatio),
  Math.ceil(screen.height * devicePixelRatio),
  matchMedia("(pointer: coarse)").matches,
  (navigator as Navigator & { deviceMemory?: number }).deviceMemory,
);
const [bakedHeightView, bakedColorView] = (() => {
  const [width, height, heightLayers, colorLayers] = bakedSurface.texture_size();
  const view = (layers: number, format: GPUTextureFormat) =>
    device.createTexture({
      size: [width!, height!, layers],
      format,
      usage: 0x04 | 0x08,
    }).createView({ dimension: "2d-array" });
  return [view(heightLayers!, "rgba16float"), view(colorLayers!, "rgba8unorm")];
})();
const bakedSurfaceFillBuffer = device.createBuffer({
  size: playground_baked_surface_fill_bytes(),
  usage: 0x40 | 0x08,
});
let bakedSurfaceFill: Uint8Array = new Uint8Array(0);
let bakedSurfaceWorkgroups = 0;
// The playground uniforms the latest bakes used; any tuning change rebakes.
let bakedTuning = new Uint8Array(0);
type BakedSurfacePass = {
  pipeline: GPUComputePipeline;
  inputBindGroup: GPUBindGroup;
  playgroundBindGroup: GPUBindGroup;
  outputBindGroup: GPUBindGroup;
};
let bakedSurfacePass: BakedSurfacePass | undefined;
// Environment, occluder, and landform horizon workgroup counts, then spacing and
// texels per occluder layer.
let fieldGrids: Float32Array = new Float32Array([1, 1, 1, 1, 5, 1, 1, 2]);

type FieldPasses = {
  environmentPipeline: GPUComputePipeline;
  environmentSceneBindGroup: GPUBindGroup;
  environmentOutputBindGroup: GPUBindGroup;
  occluderPipeline: GPUComputePipeline;
  occluderInputBindGroup: GPUBindGroup;
  occluderPlaygroundBindGroup: GPUBindGroup;
  occluderOutputBindGroup: GPUBindGroup;
};
// Lab shadow comparisons change how far horizons march, so this pass belongs to
// each scene pipeline variant.
type LandformHorizonPass = {
  landformHorizonPipeline: GPUComputePipeline;
  landformHorizonInputBindGroup: GPUBindGroup;
  landformHorizonPlaygroundBindGroup: GPUBindGroup;
  landformHorizonOutputBindGroup: GPUBindGroup;
};

const timestampQuerySet = timestampQueriesSupported
  ? device.createQuerySet({ type: "timestamp", count: 5 })
  : undefined;
const timestampResolveBuffer = timestampQueriesSupported
  ? device.createBuffer({ size: 40, usage: 0x0200 | 0x0004 })
  : undefined;
type TimestampReadback = {
  buffer: GPUBuffer;
  pending: boolean;
  pipeline?: GPURenderPipeline;
  fields?: boolean;
  upscaled?: boolean;
};
const timestampReadbacks: TimestampReadback[] = timestampQueriesSupported
  ? Array.from({ length: 3 }, () => ({
      buffer: device.createBuffer({ size: 40, usage: 0x0001 | 0x0008 }),
      pending: false,
    }))
  : [];
const startupSceneBindGroup = device.createBindGroup({
  layout: startupPipeline.getBindGroupLayout(0),
  entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
});
// A scene composition's illumination pass, which feeds its adapting field.
type IlluminationPass = {
  pipeline: GPURenderPipeline;
  sceneBindGroup: GPUBindGroup;
  playgroundBindGroup: GPUBindGroup;
};
type RenderResources = {
  pipeline: GPURenderPipeline;
  sceneBindGroup: GPUBindGroup;
  playgroundBindGroup?: GPUBindGroup;
  fields?: FieldPasses & LandformHorizonPass;
  illumination?: IlluminationPass;
  hdr?: boolean;
};
let sceneRenderResources: RenderResources = {
  pipeline: startupPipeline,
  sceneBindGroup: startupSceneBindGroup,
  hdr: hdrSupported,
};
let directSceneRenderResources = sceneRenderResources;
let sceneCompilationStatus = "compiling full scene";
const diagnosticShaderModules = new Map<string, GPUShaderModule>();
const diagnosticRenderResources = new Map<DiagnosticSelection, RenderResources>();
const diagnosticPipelinesRequested = new Set<DiagnosticSelection>();

function updateOccluderFieldInfo(resources: RenderResources): void {
  if (!occluderFieldInfoElement) return;
  const lines = resources.fields
    ? occluderFieldLayers.map(({ name, color }, layer) => {
      const [columnSpacing, rowSpacing, columns, rows] = fieldGrids.subarray(8 + 4 * layer, 12 + 4 * layer);
      const extent = (texels: number, spacing: number) => ((texels - 1) * spacing / 1000).toFixed(1);
      return `<span><i style="background:${color}"></i>${name} · ${formatDistance(rowSpacing!)} · `
        + `${columns}×${rows} · ${extent(columns!, columnSpacing!)} × ${extent(rows!, rowSpacing!)} km</span>`;
    })
    : ["<span>Direct evaluation</span>"];
  const html = lines.join("");
  if (occluderFieldInfoElement.innerHTML !== html) occluderFieldInfoElement.innerHTML = html;
}

function renderResourcesForFrame(): RenderResources {
  const selection = state.diagnostic;
  if (selection === "scene") {
    return directReference ? directSceneRenderResources : sceneRenderResources;
  }
  const ready = diagnosticRenderResources.get(selection);
  if (ready) return ready;
  if (!diagnosticPipelinesRequested.has(selection)) {
    diagnosticPipelinesRequested.add(selection);
    sceneCompilationStatus = `compiling ${selection.replaceAll("_", " ")}`;
    const configuration = diagnosticShaders[selection];
    let diagnosticShader = diagnosticShaderModules.get(configuration.name);
    if (!diagnosticShader) {
      diagnosticShader = device.createShaderModule({
        code: playground_diagnostic_shader_source(configuration.name),
      });
      diagnosticShaderModules.set(configuration.name, diagnosticShader);
    }
    void createPlaygroundPipeline(
      diagnosticShader,
      `fragment_${configuration.name}`,
      configuration.constants,
    ).then((diagnosticPipeline) => {
      diagnosticRenderResources.set(selection, {
        pipeline: diagnosticPipeline,
        sceneBindGroup: device.createBindGroup({
          layout: diagnosticPipeline.getBindGroupLayout(0),
          entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
        }),
        playgroundBindGroup: device.createBindGroup({
          layout: diagnosticPipeline.getBindGroupLayout(1),
          entries: [{ binding: 0, resource: { buffer: playgroundBuffer } }],
        }),
      });
      sceneCompilationStatus = "";
    }, (error) => {
      sceneCompilationStatus = `diagnostic failed: ${String(error)}`;
      console.error(error);
    });
  }
  return sceneRenderResources;
}
await showStartupPhase("Starting renderer…");
statusElement.classList.add("ready");

function splitSeed(value: string): [number, number] {
  try {
    const seed = BigInt(value);
    return [Number(seed & 0xffff_ffffn), Number((seed >> 32n) & 0xffff_ffffn)];
  } catch {
    return [0x5eed, 0];
  }
}

type WorldCenterDiagnostics = {
  surfaceAirTemperatureC: number;
  surfaceWind: [number, number];
  upperWind: [number, number];
  relativeHumidity: number;
  instability: number;
  lift: number;
  cloudiness: number;
  upperCloudiness: number;
  rainIntensity: number;
  sunDirection: [number, number, number];
  sunIntensity: number;
  moonDirection: [number, number, number];
  moonIntensity: number;
  solarEclipseFraction: number;
  lunarPenumbraFraction: number;
  lunarUmbraFraction: number;
  ambientIntensity: number;
  groundTemperatureC: number;
  waterTemperatureC: number;
  seasonalSnowCover: number;
  polarIceClosure: number;
  solidIceCover: number;
  waterHeightM: number;
  waterDepthM: number;
  currentVelocity: [number, number];
  waveOrbitalVelocity: [number, number];
  significantWaveHeightM: number;
  waveSurfaceDisplacementM: number;
  rogueCrestExcessM: number;
};

function parsedSeed(): bigint {
  try {
    return BigInt.asUintN(64, BigInt(state.seed));
  } catch {
    return 0x5eedn;
  }
}

function sampleWorldCenterDiagnostics(
  worldTimeSeconds: number,
): WorldCenterDiagnostics {
  const [worldTick, worldTickFraction] = splitTick(worldTimeSeconds);
  const sample = sample_playground_environment(
    state.worldWidthM,
    state.worldHeightM,
    parsedSeed(),
    worldTick,
    worldTickFraction,
    state.cameraX,
    state.cameraY,
  );
  const diagnostics: WorldCenterDiagnostics = {
    surfaceAirTemperatureC: sample.surface_air_temperature_c,
    surfaceWind: [sample.wind_x_m_s, sample.wind_y_m_s],
    upperWind: [sample.upper_wind_x_m_s, sample.upper_wind_y_m_s],
    relativeHumidity: sample.relative_humidity,
    instability: sample.instability,
    lift: sample.lift,
    cloudiness: sample.cloudiness,
    upperCloudiness: sample.upper_cloudiness,
    rainIntensity: sample.rain_intensity,
    sunDirection: [sample.sun_x, sample.sun_y, sample.sun_z],
    sunIntensity: sample.sun_intensity,
    moonDirection: [sample.moon_x, sample.moon_y, sample.moon_z],
    moonIntensity: sample.moon_intensity,
    solarEclipseFraction: sample.solar_eclipse_fraction,
    lunarPenumbraFraction: sample.lunar_penumbra_fraction,
    lunarUmbraFraction: sample.lunar_umbra_fraction,
    ambientIntensity: sample.ambient_intensity,
    groundTemperatureC: sample.ground_temperature_c,
    waterTemperatureC: sample.water_temperature_c,
    seasonalSnowCover: sample.seasonal_snow_cover,
    polarIceClosure: sample.polar_ice_closure,
    solidIceCover: sample.solid_ice_cover,
    waterHeightM: sample.water_height_m,
    waterDepthM: sample.water_depth_m,
    currentVelocity: [sample.current_x_m_s, sample.current_y_m_s],
    waveOrbitalVelocity: [sample.wave_orbital_x_m_s, sample.wave_orbital_y_m_s],
    significantWaveHeightM: sample.significant_wave_height_m,
    waveSurfaceDisplacementM: sample.wave_surface_displacement_m,
    rogueCrestExcessM: sample.rogue_crest_excess_m,
  };
  sample.free();
  return diagnostics;
}

function atmosphericOutputs(output: WorldCenterDiagnostics): Array<[string, string]> {
  const speed = (value: [number, number]) => Math.hypot(...value);
  const direction = (value: [number, number]) => Math.atan2(value[1], value[0]) * 180 / Math.PI;
  const angle = (value: [number, number, number]): string => {
    const elevation = Math.asin(value[2]) * 180 / Math.PI;
    const azimuth = Math.atan2(value[0], value[1]) * 180 / Math.PI;
    return `${elevation.toFixed(1)}° elevation · ${azimuth.toFixed(0)}° azimuth`;
  };
  const shear: [number, number] = [
    output.upperWind[0] - output.surfaceWind[0],
    output.upperWind[1] - output.surfaceWind[1],
  ];
  return [
    ["Current air temperature", `${output.surfaceAirTemperatureC.toFixed(1)} °C`],
    ["Sun", `${angle(output.sunDirection)} · ${output.sunIntensity.toFixed(2)}`],
    ["Moon", `${angle(output.moonDirection)} · ${output.moonIntensity.toPrecision(2)}`],
    ["Solar eclipse", `${(output.solarEclipseFraction * 100).toFixed(0)} %`],
    ["Lunar penumbra", `${(output.lunarPenumbraFraction * 100).toFixed(0)} %`],
    ["Lunar umbra", `${(output.lunarUmbraFraction * 100).toFixed(0)} %`],
    ["Astronomical ambient", output.ambientIntensity.toPrecision(2)],
    ["Surface wind", `${speed(output.surfaceWind).toFixed(1)} m/s · ${direction(output.surfaceWind).toFixed(0)}°`],
    ["Upper wind", `${speed(output.upperWind).toFixed(1)} m/s · ${direction(output.upperWind).toFixed(0)}°`],
    ["Vertical shear", `${speed(shear).toFixed(1)} m/s · ${direction(shear).toFixed(0)}°`],
    ["Relative humidity", `${(output.relativeHumidity * 100).toFixed(0)} %`],
    ["Instability", output.instability.toFixed(2)],
    ["Lift", output.lift.toFixed(2)],
    ["Low cloudiness", output.cloudiness.toFixed(2)],
    ["Weather canopy", output.upperCloudiness.toFixed(2)],
    ["Rain", output.rainIntensity.toFixed(2)],
  ];
}

function surfaceOutputs(output: WorldCenterDiagnostics): Array<[string, string]> {
  const vector = (value: [number, number]): string => {
    const speed = Math.hypot(...value);
    const direction = Math.atan2(value[1], value[0]) * 180 / Math.PI;
    return `${speed.toFixed(2)} m/s · ${direction.toFixed(0)}°`;
  };
  return [
    ["Ground temperature", `${output.groundTemperatureC.toFixed(1)} °C`],
    ["Water temperature", `${output.waterTemperatureC.toFixed(1)} °C`],
    ["Seasonal snow", output.seasonalSnowCover.toFixed(2)],
    ["Polar ice closure", output.polarIceClosure.toFixed(2)],
    ["Solid ice", output.solidIceCover.toFixed(2)],
    ["Tide height", `${output.waterHeightM.toFixed(2)} m`],
    ["Water depth", `${output.waterDepthM.toFixed(2)} m`],
    ["Current", vector(output.currentVelocity)],
    ["Wave orbital motion", vector(output.waveOrbitalVelocity)],
    ["Significant wave height", `${output.significantWaveHeightM.toFixed(2)} m`],
    ["Instantaneous wave surface", `${output.waveSurfaceDisplacementM.toFixed(2)} m`],
    ["Rogue crest excess", `${output.rogueCrestExcessM.toFixed(2)} m`],
  ];
}

function replaceDiagnosticRows(
  element: HTMLElement,
  rows: Array<[string, string]>,
): void {
  element.replaceChildren(...rows.flatMap(([label, value]) => {
    const name = document.createElement("span");
    name.textContent = label;
    const output = document.createElement("output");
    output.value = value;
    return [name, output];
  }));
}

function updateDerivedOutput(output: WorldCenterDiagnostics): void {
  derivedOutputHeadingElement.textContent = "Derived environment";
  replaceDiagnosticRows(derivedOutputElement, [
    ...atmosphericOutputs(output),
    ...surfaceOutputs(output),
  ]);
}

let sampledGeometryInputs: [number, number, number, number] | undefined;
let positionMeasurements: Array<[string, string]> = [];

function updateRenderedGeometry(): void {
  const inputs: [number, number, number, number] = [
    state.worldWidthM,
    state.worldHeightM,
    state.cameraX,
    state.cameraY,
  ];
  if (sampledGeometryInputs?.every((value, index) => value === inputs[index])) return;
  const sample = sample_playground_world_geometry(
    ...inputs,
  );
  positionMeasurements = [
    ["Raw world center", `${sample.raw_x_m.toFixed(1)}, ${sample.raw_y_m.toFixed(1)} m`],
    ["Cylinder center", `${sample.canonical_x_m.toFixed(1)}, ${sample.canonical_y_m.toFixed(1)} m`],
    ["Normalized geography", `${sample.normalized_x.toFixed(4)}, ${sample.normalized_y.toFixed(4)}`],
  ];
  renderedCameraX = sample.canonical_x_m;
  renderedCameraY = sample.canonical_y_m;
  sampledGeometryInputs = inputs;
  sample.free();
}

function updateViewCenterDiagnostics(
  worldTimeSeconds: number,
): void {
  updateRenderedGeometry();
  replaceDiagnosticRows(positionMeasurementsElement, positionMeasurements);
  updateDerivedOutput(sampleWorldCenterDiagnostics(
    worldTimeSeconds,
  ));
}

function splitTick(timeSeconds: number): [bigint, number] {
  const tickPosition = Math.max(0, timeSeconds * fixedTicksPerSecond);
  const wholeTick = Math.floor(tickPosition);
  return [BigInt(wholeTick), tickPosition - wholeTick];
}

function updateUniforms(
  worldTimeSeconds: number,
  elapsedSeconds: number,
  presentationTimeMs: number,
): void {
  sceneSize = playground_scene_render_size(
    previewCanvas.width,
    previewCanvas.height,
    automaticRenderScale ?? state.renderScale,
  );
  const enabled = (key: NumericStateKey): number => Number(state[key] > 0.5);
  const light = [
    enabled("sunLightVisible"),
    enabled("moonLightVisible"),
    enabled("skyLightVisible"),
    enabled("canopyLightVisible"),
  ] as const;
  const landMaterial = [
    enabled("grassColorVisible"),
    enabled("forestColorVisible"),
    enabled("intertidalColorVisible"),
    enabled("beachColorVisible"),
  ] as const;
  const solidMaterial = [
    enabled("rockColorVisible"),
    enabled("snowColorVisible"),
    enabled("landIceColorVisible"),
    enabled("seaIceColorVisible"),
  ] as const;
  const waterMaterial = [
    enabled("waterColorVisible"),
    enabled("bottomColorVisible"),
    0,
    0,
  ] as const;
  const surfaceColorLayer = landMaterial.some(Boolean)
    || solidMaterial.some(Boolean)
    || waterMaterial.some(Boolean);
  const surfaceDirectContribution = surfaceColorLayer
    && (light[0] > 0 || light[1] > 0);
  const intermediate = deriveIntermediateVisibility({
    colorLayer: surfaceColorLayer,
    sun: light[0] > 0,
    moon: light[1] > 0,
    clearSkyAmbient: light[2] > 0,
    canopyAmbient: light[3] > 0,
  }, state.diagnostic !== "scene");
  const visible = (value: boolean): number => Number(value);
  const [worldTick, worldTickFraction] = splitTick(worldTimeSeconds);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  const seed = (BigInt(seedHigh) << 32n) | BigInt(seedLow);
  const viewCloud = sample_playground_view_cloud_state(
    state.worldWidthM,
    state.worldHeightM,
    seed,
    worldTick,
    worldTickFraction,
    renderedCameraX,
    renderedCameraY,
  );
  const lowWind: [number, number] = [viewCloud[0]!, viewCloud[1]!];
  const upperWind: [number, number] = [viewCloud[2]!, viewCloud[3]!];
  adaptationClock.advance(elapsedSeconds);
  const frame = playground_scene_uniforms(
    state.worldWidthM,
    state.worldHeightM,
    seed,
    worldTick,
    worldTickFraction,
    renderedCameraX,
    renderedCameraY,
    state.viewHeight,
    previewCanvas.width,
    previewCanvas.height,
    sceneSize[1]!,
    bench ? previewCanvas.height : previewCanvas.clientHeight,
    new Float32Array(cloudMotion.advance(elapsedSeconds * state.worldTimeScale, lowWind, upperWind)),
    new Float32Array([
      adaptationClock.seconds(),
      // The browser reports HDR capability, not numeric display headroom.
      canvasHdr && window.matchMedia("(dynamic-range: high)").matches ? 4 : 1,
      Number(canvasHdr),
      (presentationTimeMs * 0.008) % 4096,
    ]),
    state.landformShadowReach,
    terrainCache,
    bakedSurface,
    fieldGridHistory,
  );
  sceneUniforms = frame.uniforms;
  fieldGrids = frame.field_grids;
  terrainCacheFill = frame.terrain_cache_fill;
  terrainCacheWorkgroups = frame.terrain_cache_workgroups;
  bakedSurfaceFill = frame.baked_surface_fill;
  bakedSurfaceWorkgroups = frame.baked_surface_workgroups;
  frame.free();

  tuningFloats.set([state.waterAttenuationLength, 0, 0, 0], 0);
  tuningFloats.set([
    state.landformReliefVisible,
    state.landformShadowsVisible,
    state.landformShadowReach,
    0,
  ], 4);
  tuningFloats.set([state.cloudShadowsVisible, 0, 0, 0], 8);
  tuningFloats.set([
    0,
    Number(surfaceDirectContribution),
    0,
    0,
  ], 12);
  tuningFloats.set([
    1,
    1,
    1,
    1,
  ], 16);
  tuningUints.set(light, 20);
  tuningUints.set(landMaterial, 24);
  tuningUints.set(solidMaterial, 28);
  tuningUints.set(waterMaterial, 32);
  tuningFloats.set([
    visible(intermediate.pressure),
    visible(intermediate.temperature),
    visible(intermediate.moisture),
    visible(intermediate.weather),
  ], 36);
  tuningFloats.set([
    visible(intermediate.materials),
    visible(intermediate.oceanForcing),
    visible(intermediate.waveSpectrum),
    visible(intermediate.materials),
  ], 40);
  tuningFloats.set([
    visible(intermediate.materials),
    visible(intermediate.materials),
    0,
    0,
  ], 44);
  tuningUints.set([occluderFieldInspection, sceneSize[0]!, sceneSize[1]!, 0], 48);
  const tuning = new Uint8Array(tuningFloats.buffer);
  if (!productionShaders && tuning.some((byte, index) => byte !== bakedTuning[index])) {
    bakedSurface.invalidate();
    bakedTuning = tuning.slice();
  }
}

function resize(): void {
  const scale = Math.min(devicePixelRatio, state.maximumPixelRatio);
  const [width, height] = bench
    ? [bench.width, bench.height]
    : boundedCanvasDimensions(
      previewCanvas.clientWidth,
      previewCanvas.clientHeight,
      scale,
      device.limits.maxTextureDimension2D,
    );
  if (previewCanvas.width !== width || previewCanvas.height !== height) {
    previewCanvas.width = width;
    previewCanvas.height = height;
  }
}

// Keep the 1-2-5 selection synchronized with `scale_bar` in
// `crates/browser/src/lib.rs`; gameplay and the lab share one physical-scale convention.
function updateScaleBar(): void {
  const targetDistance = state.viewHeight * 120 / previewCanvas.clientHeight;
  const magnitude = 10 ** Math.floor(Math.log10(targetDistance));
  const normalized = targetDistance / magnitude;
  const multiplier = normalized < 1.5 ? 1 : normalized < 3.5 ? 2 : normalized < 7.5 ? 5 : 10;
  const distance = multiplier * magnitude;
  const width = `${distance / state.viewHeight * previewCanvas.clientHeight}px`;
  const label = formatDistance(distance);
  if (scaleBarElement.style.width !== width) scaleBarElement.style.width = width;
  if (scaleLabelElement.textContent !== label) scaleLabelElement.textContent = label;
}

function formatDistance(distanceMetres: number): string {
  if (distanceMetres >= 1000) return `${(distanceMetres / 1000).toFixed(0)} km`;
  if (distanceMetres >= 1) return `${distanceMetres.toFixed(0)} m`;
  if (distanceMetres >= 0.01) return `${(distanceMetres * 100).toFixed(0)} cm`;
  if (distanceMetres >= 0.001) return `${(distanceMetres * 1000).toFixed(0)} mm`;
  if (distanceMetres >= 0.000001) return `${(distanceMetres * 1000000).toFixed(0)} µm`;
  return `${(distanceMetres * 1000000000).toFixed(0)} nm`;
}

const clock: FixedClock = { accumulatorSeconds: 0 };
let previous = performance.now();
let statisticsStartedAt = previous;
let framesInWindow = 0;
let ticksInWindow = 0;
let measuredFramesPerSecond: number | undefined;
let measuredTicksPerSecond: number | undefined;
let measuredGpuMilliseconds: number | undefined;
let gpuMillisecondsInWindow = 0;
let fieldsGpuMillisecondsInWindow = 0;
let occludersGpuMillisecondsInWindow = 0;
let sceneGpuMillisecondsInWindow = 0;
let gpuSamplesInWindow = 0;
let measuredFieldsGpuMilliseconds: number | undefined;
let measuredOccludersGpuMilliseconds: number | undefined;
let measuredSceneGpuMilliseconds: number | undefined;
let upscaleGpuMillisecondsInWindow = 0;
let measuredUpscaleGpuMilliseconds = 0;
let timedPipeline: GPURenderPipeline | undefined;
let timedUsesFields = false;
let latestPassTimes: Promise<PassTimes | undefined> = Promise.resolve(undefined);
let nextDiagnosticsAt = 0;

function updateStatus(now: number, ticks: number, worldTimeSeconds: number): void {
  updateOccluderFieldInfo(renderResourcesForFrame());
  ticksInWindow += ticks;
  const windowSeconds = (now - statisticsStartedAt) / 1000;
  if (windowSeconds < 1) return;
  measuredFramesPerSecond = framesInWindow / windowSeconds;
  measuredTicksPerSecond = ticksInWindow / windowSeconds;
  if (gpuSamplesInWindow > 0) {
    measuredGpuMilliseconds = gpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredFieldsGpuMilliseconds = fieldsGpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredOccludersGpuMilliseconds = occludersGpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredSceneGpuMilliseconds = sceneGpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredUpscaleGpuMilliseconds = upscaleGpuMillisecondsInWindow / gpuSamplesInWindow;
  }
  statisticsStartedAt = now;
  framesInWindow = 0;
  ticksInWindow = 0;
  gpuMillisecondsInWindow = 0;
  fieldsGpuMillisecondsInWindow = 0;
  occludersGpuMillisecondsInWindow = 0;
  sceneGpuMillisecondsInWindow = 0;
  upscaleGpuMillisecondsInWindow = 0;
  gpuSamplesInWindow = 0;
  const rates = `${measuredFramesPerSecond.toFixed(0)} FPS · ${measuredTicksPerSecond.toFixed(0)} TPS`;
  const gpuTiming = measuredGpuMilliseconds === undefined
    ? timestampQueriesSupported ? " · measuring GPU" : ""
    : timedUsesFields && measuredFieldsGpuMilliseconds !== undefined
      && measuredOccludersGpuMilliseconds !== undefined
      && measuredSceneGpuMilliseconds !== undefined
    ? ` · ${measuredGpuMilliseconds.toFixed(1)} ms GPU (${measuredFieldsGpuMilliseconds.toFixed(1)} fields + ${measuredOccludersGpuMilliseconds.toFixed(1)} occluders + ${measuredSceneGpuMilliseconds.toFixed(1)} scene${measuredUpscaleGpuMilliseconds > 0 ? ` + ${measuredUpscaleGpuMilliseconds.toFixed(1)} upscale` : ""})`
    : ` · ${measuredGpuMilliseconds.toFixed(1)} ms GPU scene`;
  const sceneScale = sceneSize[1]! / previewCanvas.height;
  const scale = sceneScale < 1 ? ` · scene ${(sceneScale * 100).toFixed(0)}%` : "";
  const compilation = [sceneCompilationStatus, tierCompilationStatus]
    .filter(Boolean)
    .map((status) => ` · ${status}`)
    .join("");
  statusElement.textContent = `${rates}${gpuTiming}${scale}${compilation}`;
  statusElement.title = `${adapterName} · ${previewCanvas.width} × ${previewCanvas.height} · scene ${sceneSize[0]} × ${sceneSize[1]} · ${format(worldTimeSeconds, 1)} s`;
  syncWorldDateInputs();
}

function advanceFrame(now: number): void {
  resize();
  updateScaleBar();
  const elapsed = Math.max(0, (now - previous) / 1000);
  previous = now;
  const ticks = advanceFixedClock(clock, elapsed, (elapsedTicks) => {
    const elapsedFixedSeconds = elapsedTicks * fixedTickSeconds;
    state.worldTimeSeconds = advanceProceduralTimeSeconds(
      state.worldTimeSeconds,
      elapsedFixedSeconds * state.worldTimeScale,
    );
  });
  const renderFractionSeconds = fixedClockFraction(clock) * fixedTickSeconds;
  const worldTimeSeconds = advanceProceduralTimeSeconds(
    state.worldTimeSeconds,
    renderFractionSeconds * state.worldTimeScale,
  );
  updateRenderedGeometry();
  if (diagnosticsDetailsElement.open && now >= nextDiagnosticsAt) {
    updateViewCenterDiagnostics(worldTimeSeconds);
    nextDiagnosticsAt = now + 250;
  }
  const renderResources = renderResourcesForFrame();
  if (canvasHdr !== Boolean(renderResources.hdr)) {
    configureCanvas(Boolean(renderResources.hdr));
  }
  if (state.automaticResolution > 0.5) {
    resolutionController ??= new PlaygroundResolutionController(state.renderScale);
  } else if (resolutionController) {
    resolutionController.free();
    resolutionController = undefined;
    automaticRenderScale = undefined;
  }
  const variant = scenePipelineVariant();
  if (variant !== failedScenePipelineVariant) failedScenePipelineVariant = undefined;
  if (sceneShaders && variant !== scenePipelinesVariant && variant !== failedScenePipelineVariant) {
    void matchScenePipelinesToState();
  }
  renderDue = framePacer.frame_due(elapsed, frameRateCap());
  updateUniforms(worldTimeSeconds, elapsed, now);
  updateStatus(now, ticks, worldTimeSeconds);
}

function renderFrame(): Promise<void> {
  const renderResources = renderResourcesForFrame();
  device.queue.writeBuffer(sceneBuffer, 0, sceneUniforms);
  device.queue.writeBuffer(playgroundBuffer, 0, tuningFloats);
  const encoder = device.createCommandEncoder();
  if (renderResources.pipeline !== timedPipeline) {
    timedPipeline = renderResources.pipeline;
    timedUsesFields = Boolean(renderResources.fields);
    measuredGpuMilliseconds = undefined;
    measuredFieldsGpuMilliseconds = undefined;
    measuredOccludersGpuMilliseconds = undefined;
    measuredSceneGpuMilliseconds = undefined;
    measuredUpscaleGpuMilliseconds = 0;
    gpuMillisecondsInWindow = 0;
    fieldsGpuMillisecondsInWindow = 0;
    occludersGpuMillisecondsInWindow = 0;
    sceneGpuMillisecondsInWindow = 0;
    upscaleGpuMillisecondsInWindow = 0;
    gpuSamplesInWindow = 0;
  }
  const timestampReadback = timestampQuerySet && timestampResolveBuffer
    ? timestampReadbacks.find((readback) => !readback.pending)
    : undefined;
  const fields = renderResources.fields;
  // Pass-end query indices: the field passes end at 1 and 2, the scene at 3,
  // and an upscale after it; the first timed pass also writes its beginning to 0.
  const sceneEnd = fields ? 3 : 1;
  const upscale = sceneSize[0] !== previewCanvas.width || sceneSize[1] !== previewCanvas.height
    ? prepareUpscale(sceneSize[0]!, sceneSize[1]!)
    : undefined;
  if (!upscale && upscaleTargets) {
    for (const texture of upscaleTargets.textures) texture.destroy();
    upscaleTargets = undefined;
  }
  const lastEnd = upscale ? sceneEnd + 1 : sceneEnd;
  if (terrainCachePass) {
    if (terrainCacheWorkgroups > 0) {
      device.queue.writeBuffer(terrainCacheFillBuffer, 0, terrainCacheFill);
      const terrainCacheEncoder = encoder.beginComputePass();
      terrainCacheEncoder.setPipeline(terrainCachePass.pipeline);
      terrainCacheEncoder.setBindGroup(0, terrainCachePass.sceneBindGroup);
      terrainCacheEncoder.setBindGroup(1, terrainCachePass.fillBindGroup);
      terrainCacheEncoder.dispatchWorkgroups(terrainCacheWorkgroups);
      terrainCacheEncoder.end();
    }
    terrainCache.commit();
  }
  if (bakedSurfacePass) {
    if (bakedSurfaceWorkgroups > 0) {
      device.queue.writeBuffer(bakedSurfaceFillBuffer, 0, bakedSurfaceFill);
      const bakedSurfaceEncoder = encoder.beginComputePass();
      bakedSurfaceEncoder.setPipeline(bakedSurfacePass.pipeline);
      bakedSurfaceEncoder.setBindGroup(0, bakedSurfacePass.inputBindGroup);
      bakedSurfaceEncoder.setBindGroup(1, bakedSurfacePass.playgroundBindGroup);
      bakedSurfaceEncoder.setBindGroup(2, bakedSurfacePass.outputBindGroup);
      bakedSurfaceEncoder.dispatchWorkgroups(bakedSurfaceWorkgroups);
      bakedSurfaceEncoder.end();
    }
    bakedSurface.commit();
  }
  if (fields) {
    const environmentPassDescriptor: GPUComputePassDescriptor = {};
    const occluderPassDescriptor: GPUComputePassDescriptor = {};
    if (timestampReadback) {
      environmentPassDescriptor.timestampWrites = {
        querySet: timestampQuerySet!,
        beginningOfPassWriteIndex: 0,
        endOfPassWriteIndex: 1,
      };
      occluderPassDescriptor.timestampWrites = {
        querySet: timestampQuerySet!,
        endOfPassWriteIndex: 2,
      };
    }
    const environmentPass = encoder.beginComputePass(environmentPassDescriptor);
    environmentPass.setPipeline(fields.environmentPipeline);
    environmentPass.setBindGroup(0, fields.environmentSceneBindGroup);
    environmentPass.setBindGroup(1, fields.environmentOutputBindGroup);
    environmentPass.dispatchWorkgroups(fieldGrids[0]!, fieldGrids[1]!);
    environmentPass.end();
    const occluderPass = encoder.beginComputePass(occluderPassDescriptor);
    occluderPass.setPipeline(fields.occluderPipeline);
    occluderPass.setBindGroup(0, fields.occluderInputBindGroup);
    occluderPass.setBindGroup(1, fields.occluderPlaygroundBindGroup);
    occluderPass.setBindGroup(2, fields.occluderOutputBindGroup);
    occluderPass.dispatchWorkgroups(fieldGrids[2]!, fieldGrids[3]!, fieldGrids[4]!);
    occluderPass.setPipeline(fields.landformHorizonPipeline);
    occluderPass.setBindGroup(0, fields.landformHorizonInputBindGroup);
    occluderPass.setBindGroup(1, fields.landformHorizonPlaygroundBindGroup);
    occluderPass.setBindGroup(2, fields.landformHorizonOutputBindGroup);
    occluderPass.dispatchWorkgroups(fieldGrids[5]!, fieldGrids[6]!, fieldGrids[7]!);
    occluderPass.end();
  }
  // The scene's timed span includes its adapting field.
  const illumination = renderResources.illumination;
  if (illumination) {
    const illuminationPass = encoder.beginRenderPass({
      colorAttachments: [{
        view: illuminationView,
        clearValue: { r: 0, g: 1, b: 1, a: 1 },
        loadOp: "clear",
        storeOp: "store",
      }],
      ...(timestampReadback && !fields
        ? { timestampWrites: { querySet: timestampQuerySet!, beginningOfPassWriteIndex: 0 } }
        : {}),
    });
    illuminationPass.setPipeline(illumination.pipeline);
    illuminationPass.setBindGroup(0, illumination.sceneBindGroup);
    illuminationPass.setBindGroup(1, illumination.playgroundBindGroup);
    illuminationPass.setVertexBuffer(0, vertexBuffer);
    illuminationPass.draw(6);
    illuminationPass.end();
    const fieldPass = encoder.beginRenderPass({
      colorAttachments: [{ view: adaptingFieldView, loadOp: "clear", storeOp: "store" }],
    });
    fieldPass.setPipeline(adaptingFieldPipeline);
    fieldPass.setBindGroup(0, adaptingFieldBindGroups[0]!);
    fieldPass.setBindGroup(1, adaptingFieldBindGroups[1]!);
    fieldPass.draw(3);
    fieldPass.end();
    encoder.copyTextureToTexture(
      { texture: adaptingField },
      { texture: previousAdaptingField },
      [adaptingFieldTexels!, adaptingFieldTexels!],
    );
  }
  const canvasTexture = context.getCurrentTexture();
  const renderPassDescriptor: GPURenderPassDescriptor = {
    colorAttachments: [{
      view: upscale?.sceneView ?? canvasTexture.createView({
        format: canvasHdr ? "rgba16float" : sdrViewFormat,
      }),
      clearValue: { r: 0.02, g: 0.08, b: 0.1, a: 1 },
      loadOp: "clear",
      storeOp: "store",
    }],
  };
  if (timestampReadback) {
    renderPassDescriptor.timestampWrites = {
      querySet: timestampQuerySet!,
      ...(fields || illumination ? {} : { beginningOfPassWriteIndex: 0 }),
      endOfPassWriteIndex: sceneEnd,
    };
  }
  const pass = encoder.beginRenderPass(renderPassDescriptor);
  pass.setPipeline(renderResources.pipeline);
  pass.setBindGroup(0, renderResources.sceneBindGroup);
  if (renderResources.playgroundBindGroup) {
    pass.setBindGroup(1, renderResources.playgroundBindGroup);
  }
  pass.setVertexBuffer(0, vertexBuffer);
  pass.draw(6);
  pass.end();
  if (upscale) {
    for (const [pipeline, input, view] of [
      [upscale.pipelines.easu, upscale.easuInput, upscale.easuOutput],
      [upscale.pipelines.rcas, upscale.rcasInput, canvasTexture.createView()],
    ] as const) {
      const upscalePass = encoder.beginRenderPass({
        colorAttachments: [{ view, loadOp: "clear", storeOp: "store" }],
        ...(timestampReadback && pipeline === upscale.pipelines.rcas
          ? { timestampWrites: { querySet: timestampQuerySet!, endOfPassWriteIndex: lastEnd } }
          : {}),
      });
      upscalePass.setPipeline(pipeline);
      upscalePass.setBindGroup(0, input);
      upscalePass.draw(3);
      upscalePass.end();
    }
  }
  if (timestampReadback) {
    const timestampBytes = (lastEnd + 1) * 8;
    encoder.resolveQuerySet(timestampQuerySet!, 0, lastEnd + 1, timestampResolveBuffer!, 0);
    encoder.copyBufferToBuffer(
      timestampResolveBuffer!,
      0,
      timestampReadback.buffer,
      0,
      timestampBytes,
    );
    timestampReadback.pending = true;
    timestampReadback.pipeline = renderResources.pipeline;
    timestampReadback.fields = Boolean(fields);
    timestampReadback.upscaled = Boolean(upscale);
  }
  device.queue.submit([encoder.finish()]);
  adaptationClock.frame_rendered();
  const submittedAt = performance.now();
  framesInWindow += 1;
  latestPassTimes = Promise.resolve(undefined);
  if (timestampReadback) {
    latestPassTimes = timestampReadback.buffer.mapAsync(0x0001).then(() => {
      const timestamps = new BigUint64Array(timestampReadback.buffer.getMappedRange());
      const withFields = Boolean(timestampReadback.fields);
      const sceneEnd = withFields ? 3 : 1;
      const lastEnd = timestampReadback.upscaled ? sceneEnd + 1 : sceneEnd;
      const startedAt = timestamps[0]!;
      const finishedAt = timestamps[lastEnd]!;
      const milliseconds = (begin: number, end: number): number =>
        Number(timestamps[end]! - timestamps[begin]!) / 1_000_000;
      const valid = startedAt > 0n && finishedAt >= startedAt;
      const passes: PassTimes | undefined = withFields && valid
        ? {
          fieldsMs: milliseconds(0, 1),
          occludersMs: milliseconds(1, 2),
          sceneMs: milliseconds(2, 3),
          ...(timestampReadback.upscaled ? { upscaleMs: milliseconds(3, 4) } : {}),
          totalMs: milliseconds(0, lastEnd),
        }
        : undefined;
      if (timestampReadback.pipeline === timedPipeline && valid) {
        gpuMillisecondsInWindow += milliseconds(0, lastEnd);
        if (withFields) {
          fieldsGpuMillisecondsInWindow += milliseconds(0, 1);
          occludersGpuMillisecondsInWindow += milliseconds(1, 2);
        }
        sceneGpuMillisecondsInWindow += milliseconds(sceneEnd - 1, sceneEnd);
        upscaleGpuMillisecondsInWindow += milliseconds(sceneEnd, lastEnd);
        gpuSamplesInWindow += 1;
      }
      timestampReadback.buffer.unmap();
      return passes;
    }).catch(() => undefined).finally(() => {
      timestampReadback.pending = false;
    });
  }
  const controller = resolutionController;
  return device.queue.onSubmittedWorkDone().then(() => {
    const completedAt = performance.now();
    // Work queued behind the previous frame starts when that frame completes.
    // Browsers can report several completions in one task; the later ones then
    // measure nothing and are skipped.
    const queued = submittedAt < lastCompletionAt;
    const startedAt = Math.max(submittedAt, lastCompletionAt);
    lastCompletionAt = completedAt;
    if (queued && completedAt - startedAt < 1) return;
    if (controller && controller === resolutionController) {
      automaticRenderScale = controller.record_frame(
        completedAt - startedAt,
        frameRateCap(),
        state.renderScale,
      );
    }
  });
}

if (!bench) {
  // The loop calls this only when a frame slot is free, so the pacer is charged
  // only for frames that are actually submitted.
  startRenderLoop(advanceFrame, () => {
    if (!renderDue) return Promise.resolve();
    framePacer.frame_rendered(frameRateCap());
    return renderFrame();
  }, () => renderLoopActive, (error) => {
    if (renderLoopActive) statusElement.textContent = "Rendering failed; retrying";
    console.error(error);
  });
}

// The bench renders only the scene, so it compiles no direct reference.
type SceneShaders = {
  scene: GPUShaderModule;
  direct?: GPUShaderModule;
  landformHorizon: GPUShaderModule;
};
type ScenePipelines = {
  scene: GPURenderPipeline;
  sceneIllumination: GPURenderPipeline;
  direct?: GPURenderPipeline;
  directIllumination?: GPURenderPipeline;
  landformHorizon: GPUComputePipeline;
};
let sceneShaders: SceneShaders | undefined;
let sceneFields: FieldPasses | undefined;
const scenePipelinesByVariant = new Map<string, Promise<ScenePipelines>>();
let scenePipelinesVariant: string | undefined;
let scenePipelinesMatch: Promise<void> | undefined;
let failedScenePipelineVariant: string | undefined;
let tierCompilationStatus = "";

function scenePipelineVariant(): string {
  return [
    Math.round(state.graphicsQuality),
    state.legacyShadowLook > 0.5 ? 1 : 0,
  ].join(":");
}

function scenePipelines(shaders: SceneShaders, variant: string): Promise<ScenePipelines> {
  let pipelines = scenePipelinesByVariant.get(variant);
  if (!pipelines) {
    const [quality, legacyShadowLook] = variant.split(":").map(Number);
    const constants = {
      ...graphics_quality_scene_constants(graphicsQualities[quality!]!) as Record<string, number>,
      LEGACY_SHADOW_LOOK: legacyShadowLook!,
    };
    pipelines = Promise.all([
      createPlaygroundPipeline(
        shaders.scene,
        productionShaders ? "fragment" : "fragment_playground_scene",
        constants,
      ),
      createPlaygroundPipeline(
        shaders.scene,
        productionShaders ? "fragment_illumination" : "fragment_playground_illumination",
        constants,
        "rgba32float",
      ),
      shaders.direct
        && createPlaygroundPipeline(shaders.direct, "fragment_playground_scene", constants),
      shaders.direct && createPlaygroundPipeline(
        shaders.direct,
        "fragment_playground_illumination",
        constants,
        "rgba32float",
      ),
      device.createComputePipelineAsync({
        layout: "auto",
        compute: {
          module: shaders.landformHorizon,
          entryPoint: "generate_landform_horizons",
          constants: { LEGACY_SHADOW_LOOK: legacyShadowLook! },
        },
      }),
    ]).then(([scene, sceneIllumination, direct, directIllumination, landformHorizon]) => ({
      scene,
      sceneIllumination,
      landformHorizon,
      ...(direct ? { direct } : {}),
      ...(directIllumination ? { directIllumination } : {}),
    }));
    scenePipelinesByVariant.set(variant, pipelines);
  }
  return pipelines;
}

function installScenePipelines(pipelines: ScenePipelines, fields: FieldPasses): void {
  const playgroundEntries = productionShaders
    ? []
    : [{ binding: 0, resource: { buffer: playgroundBuffer } }];
  const gridSceneEntries = [
    { binding: 0, resource: { buffer: sceneBuffer } },
    { binding: 1, resource: environmentFieldView },
    { binding: 2, resource: occluderFieldView },
    { binding: 3, resource: occluderFieldSampler },
    { binding: 4, resource: terrainCacheView },
    { binding: 5, resource: bakedHeightView },
    { binding: 6, resource: bakedColorView },
  ];
  const landformHorizonPipeline = pipelines.landformHorizon;
  sceneRenderResources = {
    pipeline: pipelines.scene,
    hdr: hdrSupported,
    fields: {
      ...fields,
      landformHorizonPipeline,
      landformHorizonInputBindGroup: device.createBindGroup({
        layout: landformHorizonPipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: sceneBuffer } },
          { binding: 2, resource: occluderFieldLayersView(0, landformHorizonLayer) },
          { binding: 3, resource: occluderFieldSampler },
        ],
      }),
      landformHorizonPlaygroundBindGroup: device.createBindGroup({
        layout: landformHorizonPipeline.getBindGroupLayout(1),
        entries: playgroundEntries,
      }),
      landformHorizonOutputBindGroup: device.createBindGroup({
        layout: landformHorizonPipeline.getBindGroupLayout(2),
        entries: [{
          binding: 0,
          resource: occluderFieldLayersView(
            landformHorizonLayer,
            occluderFieldLayers.length - landformHorizonLayer,
          ),
        }],
      }),
    },
    sceneBindGroup: device.createBindGroup({
      layout: pipelines.scene.getBindGroupLayout(0),
      entries: [...gridSceneEntries, { binding: 7, resource: adaptingFieldView }],
    }),
    playgroundBindGroup: device.createBindGroup({
      layout: pipelines.scene.getBindGroupLayout(1),
      entries: playgroundEntries,
    }),
    illumination: illuminationPass(pipelines.sceneIllumination, gridSceneEntries, playgroundEntries),
  };
  if (pipelines.direct && pipelines.directIllumination) {
    const directSceneEntries = [{ binding: 0, resource: { buffer: sceneBuffer } }];
    const directPlaygroundEntries = [{ binding: 0, resource: { buffer: playgroundBuffer } }];
    directSceneRenderResources = {
      pipeline: pipelines.direct,
      hdr: hdrSupported,
      sceneBindGroup: device.createBindGroup({
        layout: pipelines.direct.getBindGroupLayout(0),
        entries: [...directSceneEntries, { binding: 7, resource: adaptingFieldView }],
      }),
      playgroundBindGroup: device.createBindGroup({
        layout: pipelines.direct.getBindGroupLayout(1),
        entries: directPlaygroundEntries,
      }),
      illumination: illuminationPass(
        pipelines.directIllumination,
        directSceneEntries,
        directPlaygroundEntries,
      ),
    };
  }
}

// The illumination pass reads what its scene does, except the adapting field.
function illuminationPass(
  pipeline: GPURenderPipeline,
  sceneEntries: GPUBindGroupEntry[],
  playgroundEntries: GPUBindGroupEntry[],
): IlluminationPass {
  return {
    pipeline,
    sceneBindGroup: device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: sceneEntries,
    }),
    playgroundBindGroup: device.createBindGroup({
      layout: pipeline.getBindGroupLayout(1),
      entries: playgroundEntries,
    }),
  };
}

// Graphics tiers and lab shadow comparisons are pipeline override constants,
// so each variant has its own scene pipelines; the previous variant keeps
// rendering until they are ready. A variant that failed to compile is not
// retried until another one is selected.
function matchScenePipelinesToState(): Promise<void> {
  scenePipelinesMatch ??= (async () => {
    while (sceneShaders && sceneFields) {
      const variant = scenePipelineVariant();
      if (variant === scenePipelinesVariant || variant === failedScenePipelineVariant) break;
      const tier = graphicsQualities[Math.round(state.graphicsQuality)];
      tierCompilationStatus = `compiling ${tier} tier`;
      try {
        installScenePipelines(await scenePipelines(sceneShaders, variant), sceneFields);
        scenePipelinesVariant = variant;
        tierCompilationStatus = "";
      } catch (error) {
        scenePipelinesByVariant.delete(variant);
        failedScenePipelineVariant = variant;
        tierCompilationStatus = `${tier} tier failed: ${String(error)}`;
        console.error(error);
      }
    }
  })().finally(() => {
    scenePipelinesMatch = undefined;
  });
  return scenePipelinesMatch;
}

const fullSceneReady = new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  .then(async () => {
    const compilationStartedAt = performance.now();
    const sceneShader = device.createShaderModule({
      code: productionShaders
        ? production_scene_shader_source()
        : playground_scene_shader_source(),
    });
    const bakeShader = device.createShaderModule({
      code: productionShaders
        ? baked_surface_shader_source()
        : playground_baked_surface_shader_source(),
    });
    const landformHorizon = device.createShaderModule({
      code: productionShaders
        ? landform_horizon_shader_source()
        : playground_landform_horizon_shader_source(),
    });
    const shaders: SceneShaders = productionShaders
      ? { scene: sceneShader, landformHorizon }
      : {
          scene: sceneShader,
          direct: device.createShaderModule({ code: playground_direct_scene_shader_source() }),
          landformHorizon,
        };
    const variant = scenePipelineVariant();
    const occluderShader = device.createShaderModule({
      code: productionShaders
        ? occluder_field_shader_source()
        : playground_occluder_field_shader_source(),
    });
    const fieldShader = device.createShaderModule({ code: environment_field_shader_source() });
    const terrainCacheShader = device.createShaderModule({ code: terrain_cache_shader_source() });
    const [
      pipelines,
      environmentPipeline,
      occluderPipeline,
      terrainCachePipeline,
      bakedSurfacePipeline,
    ] = await Promise.all([
      scenePipelines(shaders, variant),
      device.createComputePipelineAsync({
        layout: "auto",
        compute: { module: fieldShader, entryPoint: "generate_environment_fields" },
      }),
      device.createComputePipelineAsync({
        layout: "auto",
        compute: { module: occluderShader, entryPoint: "generate_occluder_fields" },
      }),
      device.createComputePipelineAsync({
        layout: "auto",
        compute: { module: terrainCacheShader, entryPoint: "generate_terrain_cache" },
      }),
      device.createComputePipelineAsync({
        layout: "auto",
        compute: { module: bakeShader, entryPoint: "generate_baked_surface" },
      }),
    ]);
    const playgroundEntries = productionShaders
      ? []
      : [{ binding: 0, resource: { buffer: playgroundBuffer } }];
    const fields: FieldPasses = {
      environmentPipeline,
      environmentSceneBindGroup: device.createBindGroup({
        layout: environmentPipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
      }),
      environmentOutputBindGroup: device.createBindGroup({
        layout: environmentPipeline.getBindGroupLayout(1),
        entries: [{ binding: 0, resource: environmentFieldView }],
      }),
      occluderPipeline,
      occluderInputBindGroup: device.createBindGroup({
        layout: occluderPipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: sceneBuffer } },
          { binding: 1, resource: environmentFieldView },
        ],
      }),
      occluderPlaygroundBindGroup: device.createBindGroup({
        layout: occluderPipeline.getBindGroupLayout(1),
        entries: playgroundEntries,
      }),
      occluderOutputBindGroup: device.createBindGroup({
        layout: occluderPipeline.getBindGroupLayout(2),
        entries: [{ binding: 0, resource: occluderFieldLayersView(0, landformHorizonLayer) }],
      }),
    };
    installScenePipelines(pipelines, fields);
    sceneShaders = shaders;
    sceneFields = fields;
    scenePipelinesVariant = variant;
    bakedSurfacePass = {
      pipeline: bakedSurfacePipeline,
      inputBindGroup: device.createBindGroup({
        layout: bakedSurfacePipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: sceneBuffer } },
          { binding: 4, resource: terrainCacheView },
        ],
      }),
      playgroundBindGroup: device.createBindGroup({
        layout: bakedSurfacePipeline.getBindGroupLayout(1),
        entries: playgroundEntries,
      }),
      outputBindGroup: device.createBindGroup({
        layout: bakedSurfacePipeline.getBindGroupLayout(2),
        entries: [
          { binding: 0, resource: { buffer: bakedSurfaceFillBuffer } },
          { binding: 1, resource: bakedHeightView },
          { binding: 2, resource: bakedColorView },
        ],
      }),
    };
    terrainCachePass = {
      pipeline: terrainCachePipeline,
      sceneBindGroup: device.createBindGroup({
        layout: terrainCachePipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
      }),
      fillBindGroup: device.createBindGroup({
        layout: terrainCachePipeline.getBindGroupLayout(1),
        entries: [
          { binding: 0, resource: { buffer: terrainCacheFillBuffer } },
          { binding: 1, resource: terrainCacheView },
        ],
      }),
    };
    sceneCompilationStatus = `full scene ready in ${((performance.now() - compilationStartedAt) / 1000).toFixed(1)} s`;
  })
  .catch((error) => {
    sceneCompilationStatus = `scene shader failed: ${String(error)}`;
    console.error(error);
  });

// Each frame waits for the previous one to finish on the GPU, so
// submit-to-completion time excludes queueing and display synchronization.
async function runBench(settings: NonNullable<typeof bench>): Promise<void> {
  resize();
  await fullSceneReady;
  if (!sceneRenderResources.fields) {
    statusElement.textContent = `Bench cannot start: ${sceneCompilationStatus}`;
    return;
  }
  playgroundMainElement.inert = true;
  const scenarios = benchScenarios as BenchScenario[];
  const summaries: ScenarioSummary[] = [];
  // GPU clocks settle lower under sustained load; an untimed sweep first lets
  // every scenario, including the first, be timed in that settled state.
  for (const timedFrames of [0, settings.timedFrames]) for (const [index, scenario] of scenarios.entries()) {
    statusElement.textContent = timedFrames === 0
      ? `Bench warm-up ${index + 1}/${scenarios.length}`
      : `Bench ${index + 1}/${scenarios.length}: ${scenario.id}`;
    adaptationClock.free();
    adaptationClock = new PlaygroundAdaptationClock();
    Object.assign(state, {
      seed: scenario.seed,
      cameraX: scenario.cameraM[0],
      cameraY: scenario.cameraM[1],
      viewHeight: scenario.viewHeightM,
      worldTimeSeconds: scenario.worldTimeSeconds,
      renderScale: scenario.renderScale ?? 1,
      graphicsQuality: graphicsQualities.indexOf(scenario.quality ?? "high"),
    });
    await matchScenePipelinesToState();
    if (scenePipelinesVariant !== scenePipelineVariant()) {
      throw new Error(`${scenario.id}: ${tierCompilationStatus}`);
    }
    updateRenderedGeometry();
    const samples: FrameSample[] = [];
    for (let frame = 0; frame < settings.warmupFrames + timedFrames; frame += 1) {
      const now = await new Promise<number>((resolve) => requestAnimationFrame(resolve));
      if (!renderLoopActive) {
        statusElement.textContent = `Bench stopped at ${scenario.id}: ${statusElement.textContent}`;
        return;
      }
      // A fresh adaptation clock settles the eye, and zero elapsed time keeps
      // it and cloud transport fixed, as in capture.
      updateUniforms(state.worldTimeSeconds, 0, now);
      const startedAt = performance.now();
      await renderFrame();
      const submittedToDoneMs = performance.now() - startedAt;
      const passes = await latestPassTimes;
      if (frame >= settings.warmupFrames) samples.push({ submittedToDoneMs, passes });
    }
    if (timedFrames > 0) summaries.push(summarizeScenario(
      scenario.id,
      samples,
      previewCanvas.width * previewCanvas.height,
    ));
  }
  const report = {
    adapter: adapterName,
    userAgent: navigator.userAgent,
    width: previewCanvas.width,
    height: previewCanvas.height,
    timestampQueries: timestampQueriesSupported,
    warmupFrames: settings.warmupFrames,
    timedFrames: settings.timedFrames,
    scenarios: summaries,
  };
  console.log(JSON.stringify(report));
  console.table(summaries.map(({ id, submittedToDoneMs, passes, nsPerPixel }) => ({
    id,
    submittedToDoneMs,
    gpuTotalMs: passes?.totalMs,
    fieldsMs: passes?.fieldsMs,
    occludersMs: passes?.occludersMs,
    sceneMs: passes?.sceneMs,
    upscaleMs: passes?.upscaleMs,
    nsPerPixel,
  })));
  const output = document.createElement("pre");
  output.className = "bench-results";
  output.textContent = [
    `${adapterName} · ${previewCanvas.width} × ${previewCanvas.height} · median of ${settings.timedFrames} frames`,
    ...summaries.map(({ id, submittedToDoneMs, passes, nsPerPixel }) => [
      id.padEnd(30),
      `${(passes?.totalMs ?? submittedToDoneMs).toFixed(1).padStart(7)} ms`,
      `${nsPerPixel.toFixed(0).padStart(5)} ns/px`,
      passes
        ? `(${passes.fieldsMs.toFixed(1)} fields + ${passes.occludersMs.toFixed(1)} occluders + ${passes.sceneMs.toFixed(1)} scene${passes.upscaleMs === undefined ? "" : ` + ${passes.upscaleMs.toFixed(1)} upscale`})`
        : `submit to completion`,
    ].join(" ")),
  ].join("\n");
  document.body.append(output);
  statusElement.textContent = "Bench complete";
}

if (bench) {
  void runBench(bench).catch((error) => {
    statusElement.textContent = `Bench failed: ${String(error)}`;
    console.error(error);
  });
}
