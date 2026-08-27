import {
  sample_playground_environment,
  sample_playground_noise_octave,
  sample_playground_vector_field_transport,
  sample_playground_surface_weather_transport,
  sample_playground_world_geometry,
} from "browser";
import shaderSource from "../../crates/browser/src/scene.wgsl?raw";
import "./style.css";

type Domain = "weather" | "landscape" | "water";
type ControlKind = "MODEL" | "SHADER" | "DEBUG";

type State = {
  seed: string;
  cameraX: number;
  cameraY: number;
  worldWidthM: number;
  worldHeightM: number;
  viewHeight: number;
  time: number;
  cloudTimeScale: number;
  pressureTimeScale: number;
  currentTimeScale: number;
  worldTimeScale: number;
  timeScalesLocked: boolean;
  paused: boolean;
  worldTimeSeconds: number;
  pressureTimeSeconds: number;
  currentTimeSeconds: number;
  celestialLightingEnabled: number;
  detailCharacter: number;
  detailBlobiness: number;
  detailContrast: number;
  detailEdgeSoftness: number;
  maxWispCoverage: number;
  opticalDepth: number;
  opacity: number;
  darkness: number;
  relief: number;
  terrainScale: number;
  elevationScale: number;
  cliffSlope: number;
  waterAttenuationLength: number;
  bottomVisibility: number;
  landformReliefStrength: number;
  landformShadowStrength: number;
  landformShadowReach: number;
  lowCloudEnabled: number;
  lowCloudVisible: number;
  lowCloudShadow: number;
  upperCloudEnabled: number;
  upperCloudShadow: number;
  upperShadowOnLowCloud: number;
  coastalReliefVisible: number;
  vegetationVisible: number;
  surfaceTextureVisible: number;
  landscapeLightingVisible: number;
  seasonalSnowVisible: number;
  seasonalVegetationVisible: number;
  tidesEnabled: number;
  pressureWindScale: number;
  pressureCirculationEnabled: number;
  crossIsobarScale: number;
  crossIsobarEnabled: number;
  thermalWindScale: number;
  thermalWindEnabled: number;
  shelterStrength: number;
  shelterEnabled: number;
  landscapeCompositionEnabled: number;
  debugView: number;
};

type NumericStateKey = Exclude<
  keyof State,
  "seed" | "paused" | "timeScalesLocked"
>;
type RangeControl = {
  type: "range";
  key: NumericStateKey;
  label: string;
  min: number;
  max: number;
  step: number;
};
type ToggleControl = {
  type: "toggle";
  key: NumericStateKey;
  label: string;
};
type Control = RangeControl | ToggleControl;
type ControlGroup = {
  domain: Domain;
  category: string;
  layer: string;
  kind: ControlKind;
  level?: "advanced" | "visibility";
  controls: Control[];
};

const defaults: State = {
  seed: "0x5eed",
  cameraX: 0,
  cameraY: 0,
  worldWidthM: 64_000,
  worldHeightM: 64_000,
  viewHeight: 480,
  time: 37,
  cloudTimeScale: 1,
  pressureTimeScale: 1,
  currentTimeScale: 1,
  worldTimeScale: 1,
  timeScalesLocked: true,
  paused: false,
  worldTimeSeconds: 0,
  pressureTimeSeconds: 0,
  currentTimeSeconds: 0,
  celestialLightingEnabled: 1,
  detailCharacter: 0.35,
  detailBlobiness: 0.25,
  detailContrast: 1.15,
  detailEdgeSoftness: 0.06,
  maxWispCoverage: 0.25,
  opticalDepth: 1,
  opacity: 1,
  darkness: 1,
  relief: 1,
  terrainScale: 1,
  elevationScale: 3,
  cliffSlope: 0.08,
  waterAttenuationLength: 0.5,
  bottomVisibility: 1,
  landformReliefStrength: 1,
  landformShadowStrength: 0.7,
  landformShadowReach: 320,
  lowCloudEnabled: 1,
  lowCloudVisible: 1,
  lowCloudShadow: 1,
  upperCloudEnabled: 1,
  upperCloudShadow: 1,
  upperShadowOnLowCloud: 1,
  coastalReliefVisible: 1,
  vegetationVisible: 1,
  surfaceTextureVisible: 1,
  landscapeLightingVisible: 1,
  seasonalSnowVisible: 1,
  seasonalVegetationVisible: 1,
  tidesEnabled: 1,
  pressureWindScale: 1,
  pressureCirculationEnabled: 1,
  crossIsobarScale: 1,
  crossIsobarEnabled: 1,
  thermalWindScale: 1,
  thermalWindEnabled: 1,
  shelterStrength: 1,
  shelterEnabled: 1,
  landscapeCompositionEnabled: 1,
  debugView: 0,
};

const range = (
  key: NumericStateKey,
  label: string,
  min: number,
  max: number,
  step: number,
): RangeControl => ({ type: "range", key, label, min, max, step });
const toggle = (key: NumericStateKey, label: string): ToggleControl => (
  { type: "toggle", key, label }
);

const wrappedPhase = (phase: number): number => ((phase % 1) + 1) % 1;
const worldDaySeconds = 86_400;
const worldYearDays = 360;
const lunarCycleDays = 28;

function worldPhases(): [number, number, number] {
  return [
    wrappedPhase(0.5833 + state.worldTimeSeconds / worldDaySeconds),
    wrappedPhase(0.5021 + state.worldTimeSeconds / (worldDaySeconds * worldYearDays)),
    wrappedPhase(0.25 + state.worldTimeSeconds / (worldDaySeconds * lunarCycleDays)),
  ];
}

const groups: ControlGroup[] = [
  {
    domain: "weather", category: "Visibility", layer: "Lighting", kind: "DEBUG", level: "visibility", controls: [
      toggle("celestialLightingEnabled", "Celestial lighting"),
    ],
  },
  {
    domain: "weather", category: "Visibility", layer: "Low cloud", kind: "DEBUG", level: "visibility", controls: [
      toggle("lowCloudEnabled", "Low-cloud layer"),
      toggle("lowCloudVisible", "Visible body"),
      toggle("lowCloudShadow", "Terrain shadow"),
    ],
  },
  {
    domain: "weather", category: "Advanced tuning", layer: "Low-cloud shape", kind: "SHADER", level: "advanced", controls: [
      range("detailCharacter", "Billow shape", 0, 1, 0.05),
      range("detailBlobiness", "Cell separation", 0, 1, 0.05),
      range("detailContrast", "Detail contrast", 0.25, 2.5, 0.05),
      range("detailEdgeSoftness", "Body edge", 0.005, 0.25, 0.005),
    ],
  },
  {
    domain: "weather", category: "Advanced tuning", layer: "Low-cloud coverage", kind: "SHADER", level: "advanced", controls: [
      range("maxWispCoverage", "Maximum wisp coverage", 0, 1, 0.01),
    ],
  },
  {
    domain: "weather", category: "Advanced tuning", layer: "Low-cloud appearance", kind: "SHADER", level: "advanced", controls: [
      range("opticalDepth", "Optical depth", 0, 3, 0.05),
      range("opacity", "Opacity", 0, 1.5, 0.05),
      range("darkness", "Low-cloud darkening", 0, 2, 0.05),
      range("relief", "Relief shading", 0, 2, 0.05),
    ],
  },
  {
    domain: "weather", category: "Visibility", layer: "Upper cloud", kind: "DEBUG", level: "visibility", controls: [
      toggle("upperCloudEnabled", "Upper-cloud layer"),
      toggle("upperCloudShadow", "Terrain shadow"),
      toggle("upperShadowOnLowCloud", "Shadow on low cloud"),
    ],
  },
  {
    domain: "weather", category: "Advanced tuning", layer: "Wind components", kind: "MODEL", level: "advanced", controls: [
      toggle("pressureCirculationEnabled", "Pressure circulation"),
      toggle("crossIsobarEnabled", "Cross-isobar flow"),
      toggle("thermalWindEnabled", "Thermal-wind shear"),
      toggle("shelterEnabled", "Terrain shelter"),
    ],
  },
  {
    domain: "weather", category: "Advanced tuning", layer: "Wind response", kind: "MODEL", level: "advanced", controls: [
      range("pressureWindScale", "Pressure response", 0, 2, 0.05),
      range("crossIsobarScale", "Cross-isobar response", 0, 2, 0.05),
      range("thermalWindScale", "Thermal response", 0, 2, 0.05),
      range("shelterStrength", "Shelter strength", 0, 1, 0.05),
    ],
  },
  {
    domain: "landscape", category: "Advanced tuning", layer: "Terrain shape", kind: "SHADER", level: "advanced", controls: [
      range("terrainScale", "Horizontal scale", 0.25, 4, 0.05),
      range("elevationScale", "Elevation scale", 0.1, 3, 0.05),
      range("cliffSlope", "Cliff slope", 0.01, 0.3, 0.005),
    ],
  },
  {
    domain: "landscape", category: "Advanced tuning", layer: "Landform lighting", kind: "SHADER", level: "advanced", controls: [
      range("landformReliefStrength", "Relief strength", 0, 2, 0.05),
      range("landformShadowStrength", "Cast-shadow strength", 0, 1, 0.05),
      range("landformShadowReach", "Shadow reach · m", 32, 1024, 16),
    ],
  },
  {
    domain: "landscape", category: "Visibility", layer: "Surface composition", kind: "DEBUG", level: "visibility", controls: [
      toggle("landscapeCompositionEnabled", "Landscape composition"),
      toggle("coastalReliefVisible", "Coastal reef relief"),
      toggle("vegetationVisible", "Vegetation"),
      toggle("surfaceTextureVisible", "Surface texture"),
      toggle("landscapeLightingVisible", "Terrain lighting"),
    ],
  },
  {
    domain: "landscape", category: "Visibility", layer: "Climate layers", kind: "DEBUG", level: "visibility", controls: [
      toggle("seasonalSnowVisible", "Seasonal snow"),
      toggle("seasonalVegetationVisible", "Vegetation seasons"),
    ],
  },
  {
    domain: "water", category: "Visibility", layer: "Astronomical tide", kind: "DEBUG", level: "visibility", controls: [
      toggle("tidesEnabled", "Tidal water height"),
    ],
  },
  {
    domain: "water", category: "Advanced tuning", layer: "Underwater visibility", kind: "SHADER", level: "advanced", controls: [
      range("waterAttenuationLength", "Bottom attenuation length · m", 0, 1, 0.01),
      range("bottomVisibility", "Bottom visibility", 0, 2, 0.02),
    ],
  },
];

const categoryOrder = [
  "Visibility", "Advanced tuning",
];

const lowCloudDependentKeys = new Set<NumericStateKey>([
  "detailCharacter", "detailBlobiness", "detailContrast", "detailEdgeSoftness",
  "maxWispCoverage",
  "opticalDepth", "opacity", "darkness", "relief",
  "lowCloudVisible", "lowCloudShadow",
]);
const landscapeCompositionKeys = new Set<NumericStateKey>([
  "coastalReliefVisible", "vegetationVisible", "surfaceTextureVisible",
  "landscapeLightingVisible", "seasonalSnowVisible",
  "seasonalVegetationVisible",
]);
const subtoggleKeys = new Set<NumericStateKey>([
  "lowCloudVisible", "lowCloudShadow", "upperCloudShadow", "upperShadowOnLowCloud",
  ...landscapeCompositionKeys,
]);
const subrangeKeys = new Set<NumericStateKey>([
  "pressureWindScale", "crossIsobarScale", "thermalWindScale", "shelterStrength",
]);

const debugOptions: Record<Domain, Array<[number, string]>> = {
  weather: [
    [0, "Final composition"], [1, "Low detail texture"], [2, "Low detail gate"],
    [3, "Low density"], [4, "Low optical depth"], [5, "Low normals"],
    [6, "Upper optical depth"],
    [9, "Projected low shadow"], [10, "Projected upper shadow"], [11, "Combined transmittance"],
    [12, "Low height field"],
    [16, "Surface wind vector · RGB"], [17, "Upper wind vector · RGB"],
    [18, "Vertical wind shear · RGB"], [19, "Pressure gradient vector · RGB"],
    [20, "Temperature gradient vector · RGB"], [21, "Terrain shelter"],
    [52, "Surface air temperature"], [56, "Solar elevation"],
    [58, "Cloud formation"], [59, "Cloud thickness"], [60, "Surface pressure"],
  ],
  landscape: [
    [0, "Final composition"], [32, "Elevation"], [33, "Slope"], [34, "Normals"],
    [35, "Island clusters"], [36, "Island detail"], [37, "Coastal reef field"],
    [38, "Vegetation field"], [39, "Coastal character · RGB"],
    [41, "Solid ice"], [42, "Autumn vegetation"],
    [51, "Annual climate"], [53, "Spatial snow"], [54, "Spatial ice climate"],
    [57, "Climate / snow / ice · RGB"],
  ],
  water: [
    [0, "Final composition"], [55, "Tide height"],
    [62, "Visual water normals"],
    [63, "Precipitation contact normals"],
    [64, "Gameplay-driven wave normals"],
    [65, "Ocean current vector · RGB"],
    [66, "Authoritative significant wave height"],
  ],
};

const categoryPresets: Record<string, Record<string, Partial<State>>> = {
  "weather:Visibility": {
    Production: {
      celestialLightingEnabled: defaults.celestialLightingEnabled,
      lowCloudEnabled: defaults.lowCloudEnabled,
      lowCloudVisible: defaults.lowCloudVisible,
      lowCloudShadow: defaults.lowCloudShadow,
      upperCloudEnabled: defaults.upperCloudEnabled,
      upperCloudShadow: defaults.upperCloudShadow,
      upperShadowOnLowCloud: defaults.upperShadowOnLowCloud,
    },
  },
  "weather:Advanced tuning": {
    Production: {
      detailCharacter: defaults.detailCharacter,
      detailBlobiness: defaults.detailBlobiness,
      detailContrast: defaults.detailContrast,
      detailEdgeSoftness: defaults.detailEdgeSoftness,
      maxWispCoverage: defaults.maxWispCoverage,
      opticalDepth: defaults.opticalDepth,
      opacity: defaults.opacity,
      darkness: defaults.darkness,
      relief: defaults.relief,
      pressureCirculationEnabled: defaults.pressureCirculationEnabled,
      pressureWindScale: defaults.pressureWindScale,
      crossIsobarEnabled: defaults.crossIsobarEnabled,
      crossIsobarScale: defaults.crossIsobarScale,
      thermalWindEnabled: defaults.thermalWindEnabled,
      thermalWindScale: defaults.thermalWindScale,
      shelterEnabled: defaults.shelterEnabled,
      shelterStrength: defaults.shelterStrength,
    },
  },
  "landscape:Advanced tuning": {
    Production: {
      terrainScale: defaults.terrainScale,
      elevationScale: defaults.elevationScale,
      cliffSlope: defaults.cliffSlope,
      landformReliefStrength: defaults.landformReliefStrength,
      landformShadowStrength: defaults.landformShadowStrength,
      landformShadowReach: defaults.landformShadowReach,
    },
  },
  "landscape:Visibility": {
    Production: {
      landscapeCompositionEnabled: defaults.landscapeCompositionEnabled,
      coastalReliefVisible: defaults.coastalReliefVisible,
      vegetationVisible: defaults.vegetationVisible,
      surfaceTextureVisible: defaults.surfaceTextureVisible,
      landscapeLightingVisible: defaults.landscapeLightingVisible,
      seasonalSnowVisible: defaults.seasonalSnowVisible,
      seasonalVegetationVisible: defaults.seasonalVegetationVisible,
    },
  },
  "water:Advanced tuning": {
    Production: {
      waterAttenuationLength: defaults.waterAttenuationLength,
      bottomVisibility: defaults.bottomVisibility,
    },
  },
};

const state: State = { ...defaults };
let activeDomain: Domain = "weather";

const canvas = document.querySelector<HTMLCanvasElement>("#preview");
const geometryOverlay = document.querySelector<HTMLCanvasElement>("#geometry-overlay");
const controls = document.querySelector<HTMLElement>("#controls");
const domains = document.querySelector<HTMLElement>("#domains");
const worldToolbar = document.querySelector<HTMLElement>("#world-toolbar");
const status = document.querySelector<HTMLElement>("#status");
const scaleLine = document.querySelector<HTMLElement>("#scale-line");
const scaleLabel = document.querySelector<HTMLElement>("#scale-label");
const geometryGuideLabel = document.querySelector<HTMLElement>(".geometry-guide-label");
if (!canvas || !geometryOverlay || !controls || !domains || !worldToolbar || !status || !scaleLine || !scaleLabel || !geometryGuideLabel) {
  throw new Error("Procedural lab page is incomplete");
}
const previewCanvas = canvas;
const geometryOverlayCanvas = geometryOverlay;
const geometryOverlayRenderingContext = geometryOverlayCanvas.getContext("2d");
if (!geometryOverlayRenderingContext) throw new Error("World geometry diagnostic is unavailable");
const geometryOverlayContext: CanvasRenderingContext2D = geometryOverlayRenderingContext;
const controlsElement = controls;
const domainsElement = domains;
const worldToolbarElement = worldToolbar;
const statusElement = status;
const scaleLineElement = scaleLine;
const scaleLabelElement = scaleLabel;
const geometryGuideLabelElement = geometryGuideLabel;

type ValueTransition = { elapsed: number; from: number; to: number };
const valueTransitions = new Map<NumericStateKey, ValueTransition>();
const inputByKey = new Map<NumericStateKey, HTMLInputElement>();
const outputByKey = new Map<NumericStateKey, HTMLInputElement>();
const rangeByKey = new Map<NumericStateKey, RangeControl>();
const resetByKey = new Map<NumericStateKey, HTMLButtonElement>();
const controlElementByKey = new Map<NumericStateKey, HTMLElement>();
let derivedOutputElement: HTMLElement | undefined;
let renderedCameraX = 0;
let renderedCameraY = 0;

function format(value: number, step: number): string {
  return value.toFixed(step < 0.001 ? 4 : step < 0.01 ? 3 : step < 1 ? 2 : 0);
}

function tag(kind: ControlKind): HTMLSpanElement {
  const element = document.createElement("span");
  element.className = `kind kind-${kind.toLowerCase()}`;
  element.textContent = kind;
  return element;
}

function updateResetButton(key: NumericStateKey): void {
  const button = resetByKey.get(key);
  if (button) button.hidden = state[key] === defaults[key];
}

function setControlsEnabled(keys: Iterable<NumericStateKey>, enabled: boolean): void {
  for (const key of keys) {
    inputByKey.get(key)?.toggleAttribute("disabled", !enabled);
    resetByKey.get(key)?.toggleAttribute("disabled", !enabled);
    controlElementByKey.get(key)?.classList.toggle("control-disabled", !enabled);
  }
}

function updateDependentControls(): void {
  const lowCloudEnabled = state.lowCloudEnabled > 0.5;
  const upperCloudEnabled = state.upperCloudEnabled > 0.5;
  setControlsEnabled(lowCloudDependentKeys, lowCloudEnabled);
  setControlsEnabled(["upperCloudShadow"], upperCloudEnabled);
  setControlsEnabled(["upperShadowOnLowCloud"], lowCloudEnabled && upperCloudEnabled);
  setControlsEnabled(["pressureWindScale"], state.pressureCirculationEnabled > 0.5);
  setControlsEnabled(["crossIsobarScale"], state.crossIsobarEnabled > 0.5);
  setControlsEnabled(["thermalWindScale"], state.thermalWindEnabled > 0.5);
  setControlsEnabled(["shelterStrength"], state.shelterEnabled > 0.5);
  setControlsEnabled(landscapeCompositionKeys, state.landscapeCompositionEnabled > 0.5);
}

function resetButton(
  key: NumericStateKey,
  label: string,
  afterReset?: () => void,
): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "reset-value";
  button.textContent = "↺";
  button.title = `Reset ${label} to default`;
  button.setAttribute("aria-label", button.title);
  button.addEventListener("click", () => {
    valueTransitions.delete(key);
    Object.assign(state, { [key]: defaults[key] });
    syncControl(key);
    afterReset?.();
  });
  resetByKey.set(key, button);
  updateResetButton(key);
  return button;
}

function createRangeControl(control: RangeControl): HTMLDivElement {
  const container = document.createElement("div");
  container.className = "control range-control";
  container.classList.toggle("subcontrol", subrangeKeys.has(control.key));
  const heading = document.createElement("label");
  heading.className = "control-name";
  heading.textContent = control.label;
  const input = document.createElement("input");
  input.type = "range";
  input.id = `control-${control.key}`;
  heading.htmlFor = input.id;
  input.min = String(control.min);
  input.max = String(control.max);
  input.step = String(control.step);
  const controlValue = state[control.key];
  input.value = String(controlValue);
  const output = document.createElement("input");
  output.type = "number";
  output.className = "range-value";
  output.min = String(control.min);
  output.max = String(control.max);
  output.step = String(control.step);
  output.value = format(controlValue, control.step);
  input.addEventListener("input", () => {
    valueTransitions.delete(control.key);
    const value = Number(input.value);
    state[control.key] = value;
    syncControl(control.key);
  });
  output.addEventListener("input", () => {
    if (!Number.isFinite(output.valueAsNumber)) return;
    valueTransitions.delete(control.key);
    const value = Math.min(control.max, Math.max(control.min, output.valueAsNumber));
    state[control.key] = value;
    syncControl(control.key);
  });
  output.addEventListener("change", () => syncControl(control.key));
  let pointerId: number | undefined;
  let pointerX = 0;
  let dragging = false;
  input.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    pointerId = event.pointerId;
    pointerX = event.clientX;
    dragging = false;
    input.setPointerCapture(event.pointerId);
  });
  input.addEventListener("pointermove", (event) => {
    if (event.pointerId !== pointerId) return;
    if (!dragging && Math.abs(event.clientX - pointerX) < 4) return;
    dragging = true;
    valueTransitions.delete(control.key);
    state[control.key] = rangeValueAtPointer(control, input, event.clientX);
    syncControl(control.key);
  });
  input.addEventListener("pointerup", (event) => {
    if (event.pointerId !== pointerId) return;
    if (!dragging) {
      valueTransitions.set(control.key, {
        elapsed: 0,
        from: state[control.key],
        to: rangeValueAtPointer(control, input, event.clientX),
      });
    }
    pointerId = undefined;
  });
  input.addEventListener("pointercancel", () => { pointerId = undefined; });
  container.append(heading, input, output);
  container.append(resetButton(control.key, control.label));
  inputByKey.set(control.key, input);
  outputByKey.set(control.key, output);
  rangeByKey.set(control.key, control);
  controlElementByKey.set(control.key, container);
  return container;
}

function rangeValueAtPointer(control: RangeControl, input: HTMLInputElement, clientX: number): number {
  const rect = input.getBoundingClientRect();
  const amount = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  const value = control.min + amount * (control.max - control.min);
  const controlValue = Math.min(
    control.max,
    Math.max(control.min, Math.round(value / control.step) * control.step),
  );
  return controlValue;
}

function createToggleControl(control: ToggleControl): HTMLDivElement {
  const container = document.createElement("div");
  container.className = "control toggle-control";
  container.classList.toggle("subtoggle", subtoggleKeys.has(control.key));
  const input = document.createElement("input");
  input.type = "checkbox";
  input.id = `control-${control.key}`;
  input.checked = state[control.key] > 0.5;
  input.addEventListener("change", () => {
    state[control.key] = Number(input.checked);
    syncControl(control.key);
  });
  const heading = document.createElement("label");
  heading.className = "control-name";
  heading.htmlFor = input.id;
  heading.textContent = control.label;
  container.append(input, heading, resetButton(control.key, control.label));
  inputByKey.set(control.key, input);
  controlElementByKey.set(control.key, container);
  return container;
}

function renderControls(): void {
  controlsElement.replaceChildren();
  inputByKey.clear();
  outputByKey.clear();
  rangeByKey.clear();
  resetByKey.clear();
  controlElementByKey.clear();
  const domainGroups = groups.filter((group) => group.domain === activeDomain);
  const categories = [...new Set(domainGroups.map((group) => group.category))]
    .sort((left, right) => categoryOrder.indexOf(left) - categoryOrder.indexOf(right));
  for (const category of categories) {
    const section = document.createElement("section");
    section.className = "control-category";
    const title = document.createElement("h2");
    title.textContent = category;
    section.append(title);
    const presets = categoryPresets[`${activeDomain}:${category}`];
    if (presets) {
      const presetBar = document.createElement("nav");
      presetBar.className = "category-presets";
      presetBar.setAttribute("aria-label", `${category} presets`);
      const presetLabel = document.createElement("span");
      presetLabel.textContent = "PRESETS";
      presetBar.append(presetLabel);
      for (const [name, values] of Object.entries(presets)) {
        const button = document.createElement("button");
        button.textContent = name;
        button.addEventListener("click", () => {
          valueTransitions.clear();
          Object.assign(state, values);
          for (const key of Object.keys(values) as NumericStateKey[]) syncControl(key);
          updateDependentControls();
        });
        presetBar.append(button);
      }
      section.append(presetBar);
    }
    for (const group of domainGroups.filter((candidate) => candidate.category === category)) {
      const details = document.createElement("details");
      details.className = group.level ? `control-group ${group.level}` : "control-group primary";
      details.open = group.level === undefined;
      const summary = document.createElement("summary");
      const summaryName = document.createElement("span");
      summaryName.textContent = group.layer;
      summary.append(summaryName, tag(group.kind));
      details.append(summary);
      for (const control of group.controls) {
        details.append(
          control.type === "range"
            ? createRangeControl(control)
            : createToggleControl(control),
        );
      }
      section.append(details);
    }
    controlsElement.append(section);
  }
  derivedOutputElement = undefined;
  const derivedSection = document.createElement("section");
  derivedSection.className = "control-category derived-output";
  const derivedName = activeDomain === "weather" ? "atmosphere" : "surface";
  derivedSection.innerHTML = `<h2>Derived ${derivedName} · read only</h2><div class="derived-grid"></div>`;
  derivedOutputElement = derivedSection.querySelector<HTMLElement>(".derived-grid")!;
  controlsElement.append(derivedSection);
  const diagnosticSection = document.createElement("section");
  diagnosticSection.className = "control-category diagnostics";
  diagnosticSection.innerHTML = "<h2>Diagnostics</h2>";
  const diagnosticLabel = document.createElement("div");
  diagnosticLabel.className = "select-control";
  const diagnosticHeading = document.createElement("label");
  diagnosticHeading.className = "control-name";
  diagnosticHeading.append(document.createTextNode("Output"), tag("DEBUG"));
  const diagnosticSelect = document.createElement("select");
  diagnosticSelect.id = "diagnostic-output";
  diagnosticHeading.htmlFor = diagnosticSelect.id;
  for (const [value, name] of debugOptions[activeDomain]) {
    const option = document.createElement("option");
    option.value = String(value);
    option.textContent = name;
    option.selected = state.debugView === value;
    diagnosticSelect.append(option);
  }
  diagnosticSelect.addEventListener("change", () => {
    state.debugView = Number(diagnosticSelect.value);
    updateResetButton("debugView");
  });
  diagnosticLabel.append(
    diagnosticHeading,
    diagnosticSelect,
    resetButton("debugView", "diagnostic output", () => {
      diagnosticSelect.value = String(state.debugView);
    }),
  );
  diagnosticSection.append(diagnosticLabel);
  controlsElement.append(diagnosticSection);
  updateDependentControls();
}

function renderDomains(): void {
  domainsElement.replaceChildren();
  for (const domain of ["weather", "landscape", "water"] as Domain[]) {
    const button = document.createElement("button");
    button.textContent = domain[0].toUpperCase() + domain.slice(1);
    button.className = domain === activeDomain ? "active" : "";
    button.addEventListener("click", () => {
      activeDomain = domain;
      state.debugView = 0;
      renderDomains();
      renderControls();
    });
    domainsElement.append(button);
  }
}

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
    <summary><span>World and playback</span><span class="kind kind-world">WORLD</span></summary>
    <div class="text-control no-reset"><label class="control-name" for="seed">Seed</label><input id="seed" type="text" value="${state.seed}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="world-width">Longitude circumference · km</label><input id="world-width" type="range" min="8" max="256" step="1" value="${state.worldWidthM / 1000}"><input id="world-width-value" class="range-value" type="number" min="8" max="256" step="1" value="${state.worldWidthM / 1000}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="world-height">Pole-to-pole span · km</label><input id="world-height" type="range" min="8" max="256" step="1" value="${state.worldHeightM / 1000}"><input id="world-height-value" class="range-value" type="number" min="8" max="256" step="1" value="${state.worldHeightM / 1000}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="cloud-time-scale">Cloud evolution · × real time</label><input id="cloud-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.cloudTimeScale)}"><input id="cloud-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.cloudTimeScale}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="pressure-time-scale">Pressure evolution · × real time</label><input id="pressure-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.pressureTimeScale)}"><input id="pressure-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.pressureTimeScale}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="current-time-scale">Current evolution · × real time</label><input id="current-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.currentTimeScale)}"><input id="current-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.currentTimeScale}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="world-time-scale">World clock · × real time</label><input id="world-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.worldTimeScale)}"><input id="world-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.worldTimeScale}"></div>
    <div class="control toggle-control no-reset"><input id="lock-time-scales" type="checkbox"><label class="control-name" for="lock-time-scales">Lock speeds together</label></div>
    <div class="actions"><button id="pause">Pause</button></div>
  </details>
  <details>
    <summary><span>View-center diagnostics</span><span class="kind kind-debug">DEBUG</span></summary>
    <div class="geometry-measurements"><h3>View-center geometry</h3><div id="geometry-measurements" class="derived-grid"></div></div>
  </details>`;
const seedInput = worldToolbarElement.querySelector<HTMLInputElement>("#seed")!;
const worldWidthInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-width")!;
const worldWidthValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-width-value")!;
const worldHeightInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-height")!;
const worldHeightValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-height-value")!;
const geometryMeasurementsElement = worldToolbarElement.querySelector<HTMLElement>("#geometry-measurements")!;
const cloudTimeScaleInput = worldToolbarElement.querySelector<HTMLInputElement>("#cloud-time-scale")!;
const cloudTimeValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#cloud-time-value")!;
const pressureTimeScaleInput = worldToolbarElement.querySelector<HTMLInputElement>("#pressure-time-scale")!;
const pressureTimeValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#pressure-time-value")!;
const currentTimeScaleInput = worldToolbarElement.querySelector<HTMLInputElement>("#current-time-scale")!;
const currentTimeValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#current-time-value")!;
const worldTimeScaleInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-time-scale")!;
const worldTimeValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-time-value")!;
const lockTimeScalesInput = worldToolbarElement.querySelector<HTMLInputElement>("#lock-time-scales")!;
const pauseButton = worldToolbarElement.querySelector<HTMLButtonElement>("#pause")!;
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
  cloudTimeScaleInput.value = String(timeScaleToSlider(state.cloudTimeScale));
  cloudTimeValueInput.value = String(state.cloudTimeScale);
  pressureTimeScaleInput.value = String(timeScaleToSlider(state.pressureTimeScale));
  pressureTimeValueInput.value = String(state.pressureTimeScale);
  currentTimeScaleInput.value = String(timeScaleToSlider(state.currentTimeScale));
  currentTimeValueInput.value = String(state.currentTimeScale);
  worldTimeScaleInput.value = String(timeScaleToSlider(state.worldTimeScale));
  worldTimeValueInput.value = String(state.worldTimeScale);
  lockTimeScalesInput.checked = state.timeScalesLocked;
}
function setCloudTimeScale(value: number): void {
  state.cloudTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  if (state.timeScalesLocked) {
    state.pressureTimeScale = state.cloudTimeScale;
    state.currentTimeScale = state.cloudTimeScale;
    state.worldTimeScale = state.cloudTimeScale;
  }
  syncTimeScaleInputs();
}
function setPressureTimeScale(value: number): void {
  state.pressureTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  if (state.timeScalesLocked) {
    state.cloudTimeScale = state.pressureTimeScale;
    state.currentTimeScale = state.pressureTimeScale;
    state.worldTimeScale = state.pressureTimeScale;
  }
  syncTimeScaleInputs();
}
function setCurrentTimeScale(value: number): void {
  state.currentTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  if (state.timeScalesLocked) {
    state.cloudTimeScale = state.currentTimeScale;
    state.pressureTimeScale = state.currentTimeScale;
    state.worldTimeScale = state.currentTimeScale;
  }
  syncTimeScaleInputs();
}
function setWorldTimeScale(value: number): void {
  state.worldTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  if (state.timeScalesLocked) {
    state.cloudTimeScale = state.worldTimeScale;
    state.pressureTimeScale = state.worldTimeScale;
    state.currentTimeScale = state.worldTimeScale;
  }
  syncTimeScaleInputs();
}
cloudTimeScaleInput.addEventListener("input", () => {
  setCloudTimeScale(timeScaleFromSlider(cloudTimeScaleInput.valueAsNumber));
});
cloudTimeValueInput.addEventListener("input", () => {
  if (Number.isFinite(cloudTimeValueInput.valueAsNumber)) {
    setCloudTimeScale(cloudTimeValueInput.valueAsNumber);
  }
});
cloudTimeValueInput.addEventListener("change", syncTimeScaleInputs);
pressureTimeScaleInput.addEventListener("input", () => {
  setPressureTimeScale(timeScaleFromSlider(pressureTimeScaleInput.valueAsNumber));
});
pressureTimeValueInput.addEventListener("input", () => {
  if (Number.isFinite(pressureTimeValueInput.valueAsNumber)) {
    setPressureTimeScale(pressureTimeValueInput.valueAsNumber);
  }
});
pressureTimeValueInput.addEventListener("change", syncTimeScaleInputs);
currentTimeScaleInput.addEventListener("input", () => {
  setCurrentTimeScale(timeScaleFromSlider(currentTimeScaleInput.valueAsNumber));
});
currentTimeValueInput.addEventListener("input", () => {
  if (Number.isFinite(currentTimeValueInput.valueAsNumber)) {
    setCurrentTimeScale(currentTimeValueInput.valueAsNumber);
  }
});
currentTimeValueInput.addEventListener("change", syncTimeScaleInputs);
worldTimeScaleInput.addEventListener("input", () => {
  setWorldTimeScale(timeScaleFromSlider(worldTimeScaleInput.valueAsNumber));
});
worldTimeValueInput.addEventListener("input", () => {
  if (Number.isFinite(worldTimeValueInput.valueAsNumber)) {
    setWorldTimeScale(worldTimeValueInput.valueAsNumber);
  }
});
worldTimeValueInput.addEventListener("change", syncTimeScaleInputs);
lockTimeScalesInput.addEventListener("change", () => {
  state.timeScalesLocked = lockTimeScalesInput.checked;
  if (state.timeScalesLocked) {
    state.pressureTimeScale = state.cloudTimeScale;
    state.currentTimeScale = state.cloudTimeScale;
    state.worldTimeScale = state.cloudTimeScale;
  }
  syncTimeScaleInputs();
});
pauseButton.addEventListener("click", () => {
  state.paused = !state.paused;
  pauseButton.textContent = state.paused ? "Resume" : "Pause";
});

function syncControl(key: NumericStateKey): void {
  updateResetButton(key);
  if (
    key === "lowCloudEnabled"
    || key === "upperCloudEnabled"
    || key === "celestialLightingEnabled"
    || key === "pressureCirculationEnabled"
    || key === "crossIsobarEnabled"
    || key === "thermalWindEnabled"
    || key === "shelterEnabled"
    || key === "landscapeCompositionEnabled"
    || key === "tidesEnabled"
  ) updateDependentControls();
  const input = inputByKey.get(key);
  if (!input) return;
  if (input.type === "checkbox") {
    input.checked = state[key] > 0.5;
    return;
  }
  const definition = rangeByKey.get(key);
  const output = outputByKey.get(key);
  if (definition && output) {
    const value = state[key];
    input.value = String(value);
    output.value = format(value, definition.step);
  }
}

function advanceTransitions(elapsed: number): void {
  for (const [key, transition] of valueTransitions) {
    transition.elapsed += elapsed;
    const progress = Math.min(transition.elapsed / 0.3, 1);
    const amount = progress * progress * (3 - 2 * progress);
    state[key] = transition.from + (transition.to - transition.from) * amount;
    syncControl(key);
    if (progress === 1) valueTransitions.delete(key);
  }
}

renderDomains();
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

const gpu = (navigator as Navigator & { gpu?: any }).gpu;
if (!gpu) throw new Error("WebGPU is unavailable");
const adapter = await gpu.requestAdapter();
if (!adapter) throw new Error("No WebGPU adapter available");
const device = await adapter.requestDevice();
const context = previewCanvas.getContext("webgpu") as any;
const formatName = gpu.getPreferredCanvasFormat();
context.configure({ device, format: formatName, alphaMode: "opaque" });

const shader = device.createShaderModule({ code: shaderSource });
const pipeline = device.createRenderPipeline({
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
  fragment: { module: shader, entryPoint: "fragment_playground", targets: [{ format: formatName }] },
  primitive: { topology: "triangle-list" },
});

const vertices = new Float32Array([
  -1, -1, 0, 0, 0, 1, 1, -1, 0, 0, 0, 1, -1, 1, 0, 0, 0, 1,
  -1, 1, 0, 0, 0, 1, 1, -1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1,
]);
const vertexBuffer = device.createBuffer({ size: vertices.byteLength, usage: 0x20 | 0x08 });
device.queue.writeBuffer(vertexBuffer, 0, vertices);
const sceneFloats = new Float32Array(48);
const sceneUints = new Uint32Array(sceneFloats.buffer);
const tuningFloats = new Float32Array(44);
const tuningUints = new Uint32Array(tuningFloats.buffer);
const sceneBuffer = device.createBuffer({ size: sceneFloats.byteLength, usage: 0x40 | 0x08 });
const playgroundBuffer = device.createBuffer({ size: tuningFloats.byteLength, usage: 0x40 | 0x08 });
const sceneBindGroup = device.createBindGroup({
  layout: pipeline.getBindGroupLayout(0),
  entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
});
const playgroundBindGroup = device.createBindGroup({
  layout: pipeline.getBindGroupLayout(1),
  entries: [{ binding: 0, resource: { buffer: playgroundBuffer } }],
});

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

let worldCenterDiagnostics: WorldCenterDiagnostics | undefined;

function parsedSeed(): bigint {
  try {
    return BigInt.asUintN(64, BigInt(state.seed));
  } catch {
    return 0x5eedn;
  }
}

function sampleWorldCenterDiagnostics(): void {
  const sample = sample_playground_environment(
    state.worldWidthM,
    state.worldHeightM,
    parsedSeed(),
    BigInt(Math.max(0, Math.floor(state.worldTimeSeconds * 60))),
    state.cameraX,
    state.cameraY,
  );
  worldCenterDiagnostics = {
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
}

function atmosphericOutputs(): Array<[string, string]> {
  if (!worldCenterDiagnostics) return [];
  const output = worldCenterDiagnostics;
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
    ["Moon", `${angle(output.moonDirection)} · ${output.moonIntensity.toFixed(2)}`],
    ["Astronomical ambient", output.ambientIntensity.toFixed(2)],
    ["Surface wind", `${speed(output.surfaceWind).toFixed(1)} m/s · ${direction(output.surfaceWind).toFixed(0)}°`],
    ["Upper wind", `${speed(output.upperWind).toFixed(1)} m/s · ${direction(output.upperWind).toFixed(0)}°`],
    ["Vertical shear", `${speed(shear).toFixed(1)} m/s · ${direction(shear).toFixed(0)}°`],
    ["Relative humidity", `${(output.relativeHumidity * 100).toFixed(0)} %`],
    ["Instability", output.instability.toFixed(2)],
    ["Lift", output.lift.toFixed(2)],
    ["Low cloud", output.cloudiness.toFixed(2)],
    ["Upper cloud", output.upperCloudiness.toFixed(2)],
    ["Rain", output.rainIntensity.toFixed(2)],
  ];
}

function surfaceOutputs(): Array<[string, string]> {
  if (!worldCenterDiagnostics) return [];
  const output = worldCenterDiagnostics;
  const outputs: Array<[string, string]> = [
    ["Ground temperature", `${output.groundTemperatureC.toFixed(1)} °C`],
    ["Water temperature", `${output.waterTemperatureC.toFixed(1)} °C`],
    ["Seasonal snow", output.seasonalSnowCover.toFixed(2)],
    ["Polar ice closure", output.polarIceClosure.toFixed(2)],
    ["Solid ice", output.solidIceCover.toFixed(2)],
    ["Tide height", `${output.waterHeightM.toFixed(2)} m`],
    ["Water depth", `${output.waterDepthM.toFixed(2)} m`],
  ];
  if (activeDomain === "water") {
    const vector = (value: [number, number]): string => {
      const speed = Math.hypot(...value);
      const direction = Math.atan2(value[1], value[0]) * 180 / Math.PI;
      return `${speed.toFixed(2)} m/s · ${direction.toFixed(0)}°`;
    };
    outputs.push(
      ["Current", vector(output.currentVelocity)],
      ["Wave orbital motion", vector(output.waveOrbitalVelocity)],
      ["Significant wave height", `${output.significantWaveHeightM.toFixed(2)} m`],
      ["Instantaneous wave surface", `${output.waveSurfaceDisplacementM.toFixed(2)} m`],
      ["Rogue crest excess", `${output.rogueCrestExcessM.toFixed(2)} m`],
    );
  }
  return outputs;
}

function updateDerivedOutput(): void {
  if (!derivedOutputElement) return;
  const outputs = activeDomain === "weather" ? atmosphericOutputs() : surfaceOutputs();
  derivedOutputElement.replaceChildren(...outputs.flatMap(([label, value]) => {
    const name = document.createElement("span");
    name.textContent = label;
    const output = document.createElement("output");
    output.value = value;
    return [name, output];
  }));
}

function updateGeometryMeasurements(): void {
  const sample = sample_playground_world_geometry(
    state.worldWidthM,
    state.worldHeightM,
    state.cameraX,
    state.cameraY,
  );
  const values: Array<[string, string]> = [
    ["Raw world center", `${sample.raw_x_m.toFixed(1)}, ${sample.raw_y_m.toFixed(1)} m`],
    ["Cylinder center", `${sample.canonical_x_m.toFixed(1)}, ${sample.canonical_y_m.toFixed(1)} m`],
    ["Normalized geography", `${sample.normalized_x.toFixed(4)}, ${sample.normalized_y.toFixed(4)}`],
  ];
  const octaveDefinitions: Array<[string, boolean, number]> = activeDomain === "weather"
    ? [["Weather macro octave", true, 65536], ["Cloud detail octave", false, 640]]
    : activeDomain === "landscape"
      ? [["Terrain macro octave", true, 8192], ["Terrain relief octave", false, 256]]
      : [["Tide response octave", true, 131072], ["Reef detail octave", false, 32]];
  for (const [label, circleEmbedded, wavelengthM] of octaveDefinitions) {
    const octave = sample_playground_noise_octave(
      state.worldWidthM,
      state.worldHeightM,
      circleEmbedded,
      wavelengthM,
    );
    const error = 100 * Math.max(octave.relative_error_x, octave.relative_error_y);
    values.push([
      label,
      `${octave.basis} · requested ${octave.requested_wavelength_m.toFixed(0)} m · effective ${octave.effective_wavelength_x_m.toFixed(1)} × ${octave.effective_wavelength_y_m.toFixed(1)} m · ${error.toFixed(2)}% max error`,
    ]);
    octave.free();
  }
  renderedCameraX = sample.canonical_x_m;
  renderedCameraY = sample.canonical_y_m;
  sample.free();
  geometryMeasurementsElement.replaceChildren(...values.flatMap(([label, value]) => {
    const name = document.createElement("span");
    name.textContent = label;
    const output = document.createElement("output");
    output.value = value;
    return [name, output];
  }));
}

function worldToOverlayPoint(positionX: number, positionY: number, pixelsPerMetre: number): [number, number] {
  return [
    geometryOverlayCanvas.clientWidth / 2 + (positionX - state.cameraX) * pixelsPerMetre,
    geometryOverlayCanvas.clientHeight / 2 - (positionY - state.cameraY) * pixelsPerMetre,
  ];
}

function drawPeriodicGuideLines(
  periodM: number,
  offsetM: number,
  minimumM: number,
  maximumM: number,
  vertical: boolean,
): void {
  const firstIndex = Math.ceil((minimumM - offsetM) / periodM);
  const lastIndex = Math.floor((maximumM - offsetM) / periodM);
  const pixelsPerMetre = geometryOverlayCanvas.clientHeight / state.viewHeight;
  if (periodM * pixelsPerMetre < 4) return;
  let index = firstIndex;
  geometryOverlayContext.beginPath();
  while (index <= lastIndex) {
    const positionM = offsetM + index * periodM;
    if (vertical) {
      const [x] = worldToOverlayPoint(positionM, state.cameraY, pixelsPerMetre);
      geometryOverlayContext.moveTo(x, 0);
      geometryOverlayContext.lineTo(x, geometryOverlayCanvas.clientHeight);
    } else {
      const [, y] = worldToOverlayPoint(state.cameraX, positionM, pixelsPerMetre);
      geometryOverlayContext.moveTo(0, y);
      geometryOverlayContext.lineTo(geometryOverlayCanvas.clientWidth, y);
    }
    index += 1;
  }
  geometryOverlayContext.stroke();
}

function drawGuideLine(positionM: number, vertical: boolean): void {
  const pixelsPerMetre = geometryOverlayCanvas.clientHeight / state.viewHeight;
  geometryOverlayContext.beginPath();
  if (vertical) {
    const [x] = worldToOverlayPoint(positionM, state.cameraY, pixelsPerMetre);
    geometryOverlayContext.moveTo(x, 0);
    geometryOverlayContext.lineTo(x, geometryOverlayCanvas.clientHeight);
  } else {
    const [, y] = worldToOverlayPoint(state.cameraX, positionM, pixelsPerMetre);
    geometryOverlayContext.moveTo(0, y);
    geometryOverlayContext.lineTo(geometryOverlayCanvas.clientWidth, y);
  }
  geometryOverlayContext.stroke();
}

function drawGeometryOverlay(): void {
  const widthM = state.viewHeight * geometryOverlayCanvas.clientWidth / geometryOverlayCanvas.clientHeight;
  const minimumX = state.cameraX - widthM / 2;
  const maximumX = state.cameraX + widthM / 2;
  const minimumY = state.cameraY - state.viewHeight / 2;
  const maximumY = state.cameraY + state.viewHeight / 2;
  const pixelScaleX = geometryOverlayCanvas.width / geometryOverlayCanvas.clientWidth;
  const pixelScaleY = geometryOverlayCanvas.height / geometryOverlayCanvas.clientHeight;
  geometryOverlayContext.setTransform(pixelScaleX, 0, 0, pixelScaleY, 0, 0);
  geometryOverlayContext.clearRect(0, 0, geometryOverlayCanvas.clientWidth, geometryOverlayCanvas.clientHeight);
  geometryGuideLabelElement.hidden = false;
  geometryOverlayContext.lineWidth = 1;
  geometryOverlayContext.strokeStyle = "rgb(105 189 232 / 68%)";
  geometryOverlayContext.setLineDash([]);
  drawPeriodicGuideLines(state.worldWidthM, 0, minimumX, maximumX, true);
  if (minimumY <= 0 && maximumY >= 0) drawGuideLine(0, false);
  geometryOverlayContext.strokeStyle = "rgb(229 120 136 / 82%)";
  geometryOverlayContext.setLineDash([7, 5]);
  drawPeriodicGuideLines(state.worldWidthM, state.worldWidthM / 2, minimumX, maximumX, true);
  for (const poleY of [-state.worldHeightM / 2, state.worldHeightM / 2]) {
    if (minimumY <= poleY && maximumY >= poleY) drawGuideLine(poleY, false);
  }
  geometryOverlayContext.setLineDash([]);
  geometryOverlayContext.strokeStyle = "#ff334f";
  geometryOverlayContext.lineWidth = 2;
  const centerX = geometryOverlayCanvas.clientWidth / 2;
  const centerY = geometryOverlayCanvas.clientHeight / 2;
  geometryOverlayContext.beginPath();
  geometryOverlayContext.moveTo(centerX - 9, centerY);
  geometryOverlayContext.lineTo(centerX + 9, centerY);
  geometryOverlayContext.moveTo(centerX, centerY - 9);
  geometryOverlayContext.lineTo(centerX, centerY + 9);
  geometryOverlayContext.stroke();
}

function writeUniforms(): void {
  const aspect = previewCanvas.width / previewCanvas.height;
  sceneFloats.set([
    renderedCameraX,
    renderedCameraY,
    state.viewHeight * aspect,
    state.viewHeight,
  ], 0);
  const transport = sample_playground_surface_weather_transport(
    state.worldWidthM,
    state.worldHeightM,
    parsedSeed(),
    BigInt(Math.max(0, Math.floor(state.worldTimeSeconds * 60))),
  );
  sceneFloats.set([
    transport.upper_air_temperature_x_m,
    transport.upper_air_temperature_y_m,
    0,
    0,
  ], 4);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  const lowCloudEpoch = BigInt(Math.floor(state.time / 1800));
  const upperCloudEpoch = BigInt(Math.floor(state.time / 3600));
  sceneUints.set([
    seedLow,
    seedHigh,
    Number(lowCloudEpoch & 0xffff_ffffn),
    Number((lowCloudEpoch >> 32n) & 0xffff_ffffn),
  ], 8);
  sceneFloats.set([state.worldWidthM, state.worldHeightM], 12);
  sceneUints.set([
    Number(upperCloudEpoch & 0xffff_ffffn),
    Number((upperCloudEpoch >> 32n) & 0xffff_ffffn),
  ], 14);
  sceneFloats.set([
    (state.time % 1800) / 1800,
    (state.time % 3600) / 3600,
    0,
    0,
  ], 16);
  sceneFloats.set([
    transport.air_temperature_x_m,
    transport.air_temperature_y_m,
    transport.water_vapor_x_m,
    transport.water_vapor_y_m,
  ], 20);
  transport.free();
  const [dayPhase, seasonPhase, moonPhase] = worldPhases();
  sceneFloats.set([dayPhase, seasonPhase, moonPhase, 0], 24);
  const worldTick = BigInt(Math.max(0, Math.floor(state.worldTimeSeconds * 60)));
  const pressureTick = BigInt(Math.max(0, Math.floor(state.pressureTimeSeconds * 60)));
  const currentTick = BigInt(Math.max(0, Math.floor(state.currentTimeSeconds * 60)));
  sceneUints.set([
    Number(worldTick & 0xffff_ffffn),
    Number((worldTick >> 32n) & 0xffff_ffffn),
    0,
    0,
  ], 28);
  sceneFloats.set(sample_playground_vector_field_transport(
    state.worldWidthM,
    state.worldHeightM,
    parsedSeed(),
    pressureTick,
    currentTick,
  ), 32);
  device.queue.writeBuffer(sceneBuffer, 0, sceneFloats);

  tuningFloats.set([
    state.detailCharacter, state.detailBlobiness, state.detailContrast, state.detailEdgeSoftness,
  ], 0);
  tuningFloats.set([
    state.maxWispCoverage, 0, 0, 0,
  ], 4);
  tuningFloats.set([state.opticalDepth, state.opacity, state.darkness, state.relief], 8);
  tuningFloats.set([state.terrainScale, state.elevationScale, state.cliffSlope, 0], 12);
  tuningFloats.set([
    state.waterAttenuationLength, state.bottomVisibility, 0, 0,
  ], 16);
  tuningFloats.set([
    state.landformReliefStrength,
    state.landformShadowStrength,
    state.landformShadowReach,
    0,
  ], 20);
  tuningFloats.set([
    state.lowCloudEnabled * state.lowCloudVisible,
    state.lowCloudEnabled * state.lowCloudShadow,
    state.upperCloudEnabled * state.upperCloudShadow,
    state.lowCloudEnabled * state.upperCloudEnabled * state.upperShadowOnLowCloud,
  ], 24);
  tuningFloats.set([
    state.landscapeCompositionEnabled * state.coastalReliefVisible,
    state.landscapeCompositionEnabled * state.vegetationVisible,
    state.landscapeCompositionEnabled * state.surfaceTextureVisible,
    state.landscapeCompositionEnabled * state.landscapeLightingVisible,
  ], 28);
  tuningFloats.set([
    state.pressureCirculationEnabled * state.pressureWindScale,
    state.crossIsobarEnabled * state.crossIsobarScale,
    state.thermalWindEnabled * state.thermalWindScale,
    state.shelterEnabled * state.shelterStrength,
  ], 32);
  tuningFloats.set([
    state.landscapeCompositionEnabled * state.seasonalSnowVisible,
    state.tidesEnabled,
    0,
    state.landscapeCompositionEnabled * state.seasonalVegetationVisible,
  ], 36);
  tuningUints.set([state.debugView, state.celestialLightingEnabled, 0, 0], 40);
  device.queue.writeBuffer(playgroundBuffer, 0, tuningFloats);
}

function resize(): void {
  const scale = Math.min(devicePixelRatio, 2);
  const width = Math.max(1, Math.floor(previewCanvas.clientWidth * scale));
  const height = Math.max(1, Math.floor(previewCanvas.clientHeight * scale));
  if (previewCanvas.width !== width || previewCanvas.height !== height) {
    previewCanvas.width = width;
    previewCanvas.height = height;
    geometryOverlayCanvas.width = width;
    geometryOverlayCanvas.height = height;
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
  scaleLineElement.style.width = `${distance / state.viewHeight * previewCanvas.clientHeight}px`;
  scaleLabelElement.textContent = formatDistance(distance);
}

function formatDistance(distanceMetres: number): string {
  if (distanceMetres >= 1000) return `${(distanceMetres / 1000).toFixed(0)} km`;
  if (distanceMetres >= 1) return `${distanceMetres.toFixed(0)} m`;
  if (distanceMetres >= 0.01) return `${(distanceMetres * 100).toFixed(0)} cm`;
  if (distanceMetres >= 0.001) return `${(distanceMetres * 1000).toFixed(0)} mm`;
  if (distanceMetres >= 0.000001) return `${(distanceMetres * 1000000).toFixed(0)} µm`;
  return `${(distanceMetres * 1000000000).toFixed(0)} nm`;
}

let previous = performance.now();
function frame(now: number): void {
  resize();
  updateScaleBar();
  const elapsed = Math.min((now - previous) / 1000, 0.1);
  previous = now;
  advanceTransitions(elapsed);
  if (!state.paused) {
    const simulationElapsed = elapsed * state.cloudTimeScale;
    state.time += simulationElapsed;
    state.pressureTimeSeconds += elapsed * state.pressureTimeScale;
    state.currentTimeSeconds += elapsed * state.currentTimeScale;
    state.worldTimeSeconds += elapsed * state.worldTimeScale;
  }
  sampleWorldCenterDiagnostics();
  updateDerivedOutput();
  updateGeometryMeasurements();
  writeUniforms();
  const encoder = device.createCommandEncoder();
  const pass = encoder.beginRenderPass({
    colorAttachments: [{
      view: context.getCurrentTexture().createView(),
      clearValue: { r: 0.02, g: 0.08, b: 0.1, a: 1 },
      loadOp: "clear",
      storeOp: "store",
    }],
  });
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, sceneBindGroup);
  pass.setBindGroup(1, playgroundBindGroup);
  pass.setVertexBuffer(0, vertexBuffer);
  pass.draw(6);
  pass.end();
  device.queue.submit([encoder.finish()]);
  drawGeometryOverlay();
  statusElement.textContent = state.paused
    ? `Paused at ${format(state.time, 1)} s`
    : `${previewCanvas.width} × ${previewCanvas.height} · procedural world · ${state.cloudTimeScale}× clouds · ${state.pressureTimeScale}× pressure · ${state.currentTimeScale}× currents · ${state.worldTimeScale}× world clock`;
  requestAnimationFrame(frame);
}

requestAnimationFrame(frame);
