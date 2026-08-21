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
  cloudTimeScale: number;
  worldTimeScale: number;
  timeScalesLocked: boolean;
  paused: boolean;
  pressureHpa: number;
  pressureGradient: number;
  pressureGradientDirection: number;
  synopticPressureGradient: number;
  synopticPressureGradientDirection: number;
  annualMeanAirTemperature: number;
  currentAirTemperatureAnomaly: number;
  seasonalTemperatureRange: number;
  diurnalTemperatureRange: number;
  currentUpperAirTemperature: number;
  temperatureGradient: number;
  temperatureGradientDirection: number;
  currentWaterVapor: number;
  aerosolOpticalDepth: number;
  geographicLatitude: number;
  seasonPhase: number;
  dayPhase: number;
  moonPhase: number;
  tidalRange: number;
  tidalPhaseOffset: number;
  celestialLightingEnabled: number;
  detailCharacter: number;
  detailBlobiness: number;
  detailContrast: number;
  detailEdgeSoftness: number;
  maxVisibleCoverage: number;
  opticalDepth: number;
  opacity: number;
  darkness: number;
  relief: number;
  terrainScale: number;
  elevationScale: number;
  cliffSlope: number;
  waterAttenuationLength: number;
  bottomLightContribution: number;
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

type NumericStateKey = Exclude<keyof State, "seed" | "paused" | "timeScalesLocked">;
type RangeControl = {
  type: "range";
  key: NumericStateKey;
  label: string;
  min: number;
  max: number;
  step: number;
  toState?: (value: number) => number;
  fromState?: (value: number) => number;
  context?: (value: number) => string;
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

type WeatherScenario = Pick<State,
  | "pressureHpa" | "pressureGradient" | "pressureGradientDirection"
  | "synopticPressureGradient" | "synopticPressureGradientDirection"
  | "annualMeanAirTemperature" | "currentAirTemperatureAnomaly"
  | "currentUpperAirTemperature"
  | "seasonalTemperatureRange" | "diurnalTemperatureRange"
  | "temperatureGradient" | "temperatureGradientDirection" | "currentWaterVapor"
  | "aerosolOpticalDepth"
  | "geographicLatitude" | "seasonPhase" | "dayPhase" | "moonPhase"
  | "tidalRange" | "tidalPhaseOffset"
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
  cloudTimeScale: 1,
  worldTimeScale: 0,
  timeScalesLocked: false,
  paused: false,
  pressureHpa: 1008,
  pressureGradient: 1.0,
  pressureGradientDirection: -81,
  synopticPressureGradient: 0.85,
  synopticPressureGradientDirection: -81,
  annualMeanAirTemperature: 14,
  currentAirTemperatureAnomaly: 0,
  seasonalTemperatureRange: 12,
  diurnalTemperatureRange: 4,
  currentUpperAirTemperature: -14,
  temperatureGradient: 0.12,
  temperatureGradientDirection: -65,
  currentWaterVapor: 13,
  aerosolOpticalDepth: 0.08,
  geographicLatitude: 45,
  seasonPhase: 0.2521,
  dayPhase: 0.5833,
  moonPhase: 0.25,
  tidalRange: 1.25,
  tidalPhaseOffset: 0,
  celestialLightingEnabled: 1,
  detailCharacter: 0.35,
  detailBlobiness: 0.25,
  detailContrast: 1.15,
  detailEdgeSoftness: 0.06,
  maxVisibleCoverage: 0.5,
  opticalDepth: 1,
  opacity: 1,
  darkness: 1,
  relief: 1,
  terrainScale: 1,
  elevationScale: 3,
  cliffSlope: 0.08,
  waterAttenuationLength: 12,
  bottomLightContribution: 0.55,
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

const monthNames = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

const wrappedPhase = (phase: number): number => ((phase % 1) + 1) % 1;

function lunarPhaseName(phase: number): string {
  const names = [
    "New moon", "Waxing crescent", "First quarter", "Waxing gibbous",
    "Full moon", "Waning gibbous", "Last quarter", "Waning crescent",
  ];
  return names[Math.floor(wrappedPhase(phase) * 8 + 0.5) % 8];
}

const groups: ControlGroup[] = [
  {
    domain: "weather", category: "Location & time", layer: "Place and sky", kind: "WORLD", controls: [
      range("geographicLatitude", "Geographic latitude · °", -90, 90, 1),
      {
        ...range("seasonPhase", "Season · month", 1, 12.9999, 0.0001),
        toState: (month) => (month - 1) / 12,
        fromState: (phase) => 1 + 12 * wrappedPhase(phase),
        context: (phase) => monthNames[Math.floor(12 * wrappedPhase(phase))],
      },
      {
        ...range("dayPhase", "Local solar time · h", 0, 23.999, 0.001),
        toState: (hours) => hours / 24,
        fromState: (phase) => 24 * wrappedPhase(phase),
        context: (phase) => `${(24 * wrappedPhase(phase)).toFixed(2)} h`,
      },
      {
        ...range("moonPhase", "Lunar phase", 0, 1, 0.01),
        context: lunarPhaseName,
      },
    ],
  },
  {
    domain: "weather", category: "Climate", layer: "Temperature normals", kind: "WORLD", controls: [
      range("annualMeanAirTemperature", "Annual mean air temperature · °C", -35, 45, 0.5),
      range("seasonalTemperatureRange", "Seasonal range · °C", 0, 40, 0.5),
      range("diurnalTemperatureRange", "Daily range · °C", 0, 20, 0.5),
    ],
  },
  {
    domain: "weather", category: "Current weather", layer: "Pressure", kind: "WORLD", controls: [
      range("pressureHpa", "Pressure · hPa", 970, 1040, 0.5),
      range("pressureGradient", "Gradient · hPa/km", 0, 3, 0.02),
      range("pressureGradientDirection", "Gradient direction", -180, 180, 1),
      range("synopticPressureGradient", "Broad gradient · hPa/km", 0, 3, 0.02),
      range("synopticPressureGradientDirection", "Broad direction", -180, 180, 1),
    ],
  },
  {
    domain: "weather", category: "Current weather", layer: "Air and moisture", kind: "WORLD", controls: [
      range("currentAirTemperatureAnomaly", "Air temperature anomaly · °C", -25, 25, 0.5),
      range("currentUpperAirTemperature", "500 hPa temperature · °C", -80, 10, 0.5),
      range("temperatureGradient", "Horizontal ΔT · °C/km", 0, 1, 0.01),
      range("temperatureGradientDirection", "ΔT direction", -180, 180, 1),
      range("currentWaterVapor", "Current water vapour · g/kg", 0, 25, 0.1),
    ],
  },
  {
    domain: "weather", category: "Current weather", layer: "Atmospheric optics", kind: "WORLD", controls: [
      range("aerosolOpticalDepth", "Aerosol optical depth", 0, 1, 0.01),
    ],
  },
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
      range("maxVisibleCoverage", "Maximum coverage", 0, 1, 0.01),
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
    domain: "weather", category: "Advanced tuning", layer: "Wind components", kind: "DEBUG", level: "advanced", controls: [
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
    domain: "water", category: "Local tide", layer: "Astronomical tide", kind: "WORLD", controls: [
      toggle("tidesEnabled", "Tidal water height"),
      range("tidalRange", "Reference tidal range · m", 0, 5, 0.05),
      range("tidalPhaseOffset", "Local phase offset", -0.5, 0.5, 0.005),
    ],
  },
  {
    domain: "water", category: "Advanced tuning", layer: "Underwater optics", kind: "SHADER", level: "advanced", controls: [
      range("waterAttenuationLength", "Attenuation length · m", 0.5, 40, 0.5),
      range("bottomLightContribution", "Bottom light contribution", 0, 1, 0.01),
    ],
  },
];

const categoryOrder = [
  "Location & time", "Climate", "Current weather", "Local tide", "Visibility", "Advanced tuning",
];

const lowCloudDependentKeys = new Set<NumericStateKey>([
  "detailCharacter", "detailBlobiness", "detailContrast", "detailEdgeSoftness",
  "maxVisibleCoverage",
  "opticalDepth", "opacity", "darkness", "relief",
  "lowCloudVisible", "lowCloudShadow",
]);
const landscapeCompositionKeys = new Set<NumericStateKey>([
  "coastalReliefVisible", "vegetationVisible", "surfaceTextureVisible",
  "landscapeLightingVisible", "seasonalSnowVisible",
  "seasonalVegetationVisible",
]);
const tideKeys = new Set<NumericStateKey>(["tidalRange", "tidalPhaseOffset"]);
const subtoggleKeys = new Set<NumericStateKey>([
  "lowCloudVisible", "lowCloudShadow", "upperCloudShadow", "upperShadowOnLowCloud",
  ...landscapeCompositionKeys,
]);
const subrangeKeys = new Set<NumericStateKey>([
  "tidalRange", "tidalPhaseOffset",
  "pressureWindScale", "crossIsobarScale", "thermalWindScale", "shelterStrength",
]);

const debugOptions: Record<Domain, Array<[number, string]>> = {
  weather: [
    [0, "Final composition"], [1, "Low detail texture"], [2, "Low detail gate"],
    [3, "Low density"], [4, "Low optical depth"], [5, "Low normals"],
    [6, "Upper optical depth"],
    [9, "Projected low shadow"], [10, "Projected upper shadow"], [11, "Combined transmittance"],
    [12, "Low height field"],
    [16, "Surface wind"], [17, "Upper wind"], [18, "Vertical wind shear"],
    [19, "Pressure gradient"], [20, "Temperature gradient"], [21, "Terrain shelter"],
  ],
  landscape: [
    [0, "Final composition"], [32, "Elevation"], [33, "Slope"], [34, "Normals"],
    [35, "Island clusters"], [36, "Island detail"], [37, "Coastal reef field"],
    [38, "Vegetation field"], [39, "Beach geology"], [40, "Seasonal snow"],
    [41, "Solid ice"], [42, "Autumn vegetation"],
  ],
  water: [[0, "Final composition"], [48, "Tide height"]],
};

const weatherScenarios = scenarioDefinitions as WeatherScenarioDefinition[];

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
      maxVisibleCoverage: defaults.maxVisibleCoverage,
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
      bottomLightContribution: defaults.bottomLightContribution,
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
const outputByKey = new Map<NumericStateKey, HTMLInputElement>();
const contextByKey = new Map<NumericStateKey, HTMLOutputElement>();
const rangeByKey = new Map<NumericStateKey, RangeControl>();
const resetByKey = new Map<NumericStateKey, HTMLButtonElement>();
const controlElementByKey = new Map<NumericStateKey, HTMLElement>();
let derivedOutputElement: HTMLElement | undefined;

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
  setControlsEnabled(tideKeys, state.tidesEnabled > 0.5);
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

function createRangeControl(control: RangeControl, kind: ControlKind): HTMLDivElement {
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
  const controlValue = control.fromState?.(state[control.key]) ?? state[control.key];
  input.value = String(controlValue);
  const output = document.createElement("input");
  output.type = "number";
  output.className = "range-value";
  output.min = String(control.min);
  output.max = String(control.max);
  output.step = String(control.step);
  output.value = format(controlValue, control.step);
  input.addEventListener("input", () => {
    presetTransition = undefined;
    valueTransitions.delete(control.key);
    const value = Number(input.value);
    state[control.key] = control.toState?.(value) ?? value;
    syncControl(control.key);
  });
  output.addEventListener("input", () => {
    if (!Number.isFinite(output.valueAsNumber)) return;
    presetTransition = undefined;
    valueTransitions.delete(control.key);
    const value = Math.min(control.max, Math.max(control.min, output.valueAsNumber));
    state[control.key] = control.toState?.(value) ?? value;
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
  if (kind === "WORLD") {
    container.classList.add("no-reset");
  } else {
    container.append(resetButton(control.key, control.label));
  }
  if (control.context) {
    const context = document.createElement("output");
    context.className = "range-context";
    context.value = control.context(state[control.key]);
    container.append(context);
    contextByKey.set(control.key, context);
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
  const controlValue = Math.min(
    control.max,
    Math.max(control.min, Math.round(value / control.step) * control.step),
  );
  return control.toState?.(controlValue) ?? controlValue;
}

function createToggleControl(control: ToggleControl, kind: ControlKind): HTMLDivElement {
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
  container.append(input, heading);
  if (kind === "WORLD") {
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
  contextByKey.clear();
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
            ? createRangeControl(control, group.kind)
            : createToggleControl(control, group.kind),
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
    <div class="control range-control no-reset"><label class="control-name" for="cloud-time-scale">Cloud evolution · × real time</label><input id="cloud-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.cloudTimeScale)}"><input id="cloud-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.cloudTimeScale}"></div>
    <div class="control range-control no-reset"><label class="control-name" for="world-time-scale">World clock · × real time</label><input id="world-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.worldTimeScale)}"><input id="world-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.worldTimeScale}"></div>
    <div class="control toggle-control no-reset"><input id="lock-time-scales" type="checkbox"><label class="control-name" for="lock-time-scales">Lock speeds together</label></div>
    <div class="actions"><button id="pause">Pause</button></div>
  </details>`;
const seedInput = worldToolbarElement.querySelector<HTMLInputElement>("#seed")!;
const cloudTimeScaleInput = worldToolbarElement.querySelector<HTMLInputElement>("#cloud-time-scale")!;
const cloudTimeValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#cloud-time-value")!;
const worldTimeScaleInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-time-scale")!;
const worldTimeValueInput = worldToolbarElement.querySelector<HTMLInputElement>("#world-time-value")!;
const lockTimeScalesInput = worldToolbarElement.querySelector<HTMLInputElement>("#lock-time-scales")!;
const pauseButton = worldToolbarElement.querySelector<HTMLButtonElement>("#pause")!;
seedInput.addEventListener("input", () => {
  state.seed = seedInput.value;
});
function syncTimeScaleInputs(): void {
  cloudTimeScaleInput.value = String(timeScaleToSlider(state.cloudTimeScale));
  cloudTimeValueInput.value = String(state.cloudTimeScale);
  worldTimeScaleInput.value = String(timeScaleToSlider(state.worldTimeScale));
  worldTimeValueInput.value = String(state.worldTimeScale);
  lockTimeScalesInput.checked = state.timeScalesLocked;
}
function setCloudTimeScale(value: number): void {
  state.cloudTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  if (state.timeScalesLocked) state.worldTimeScale = state.cloudTimeScale;
  syncTimeScaleInputs();
}
function setWorldTimeScale(value: number): void {
  state.worldTimeScale = Math.min(maximumTimeScale, Math.max(0, value));
  if (state.timeScalesLocked) state.cloudTimeScale = state.worldTimeScale;
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
  if (state.timeScalesLocked) state.worldTimeScale = state.cloudTimeScale;
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
    const value = definition.fromState?.(state[key]) ?? state[key];
    input.value = String(value);
    output.value = format(value, definition.step);
    const context = contextByKey.get(key);
    if (context && definition.context) context.value = definition.context(state[key]);
  }
}

function advanceTransitions(elapsed: number): void {
  if (presetTransition) {
    presetTransition.elapsed += elapsed;
    const progress = Math.min(presetTransition.elapsed / 1.5, 1);
    const amount = progress * progress * (3 - 2 * progress);
    for (const key of Object.keys(presetTransition.to) as NumericStateKey[]) {
      const from = presetTransition.from[key] as number;
      const to = presetTransition.to[key] as number;
      if (key === "dayPhase" || key === "seasonPhase" || key === "moonPhase") {
        const delta = (to - from + 1.5) % 1 - 0.5;
        state[key] = (from + delta * amount + 1) % 1;
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
const sceneFloats = new Float32Array(32);
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

function directedVector(magnitude: number, degrees: number): [number, number] {
  const radians = degrees * Math.PI / 180;
  return [magnitude * Math.cos(radians), magnitude * Math.sin(radians)];
}

function currentAirTemperature(): number {
  const seasonalPhase = 2 * Math.PI * (state.seasonPhase - 0.09);
  const diurnalPhase = 2 * Math.PI * (state.dayPhase - 0.625);
  return state.annualMeanAirTemperature
    + 0.5 * state.seasonalTemperatureRange * seasonalLatitudeFactor()
      * Math.sin(seasonalPhase)
    + 0.5 * state.diurnalTemperatureRange * Math.cos(diurnalPhase)
    + state.currentAirTemperatureAnomaly;
}

function seasonalLatitudeFactor(): number {
  const magnitude = smoothstepValue(0, 45, Math.abs(state.geographicLatitude));
  return state.geographicLatitude < 0 ? -magnitude : magnitude;
}

function groundTemperature(elevation: number): number {
  return state.annualMeanAirTemperature
    + 0.42 * state.seasonalTemperatureRange * seasonalLatitudeFactor()
      * Math.sin(2 * Math.PI * (state.seasonPhase - 0.13))
    - 0.0065 * Math.max(elevation, 0);
}

function waterTemperature(): number {
  return state.annualMeanAirTemperature
    + 0.28 * state.seasonalTemperatureRange * seasonalLatitudeFactor()
      * Math.sin(2 * Math.PI * (state.seasonPhase - 0.175));
}

function tideHeight(): number {
  const solarHourAngle = 2 * Math.PI * (state.dayPhase - 0.5);
  const lunarHourAngle = solarHourAngle + 2 * Math.PI * state.moonPhase;
  const phaseOffset = 2 * Math.PI * state.tidalPhaseOffset;
  const lunarTide = Math.cos(2 * (lunarHourAngle + phaseOffset));
  const solarTide = 0.46 * Math.cos(2 * (solarHourAngle + phaseOffset));
  return 0.5 * state.tidesEnabled * state.tidalRange * (lunarTide + solarTide) / 1.46;
}

type Light = { direction: [number, number, number]; intensity: number };
type CelestialLighting = { sun: Light; moon: Light; ambient: number };

function celestialDirection(declination: number, hourAngle: number): [number, number, number] {
  const latitude = state.geographicLatitude * Math.PI / 180;
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
  const solarLongitude = 2 * Math.PI * state.seasonPhase;
  const solarDeclination = Math.asin(Math.sin(obliquity) * Math.sin(solarLongitude));
  const solarHourAngle = 2 * Math.PI * (state.dayPhase - 0.5);
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
  const airTemperature = currentAirTemperature();
  const humidity = Math.min(1.5, Math.max(
    0,
    state.currentWaterVapor / (3.5 * Math.exp(0.055 * (airTemperature + 5))),
  ));
  const instability = smoothstepValue(
    28,
    48,
    airTemperature - state.currentUpperAirTemperature,
  );
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
  const sunsetScattering = Math.min(
    1,
    0.82 * smoothstepValue(0.06, 0.4, state.aerosolOpticalDepth)
      + 0.18 * smoothstepValue(0.65, 1.1, humidity),
  );
  return [
    ["Current air temperature", `${airTemperature.toFixed(1)} °C`],
    ["Sun", `${angle(celestial.sun.direction)} · ${celestial.sun.intensity.toFixed(2)}`],
    ["Moon", `${angle(celestial.moon.direction)} · ${celestial.moon.intensity.toFixed(2)}`],
    ["Ambient light", (celestial.ambient * (1 - 0.42 * overcast)).toFixed(2)],
    ["Sunset scattering", sunsetScattering.toFixed(2)],
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

function surfaceOutputs(): Array<[string, string]> {
  const moisture = smoothstepValue(2, 10, state.currentWaterVapor);
  const ground = groundTemperature(0);
  const alpineGround = groundTemperature(600);
  const water = waterTemperature();
  const snow = (1 - smoothstepValue(-6, 2, alpineGround)) * (0.35 + 0.65 * moisture);
  const annualCold = 1 - smoothstepValue(-5, 1, state.annualMeanAirTemperature);
  const polarLatitude = smoothstepValue(55, 75, Math.abs(state.geographicLatitude));
  const polarIceClosure = polarLatitude * smoothstepValue(0.97, 1, annualCold);
  const solidIce = 1 - smoothstepValue(-5, 1, state.annualMeanAirTemperature - 3.9);
  return [
    ["Ground temperature", `${ground.toFixed(1)} °C`],
    ["Water temperature", `${water.toFixed(1)} °C`],
    ["Snow at 600 m", snow.toFixed(2)],
    ["Polar ice closure", polarIceClosure.toFixed(2)],
    ["Solid ice at 600 m", solidIce.toFixed(2)],
    ["Tide height", `${tideHeight().toFixed(2)} m`],
  ];
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

function writeUniforms(): void {
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
  sceneFloats.set([
    ...synopticGradient, state.annualMeanAirTemperature, state.currentUpperAirTemperature,
  ], 8);
  sceneFloats.set([
    state.currentWaterVapor, ...temperatureGradient, state.diurnalTemperatureRange,
  ], 12);
  sceneFloats.set([
    state.geographicLatitude, state.seasonPhase, state.dayPhase, state.moonPhase,
  ], 16);
  sceneFloats.set([
    state.tidesEnabled * state.tidalRange,
    state.tidalPhaseOffset,
    state.currentAirTemperatureAnomaly,
    0,
  ], 20);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  sceneUints.set([seedLow, seedHigh, seedLow ^ 0x6174, seedHigh ^ 0x7079626f], 24);
  sceneFloats[28] = state.time;
  sceneFloats[29] = state.aerosolOpticalDepth;
  device.queue.writeBuffer(sceneBuffer, 0, sceneFloats);

  tuningFloats.set([
    state.detailCharacter, state.detailBlobiness, state.detailContrast, state.detailEdgeSoftness,
  ], 0);
  tuningFloats.set([
    state.maxVisibleCoverage, 0, 0, 0,
  ], 4);
  tuningFloats.set([state.opticalDepth, state.opacity, state.darkness, state.relief], 8);
  tuningFloats.set([state.terrainScale, state.elevationScale, state.cliffSlope, 0], 12);
  tuningFloats.set([
    state.waterAttenuationLength, state.bottomLightContribution, 0, 0,
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
    0,
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
const worldDaySeconds = 86_400;
const worldYearDays = 360;
const lunarCycleDays = 28;
function frame(now: number): void {
  resize();
  updateScaleBar();
  const elapsed = Math.min((now - previous) / 1000, 0.1);
  previous = now;
  advanceTransitions(elapsed);
  if (!state.paused) {
    const simulationElapsed = elapsed * state.cloudTimeScale;
    state.time += simulationElapsed;
    const worldElapsed = elapsed * state.worldTimeScale;
    state.dayPhase = (state.dayPhase + worldElapsed / worldDaySeconds) % 1;
    state.seasonPhase = (state.seasonPhase + worldElapsed / (worldDaySeconds * worldYearDays)) % 1;
    state.moonPhase = (state.moonPhase + worldElapsed / (worldDaySeconds * lunarCycleDays)) % 1;
    syncControl("dayPhase");
    syncControl("seasonPhase");
    syncControl("moonPhase");
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
    : `${previewCanvas.width} × ${previewCanvas.height} · ${state.cloudTimeScale}× clouds · ${state.worldTimeScale}× world clock · day ${state.dayPhase.toFixed(3)} · season ${state.seasonPhase.toFixed(3)}`;
  requestAnimationFrame(frame);
}

requestAnimationFrame(frame);
