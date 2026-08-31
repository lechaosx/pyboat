import {
  playground_cloud_shadow_cache_shader_source,
  playground_diagnostic_shader_source,
  playground_direct_scene_shader_source,
  playground_scene_shader_source,
  playground_startup_shader_source,
  sample_playground_celestial_orbit,
  sample_playground_environment,
  sample_playground_field_time_coordinates,
  sample_playground_seasonal_phase_factors,
  sample_playground_view_cloud_state,
  sample_playground_world_geometry,
} from "browser/playground";
import {
  advanceFixedClock,
  fixedClockFraction,
  fixedTickSeconds,
  type FixedClock,
} from "./fixed-clock";
import { deriveIntermediateVisibility } from "./feature-visibility";
import {
  diagnosticGroups,
  diagnosticShaders,
  type DiagnosticSelection,
} from "./diagnostics";
import "./style.css";

type Domain = "weather" | "landscape" | "water";

let useShadowCache = true;
let shadowCacheTileTexels = 128;
let shadowCacheDebugMode = 0;
let shadowCacheInfoElement: HTMLElement | undefined;

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
  worldTimeSeconds: number;
  pressureTimeSeconds: number;
  currentTimeSeconds: number;
  sunLightVisible: number;
  moonLightVisible: number;
  skyLightVisible: number;
  canopyLightVisible: number;
  sunLightStrength: number;
  moonLightStrength: number;
  skyLightStrength: number;
  canopyLightStrength: number;
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
  lowCloudColorVisible: number;
  detailCharacter: number;
  detailBlobiness: number;
  detailContrast: number;
  detailEdgeSoftness: number;
  maxWispCoverage: number;
  cloudNormalStrength: number;
  lowCloudBodySize: number;
  lowCloudOccurrence: number;
  opticalDepth: number;
  opacity: number;
  darkness: number;
  cloudSelfShadowVisible: number;
  cloudEdgeLightStrength: number;
  cloudSelfShadowStrength: number;
  lowCloudShadowsVisible: number;
  canopyShadowsVisible: number;
  lowCloudShadowStrength: number;
  canopyShadowStrength: number;
  cloudShadowDensity: number;
  cloudShadowOpacity: number;
  terrainScale: number;
  elevationScale: number;
  cliffSlope: number;
  waterAttenuationLength: number;
  bottomVisibility: number;
  bottomReflectance: number;
  bottomVeil: number;
  waterWaveNormalStrength: number;
  waterCapillaryNormalStrength: number;
  waterRainNormalStrength: number;
  waterRoughnessScale: number;
  landIceNormalStrength: number;
  landIceRoughness: number;
  seaIceNormalStrength: number;
  seaIceRoughnessScale: number;
  seaIcePlateSize: number;
  seaIceFractureWidth: number;
  seaIceFractureContrast: number;
  seaIceShallowTint: number;
  rockNormalStrength: number;
  grassNormalStrength: number;
  forestNormalStrength: number;
  snowNormalStrength: number;
  forestCoverage: number;
  rockCoverage: number;
  beachCoverage: number;
  snowCoverage: number;
  landformReliefStrength: number;
  landformShadowsVisible: number;
  landformShadowStrength: number;
  landformShadowReach: number;
  pressureWindScale: number;
  crossIsobarScale: number;
  thermalWindScale: number;
  shelterStrength: number;
  diagnostic: DiagnosticSelection;
};

type NumericStateKey = Exclude<
  keyof State,
  "seed" | "timeScalesLocked" | "diagnostic"
>;
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
  worldTimeSeconds: 0,
  pressureTimeSeconds: 0,
  currentTimeSeconds: 0,
  sunLightVisible: 1,
  moonLightVisible: 1,
  skyLightVisible: 1,
  canopyLightVisible: 1,
  sunLightStrength: 1,
  moonLightStrength: 1,
  skyLightStrength: 1,
  canopyLightStrength: 1,
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
  lowCloudColorVisible: 1,
  detailCharacter: 0.35,
  detailBlobiness: 0.25,
  detailContrast: 1.15,
  detailEdgeSoftness: 0.06,
  maxWispCoverage: 0.25,
  cloudNormalStrength: 0,
  lowCloudBodySize: 1,
  lowCloudOccurrence: 1,
  opticalDepth: 1,
  opacity: 1,
  darkness: 1,
  cloudSelfShadowVisible: 1,
  cloudEdgeLightStrength: 0.18,
  cloudSelfShadowStrength: 0.14,
  lowCloudShadowsVisible: 1,
  canopyShadowsVisible: 1,
  lowCloudShadowStrength: 1,
  canopyShadowStrength: 1,
  cloudShadowDensity: 0.85,
  cloudShadowOpacity: 1,
  terrainScale: 1,
  elevationScale: 3,
  cliffSlope: 0.08,
  waterAttenuationLength: 0.5,
  bottomVisibility: 1,
  bottomReflectance: 3,
  bottomVeil: 0.35,
  waterWaveNormalStrength: 2.5,
  waterCapillaryNormalStrength: 0.2,
  waterRainNormalStrength: 1,
  waterRoughnessScale: 1,
  landIceNormalStrength: 0.18,
  landIceRoughness: 0.32,
  seaIceNormalStrength: 1,
  seaIceRoughnessScale: 1,
  seaIcePlateSize: 192,
  seaIceFractureWidth: 1,
  seaIceFractureContrast: 1,
  seaIceShallowTint: 1,
  rockNormalStrength: 1,
  grassNormalStrength: 1,
  forestNormalStrength: 1,
  snowNormalStrength: 1,
  forestCoverage: 1,
  rockCoverage: 1,
  beachCoverage: 1,
  snowCoverage: 1,
  landformReliefStrength: 1,
  landformShadowsVisible: 1,
  landformShadowStrength: 0.7,
  landformShadowReach: 320,
  pressureWindScale: 1,
  crossIsobarScale: 1,
  thermalWindScale: 1,
  shelterStrength: 1,
  diagnostic: "scene",
};

const wrappedPhase = (phase: number): number => ((phase % 1) + 1) % 1;
const worldDaySeconds = 86_400;
const worldYearDays = 336;
// Keep these calendar-derived phases synchronized with world_celestial_conditions.
const lunarOrbitDays = 1 / (1 / 28 + 1 / worldYearDays);
const firstBloodMoonSeconds = 91 * worldDaySeconds - 4 * 60;
const celestialEventPresets = {
  "solar-eclipse": 77 * worldDaySeconds,
  "lunar-eclipse": firstBloodMoonSeconds - 60 * 60,
  "blood-moon": firstBloodMoonSeconds,
} as const;

function worldPhases(worldTimeSeconds: number): [number, number, number] {
  return [
    wrappedPhase(0.5833 + worldTimeSeconds / worldDaySeconds),
    wrappedPhase(0.5021 + worldTimeSeconds / (worldDaySeconds * worldYearDays)),
    wrappedPhase(0.5021 + worldTimeSeconds / (worldDaySeconds * lunarOrbitDays)),
  ];
}

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
let previousCloudTimeSeconds: number | undefined;
const lowCloudMotionM = [0, 0];
const upperCloudMotionM = [0, 0];
let previousLowCloudWindMS = [0, 0];
let previousUpperCloudWindMS = [0, 0];

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
  for (const domain of ["weather", "landscape", "water"] as Domain[]) {
    for (const group of diagnosticGroups[domain]) {
      if (group.label === "Overview" && domain !== "weather") continue;
      const optionGroup = document.createElement("optgroup");
      optionGroup.label = group.label === "Overview"
        ? group.label
        : `${domain[0].toUpperCase()}${domain.slice(1)} · ${group.label}`;
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
    if (state.diagnostic !== "scene" && shadowCacheDebugMode !== 0) {
      shadowCacheDebugMode = 0;
      cacheDebugSelect.value = "0";
      cacheLegend.hidden = true;
      recreateShadowCache();
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
  const cacheLabel = document.createElement("div");
  cacheLabel.className = "control toggle-control no-reset";
  const cacheInput = document.createElement("input");
  cacheInput.id = "shadow-cache-enabled";
  cacheInput.type = "checkbox";
  cacheInput.checked = useShadowCache;
  const cacheHeading = document.createElement("label");
  cacheHeading.className = "control-name";
  cacheHeading.htmlFor = cacheInput.id;
  cacheHeading.textContent = "Cached shadow fields";
  cacheInput.addEventListener("change", () => {
    useShadowCache = cacheInput.checked;
    if (!useShadowCache && shadowCacheDebugMode !== 0) {
      shadowCacheDebugMode = 0;
      cacheDebugSelect.value = "0";
      cacheLegend.hidden = true;
      recreateShadowCache();
    }
  });
  cacheLabel.append(cacheInput, cacheHeading);
  const cacheDebugLabel = document.createElement("div");
  cacheDebugLabel.className = "select-control cache-control";
  const cacheDebugHeading = document.createElement("label");
  cacheDebugHeading.className = "control-name";
  cacheDebugHeading.textContent = "Cache view";
  const cacheDebugSelect = document.createElement("select");
  cacheDebugSelect.id = "shadow-cache-debug";
  cacheDebugHeading.htmlFor = cacheDebugSelect.id;
  for (const [value, label] of [
    ["0", "Scene"],
    ["1", "Lookups"],
    ["2", "Tiles"],
    ["3", "Atlas"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    cacheDebugSelect.append(option);
  }
  cacheDebugSelect.value = String(shadowCacheDebugMode);
  cacheDebugSelect.addEventListener("change", () => {
    shadowCacheDebugMode = Number(cacheDebugSelect.value);
    cacheLegend.hidden = shadowCacheDebugMode === 0;
    cacheFallbackLegend.hidden = shadowCacheDebugMode !== 1;
    if (shadowCacheDebugMode !== 0) {
      useShadowCache = true;
      cacheInput.checked = true;
      state.diagnostic = "scene";
      diagnosticSelect.value = state.diagnostic;
      updateDiagnosticUI();
    }
    recreateShadowCache();
  });
  cacheDebugLabel.append(cacheDebugHeading, cacheDebugSelect);
  const tileSizeLabel = document.createElement("div");
  tileSizeLabel.className = "select-control cache-control";
  const tileSizeHeading = document.createElement("label");
  tileSizeHeading.className = "control-name";
  tileSizeHeading.textContent = "Tile size";
  const tileSizeSelect = document.createElement("select");
  tileSizeSelect.id = "shadow-cache-tile-size";
  tileSizeHeading.htmlFor = tileSizeSelect.id;
  for (const tileTexels of [32, 64, 128, 256]) {
    const option = document.createElement("option");
    option.value = String(tileTexels);
    option.textContent = `${tileTexels} px`;
    tileSizeSelect.append(option);
  }
  tileSizeSelect.value = String(shadowCacheTileTexels);
  tileSizeSelect.addEventListener("change", () => {
    shadowCacheTileTexels = Number(tileSizeSelect.value);
    recreateShadowCache();
  });
  tileSizeLabel.append(tileSizeHeading, tileSizeSelect);
  shadowCacheInfoElement = document.createElement("div");
  shadowCacheInfoElement.className = "cache-info";
  const cacheLegend = document.createElement("div");
  cacheLegend.className = "cache-legend";
  cacheLegend.hidden = shadowCacheDebugMode === 0;
  cacheLegend.innerHTML = [
    '<span><i class="cache-low"></i>Low cloud</span>',
    '<span><i class="cache-upper"></i>Canopy</span>',
    '<span><i class="cache-terrain"></i>Terrain</span>',
    '<span class="cache-fallback-label"><i class="cache-fallback"></i>Fallback</span>',
  ].join("");
  const cacheFallbackLegend = cacheLegend.querySelector<HTMLElement>(
    ".cache-fallback-label",
  )!;
  cacheFallbackLegend.hidden = shadowCacheDebugMode !== 1;
  diagnosticSection.append(
    cacheLabel,
    cacheDebugLabel,
    tileSizeLabel,
    shadowCacheInfoElement,
    cacheLegend,
  );
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
      { type: "range", key: "sunLightStrength", label: "Sun strength", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "moonLightVisible", label: "Direct Moon" },
      { type: "range", key: "moonLightStrength", label: "Moon strength", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "skyLightVisible", label: "Clear-sky ambient" },
      { type: "range", key: "skyLightStrength", label: "Clear-sky strength", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "canopyLightVisible", label: "Canopy ambient" },
      { type: "range", key: "canopyLightStrength", label: "Canopy strength", min: 0, max: 2, step: 0.05 },
    ],
  },
  {
    label: "Land",
    controls: [
      { type: "range", key: "terrainScale", label: "Feature scale", min: 0.25, max: 4, step: 0.05 },
      { type: "range", key: "elevationScale", label: "Elevation", min: 0.1, max: 3, step: 0.05 },
      { type: "range", key: "cliffSlope", label: "Cliff response", min: 0.01, max: 0.3, step: 0.005 },
      { type: "range", key: "landformReliefStrength", label: "Relief strength", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "landformShadowsVisible", label: "Cast shadows" },
      { type: "range", key: "landformShadowStrength", label: "Shadow strength", min: 0, max: 1, step: 0.05 },
      { type: "range", key: "landformShadowReach", label: "Shadow reach · m", min: 40, max: 1200, step: 10 },
      { type: "toggle", key: "grassColorVisible", label: "Grass" },
      { type: "range", key: "grassNormalStrength", label: "Grass normals", min: 0, max: 3, step: 0.05 },
      { type: "toggle", key: "forestColorVisible", label: "Forest" },
      { type: "range", key: "forestCoverage", label: "Forest coverage", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "forestNormalStrength", label: "Forest normals", min: 0, max: 3, step: 0.05 },
      { type: "toggle", key: "intertidalColorVisible", label: "Intertidal ground" },
      { type: "toggle", key: "beachColorVisible", label: "Beach sediment" },
      { type: "range", key: "beachCoverage", label: "Beach coverage", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "rockColorVisible", label: "Rock" },
      { type: "range", key: "rockCoverage", label: "Rock coverage", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "rockNormalStrength", label: "Rock normals", min: 0, max: 3, step: 0.05 },
      { type: "toggle", key: "snowColorVisible", label: "Snow" },
      { type: "range", key: "snowCoverage", label: "Snow coverage", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "snowNormalStrength", label: "Snow normals", min: 0, max: 3, step: 0.05 },
    ],
  },
  {
    label: "Water",
    controls: [
      { type: "toggle", key: "waterColorVisible", label: "Water body" },
      { type: "range", key: "bottomVeil", label: "Water veil", min: 0, max: 1, step: 0.05 },
      { type: "range", key: "waterWaveNormalStrength", label: "Wave normals", min: 0, max: 5, step: 0.05 },
      { type: "range", key: "waterCapillaryNormalStrength", label: "Capillary normals", min: 0, max: 1, step: 0.02 },
      { type: "range", key: "waterRainNormalStrength", label: "Rain normals", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "waterRoughnessScale", label: "Roughness", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "bottomColorVisible", label: "Visible bottom" },
      { type: "range", key: "waterAttenuationLength", label: "Bottom attenuation · m", min: 0.1, max: 4, step: 0.05 },
      { type: "range", key: "bottomVisibility", label: "Bottom visibility", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "bottomReflectance", label: "Bottom reflectance", min: 0, max: 6, step: 0.1 },
    ],
  },
  {
    label: "Ice",
    controls: [
      { type: "toggle", key: "landIceColorVisible", label: "Land ice" },
      { type: "range", key: "landIceNormalStrength", label: "Land-ice normals", min: 0, max: 0.6, step: 0.01 },
      { type: "range", key: "landIceRoughness", label: "Land-ice roughness", min: 0, max: 1, step: 0.02 },
      { type: "toggle", key: "seaIceColorVisible", label: "Sea ice" },
      { type: "range", key: "seaIceNormalStrength", label: "Sea-ice normals", min: 0, max: 3, step: 0.05 },
      { type: "range", key: "seaIceRoughnessScale", label: "Sea-ice roughness", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "seaIcePlateSize", label: "Plate size · m", min: 48, max: 768, step: 8 },
      { type: "range", key: "seaIceFractureWidth", label: "Fracture width", min: 0.25, max: 3, step: 0.05 },
      { type: "range", key: "seaIceFractureContrast", label: "Fracture contrast", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "seaIceShallowTint", label: "Shallow tint", min: 0, max: 2, step: 0.05 },
    ],
  },
  {
    label: "Clouds",
    controls: [
      { type: "toggle", key: "lowCloudColorVisible", label: "Cloud body" },
      { type: "range", key: "detailCharacter", label: "Detail character", min: 0, max: 1, step: 0.05 },
      { type: "range", key: "detailBlobiness", label: "Blobiness", min: 0, max: 1, step: 0.05 },
      { type: "range", key: "maxWispCoverage", label: "Wisp coverage", min: 0, max: 0.95, step: 0.05 },
      { type: "range", key: "lowCloudBodySize", label: "Body size", min: 0.25, max: 4, step: 0.05 },
      { type: "range", key: "lowCloudOccurrence", label: "Occurrence", min: 0, max: 4, step: 0.05 },
      { type: "range", key: "detailContrast", label: "Shape contrast", min: 0.25, max: 2.5, step: 0.05 },
      { type: "range", key: "detailEdgeSoftness", label: "Edge softness", min: 0.005, max: 0.25, step: 0.005 },
      { type: "range", key: "cloudNormalStrength", label: "Normal strength", min: 0, max: 1, step: 0.05 },
      { type: "range", key: "opticalDepth", label: "Optical depth", min: 0, max: 3, step: 0.05 },
      { type: "range", key: "opacity", label: "Opacity", min: 0, max: 1.5, step: 0.05 },
      { type: "range", key: "darkness", label: "Darkening", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "cloudEdgeLightStrength", label: "Edge light", min: 0, max: 0.6, step: 0.01 },
      { type: "toggle", key: "cloudSelfShadowVisible", label: "Self-shadow" },
      { type: "range", key: "cloudSelfShadowStrength", label: "Self-shadow", min: 0, max: 0.6, step: 0.01 },
      { type: "toggle", key: "lowCloudShadowsVisible", label: "Low-cloud shadows" },
      { type: "range", key: "lowCloudShadowStrength", label: "Low-cloud depth", min: 0, max: 2, step: 0.05 },
      { type: "toggle", key: "canopyShadowsVisible", label: "Canopy shadows" },
      { type: "range", key: "canopyShadowStrength", label: "Canopy depth", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "cloudShadowDensity", label: "Density", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "cloudShadowOpacity", label: "Opacity", min: 0, max: 1, step: 0.05 },
    ],
  },
  {
    label: "Environment motion",
    controls: [
      { type: "range", key: "pressureWindScale", label: "Pressure circulation", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "crossIsobarScale", label: "Cross-isobar flow", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "thermalWindScale", label: "Thermal shear", min: 0, max: 2, step: 0.05 },
      { type: "range", key: "shelterStrength", label: "Terrain shelter", min: 0, max: 1, step: 0.05 },
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
        <label><span>Year</span><input id="world-year" type="number" min="1" step="1"></label>
        <label><span>Month</span><input id="world-month" type="number" min="1" max="12" step="1"></label>
        <label><span>Day</span><input id="world-day" type="number" min="1" max="28" step="1"></label>
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
      <div class="control range-control no-reset"><label class="control-name" for="cloud-time-scale">Clouds · ×</label><input id="cloud-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.cloudTimeScale)}"><input id="cloud-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.cloudTimeScale}"></div>
      <div class="control range-control no-reset"><label class="control-name" for="pressure-time-scale">Pressure · ×</label><input id="pressure-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.pressureTimeScale)}"><input id="pressure-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.pressureTimeScale}"></div>
      <div class="control range-control no-reset"><label class="control-name" for="current-time-scale">Currents · ×</label><input id="current-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.currentTimeScale)}"><input id="current-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.currentTimeScale}"></div>
      <div class="control range-control no-reset"><label class="control-name" for="world-time-scale">World clock · ×</label><input id="world-time-scale" type="range" min="${minimumTimeScaleSlider}" max="5" step="0.01" value="${timeScaleToSlider(state.worldTimeScale)}"><input id="world-time-value" class="range-value" type="number" min="0" max="${maximumTimeScale}" step="0.1" value="${state.worldTimeScale}"></div>
      <div class="control toggle-control no-reset"><input id="lock-time-scales" type="checkbox"><label class="control-name" for="lock-time-scales">Lock speeds together</label></div>
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
const worldMonthInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-month")!;
const worldDayInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-day")!;
const worldTimeInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-time")!;
const positionMeasurementsElement = diagnosticToolbarElement.querySelector<HTMLElement>("#position-measurements")!;
const derivedOutputHeadingElement = diagnosticToolbarElement.querySelector<HTMLElement>("#derived-output-heading")!;
const derivedOutputElement = diagnosticToolbarElement.querySelector<HTMLElement>("#derived-output")!;
const diagnosticsDetailsElement = positionMeasurementsElement.closest<HTMLDetailsElement>("details")!;
const cloudTimeScaleInput = timeToolbarElement.querySelector<HTMLInputElement>("#cloud-time-scale")!;
const cloudTimeValueInput = timeToolbarElement.querySelector<HTMLInputElement>("#cloud-time-value")!;
const pressureTimeScaleInput = timeToolbarElement.querySelector<HTMLInputElement>("#pressure-time-scale")!;
const pressureTimeValueInput = timeToolbarElement.querySelector<HTMLInputElement>("#pressure-time-value")!;
const currentTimeScaleInput = timeToolbarElement.querySelector<HTMLInputElement>("#current-time-scale")!;
const currentTimeValueInput = timeToolbarElement.querySelector<HTMLInputElement>("#current-time-value")!;
const worldTimeScaleInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-time-scale")!;
const worldTimeValueInput = timeToolbarElement.querySelector<HTMLInputElement>("#world-time-value")!;
const lockTimeScalesInput = timeToolbarElement.querySelector<HTMLInputElement>("#lock-time-scales")!;

function syncWorldDateInputs(): void {
  const totalSeconds = Math.max(0, Math.floor(state.worldTimeSeconds));
  const totalDays = Math.floor(totalSeconds / worldDaySeconds);
  const dayOfYear = totalDays % worldYearDays;
  const secondsOfDay = totalSeconds % worldDaySeconds;
  const hours = Math.floor(secondsOfDay / 3600);
  const minutes = Math.floor(secondsOfDay % 3600 / 60);
  const seconds = secondsOfDay % 60;
  const values: Array<[HTMLInputElement, string]> = [
    [worldYearInput, String(Math.floor(totalDays / worldYearDays) + 1)],
    [worldMonthInput, String(Math.floor(dayOfYear / 28) + 1)],
    [worldDayInput, String(dayOfYear % 28 + 1)],
    [worldTimeInput, [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":")],
  ];
  for (const [input, value] of values) {
    if (document.activeElement !== input && input.value !== value) input.value = value;
  }
}

function setWorldDateFromInputs(): void {
  const year = Math.max(1, Math.floor(worldYearInput.valueAsNumber || 1));
  const month = Math.min(12, Math.max(1, Math.floor(worldMonthInput.valueAsNumber || 1)));
  const day = Math.min(28, Math.max(1, Math.floor(worldDayInput.valueAsNumber || 1)));
  const [hours = 0, minutes = 0, seconds = 0] = worldTimeInput.value
    .split(":")
    .map((value) => Number.parseInt(value, 10) || 0);
  const dayIndex = (year - 1) * worldYearDays + (month - 1) * 28 + day - 1;
  state.worldTimeSeconds = dayIndex * worldDaySeconds
    + Math.min(23, hours) * 3600
    + Math.min(59, minutes) * 60
    + Math.min(59, seconds);
  syncWorldDateInputs();
}

for (const input of [worldYearInput, worldMonthInput, worldDayInput, worldTimeInput]) {
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
const gpu = (navigator as Navigator & { gpu?: any }).gpu;
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
if (import.meta.hot) import.meta.hot.dispose(() => device.destroy());
window.addEventListener("pagehide", () => device.destroy(), { once: true });
const context = previewCanvas.getContext("webgpu") as any;
const formatName = gpu.getPreferredCanvasFormat();
context.configure({ device, format: formatName, alphaMode: "opaque" });

await showStartupPhase("Creating startup shader…");
const startupShader = device.createShaderModule({ code: playground_startup_shader_source() });
function createPlaygroundPipeline(
  shader: any,
  entryPoint: string,
  constants?: Record<string, number>,
): Promise<any> {
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
    fragment: { module: shader, entryPoint, constants, targets: [{ format: formatName }] },
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
const sceneFloats = new Float32Array(56);
const sceneUints = new Uint32Array(sceneFloats.buffer);
const tuningFloats = new Float32Array(104);
const tuningUints = new Uint32Array(tuningFloats.buffer);
const sceneBuffer = device.createBuffer({ size: sceneFloats.byteLength, usage: 0x40 | 0x08 });
const playgroundBuffer = device.createBuffer({ size: tuningFloats.byteLength, usage: 0x40 | 0x08 });

type CloudShadowCache = {
  width: number;
  height: number;
  tileTexels: number;
  maximumTiles: number;
  atlasWidth: number;
  atlasHeight: number;
  liveTiles?: number;
  tileCountPending: boolean;
  tileCountReadback: any;
  discoveryPipeline: any;
  fillPipeline: any;
  discoverySceneBindGroup: any;
  discoveryPlaygroundBindGroup: any;
  discoveryBindGroup: any;
  fillSceneBindGroup: any;
  fillPlaygroundBindGroup: any;
  fillBindGroup: any;
  sampleBindGroup: any;
  entries: any;
  header: any;
  atlasView: any;
  resources: any[];
};

function createCloudShadowCache(
  discoveryPipeline: any,
  fillPipeline: any,
  scenePipeline: any,
): CloudShadowCache {
  const width = previewCanvas.width;
  const height = previewCanvas.height;
  const tileTexels = shadowCacheTileTexels;
  const physicalTileTexels = tileTexels + 2;
  const maximumAxisTiles = Math.max(
    1,
    Math.floor(device.limits.maxTextureDimension2D / physicalTileTexels),
  );
  const desiredTiles = Math.max(
    1,
    6 * (Math.ceil(width / tileTexels) + 1) * (Math.ceil(height / tileTexels) + 1),
  );
  const maximumTiles = Math.min(desiredTiles, maximumAxisTiles ** 2);
  const atlasColumns = Math.max(
    1,
    Math.min(maximumAxisTiles, Math.ceil(Math.sqrt(maximumTiles))),
  );
  const atlasRows = Math.ceil(maximumTiles / atlasColumns);
  let tableEntries = 1;
  while (tableEntries < 2 * maximumTiles) tableEntries *= 2;
  const cacheUniforms = new Uint32Array([
    width,
    height,
    atlasColumns * physicalTileTexels,
    atlasRows * physicalTileTexels,
    tableEntries - 1,
    maximumTiles,
    atlasColumns,
    tileTexels,
    0,
    0,
    0,
    0,
  ]);
  const uniformBuffer = device.createBuffer({
    size: cacheUniforms.byteLength,
    usage: 0x40 | 0x08,
    mappedAtCreation: true,
  });
  new Uint32Array(uniformBuffer.getMappedRange()).set(cacheUniforms);
  uniformBuffer.unmap();
  const header = device.createBuffer({
    size: 16,
    usage: 0x80 | 0x100 | 0x08 | 0x04,
    mappedAtCreation: true,
  });
  new Uint32Array(header.getMappedRange()).set([6, 0, 0, 0]);
  header.unmap();
  const entries = device.createBuffer({
    size: tableEntries * 20,
    usage: 0x80 | 0x08,
  });
  const tiles = device.createBuffer({ size: maximumTiles * 16, usage: 0x80 });
  const tileCountReadback = device.createBuffer({ size: 16, usage: 0x01 | 0x08 });
  const debugUniforms = new Uint32Array([shadowCacheDebugMode, 0, 0, 0]);
  const debugBuffer = device.createBuffer({
    size: debugUniforms.byteLength,
    usage: 0x40,
    mappedAtCreation: true,
  });
  new Uint32Array(debugBuffer.getMappedRange()).set(debugUniforms);
  debugBuffer.unmap();
  const atlas = device.createTexture({
    size: [cacheUniforms[2], cacheUniforms[3]],
    format: "rg16float",
    usage: 0x04 | 0x10,
  });
  const atlasView = atlas.createView();
  const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
  const uniformEntry = { binding: 0, resource: { buffer: uniformBuffer } };
  return {
    width,
    height,
    tileTexels,
    maximumTiles,
    atlasWidth: cacheUniforms[2],
    atlasHeight: cacheUniforms[3],
    tileCountPending: false,
    tileCountReadback,
    discoveryPipeline,
    fillPipeline,
    discoverySceneBindGroup: device.createBindGroup({
      layout: discoveryPipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
    }),
    discoveryPlaygroundBindGroup: device.createBindGroup({
      layout: discoveryPipeline.getBindGroupLayout(1),
      entries: [{ binding: 0, resource: { buffer: playgroundBuffer } }],
    }),
    discoveryBindGroup: device.createBindGroup({
      layout: discoveryPipeline.getBindGroupLayout(2),
      entries: [
        uniformEntry,
        { binding: 1, resource: { buffer: header } },
        { binding: 2, resource: { buffer: entries } },
        { binding: 3, resource: { buffer: tiles } },
      ],
    }),
    fillSceneBindGroup: device.createBindGroup({
      layout: fillPipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
    }),
    fillPlaygroundBindGroup: device.createBindGroup({
      layout: fillPipeline.getBindGroupLayout(1),
      entries: [{ binding: 0, resource: { buffer: playgroundBuffer } }],
    }),
    fillBindGroup: device.createBindGroup({
      layout: fillPipeline.getBindGroupLayout(2),
      entries: [uniformEntry, { binding: 4, resource: { buffer: tiles } }],
    }),
    sampleBindGroup: device.createBindGroup({
      layout: scenePipeline.getBindGroupLayout(2),
      entries: [
        uniformEntry,
        { binding: 1, resource: { buffer: entries } },
        { binding: 2, resource: atlasView },
        { binding: 3, resource: sampler },
        { binding: 4, resource: { buffer: tiles } },
        { binding: 5, resource: { buffer: header } },
        { binding: 6, resource: { buffer: debugBuffer } },
      ],
    }),
    entries,
    header,
    atlasView,
    resources: [uniformBuffer, tiles, atlas, sampler, tileCountReadback, debugBuffer],
  };
}
const timestampQuerySet = timestampQueriesSupported
  ? device.createQuerySet({ type: "timestamp", count: 4 })
  : undefined;
const timestampResolveBuffer = timestampQueriesSupported
  ? device.createBuffer({ size: 32, usage: 0x0200 | 0x0004 })
  : undefined;
type TimestampReadback = {
  buffer: any;
  pending: boolean;
  pipeline?: any;
  cached?: boolean;
};
const timestampReadbacks: TimestampReadback[] = timestampQueriesSupported
  ? Array.from({ length: 3 }, () => ({
      buffer: device.createBuffer({ size: 32, usage: 0x0001 | 0x0008 }),
      pending: false,
    }))
  : [];
const startupSceneBindGroup = device.createBindGroup({
  layout: startupPipeline.getBindGroupLayout(0),
  entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
});
type RenderResources = {
  pipeline: any;
  sceneBindGroup: any;
  playgroundBindGroup?: any;
  cloudShadowCache?: CloudShadowCache;
};
let sceneRenderResources: RenderResources = {
  pipeline: startupPipeline,
  sceneBindGroup: startupSceneBindGroup,
};
let directSceneRenderResources = sceneRenderResources;
let sceneCompilationStatus = "compiling full scene";
const diagnosticShaderModules = new Map<string, any>();
const diagnosticRenderResources = new Map<DiagnosticSelection, RenderResources>();
const diagnosticPipelinesRequested = new Set<DiagnosticSelection>();

function updateShadowCacheInfo(cache?: CloudShadowCache): void {
  if (!shadowCacheInfoElement) return;
  if (!cache) {
    if (shadowCacheInfoElement.textContent !== "Direct evaluation") {
      shadowCacheInfoElement.textContent = "Direct evaluation";
    }
    return;
  }
  const liveTiles = cache.liveTiles === undefined ? "—" : String(cache.liveTiles);
  const worldTexelM = state.viewHeight / previewCanvas.height;
  const text = `${liveTiles} / ${cache.maximumTiles} live tiles · ${cache.atlasWidth} × ${cache.atlasHeight} atlas · ${formatDistance(worldTexelM)}/texel`;
  if (shadowCacheInfoElement.textContent !== text) shadowCacheInfoElement.textContent = text;
}

function recreateShadowCache(): void {
  const cache = sceneRenderResources.cloudShadowCache;
  if (!cache) return;
  sceneRenderResources.cloudShadowCache = createCloudShadowCache(
    cache.discoveryPipeline,
    cache.fillPipeline,
    sceneRenderResources.pipeline,
  );
  updateShadowCacheInfo(sceneRenderResources.cloudShadowCache);
}

function renderResourcesForFrame(): RenderResources {
  const selection = state.diagnostic;
  if (selection === "scene") {
    return useShadowCache ? sceneRenderResources : directSceneRenderResources;
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

function sampleWorldCenterDiagnostics(worldTimeSeconds: number): WorldCenterDiagnostics {
  const sample = sample_playground_environment(
    state.worldWidthM,
    state.worldHeightM,
    parsedSeed(),
    BigInt(Math.max(0, Math.floor(worldTimeSeconds * 60))),
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
    ["Moon", `${angle(output.moonDirection)} · ${output.moonIntensity.toFixed(2)}`],
    ["Solar eclipse", `${(output.solarEclipseFraction * 100).toFixed(0)} %`],
    ["Lunar penumbra", `${(output.lunarPenumbraFraction * 100).toFixed(0)} %`],
    ["Lunar umbra", `${(output.lunarUmbraFraction * 100).toFixed(0)} %`],
    ["Astronomical ambient", output.ambientIntensity.toFixed(2)],
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

function updateViewCenterDiagnostics(worldTimeSeconds: number): void {
  updateRenderedGeometry();
  replaceDiagnosticRows(positionMeasurementsElement, positionMeasurements);
  updateDerivedOutput(sampleWorldCenterDiagnostics(worldTimeSeconds));
}

function splitTick(timeSeconds: number): [bigint, number] {
  const tickPosition = Math.max(0, timeSeconds * 60);
  const wholeTick = Math.floor(tickPosition);
  return [BigInt(wholeTick), tickPosition - wholeTick];
}

function writeUniforms(
  cloudTimeSeconds: number,
  worldTimeSeconds: number,
  pressureTimeSeconds: number,
  currentTimeSeconds: number,
): void {
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
  const layerMaterial = [enabled("lowCloudColorVisible"), 0, 0, 0] as const;
  const surfaceColorLayer = landMaterial.some(Boolean)
    || solidMaterial.some(Boolean)
    || waterMaterial.some(Boolean);
  const anyColorLayer = surfaceColorLayer || layerMaterial[0] > 0;
  const anyContribution = anyColorLayer && light.some(Boolean);
  const surfaceDirectContribution = surfaceColorLayer
    && (light[0] > 0 || light[1] > 0);
  const intermediate = deriveIntermediateVisibility({
    colorLayer: anyColorLayer,
    sun: light[0] > 0,
    moon: light[1] > 0,
    clearSkyAmbient: light[2] > 0,
    canopyAmbient: light[3] > 0,
  }, state.diagnostic !== "scene");
  const visible = (value: boolean): number => Number(value);
  const aspect = previewCanvas.width / previewCanvas.height;
  sceneFloats.set([
    renderedCameraX,
    renderedCameraY,
    state.viewHeight * aspect,
    state.viewHeight,
  ], 0);
  const [worldTick] = splitTick(worldTimeSeconds);
  const [cloudTick, cloudTickFraction] = splitTick(cloudTimeSeconds);
  const [pressureTick, pressureTickFraction] = splitTick(pressureTimeSeconds);
  const [currentTick, currentTickFraction] = splitTick(currentTimeSeconds);
  const fieldTimes = sample_playground_field_time_coordinates(
    state.worldWidthM,
    state.worldHeightM,
    cloudTick,
    cloudTickFraction,
    pressureTick,
    pressureTickFraction,
    currentTick,
    currentTickFraction,
  );
  sceneFloats.set(fieldTimes.subarray(0, 4), 4);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  const precipitationEpoch = BigInt(Math.floor(cloudTimeSeconds / 1800));
  sceneUints.set([
    seedLow,
    seedHigh,
    Number(precipitationEpoch & 0xffff_ffffn),
    Number((precipitationEpoch >> 32n) & 0xffff_ffffn),
  ], 8);
  sceneFloats.set([
    state.worldWidthM,
    state.worldHeightM,
    1 / state.worldWidthM,
    1 / state.worldHeightM,
  ], 12);
  sceneFloats.set([
    (cloudTimeSeconds % 1800) / 1800,
    0,
    0,
    0,
  ], 16);
  const [dayPhase, seasonPhase, lunarLongitudePhase] = worldPhases(worldTimeSeconds);
  sceneFloats.set([
    dayPhase,
    seasonPhase,
    lunarLongitudePhase,
    state.viewHeight / previewCanvas.height,
  ], 20);
  sceneUints.set([
    Number(worldTick & 0xffff_ffffn),
    Number((worldTick >> 32n) & 0xffff_ffffn),
    0,
    0,
  ], 24);
  sceneFloats.set(fieldTimes.subarray(4, 8), 28);
  sceneFloats.set(fieldTimes.subarray(8, 12), 32);
  sceneFloats.set(sample_playground_seasonal_phase_factors(seasonPhase), 36);
  sceneFloats.set(sample_playground_celestial_orbit(
    seasonPhase,
    lunarLongitudePhase,
  ), 40);
  const viewCloud = sample_playground_view_cloud_state(
    state.worldWidthM,
    state.worldHeightM,
    (BigInt(seedHigh) << 32n) | BigInt(seedLow),
    cloudTick,
    cloudTickFraction,
    renderedCameraX,
    renderedCameraY,
  );
  const lowWind = [viewCloud[0], viewCloud[1]];
  const upperWind = [viewCloud[2], viewCloud[3]];
  if (previousCloudTimeSeconds === undefined || cloudTimeSeconds < previousCloudTimeSeconds) {
    lowCloudMotionM.fill(0);
    upperCloudMotionM.fill(0);
  } else {
    const elapsedSeconds = cloudTimeSeconds - previousCloudTimeSeconds;
    for (let axis = 0; axis < 2; axis += 1) {
      lowCloudMotionM[axis] += 0.5 * elapsedSeconds
        * (previousLowCloudWindMS[axis] + lowWind[axis]);
      upperCloudMotionM[axis] += 0.5 * elapsedSeconds
        * (previousUpperCloudWindMS[axis] + upperWind[axis]);
    }
  }
  previousCloudTimeSeconds = cloudTimeSeconds;
  previousLowCloudWindMS = lowWind;
  previousUpperCloudWindMS = upperWind;
  sceneFloats.set([...lowCloudMotionM, ...upperCloudMotionM], 48);
  sceneFloats.set(viewCloud.subarray(4, 8), 52);
  device.queue.writeBuffer(sceneBuffer, 0, sceneFloats);

  tuningFloats.set([
    state.detailCharacter, state.detailBlobiness, state.detailContrast, state.detailEdgeSoftness,
  ], 0);
  tuningFloats.set([
    state.maxWispCoverage,
    state.cloudNormalStrength,
    state.lowCloudBodySize,
    state.lowCloudOccurrence,
  ], 4);
  tuningFloats.set([state.opticalDepth, state.opacity, state.darkness, 0], 8);
  tuningFloats.set([state.terrainScale, state.elevationScale, state.cliffSlope, 0], 12);
  tuningFloats.set([
    state.waterAttenuationLength,
    state.bottomVisibility,
    state.bottomReflectance,
    state.bottomVeil,
  ], 16);
  tuningFloats.set([
    state.landformReliefStrength,
    state.landformShadowsVisible * state.landformShadowStrength,
    state.landformShadowReach,
    0,
  ], 20);
  tuningFloats.set([
    state.cloudEdgeLightStrength,
    state.cloudSelfShadowVisible * state.cloudSelfShadowStrength,
    0,
    0,
  ], 24);
  tuningFloats.set([
    state.lowCloudShadowsVisible * state.lowCloudShadowStrength,
    state.canopyShadowsVisible * state.canopyShadowStrength,
    state.cloudShadowDensity,
    state.cloudShadowOpacity,
  ], 28);
  tuningFloats.set([
    state.waterWaveNormalStrength,
    state.waterCapillaryNormalStrength,
    state.waterRainNormalStrength,
    state.waterRoughnessScale,
  ], 32);
  tuningFloats.set([
    state.landIceNormalStrength,
    state.landIceRoughness,
    state.seaIceNormalStrength,
    state.seaIceRoughnessScale,
  ], 36);
  tuningFloats.set([
    state.seaIcePlateSize,
    state.seaIceFractureWidth,
    state.seaIceFractureContrast,
    state.seaIceShallowTint,
  ], 40);
  tuningFloats.set([
    state.rockNormalStrength,
    state.grassNormalStrength,
    state.forestNormalStrength,
    state.snowNormalStrength,
  ], 44);
  tuningFloats.set([
    state.forestCoverage,
    state.rockCoverage,
    state.beachCoverage,
    state.snowCoverage,
  ], 48);
  tuningFloats.set([
    state.sunLightStrength,
    state.moonLightStrength,
    state.skyLightStrength,
    state.canopyLightStrength,
  ], 52);
  tuningFloats.set([
    Number(anyContribution),
    Number(surfaceDirectContribution),
    Number(surfaceDirectContribution),
    Number(layerMaterial[0] > 0 && light.some(Boolean)),
  ], 56);
  tuningFloats.set([
    1,
    1,
    1,
    1,
  ], 60);
  tuningFloats.set([
    state.pressureWindScale,
    state.crossIsobarScale,
    state.thermalWindScale,
    state.shelterStrength,
  ], 64);
  tuningFloats.set([
    1,
    1,
    0,
    1,
  ], 68);
  tuningUints.set(light, 72);
  tuningUints.set(landMaterial, 76);
  tuningUints.set(solidMaterial, 80);
  tuningUints.set(waterMaterial, 84);
  tuningUints.set(layerMaterial, 88);
  tuningFloats.set([
    visible(intermediate.pressure),
    visible(intermediate.temperature),
    visible(intermediate.moisture),
    visible(intermediate.weather),
  ], 92);
  tuningFloats.set([
    visible(intermediate.materials),
    visible(intermediate.oceanForcing),
    visible(intermediate.waveSpectrum),
    visible(intermediate.materials),
  ], 96);
  tuningFloats.set([
    visible(intermediate.materials),
    visible(intermediate.materials),
    0,
    0,
  ], 100);
  device.queue.writeBuffer(playgroundBuffer, 0, tuningFloats);
}

function resize(): void {
  const scale = Math.min(devicePixelRatio, 2);
  const width = Math.max(1, Math.floor(previewCanvas.clientWidth * scale));
  const height = Math.max(1, Math.floor(previewCanvas.clientHeight * scale));
  if (previewCanvas.width !== width || previewCanvas.height !== height) {
    previewCanvas.width = width;
    previewCanvas.height = height;
    const cache = sceneRenderResources.cloudShadowCache;
    if (cache) {
      recreateShadowCache();
    }
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
let discoveryGpuMillisecondsInWindow = 0;
let fillGpuMillisecondsInWindow = 0;
let sceneGpuMillisecondsInWindow = 0;
let gpuSamplesInWindow = 0;
let measuredDiscoveryGpuMilliseconds: number | undefined;
let measuredFillGpuMilliseconds: number | undefined;
let measuredSceneGpuMilliseconds: number | undefined;
let timedPipeline: any;
let timedCache: CloudShadowCache | undefined;
let timedUsesShadowCache = false;
let nextDiagnosticsAt = 0;
let nextShadowCacheInfoAt = 0;

function updateStatus(now: number, ticks: number, cloudTimeSeconds: number): void {
  updateShadowCacheInfo(renderResourcesForFrame().cloudShadowCache);
  framesInWindow += 1;
  ticksInWindow += ticks;
  const windowSeconds = (now - statisticsStartedAt) / 1000;
  if (windowSeconds < 1) return;
  measuredFramesPerSecond = framesInWindow / windowSeconds;
  measuredTicksPerSecond = ticksInWindow / windowSeconds;
  if (gpuSamplesInWindow > 0) {
    measuredGpuMilliseconds = gpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredDiscoveryGpuMilliseconds = discoveryGpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredFillGpuMilliseconds = fillGpuMillisecondsInWindow / gpuSamplesInWindow;
    measuredSceneGpuMilliseconds = sceneGpuMillisecondsInWindow / gpuSamplesInWindow;
  }
  statisticsStartedAt = now;
  framesInWindow = 0;
  ticksInWindow = 0;
  gpuMillisecondsInWindow = 0;
  discoveryGpuMillisecondsInWindow = 0;
  fillGpuMillisecondsInWindow = 0;
  sceneGpuMillisecondsInWindow = 0;
  gpuSamplesInWindow = 0;
  const rates = `${measuredFramesPerSecond.toFixed(0)} FPS · ${measuredTicksPerSecond.toFixed(0)} TPS`;
  const gpuTiming = measuredGpuMilliseconds === undefined
    ? timestampQueriesSupported ? " · measuring GPU" : ""
    : timedUsesShadowCache && measuredDiscoveryGpuMilliseconds !== undefined
      && measuredFillGpuMilliseconds !== undefined && measuredSceneGpuMilliseconds !== undefined
    ? ` · ${measuredGpuMilliseconds.toFixed(1)} ms GPU (${measuredDiscoveryGpuMilliseconds.toFixed(1)} discover + ${measuredFillGpuMilliseconds.toFixed(1)} fill + ${measuredSceneGpuMilliseconds.toFixed(1)} scene)`
    : ` · ${measuredGpuMilliseconds.toFixed(1)} ms GPU scene`;
  const compilation = sceneCompilationStatus ? ` · ${sceneCompilationStatus}` : "";
  statusElement.textContent = `${rates}${gpuTiming}${compilation}`;
  statusElement.title = `${adapterName} · ${previewCanvas.width} × ${previewCanvas.height} · ${format(cloudTimeSeconds, 1)} s`;
  syncWorldDateInputs();
}

function frame(now: number): void {
  resize();
  updateScaleBar();
  const elapsed = Math.max(0, (now - previous) / 1000);
  previous = now;
  const ticks = advanceFixedClock(clock, elapsed, () => {
    state.time += fixedTickSeconds * state.cloudTimeScale;
    state.pressureTimeSeconds += fixedTickSeconds * state.pressureTimeScale;
    state.currentTimeSeconds += fixedTickSeconds * state.currentTimeScale;
    state.worldTimeSeconds += fixedTickSeconds * state.worldTimeScale;
  });
  const renderFractionSeconds = fixedClockFraction(clock) * fixedTickSeconds;
  const cloudTimeSeconds = state.time + renderFractionSeconds * state.cloudTimeScale;
  const pressureTimeSeconds = state.pressureTimeSeconds
    + renderFractionSeconds * state.pressureTimeScale;
  const currentTimeSeconds = state.currentTimeSeconds
    + renderFractionSeconds * state.currentTimeScale;
  const worldTimeSeconds = state.worldTimeSeconds
    + renderFractionSeconds * state.worldTimeScale;
  updateRenderedGeometry();
  if (diagnosticsDetailsElement.open && now >= nextDiagnosticsAt) {
    updateViewCenterDiagnostics(worldTimeSeconds);
    nextDiagnosticsAt = now + 250;
  }
  writeUniforms(
    cloudTimeSeconds,
    worldTimeSeconds,
    pressureTimeSeconds,
    currentTimeSeconds,
  );
  const encoder = device.createCommandEncoder();
  const renderResources = renderResourcesForFrame();
  if (renderResources.pipeline !== timedPipeline || renderResources.cloudShadowCache !== timedCache) {
    timedPipeline = renderResources.pipeline;
    timedCache = renderResources.cloudShadowCache;
    timedUsesShadowCache = Boolean(renderResources.cloudShadowCache);
    measuredGpuMilliseconds = undefined;
    measuredDiscoveryGpuMilliseconds = undefined;
    measuredFillGpuMilliseconds = undefined;
    measuredSceneGpuMilliseconds = undefined;
    gpuMillisecondsInWindow = 0;
    discoveryGpuMillisecondsInWindow = 0;
    fillGpuMillisecondsInWindow = 0;
    sceneGpuMillisecondsInWindow = 0;
    gpuSamplesInWindow = 0;
  }
  const timestampReadback = timestampReadbacks.find((readback) => !readback.pending);
  const cache = renderResources.cloudShadowCache;
  if (cache) {
    encoder.clearBuffer(cache.entries);
    encoder.clearBuffer(cache.header, 4, 4);
    const discoveryPassDescriptor: any = {};
    if (timestampReadback) {
      discoveryPassDescriptor.timestampWrites = {
        querySet: timestampQuerySet,
        beginningOfPassWriteIndex: 0,
        endOfPassWriteIndex: 1,
      };
    }
    const discoveryPass = encoder.beginComputePass(discoveryPassDescriptor);
    discoveryPass.setPipeline(cache.discoveryPipeline);
    discoveryPass.setBindGroup(0, cache.discoverySceneBindGroup);
    discoveryPass.setBindGroup(1, cache.discoveryPlaygroundBindGroup);
    discoveryPass.setBindGroup(2, cache.discoveryBindGroup);
    discoveryPass.dispatchWorkgroups(
      Math.ceil(previewCanvas.width / 128),
      Math.ceil(previewCanvas.height / 128),
    );
    discoveryPass.end();
    const fillPassDescriptor: any = {
      colorAttachments: [{
        view: cache.atlasView,
        ...(shadowCacheDebugMode === 3
          ? { clearValue: { r: 0, g: 0, b: 0, a: 0 }, loadOp: "clear" }
          : { loadOp: "load" }),
        storeOp: "store",
      }],
    };
    if (timestampReadback) {
      fillPassDescriptor.timestampWrites = {
        querySet: timestampQuerySet,
        endOfPassWriteIndex: 2,
      };
    }
    const fillPass = encoder.beginRenderPass(fillPassDescriptor);
    fillPass.setPipeline(cache.fillPipeline);
    fillPass.setBindGroup(0, cache.fillSceneBindGroup);
    fillPass.setBindGroup(1, cache.fillPlaygroundBindGroup);
    fillPass.setBindGroup(2, cache.fillBindGroup);
    fillPass.drawIndirect(cache.header, 0);
    fillPass.end();
  }
  const renderPassDescriptor: any = {
    colorAttachments: [{
      view: context.getCurrentTexture().createView(),
      clearValue: { r: 0.02, g: 0.08, b: 0.1, a: 1 },
      loadOp: "clear",
      storeOp: "store",
    }],
  };
  if (timestampReadback) {
    renderPassDescriptor.timestampWrites = {
      querySet: timestampQuerySet,
      ...(cache ? {} : { beginningOfPassWriteIndex: 0 }),
      endOfPassWriteIndex: cache ? 3 : 1,
    };
  }
  const pass = encoder.beginRenderPass(renderPassDescriptor);
  pass.setPipeline(renderResources.pipeline);
  pass.setBindGroup(0, renderResources.sceneBindGroup);
  if (renderResources.playgroundBindGroup) {
    pass.setBindGroup(1, renderResources.playgroundBindGroup);
  }
  if (cache) pass.setBindGroup(2, cache.sampleBindGroup);
  pass.setVertexBuffer(0, vertexBuffer);
  pass.draw(6);
  pass.end();
  let cacheInfoReadback: CloudShadowCache | undefined;
  if (cache && now >= nextShadowCacheInfoAt && !cache.tileCountPending) {
    encoder.copyBufferToBuffer(cache.header, 0, cache.tileCountReadback, 0, 16);
    cache.tileCountPending = true;
    cacheInfoReadback = cache;
    nextShadowCacheInfoAt = now + 500;
  }
  if (timestampReadback) {
    const timestampCount = cache ? 4 : 2;
    const timestampBytes = timestampCount * 8;
    encoder.resolveQuerySet(timestampQuerySet, 0, timestampCount, timestampResolveBuffer, 0);
    encoder.copyBufferToBuffer(
      timestampResolveBuffer,
      0,
      timestampReadback.buffer,
      0,
      timestampBytes,
    );
    timestampReadback.pending = true;
    timestampReadback.pipeline = renderResources.pipeline;
    timestampReadback.cached = Boolean(cache);
  }
  device.queue.submit([encoder.finish()]);
  if (cacheInfoReadback) {
    const readbackCache = cacheInfoReadback;
    void readbackCache.tileCountReadback.mapAsync(0x0001).then(() => {
      readbackCache.liveTiles = new Uint32Array(
        readbackCache.tileCountReadback.getMappedRange(),
      )[1];
      readbackCache.tileCountReadback.unmap();
      if (sceneRenderResources.cloudShadowCache === readbackCache) {
        updateShadowCacheInfo(readbackCache);
      }
    }).catch(() => {}).finally(() => {
      readbackCache.tileCountPending = false;
    });
  }
  if (timestampReadback) {
    void timestampReadback.buffer.mapAsync(0x0001).then(() => {
      const timestamps = new BigUint64Array(timestampReadback.buffer.getMappedRange());
      const startedAt = timestamps[0];
      const cached = timestampReadback.cached;
      const finishedAt = timestamps[cached ? 3 : 1];
      if (
        timestampReadback.pipeline === timedPipeline
        && startedAt > 0n
        && finishedAt >= startedAt
      ) {
        gpuMillisecondsInWindow += Number(finishedAt - startedAt) / 1_000_000;
        if (cached) {
          discoveryGpuMillisecondsInWindow += Number(timestamps[1] - timestamps[0]) / 1_000_000;
          fillGpuMillisecondsInWindow += Number(timestamps[2] - timestamps[1]) / 1_000_000;
          sceneGpuMillisecondsInWindow += Number(timestamps[3] - timestamps[2]) / 1_000_000;
        } else {
          sceneGpuMillisecondsInWindow += Number(timestamps[1] - timestamps[0]) / 1_000_000;
        }
        gpuSamplesInWindow += 1;
      }
      timestampReadback.buffer.unmap();
    }).catch(() => {}).finally(() => {
      timestampReadback.pending = false;
    });
  }
  updateStatus(now, ticks, cloudTimeSeconds);
  requestAnimationFrame(frame);
}

requestAnimationFrame(frame);

void new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  .then(async () => {
    const compilationStartedAt = performance.now();
    const sceneShader = device.createShaderModule({ code: playground_scene_shader_source() });
    const directSceneShader = device.createShaderModule({
      code: playground_direct_scene_shader_source(),
    });
    const cacheShader = device.createShaderModule({
      code: playground_cloud_shadow_cache_shader_source(),
    });
    const [scenePipeline, directScenePipeline, discoveryPipeline, fillPipeline] = await Promise.all([
      createPlaygroundPipeline(sceneShader, "fragment_playground_scene"),
      createPlaygroundPipeline(directSceneShader, "fragment_playground_scene"),
      device.createComputePipelineAsync({
        layout: "auto",
        compute: {
          module: cacheShader,
          entryPoint: "discover_cloud_shadow_cache_tiles",
        },
      }),
      device.createRenderPipelineAsync({
        layout: "auto",
        vertex: { module: cacheShader, entryPoint: "cloud_shadow_cache_vertex" },
        fragment: {
          module: cacheShader,
          entryPoint: "cloud_shadow_cache_fragment",
          targets: [{ format: "rg16float" }],
        },
        primitive: { topology: "triangle-list" },
      }),
    ]);
    sceneRenderResources = {
      pipeline: scenePipeline,
      sceneBindGroup: device.createBindGroup({
        layout: scenePipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
      }),
      playgroundBindGroup: device.createBindGroup({
        layout: scenePipeline.getBindGroupLayout(1),
        entries: [{ binding: 0, resource: { buffer: playgroundBuffer } }],
      }),
      cloudShadowCache: createCloudShadowCache(
        discoveryPipeline,
        fillPipeline,
        scenePipeline,
      ),
    };
    directSceneRenderResources = {
      pipeline: directScenePipeline,
      sceneBindGroup: device.createBindGroup({
        layout: directScenePipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: { buffer: sceneBuffer } }],
      }),
      playgroundBindGroup: device.createBindGroup({
        layout: directScenePipeline.getBindGroupLayout(1),
        entries: [{ binding: 0, resource: { buffer: playgroundBuffer } }],
      }),
    };
    sceneCompilationStatus = `full scene ready in ${((performance.now() - compilationStartedAt) / 1000).toFixed(1)} s`;
  })
  .catch((error) => {
    sceneCompilationStatus = `scene shader failed: ${String(error)}`;
    console.error(error);
  });
