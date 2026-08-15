import shaderSource from "../../crates/browser/src/scene.wgsl?raw";
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
  paused: boolean;
  cover: number;
  rain: number;
  moisture: number;
  instability: number;
  upperCloud: number;
  surfaceWindSpeed: number;
  surfaceWindDirection: number;
  synopticWindSpeed: number;
  synopticWindDirection: number;
  sunAzimuth: number;
  sunElevation: number;
  sunIntensity: number;
  sunEnabled: number;
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
  synopticWeight: number;
  synopticComponentEnabled: number;
  regionalWeight: number;
  regionalComponentEnabled: number;
  gustWeight: number;
  gustEnabled: number;
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
  | "cover" | "rain" | "moisture" | "instability" | "upperCloud"
  | "surfaceWindSpeed" | "surfaceWindDirection"
  | "synopticWindSpeed" | "synopticWindDirection"
  | "sunAzimuth" | "sunElevation" | "sunIntensity"
>;

const defaults: State = {
  seed: "0x5eed",
  cameraX: 0,
  cameraY: 0,
  viewHeight: 480,
  time: 37,
  timeRate: 1,
  paused: false,
  cover: 0.48,
  rain: 0,
  moisture: 0.48,
  instability: 0.82,
  upperCloud: 0.04,
  surfaceWindSpeed: 6.5,
  surfaceWindDirection: 9,
  synopticWindSpeed: 7.1,
  synopticWindDirection: 9,
  sunAzimuth: -126,
  sunElevation: 47,
  sunIntensity: 1,
  sunEnabled: 1,
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
  synopticWeight: 1,
  synopticComponentEnabled: 1,
  regionalWeight: 1,
  regionalComponentEnabled: 1,
  gustWeight: 1,
  gustEnabled: 1,
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
    domain: "weather", category: "Conditions", layer: "Atmosphere", controls: [
      range("cover", "Low-cloud cover", "WORLD", 0, 1, 0.01),
      range("upperCloud", "Upper-cloud cover", "WORLD", 0, 1, 0.01),
      range("moisture", "Moisture", "WORLD", 0, 1, 0.01),
      range("instability", "Instability", "WORLD", 0, 1, 0.01),
      range("rain", "Rain", "WORLD", 0, 1, 0.01),
    ],
  },
  {
    domain: "weather", category: "Conditions", layer: "Wind", controls: [
      range("surfaceWindSpeed", "Surface speed", "WORLD", 0, 30, 0.1),
      range("surfaceWindDirection", "Surface direction", "WORLD", -180, 180, 1),
      range("synopticWindSpeed", "Synoptic speed", "WORLD", 0, 40, 0.1),
      range("synopticWindDirection", "Synoptic direction", "WORLD", -180, 180, 1),
    ],
  },
  {
    domain: "weather", category: "Conditions", layer: "Sun", controls: [
      toggle("sunEnabled", "Sun lighting", "DEBUG"),
      range("sunAzimuth", "Azimuth", "WORLD", -180, 180, 1),
      range("sunElevation", "Elevation", "WORLD", -10, 90, 1),
      range("sunIntensity", "Intensity", "WORLD", 0, 1.5, 0.05),
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
    domain: "weather", category: "Wind model", layer: "Approximate components", controls: [
      toggle("synopticComponentEnabled", "Synoptic component", "DEBUG"),
      range("synopticWeight", "Synoptic weight", "MODEL", 0, 2, 0.05),
      toggle("regionalComponentEnabled", "Regional component", "DEBUG"),
      range("regionalWeight", "Regional weight", "MODEL", 0, 3, 0.05),
      toggle("gustEnabled", "Gust component", "DEBUG"),
      range("gustWeight", "Gust weight", "MODEL", 0, 3, 0.05),
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
  "sunAzimuth", "sunElevation", "sunIntensity",
  "synopticWeight", "regionalWeight", "gustWeight", "shelterStrength",
]);

const debugOptions: Record<Domain, Array<[number, string]>> = {
  weather: [
    [0, "Final composition"], [1, "Low detail texture"], [2, "Low detail gate"],
    [3, "Formation mask"], [4, "Formation gate"], [5, "Low density"],
    [6, "Low optical depth"], [7, "Low normals"], [8, "Upper optical depth"],
    [9, "Projected low shadow"], [10, "Projected upper shadow"], [11, "Combined transmittance"],
    [12, "Low height field"],
    [16, "Surface wind"], [17, "Synoptic wind"], [18, "Regional residual"],
    [19, "Approximate gusts"], [20, "Approximate combined wind"], [21, "Approximate shelter"],
  ],
  landscape: [
    [0, "Final composition"], [32, "Elevation"], [33, "Slope"], [34, "Normals"],
    [35, "Island clusters"], [36, "Island detail"], [37, "Coastal reef field"],
    [38, "Vegetation field"], [39, "Beach geology"],
  ],
  water: [[0, "Final composition"]],
};

const weatherScenarios: Record<string, WeatherScenario> = {
  Clear: {
    cover: 0.05, upperCloud: 0.02, moisture: 0.25, instability: 0.20, rain: 0,
    surfaceWindSpeed: 3.5, surfaceWindDirection: 15,
    synopticWindSpeed: 4.2, synopticWindDirection: 12,
    sunAzimuth: -110, sunElevation: 58, sunIntensity: 1.05,
  },
  Fair: {
    cover: 0.38, upperCloud: 0.08, moisture: 0.45, instability: 0.55, rain: 0,
    surfaceWindSpeed: 6.5, surfaceWindDirection: 9,
    synopticWindSpeed: 7.1, synopticWindDirection: 9,
    sunAzimuth: -126, sunElevation: 47, sunIntensity: 1,
  },
  Cloudy: {
    cover: 0.72, upperCloud: 0.45, moisture: 0.76, instability: 0.48, rain: 0,
    surfaceWindSpeed: 8, surfaceWindDirection: -15,
    synopticWindSpeed: 11, synopticWindDirection: -8,
    sunAzimuth: -140, sunElevation: 28, sunIntensity: 0.72,
  },
  Rain: {
    cover: 0.72, upperCloud: 0.68, moisture: 0.92, instability: 0.65, rain: 0.62,
    surfaceWindSpeed: 11, surfaceWindDirection: -24,
    synopticWindSpeed: 15, synopticWindDirection: -17,
    sunAzimuth: -150, sunElevation: 22, sunIntensity: 0.46,
  },
  Storm: {
    cover: 0.80, upperCloud: 0.92, moisture: 0.98, instability: 0.96, rain: 0.92,
    surfaceWindSpeed: 19, surfaceWindDirection: -38,
    synopticWindSpeed: 25, synopticWindDirection: -25,
    sunAzimuth: -165, sunElevation: 16, sunIntensity: 0.28,
  },
  Shear: {
    cover: 0.58, upperCloud: 0.72, moisture: 0.72, instability: 0.78, rain: 0.10,
    surfaceWindSpeed: 6, surfaceWindDirection: -30,
    synopticWindSpeed: 18, synopticWindDirection: 35,
    sunAzimuth: -118, sunElevation: 34, sunIntensity: 0.82,
  },
};

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
      synopticComponentEnabled: defaults.synopticComponentEnabled,
      synopticWeight: defaults.synopticWeight,
      regionalComponentEnabled: defaults.regionalComponentEnabled,
      regionalWeight: defaults.regionalWeight,
      gustEnabled: defaults.gustEnabled,
      gustWeight: defaults.gustWeight,
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
let lowOffset: [number, number] = [0, 0];
let upperOffset: [number, number] = [0, 0];

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
  setControlsEnabled(
    ["sunAzimuth", "sunElevation", "sunIntensity"],
    state.sunEnabled > 0.5,
  );
  setControlsEnabled(["synopticWeight"], state.synopticComponentEnabled > 0.5);
  setControlsEnabled(["regionalWeight"], state.regionalComponentEnabled > 0.5);
  setControlsEnabled(["gustWeight"], state.gustEnabled > 0.5);
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
  for (const [name, values] of Object.entries(weatherScenarios)) {
    const button = document.createElement("button");
    button.textContent = name;
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
    <div class="control range-control no-reset"><label class="control-name" for="time-rate">Time rate <span class="kind kind-world">WORLD</span></label><input id="time-rate" type="range" min="0" max="20" step="0.1" value="${state.timeRate}"><output>${format(state.timeRate, 0.1)}</output></div>
    <div class="actions"><button id="pause">Pause</button><button id="zero-offset">Reset drift</button></div>
  </details>`;
const seedInput = worldToolbarElement.querySelector<HTMLInputElement>("#seed")!;
const timeRateInput = worldToolbarElement.querySelector<HTMLInputElement>("#time-rate")!;
const timeRateOutput = timeRateInput.nextElementSibling as HTMLOutputElement;
const pauseButton = worldToolbarElement.querySelector<HTMLButtonElement>("#pause")!;
seedInput.addEventListener("input", () => {
  state.seed = seedInput.value;
});
timeRateInput.addEventListener("input", () => {
  state.timeRate = Number(timeRateInput.value);
  timeRateOutput.value = format(state.timeRate, 0.1);
});
pauseButton.addEventListener("click", () => {
  state.paused = !state.paused;
  pauseButton.textContent = state.paused ? "Resume" : "Pause";
});
worldToolbarElement.querySelector<HTMLButtonElement>("#zero-offset")!.addEventListener("click", () => {
  lowOffset = [0, 0];
  upperOffset = [0, 0];
});

function syncControl(key: NumericStateKey): void {
  updateResetButton(key);
  if (
    key === "lowCloudEnabled"
    || key === "upperCloudEnabled"
    || key === "sunEnabled"
    || key === "synopticComponentEnabled"
    || key === "regionalComponentEnabled"
    || key === "gustEnabled"
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
      state[key] = from + (to - from) * amount;
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
const sceneBuffer = device.createBuffer({ size: 128, usage: 0x40 | 0x08 });
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

function wind(speed: number, degrees: number): [number, number] {
  const radians = degrees * Math.PI / 180;
  return [speed * Math.cos(radians), speed * Math.sin(radians)];
}

function sunDirection(azimuthDegrees: number, elevationDegrees: number): [number, number, number] {
  const azimuth = azimuthDegrees * Math.PI / 180;
  const elevation = elevationDegrees * Math.PI / 180;
  const horizontal = Math.cos(elevation);
  return [horizontal * Math.cos(azimuth), horizontal * Math.sin(azimuth), Math.sin(elevation)];
}

function writeUniforms(): void {
  const sceneData = new ArrayBuffer(128);
  const sceneFloats = new Float32Array(sceneData);
  const sceneUints = new Uint32Array(sceneData);
  const aspect = previewCanvas.width / previewCanvas.height;
  const surface = wind(state.surfaceWindSpeed, state.surfaceWindDirection);
  const synoptic = wind(state.synopticWindSpeed, state.synopticWindDirection);
  sceneFloats.set([state.cameraX, state.cameraY, state.viewHeight * aspect, state.viewHeight], 0);
  sceneFloats.set([surface[0], surface[1], state.cover, state.rain], 4);
  sceneFloats.set([state.moisture, state.instability, state.upperCloud, 0], 8);
  sceneFloats.set([
    ...sunDirection(state.sunAzimuth, state.sunElevation),
    state.sunEnabled * state.sunIntensity,
  ], 12);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  sceneUints.set([seedLow, seedHigh, seedLow ^ 0x6174, seedHigh ^ 0x7079626f], 16);
  sceneFloats.set([...lowOffset, ...upperOffset], 20);
  sceneFloats.set([...surface, ...synoptic], 24);
  sceneFloats[28] = state.time;
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
    state.synopticComponentEnabled * state.synopticWeight,
    state.regionalComponentEnabled * state.regionalWeight,
    state.gustEnabled * state.gustWeight,
    state.shelterEnabled * state.shelterStrength,
  ], 28);
  tuningUints.set([state.debugView, 0, 0, 0], 32);
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
    const surface = wind(state.surfaceWindSpeed, state.surfaceWindDirection);
    const synoptic = wind(state.synopticWindSpeed, state.synopticWindDirection);
    lowOffset = [
      lowOffset[0] + surface[0] * simulationElapsed,
      lowOffset[1] + surface[1] * simulationElapsed,
    ];
    upperOffset = [
      upperOffset[0] + synoptic[0] * simulationElapsed,
      upperOffset[1] + synoptic[1] * simulationElapsed,
    ];
  }
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
    : `${previewCanvas.width} × ${previewCanvas.height} · ${format(state.time, 1)} s · ${format(state.timeRate, 0.1)}×`;
  requestAnimationFrame(frame);
}

requestAnimationFrame(frame);
