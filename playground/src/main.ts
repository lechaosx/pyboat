import shaderSource from "../../crates/browser/src/scene.wgsl?raw";
import scenarioDefinitions from "../scenarios.json";
import "./style.css";

type Domain = "weather" | "landscape" | "water";
type ControlKind = "WORLD" | "MODEL" | "SHADER" | "DEBUG";

type State = {
  seed: string;
  cameraX: number;
  cameraY: number;
  viewHeight: number;
  time: number;
  timeRate: number;
  dayRate: number;
  paused: boolean;
  pressureHpa: number;
  pressureGradient: number;
  pressureGradientDirection: number;
  synopticPressureGradient: number;
  synopticPressureGradientDirection: number;
  lowTemperature: number;
  seasonalTemperatureRange: number;
  diurnalTemperatureRange: number;
  upperTemperature: number;
  temperatureGradient: number;
  temperatureGradientDirection: number;
  waterVapor: number;
  latitude: number;
  dayOfYear: number;
  timeOfDay: number;
  moonPhase: number;
  celestialLightingEnabled: number;
  detailCharacter: number;
  detailBlobiness: number;
  detailContrast: number;
  detailEdgeSoftness: number;
  maskInfluence: number;
  maskScale: number;
  maskElongation: number;
  maxVisibleCoverage: number;
  motionInMask: number;
  opticalDepth: number;
  opacity: number;
  darkness: number;
  relief: number;
  terrainScale: number;
  elevationScale: number;
  cliffSlope: number;
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

type NumericStateKey = Exclude<keyof State, "seed" | "paused">;
type RangeControl = {
  type: "range";
  key: NumericStateKey;
  label: string;
  kind: ControlKind;
  min: number;
  max: number;
  step: number;
};
type ToggleControl = {
  type: "toggle";
  key: NumericStateKey;
  label: string;
  kind: ControlKind;
};
type Control = RangeControl | ToggleControl;
type ControlGroup = {
  domain: Domain;
  category: string;
  layer: string;
  controls: Control[];
};

type WeatherScenario = Pick<State,
  | "pressureHpa" | "pressureGradient" | "pressureGradientDirection"
  | "synopticPressureGradient" | "synopticPressureGradientDirection"
  | "lowTemperature" | "upperTemperature"
  | "seasonalTemperatureRange" | "diurnalTemperatureRange"
  | "temperatureGradient" | "temperatureGradientDirection" | "waterVapor"
  | "latitude" | "dayOfYear" | "timeOfDay" | "moonPhase"
>;
type WeatherScenarioDefinition = WeatherScenario & {
  slug: string;
  label: string;
  description: string;
};

const defaults: State = {
  seed: "0x5eed",
  cameraX: 0,
  cameraY: 0,
  viewHeight: 480,
  time: 37,
  timeRate: 1,
  dayRate: 0,
  paused: false,
  pressureHpa: 1008,
  pressureGradient: 1.0,
  pressureGradientDirection: -81,
  synopticPressureGradient: 0.85,
  synopticPressureGradientDirection: -81,
  lowTemperature: 14,
  seasonalTemperatureRange: 12,
  diurnalTemperatureRange: 4,
  upperTemperature: -14,
  temperatureGradient: 0.12,
  temperatureGradientDirection: -65,
  waterVapor: 13,
  latitude: 45,
  dayOfYear: 172,
  timeOfDay: 14,
  moonPhase: 0.25,
  celestialLightingEnabled: 1,
  detailCharacter: 0.35,
  detailBlobiness: 0.25,
  detailContrast: 1.15,
  detailEdgeSoftness: 0.06,
  maskInfluence: 0.85,
  maskScale: 1,
  maskElongation: 2.4,
  maxVisibleCoverage: 0.5,
  motionInMask: 0.65,
  opticalDepth: 1,
  opacity: 1,
  darkness: 1,
  relief: 1,
  terrainScale: 1,
  elevationScale: 3,
  cliffSlope: 0.08,
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
  kind: ControlKind,
  min: number,
  max: number,
  step: number,
): RangeControl => ({ type: "range", key, label, kind, min, max, step });
const toggle = (key: NumericStateKey, label: string, kind: ControlKind): ToggleControl => (
  { type: "toggle", key, label, kind }
);

const groups: ControlGroup[] = [
  {
    domain: "weather", category: "Conditions", layer: "Pressure", controls: [
      range("pressureHpa", "Pressure · hPa", "WORLD", 970, 1040, 0.5),
      range("pressureGradient", "Gradient · hPa/km", "WORLD", 0, 3, 0.02),
      range("pressureGradientDirection", "Gradient direction", "WORLD", -180, 180, 1),
      range("synopticPressureGradient", "Broad gradient · hPa/km", "WORLD", 0, 3, 0.02),
      range("synopticPressureGradientDirection", "Broad direction", "WORLD", -180, 180, 1),
    ],
  },
  {
    domain: "weather", category: "Conditions", layer: "Thermodynamics", controls: [
      range("lowTemperature", "Annual mean low temperature · °C", "WORLD", -35, 45, 0.5),
      range("seasonalTemperatureRange", "Seasonal range · °C", "WORLD", 0, 40, 0.5),
      range("diurnalTemperatureRange", "Daily range · °C", "WORLD", 0, 20, 0.5),
      range("upperTemperature", "Upper temperature · °C", "WORLD", -80, 10, 0.5),
      range("temperatureGradient", "Horizontal ΔT · °C/km", "WORLD", 0, 1, 0.01),
      range("temperatureGradientDirection", "ΔT direction", "WORLD", -180, 180, 1),
      range("waterVapor", "Water vapour · g/kg", "WORLD", 0, 25, 0.1),
    ],
  },
  {
    domain: "weather", category: "Conditions", layer: "Time and sky", controls: [
      toggle("celestialLightingEnabled", "Celestial lighting", "DEBUG"),
      range("latitude", "Latitude · °", "WORLD", -80, 80, 1),
      range("dayOfYear", "Day of year", "WORLD", 1, 365, 1),
      range("timeOfDay", "Local solar time · h", "WORLD", 0, 23.99, 0.05),
      range("moonPhase", "Moon phase · 0 new, 0.5 full", "WORLD", 0, 1, 0.01),
    ],
  },
  {
    domain: "weather", category: "Layers", layer: "Low cloud · visibility", controls: [
      toggle("lowCloudEnabled", "Low-cloud layer", "DEBUG"),
      toggle("lowCloudVisible", "Visible body", "DEBUG"),
      toggle("lowCloudShadow", "Terrain shadow", "DEBUG"),
    ],
  },
  {
    domain: "weather", category: "Layers", layer: "Low cloud · detail", controls: [
      range("detailCharacter", "Billowy character", "SHADER", 0, 1, 0.05),
      range("detailBlobiness", "Detail blobiness", "SHADER", 0, 1, 0.05),
      range("detailContrast", "Detail contrast", "SHADER", 0.25, 2.5, 0.05),
      range("detailEdgeSoftness", "Body edge", "SHADER", 0.005, 0.25, 0.005),
    ],
  },
  {
    domain: "weather", category: "Layers", layer: "Low cloud · formation", controls: [
      range("maskInfluence", "Mask influence", "SHADER", 0, 1, 0.05),
      range("maskScale", "Mask scale", "SHADER", 0.25, 4, 0.05),
      range("maskElongation", "Mask elongation", "SHADER", 1, 16, 0.25),
      range("maxVisibleCoverage", "Maximum coverage", "SHADER", 0, 1, 0.01),
      range("motionInMask", "Motion in mask", "SHADER", 0, 1, 0.05),
    ],
  },
  {
    domain: "weather", category: "Layers", layer: "Low cloud · material", controls: [
      range("opticalDepth", "Optical depth", "SHADER", 0, 3, 0.05),
      range("opacity", "Opacity", "SHADER", 0, 1.5, 0.05),
      range("darkness", "Rain darkness", "SHADER", 0, 2, 0.05),
      range("relief", "Height relief", "SHADER", 0, 2, 0.05),
    ],
  },
  {
    domain: "weather", category: "Layers", layer: "Upper cloud · visibility", controls: [
      toggle("upperCloudEnabled", "Upper-cloud layer", "DEBUG"),
      toggle("upperCloudShadow", "Terrain shadow", "DEBUG"),
      toggle("upperShadowOnLowCloud", "Shadow on low cloud", "DEBUG"),
    ],
  },
  {
    domain: "weather", category: "Wind model", layer: "Derived flow", controls: [
      toggle("pressureCirculationEnabled", "Pressure circulation", "DEBUG"),
      range("pressureWindScale", "Pressure response", "MODEL", 0, 2, 0.05),
      toggle("crossIsobarEnabled", "Cross-isobar flow", "DEBUG"),
      range("crossIsobarScale", "Cross-isobar response", "MODEL", 0, 2, 0.05),
      toggle("thermalWindEnabled", "Thermal-wind shear", "DEBUG"),
      range("thermalWindScale", "Thermal response", "MODEL", 0, 2, 0.05),
      toggle("shelterEnabled", "Terrain shelter", "DEBUG"),
      range("shelterStrength", "Shelter strength", "MODEL", 0, 1, 0.05),
    ],
  },
  {
    domain: "landscape", category: "Structure", layer: "Terrain shape", controls: [
      range("terrainScale", "Horizontal scale", "SHADER", 0.25, 4, 0.05),
      range("elevationScale", "Elevation scale", "SHADER", 0.1, 3, 0.05),
      range("cliffSlope", "Cliff slope", "SHADER", 0.01, 0.3, 0.005),
    ],
  },
  {
    domain: "landscape", category: "Layers", layer: "Composition", controls: [
      toggle("landscapeCompositionEnabled", "Landscape composition", "DEBUG"),
      toggle("coastalReliefVisible", "Coastal reef relief", "DEBUG"),
      toggle("vegetationVisible", "Vegetation", "DEBUG"),
      toggle("surfaceTextureVisible", "Surface texture", "DEBUG"),
      toggle("landscapeLightingVisible", "Terrain lighting", "DEBUG"),
    ],
  },
];

const lowCloudDependentKeys = new Set<NumericStateKey>([
  "detailCharacter", "detailBlobiness", "detailContrast", "detailEdgeSoftness",
  "maskInfluence", "maskScale", "maskElongation", "maxVisibleCoverage", "motionInMask",
  "opticalDepth", "opacity", "darkness", "relief",
  "lowCloudVisible", "lowCloudShadow",
]);
const landscapeCompositionKeys = new Set<NumericStateKey>([
  "coastalReliefVisible", "vegetationVisible", "surfaceTextureVisible",
  "landscapeLightingVisible",
]);
const subtoggleKeys = new Set<NumericStateKey>([
  "lowCloudVisible", "lowCloudShadow", "upperCloudShadow", "upperShadowOnLowCloud",
  ...landscapeCompositionKeys,
]);
const subrangeKeys = new Set<NumericStateKey>([
  "latitude", "dayOfYear", "timeOfDay", "moonPhase",
  "pressureWindScale", "crossIsobarScale", "thermalWindScale", "shelterStrength",
]);

const debugOptions: Record<Domain, Array<[number, string]>> = {
  weather: [
    [0, "Final composition"], [1, "Low detail texture"], [2, "Low detail gate"],
    [3, "Formation mask"], [4, "Formation gate"], [5, "Low density"],
    [6, "Low optical depth"], [7, "Low normals"], [8, "Upper optical depth"],
    [9, "Projected low shadow"], [10, "Projected upper shadow"], [11, "Combined transmittance"],
    [12, "Low height field"],
    [16, "Surface wind"], [17, "Upper wind"], [18, "Vertical wind shear"],
    [19, "Pressure gradient"], [20, "Temperature gradient"], [21, "Terrain shelter"],
  ],
  landscape: [
    [0, "Final composition"], [32, "Elevation"], [33, "Slope"], [34, "Normals"],
    [35, "Island clusters"], [36, "Island detail"], [37, "Coastal reef field"],
    [38, "Vegetation field"], [39, "Beach geology"],
  ],
  water: [[0, "Final composition"]],
};

const weatherScenarios = scenarioDefinitions as WeatherScenarioDefinition[];

const categoryPresets: Record<string, Record<string, Partial<State>>> = {
  "weather:Layers": {
    Production: {
      detailCharacter: defaults.detailCharacter,
      detailBlobiness: defaults.detailBlobiness,
      detailContrast: defaults.detailContrast,
      detailEdgeSoftness: defaults.detailEdgeSoftness,
      maskInfluence: defaults.maskInfluence,
      maskScale: defaults.maskScale,
      maskElongation: defaults.maskElongation,
      maxVisibleCoverage: defaults.maxVisibleCoverage,
      motionInMask: defaults.motionInMask,
      opticalDepth: defaults.opticalDepth,
      opacity: defaults.opacity,
      darkness: defaults.darkness,
      relief: defaults.relief,
      lowCloudEnabled: defaults.lowCloudEnabled,
      lowCloudVisible: defaults.lowCloudVisible,
      lowCloudShadow: defaults.lowCloudShadow,
      upperCloudEnabled: defaults.upperCloudEnabled,
      upperCloudShadow: defaults.upperCloudShadow,
      upperShadowOnLowCloud: defaults.upperShadowOnLowCloud,
    },
  },
  "weather:Wind model": {
    Production: {
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
  "landscape:Structure": {
    Production: {
      terrainScale: defaults.terrainScale,
      elevationScale: defaults.elevationScale,
      cliffSlope: defaults.cliffSlope,
    },
  },
  "landscape:Layers": {
    Production: {
      landscapeCompositionEnabled: defaults.landscapeCompositionEnabled,
      coastalReliefVisible: defaults.coastalReliefVisible,
      vegetationVisible: defaults.vegetationVisible,
      surfaceTextureVisible: defaults.surfaceTextureVisible,
      landscapeLightingVisible: defaults.landscapeLightingVisible,
    },
  },
};

const state: State = { ...defaults };
let activeDomain: Domain = "weather";

const canvas = document.querySelector<HTMLCanvasElement>("#preview");
const controls = document.querySelector<HTMLElement>("#controls");
const domains = document.querySelector<HTMLElement>("#domains");
const presets = document.querySelector<HTMLElement>("#presets");
const worldToolbar = document.querySelector<HTMLElement>("#world-toolbar");
const status = document.querySelector<HTMLElement>("#status");
const scaleLine = document.querySelector<HTMLElement>("#scale-line");
const scaleLabel = document.querySelector<HTMLElement>("#scale-label");
if (!canvas || !controls || !domains || !presets || !worldToolbar || !status || !scaleLine || !scaleLabel) {
  throw new Error("Procedural lab page is incomplete");
}
const previewCanvas = canvas;
const controlsElement = controls;
const domainsElement = domains;
const presetsElement = presets;
const worldToolbarElement = worldToolbar;
const statusElement = status;
const scaleLineElement = scaleLine;
const scaleLabelElement = scaleLabel;

type ValueTransition = { elapsed: number; from: number; to: number };
type PresetTransition = { elapsed: number; from: Partial<State>; to: Partial<State> };
const valueTransitions = new Map<NumericStateKey, ValueTransition>();
let presetTransition: PresetTransition | undefined;
const inputByKey = new Map<NumericStateKey, HTMLInputElement>();
const outputByKey = new Map<NumericStateKey, HTMLOutputElement>();
const rangeByKey = new Map<NumericStateKey, RangeControl>();
const resetByKey = new Map<NumericStateKey, HTMLButtonElement>();
const controlElementByKey = new Map<NumericStateKey, HTMLElement>();
let derivedOutputElement: HTMLElement | undefined;

function format(value: number, step: number): string {
  return value.toFixed(step < 0.01 ? 3 : step < 1 ? 2 : 0);
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
  setControlsEnabled(["moonPhase"], state.celestialLightingEnabled > 0.5);
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
    presetTransition = undefined;
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
  heading.append(document.createTextNode(control.label), tag(control.kind));
  const input = document.createElement("input");
  input.type = "range";
  input.id = `control-${control.key}`;
  heading.htmlFor = input.id;
  input.min = String(control.min);
  input.max = String(control.max);
  input.step = String(control.step);
  input.value = String(state[control.key]);
  const output = document.createElement("output");
  output.value = format(state[control.key], control.step);
  input.addEventListener("input", () => {
    presetTransition = undefined;
    valueTransitions.delete(control.key);
    state[control.key] = Number(input.value);
    syncControl(control.key);
  });
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
    presetTransition = undefined;
    valueTransitions.delete(control.key);
    state[control.key] = rangeValueAtPointer(control, input, event.clientX);
    syncControl(control.key);
  });
  input.addEventListener("pointerup", (event) => {
    if (event.pointerId !== pointerId) return;
    if (!dragging) {
      presetTransition = undefined;
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
  if (control.kind === "WORLD") {
    container.classList.add("no-reset");
  } else {
    container.append(resetButton(control.key, control.label));
  }
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
  return Math.min(
    control.max,
    Math.max(control.min, Math.round(value / control.step) * control.step),
  );
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
  heading.append(document.createTextNode(control.label), tag(control.kind));
  container.append(input, heading);
  if (control.kind === "WORLD") {
    container.classList.add("no-reset");
  } else {
    container.append(resetButton(control.key, control.label));
  }
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
  for (const category of [...new Set(domainGroups.map((group) => group.category))]) {
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
          presetTransition = undefined;
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
      details.open = true;
      const summary = document.createElement("summary");
      summary.textContent = group.layer;
      details.append(summary);
      for (const control of group.controls) {
        details.append(control.type === "range" ? createRangeControl(control) : createToggleControl(control));
      }
      section.append(details);
    }
    controlsElement.append(section);
  }
  derivedOutputElement = undefined;
  if (activeDomain === "weather") {
    const derivedSection = document.createElement("section");
    derivedSection.className = "control-category derived-output";
    derivedSection.innerHTML = "<h2>Derived atmosphere · read only</h2><div class=\"derived-grid\"></div>";
    derivedOutputElement = derivedSection.querySelector<HTMLElement>(".derived-grid")!;
    controlsElement.append(derivedSection);
  }
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
  if (activeDomain === "water") {
    const empty = document.createElement("section");
    empty.className = "empty-domain";
    empty.innerHTML = "<h2>Water systems</h2><p>Currents, waves and water rendering will live here when their first implementation slice exists.</p>";
    controlsElement.prepend(empty);
  }
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
      renderScenarios();
      renderControls();
    });
    domainsElement.append(button);
  }
}

function renderScenarios(): void {
  presetsElement.replaceChildren();
  if (activeDomain !== "weather") return;
  const label = document.createElement("span");
  label.textContent = "SCENARIOS";
  presetsElement.append(label);
  for (const scenario of weatherScenarios) {
    const { slug: _slug, label, description, ...values } = scenario;
    const button = document.createElement("button");
    button.textContent = label;
    button.title = description;
    button.addEventListener("click", () => {
      valueTransitions.clear();
      const from: Partial<State> = {};
      for (const key of Object.keys(values) as NumericStateKey[]) {
        from[key] = state[key] as never;
      }
      presetTransition = { elapsed: 0, from, to: values };
    });
    presetsElement.append(button);
  }
}

worldToolbarElement.innerHTML = `
  <details open>
    <summary>World and playback</summary>
    <div class="text-control no-reset"><label class="control-name" for="seed">Seed <span class="kind kind-world">WORLD</span></label><input id="seed" type="text" value="${state.seed}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="time-rate">Cloud time rate <span class="kind kind-world">WORLD</span></label><input id="time-rate" type="range" min="0" max="20" step="0.1" value="${state.timeRate}"><output>${format(state.timeRate, 0.1)}</output></div>
    <div class="control range-control no-reset"><label class="control-name" for="day-rate">Day cycle · h/s <span class="kind kind-world">WORLD</span></label><input id="day-rate" type="range" min="0" max="2" step="0.01" value="${state.dayRate}"><output>${format(state.dayRate, 0.01)}</output></div>
    <div class="actions"><button id="pause">Pause</button></div>
  </details>`;
const seedInput = worldToolbarElement.querySelector<HTMLInputElement>("#seed")!;
const timeRateInput = worldToolbarElement.querySelector<HTMLInputElement>("#time-rate")!;
const timeRateOutput = timeRateInput.nextElementSibling as HTMLOutputElement;
const dayRateInput = worldToolbarElement.querySelector<HTMLInputElement>("#day-rate")!;
const dayRateOutput = dayRateInput.nextElementSibling as HTMLOutputElement;
const pauseButton = worldToolbarElement.querySelector<HTMLButtonElement>("#pause")!;
seedInput.addEventListener("input", () => {
  state.seed = seedInput.value;
});
timeRateInput.addEventListener("input", () => {
  state.timeRate = Number(timeRateInput.value);
  timeRateOutput.value = format(state.timeRate, 0.1);
});
dayRateInput.addEventListener("input", () => {
  state.dayRate = Number(dayRateInput.value);
  dayRateOutput.value = format(state.dayRate, 0.01);
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
  ) updateDependentControls();
  const input = inputByKey.get(key);
  if (!input) return;
  if (input.type === "checkbox") {
    input.checked = state[key] > 0.5;
    return;
  }
  input.value = String(state[key]);
  const definition = rangeByKey.get(key);
  const output = outputByKey.get(key);
  if (definition && output) output.value = format(state[key], definition.step);
}

function advanceTransitions(elapsed: number): void {
  if (presetTransition) {
    presetTransition.elapsed += elapsed;
    const progress = Math.min(presetTransition.elapsed / 1.5, 1);
    const amount = progress * progress * (3 - 2 * progress);
    for (const key of Object.keys(presetTransition.to) as NumericStateKey[]) {
      const from = presetTransition.from[key] as number;
      const to = presetTransition.to[key] as number;
      if (key === "timeOfDay") {
        const delta = (to - from + 36) % 24 - 12;
        state[key] = (from + delta * amount + 24) % 24;
      } else {
        state[key] = from + (to - from) * amount;
      }
      syncControl(key);
    }
    if (progress === 1) presetTransition = undefined;
  }
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
renderScenarios();
renderControls();

let dragPointer: number | undefined;
let dragX = 0;
let dragY = 0;
previewCanvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  dragPointer = event.pointerId;
  dragX = event.clientX;
  dragY = event.clientY;
  previewCanvas.setPointerCapture(event.pointerId);
  previewCanvas.classList.add("dragging");
});
previewCanvas.addEventListener("pointermove", (event) => {
  if (event.pointerId !== dragPointer) return;
  const aspect = previewCanvas.clientWidth / previewCanvas.clientHeight;
  state.cameraX -= (event.clientX - dragX) / previewCanvas.clientWidth * state.viewHeight * aspect;
  state.cameraY -= (event.clientY - dragY) / previewCanvas.clientHeight * state.viewHeight;
  dragX = event.clientX;
  dragY = event.clientY;
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
  const pointerX = (event.clientX - rect.left) / rect.width - 0.5;
  const pointerY = (event.clientY - rect.top) / rect.height - 0.5;
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
const sceneBuffer = device.createBuffer({ size: 112, usage: 0x40 | 0x08 });
const playgroundBuffer = device.createBuffer({ size: 144, usage: 0x40 | 0x08 });
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

function directedVector(magnitude: number, degrees: number): [number, number] {
  const radians = degrees * Math.PI / 180;
  return [magnitude * Math.cos(radians), magnitude * Math.sin(radians)];
}

function currentLowTemperature(): number {
  const hemisphere = state.latitude < 0 ? -1 : 1;
  const seasonalPhase = 2 * Math.PI * (state.dayOfYear - 205) / 365;
  const diurnalPhase = 2 * Math.PI * (state.timeOfDay - 15) / 24;
  return state.lowTemperature
    + 0.5 * state.seasonalTemperatureRange * hemisphere * Math.cos(seasonalPhase)
    + 0.5 * state.diurnalTemperatureRange * Math.cos(diurnalPhase);
}

type Light = { direction: [number, number, number]; intensity: number };
type CelestialLighting = { sun: Light; moon: Light; ambient: number };

function celestialDirection(declination: number, hourAngle: number): [number, number, number] {
  const latitude = state.latitude * Math.PI / 180;
  return [
    -Math.cos(declination) * Math.sin(hourAngle),
    Math.sin(declination) * Math.cos(latitude)
      - Math.cos(declination) * Math.sin(latitude) * Math.cos(hourAngle),
    Math.sin(latitude) * Math.sin(declination)
      + Math.cos(latitude) * Math.cos(declination) * Math.cos(hourAngle),
  ];
}

function celestialLighting(): CelestialLighting {
  const obliquity = 23.44 * Math.PI / 180;
  const solarLongitude = 2 * Math.PI * (state.dayOfYear - 80) / 365;
  const solarDeclination = Math.asin(Math.sin(obliquity) * Math.sin(solarLongitude));
  const solarHourAngle = 2 * Math.PI * (state.timeOfDay - 12) / 24;
  const sunDirection = celestialDirection(solarDeclination, solarHourAngle);
  const sunElevation = Math.sqrt(Math.max(sunDirection[2], 0));
  const sunHorizon = smoothstepValue(0, 0.08, sunDirection[2]);
  const sunIntensity = sunHorizon * (0.35 + 0.65 * sunElevation);
  const phaseAngle = 2 * Math.PI * ((state.moonPhase % 1 + 1) % 1);
  const lunarDeclination = Math.asin(
    Math.sin(obliquity) * Math.sin(solarLongitude + phaseAngle),
  );
  const moonDirection = celestialDirection(lunarDeclination, solarHourAngle + phaseAngle);
  const illuminatedFraction = 0.5 - 0.5 * Math.cos(phaseAngle);
  const moonIntensity = 0.12 * illuminatedFraction
    * smoothstepValue(0, 0.08, moonDirection[2]);
  const twilight = smoothstepValue(-0.18, 0.12, sunDirection[2]);
  const ambient = Math.min(
    1,
    0.035 + twilight * (0.45 + 0.515 * sunElevation) + 0.85 * moonIntensity,
  );
  return {
    sun: { direction: sunDirection, intensity: sunIntensity },
    moon: { direction: moonDirection, intensity: moonIntensity },
    ambient,
  };
}

function smoothstepValue(lower: number, upper: number, value: number): number {
  const amount = Math.min(1, Math.max(0, (value - lower) / (upper - lower)));
  return amount * amount * (3 - 2 * amount);
}

function atmosphericOutputs(): Array<[string, string]> {
  const pressureGradient = directedVector(state.pressureGradient / 1000, state.pressureGradientDirection);
  const synopticGradient = directedVector(
    state.synopticPressureGradient / 1000,
    state.synopticPressureGradientDirection,
  );
  const temperatureGradient = directedVector(
    state.temperatureGradient / 1000,
    state.temperatureGradientDirection,
  );
  const pressureScale = 6000 * state.pressureCirculationEnabled * state.pressureWindScale;
  const cross = 0.12 * state.crossIsobarEnabled * state.crossIsobarScale;
  const pressureWind = (gradient: [number, number], scale = 1): [number, number] => [
    pressureScale * scale * (-gradient[1] - cross * gradient[0]) / Math.sqrt(1 + cross * cross),
    pressureScale * scale * (gradient[0] - cross * gradient[1]) / Math.sqrt(1 + cross * cross),
  ];
  const surface = pressureWind(pressureGradient);
  const upperPressure = pressureWind(synopticGradient, 1.12);
  const thermalScale = 12000 * state.thermalWindEnabled * state.thermalWindScale;
  const upper: [number, number] = [
    upperPressure[0] - thermalScale * temperatureGradient[1],
    upperPressure[1] + thermalScale * temperatureGradient[0],
  ];
  const shear: [number, number] = [upper[0] - surface[0], upper[1] - surface[1]];
  const speed = (value: [number, number]) => Math.hypot(...value);
  const direction = (value: [number, number]) => Math.atan2(value[1], value[0]) * 180 / Math.PI;
  const lowTemperature = currentLowTemperature();
  const humidity = Math.min(1.5, Math.max(
    0,
    state.waterVapor / (3.5 * Math.exp(0.055 * (lowTemperature + 5))),
  ));
  const instability = smoothstepValue(28, 48, lowTemperature - state.upperTemperature);
  const pressureLift = 1 - smoothstepValue(990, 1012, state.pressureHpa);
  const gradientLift = smoothstepValue(0.0002, 0.0015, Math.hypot(...pressureGradient));
  const frontalLift = smoothstepValue(0.00005, 0.0005, Math.hypot(...temperatureGradient));
  const lift = Math.min(1, Math.max(0, 0.45 * pressureLift + 0.35 * gradientLift + 0.2 * frontalLift));
  const condensation = smoothstepValue(0.45, 0.95, humidity);
  const lowCloud = smoothstepValue(
    0.5,
    0.98,
    0.68 * condensation + 0.22 * lift + 0.1 * instability * condensation,
  );
  const upperCloud = smoothstepValue(
    0.68,
    0.9,
    0.55 * condensation + 0.3 * lift + 0.15 * frontalLift,
  );
  const rain = smoothstepValue(0.55, 0.9, lowCloud * (0.45 + 0.2 * instability + 0.45 * lift));
  const celestial = celestialLighting();
  const angle = (direction: [number, number, number]): string => {
    const elevation = Math.asin(direction[2]) * 180 / Math.PI;
    const azimuth = Math.atan2(direction[0], direction[1]) * 180 / Math.PI;
    return `${elevation.toFixed(1)}° elevation · ${azimuth.toFixed(0)}° azimuth`;
  };
  const overcast = Math.min(1, 0.42 * lowCloud + 0.58 * upperCloud + 0.3 * rain);
  return [
    ["Current low temperature", `${lowTemperature.toFixed(1)} °C`],
    ["Sun", `${angle(celestial.sun.direction)} · ${celestial.sun.intensity.toFixed(2)}`],
    ["Moon", `${angle(celestial.moon.direction)} · ${celestial.moon.intensity.toFixed(2)}`],
    ["Ambient light", (celestial.ambient * (1 - 0.42 * overcast)).toFixed(2)],
    ["Surface wind", `${speed(surface).toFixed(1)} m/s · ${direction(surface).toFixed(0)}°`],
    ["Upper wind", `${speed(upper).toFixed(1)} m/s · ${direction(upper).toFixed(0)}°`],
    ["Vertical shear", `${speed(shear).toFixed(1)} m/s · ${direction(shear).toFixed(0)}°`],
    ["Relative humidity", `${(humidity * 100).toFixed(0)} %`],
    ["Instability", instability.toFixed(2)],
    ["Lift", lift.toFixed(2)],
    ["Low cloud", lowCloud.toFixed(2)],
    ["Upper cloud", upperCloud.toFixed(2)],
    ["Rain", rain.toFixed(2)],
  ];
}

function updateDerivedOutput(): void {
  if (!derivedOutputElement) return;
  derivedOutputElement.replaceChildren(...atmosphericOutputs().flatMap(([label, value]) => {
    const name = document.createElement("span");
    name.textContent = label;
    const output = document.createElement("output");
    output.value = value;
    return [name, output];
  }));
}

function writeUniforms(): void {
  const sceneData = new ArrayBuffer(112);
  const sceneFloats = new Float32Array(sceneData);
  const sceneUints = new Uint32Array(sceneData);
  const aspect = previewCanvas.width / previewCanvas.height;
  const pressureGradient = directedVector(state.pressureGradient / 1000, state.pressureGradientDirection);
  const synopticGradient = directedVector(
    state.synopticPressureGradient / 1000,
    state.synopticPressureGradientDirection,
  );
  const temperatureGradient = directedVector(
    state.temperatureGradient / 1000,
    state.temperatureGradientDirection,
  );
  sceneFloats.set([state.cameraX, state.cameraY, state.viewHeight * aspect, state.viewHeight], 0);
  sceneFloats.set([state.pressureHpa, ...pressureGradient, state.seasonalTemperatureRange], 4);
  sceneFloats.set([...synopticGradient, state.lowTemperature, state.upperTemperature], 8);
  sceneFloats.set([state.waterVapor, ...temperatureGradient, state.diurnalTemperatureRange], 12);
  sceneFloats.set([state.latitude, state.dayOfYear, state.timeOfDay, state.moonPhase], 16);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  sceneUints.set([seedLow, seedHigh, seedLow ^ 0x6174, seedHigh ^ 0x7079626f], 20);
  sceneFloats[24] = state.time;
  device.queue.writeBuffer(sceneBuffer, 0, sceneData);

  const tuningData = new ArrayBuffer(144);
  const tuningFloats = new Float32Array(tuningData);
  const tuningUints = new Uint32Array(tuningData);
  tuningFloats.set([
    state.detailCharacter, state.detailBlobiness, state.detailContrast, state.detailEdgeSoftness,
  ], 0);
  tuningFloats.set([
    state.maskInfluence, state.maskScale, state.maskElongation, state.maxVisibleCoverage,
  ], 4);
  tuningFloats.set([state.opticalDepth, state.opacity, state.darkness, state.relief], 8);
  tuningFloats.set([state.motionInMask, 0, 0, 0], 12);
  tuningFloats.set([state.terrainScale, state.elevationScale, state.cliffSlope, 0], 16);
  tuningFloats.set([
    state.lowCloudEnabled * state.lowCloudVisible,
    state.lowCloudEnabled * state.lowCloudShadow,
    state.upperCloudEnabled * state.upperCloudShadow,
    state.lowCloudEnabled * state.upperCloudEnabled * state.upperShadowOnLowCloud,
  ], 20);
  tuningFloats.set([
    state.landscapeCompositionEnabled * state.coastalReliefVisible,
    state.landscapeCompositionEnabled * state.vegetationVisible,
    state.landscapeCompositionEnabled * state.surfaceTextureVisible,
    state.landscapeCompositionEnabled * state.landscapeLightingVisible,
  ], 24);
  tuningFloats.set([
    state.pressureCirculationEnabled * state.pressureWindScale,
    state.crossIsobarEnabled * state.crossIsobarScale,
    state.thermalWindEnabled * state.thermalWindScale,
    state.shelterEnabled * state.shelterStrength,
  ], 28);
  tuningUints.set([state.debugView, state.celestialLightingEnabled, 0, 0], 32);
  device.queue.writeBuffer(playgroundBuffer, 0, tuningData);
}

function resize(): void {
  const scale = Math.min(devicePixelRatio, 2);
  const width = Math.max(1, Math.floor(previewCanvas.clientWidth * scale));
  const height = Math.max(1, Math.floor(previewCanvas.clientHeight * scale));
  if (previewCanvas.width !== width || previewCanvas.height !== height) {
    previewCanvas.width = width;
    previewCanvas.height = height;
  }
}

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
    const simulationElapsed = elapsed * state.timeRate;
    state.time += simulationElapsed;
    state.timeOfDay = (state.timeOfDay + elapsed * state.dayRate) % 24;
    syncControl("timeOfDay");
  }
  updateDerivedOutput();
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
  statusElement.textContent = state.paused
    ? `Paused at ${format(state.time, 1)} s`
    : `${previewCanvas.width} × ${previewCanvas.height} · ${format(state.time, 1)} s · ${state.timeOfDay.toFixed(2)} h`;
  requestAnimationFrame(frame);
}

requestAnimationFrame(frame);
