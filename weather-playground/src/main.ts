import shaderSource from "../../crates/browser/src/scene.wgsl?raw";
import "./style.css";

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
  highCloud: number;
  lowWindSpeed: number;
  lowWindDirection: number;
  highWindSpeed: number;
  highWindDirection: number;
  cloudScale: number;
  cloudDetail: number;
  stratusLength: number;
  cirrusLength: number;
  opticalDepth: number;
  opacity: number;
  darkness: number;
  relief: number;
  terrainScale: number;
  elevationScale: number;
  cliffSlope: number;
};

type NumericStateKey = Exclude<keyof State, "seed" | "paused">;

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
  highCloud: 0.04,
  lowWindSpeed: 6.5,
  lowWindDirection: 9,
  highWindSpeed: 7.1,
  highWindDirection: 9,
  cloudScale: 1,
  cloudDetail: 1,
  stratusLength: 5,
  cirrusLength: 12.5,
  opticalDepth: 1,
  opacity: 1,
  darkness: 1,
  relief: 1,
  terrainScale: 1,
  elevationScale: 1,
  cliffSlope: 0.08,
};

const state: State = { ...defaults };
const canvas = document.querySelector<HTMLCanvasElement>("#preview");
const controls = document.querySelector<HTMLElement>("#controls");
const presets = document.querySelector<HTMLElement>("#presets");
const status = document.querySelector<HTMLElement>("#status");
if (!canvas || !controls || !presets || !status) throw new Error("Playground page is incomplete");
const previewCanvas = canvas;
const controlsElement = controls;
const presetsElement = presets;
const statusElement = status;

type Range = {
  key: NumericStateKey;
  label: string;
  min: number;
  max: number;
  step: number;
};

const groups: Array<[string, Range[]]> = [
  ["View", [
    { key: "cameraX", label: "Camera X (m)", min: -100000000, max: 100000000, step: 10 },
    { key: "cameraY", label: "Camera Y (m)", min: -100000000, max: 100000000, step: 10 },
    { key: "viewHeight", label: "View height (m)", min: 0.1, max: 10000000, step: 0.1 },
    { key: "time", label: "Time (s)", min: 0, max: 86400, step: 1 },
    { key: "timeRate", label: "Time rate", min: 0, max: 20, step: 0.1 },
  ]],
  ["Weather", [
    { key: "cover", label: "Cloud cover", min: 0, max: 1, step: 0.01 },
    { key: "moisture", label: "Moisture", min: 0, max: 1, step: 0.01 },
    { key: "instability", label: "Instability", min: 0, max: 1, step: 0.01 },
    { key: "highCloud", label: "High cloud", min: 0, max: 1, step: 0.01 },
    { key: "rain", label: "Rain", min: 0, max: 1, step: 0.01 },
  ]],
  ["Layer winds", [
    { key: "lowWindSpeed", label: "Low speed (m/s)", min: 0, max: 30, step: 0.1 },
    { key: "lowWindDirection", label: "Low direction (°)", min: -180, max: 180, step: 1 },
    { key: "highWindSpeed", label: "High speed (m/s)", min: 0, max: 40, step: 0.1 },
    { key: "highWindDirection", label: "High direction (°)", min: -180, max: 180, step: 1 },
  ]],
  ["Cloud noise", [
    { key: "cloudScale", label: "Feature scale", min: 0.25, max: 4, step: 0.05 },
    { key: "cloudDetail", label: "Fine detail", min: 0, max: 2, step: 0.05 },
    { key: "stratusLength", label: "Stratus length", min: 1, max: 16, step: 0.25 },
    { key: "cirrusLength", label: "Cirrus length", min: 1, max: 30, step: 0.5 },
  ]],
  ["Cloud material", [
    { key: "opticalDepth", label: "Optical depth", min: 0, max: 3, step: 0.05 },
    { key: "opacity", label: "Opacity", min: 0, max: 1.5, step: 0.05 },
    { key: "darkness", label: "Darkness", min: 0, max: 2, step: 0.05 },
    { key: "relief", label: "Relief", min: 0, max: 2, step: 0.05 },
  ]],
  ["Terrain", [
    { key: "terrainScale", label: "Horizontal scale", min: 0.25, max: 4, step: 0.05 },
    { key: "elevationScale", label: "Elevation scale", min: 0.1, max: 3, step: 0.05 },
    { key: "cliffSlope", label: "Cliff slope", min: 0.01, max: 0.3, step: 0.005 },
  ]],
];

const rangeInputs = new Map<NumericStateKey, HTMLInputElement>();
const rangeOutputs = new Map<NumericStateKey, HTMLOutputElement>();
const rangesByKey = new Map<NumericStateKey, Range>();
for (const [title, ranges] of groups) {
  const fieldset = document.createElement("fieldset");
  const legend = document.createElement("legend");
  legend.textContent = title;
  fieldset.append(legend);
  for (const range of ranges) {
    const label = document.createElement("label");
    const name = document.createElement("span");
    const input = document.createElement("input");
    const output = document.createElement("output");
    name.textContent = range.label;
    input.type = "range";
    input.min = String(range.min);
    input.max = String(range.max);
    input.step = String(range.step);
    input.value = String(state[range.key]);
    output.value = format(Number(input.value), range.step);
    input.addEventListener("input", () => {
      (state[range.key] as number) = Number(input.value);
      output.value = format(Number(input.value), range.step);
    });
    label.append(name, input, output);
    fieldset.append(label);
    rangeInputs.set(range.key, input);
    rangeOutputs.set(range.key, output);
    rangesByKey.set(range.key, range);
  }
  controlsElement.append(fieldset);
}

const seedFieldset = document.createElement("fieldset");
seedFieldset.innerHTML = `<legend>Seed and playback</legend><label><span>Seed</span><input id="seed" type="text" value="${state.seed}"><output></output></label><div class="actions"><button id="pause">Pause</button><button id="reset">Reset all</button><button id="zero-offset">Reset drift</button></div>`;
controlsElement.prepend(seedFieldset);
const seedInput = seedFieldset.querySelector<HTMLInputElement>("#seed")!;
const pauseButton = seedFieldset.querySelector<HTMLButtonElement>("#pause")!;
seedInput.addEventListener("change", () => { state.seed = seedInput.value; });
pauseButton.addEventListener("click", () => {
  state.paused = !state.paused;
  pauseButton.textContent = state.paused ? "Resume" : "Pause";
});

let lowOffset = [0, 0];
let highOffset = [0, 0];
seedFieldset.querySelector<HTMLButtonElement>("#zero-offset")!.addEventListener("click", () => {
  lowOffset = [0, 0];
  highOffset = [0, 0];
});
seedFieldset.querySelector<HTMLButtonElement>("#reset")!.addEventListener("click", () => {
  Object.assign(state, defaults);
  lowOffset = [0, 0];
  highOffset = [0, 0];
  seedInput.value = state.seed;
  pauseButton.textContent = "Pause";
  syncControls();
});

const weatherPresets: Record<string, Partial<State>> = {
  Fair: { cover: 0.48, moisture: 0.48, instability: 0.82, highCloud: 0.04, rain: 0 },
  Stratus: { cover: 0.95, moisture: 0.98, instability: 0.08, highCloud: 0.05, rain: 0 },
  Cirrus: { cover: 0, moisture: 0.28, instability: 0.35, highCloud: 0.96, rain: 0 },
  Squall: { cover: 0.64, moisture: 0.92, instability: 0.62, highCloud: 0.18, rain: 0.62 },
  Storm: { cover: 0.76, moisture: 0.98, instability: 0.96, highCloud: 0.28, rain: 0.9 },
};
for (const [name, values] of Object.entries(weatherPresets)) {
  const button = document.createElement("button");
  button.textContent = name;
  button.addEventListener("click", () => { Object.assign(state, values); syncControls(); });
  presetsElement.append(button);
}

function format(value: number, step: number): string {
  return value.toFixed(step < 0.01 ? 3 : step < 1 ? 2 : 0);
}

function syncControls(): void {
  for (const key of rangeInputs.keys()) {
    syncControl(key);
  }
}

function syncControl(key: NumericStateKey): void {
  const value = state[key];
  rangeInputs.get(key)!.value = String(value);
  rangeOutputs.get(key)!.value = format(value, rangesByKey.get(key)!.step);
}

function syncNavigationControls(): void {
  syncControl("cameraX");
  syncControl("cameraY");
  syncControl("viewHeight");
}

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
  syncNavigationControls();
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
  const newHeight = Math.min(10000000, Math.max(0.1,
    oldHeight * Math.exp(event.deltaY * deltaScale * 0.001),
  ));
  const newWidth = newHeight * rect.width / rect.height;
  state.cameraX += pointerX * (oldWidth - newWidth);
  state.cameraY += pointerY * (oldHeight - newHeight);
  state.viewHeight = newHeight;
  syncNavigationControls();
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
  fragment: { module: shader, entryPoint: "fragment", targets: [{ format: formatName }] },
  primitive: { topology: "triangle-list" },
});

const vertices = new Float32Array([
  -1, -1, 0, 0, 0, 1, 1, -1, 0, 0, 0, 1, -1, 1, 0, 0, 0, 1,
  -1, 1, 0, 0, 0, 1, 1, -1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1,
]);
const vertexBuffer = device.createBuffer({ size: vertices.byteLength, usage: 0x20 | 0x08 });
device.queue.writeBuffer(vertexBuffer, 0, vertices);
const uniformBuffer = device.createBuffer({ size: 160, usage: 0x40 | 0x08 });
const bindGroup = device.createBindGroup({
  layout: pipeline.getBindGroupLayout(0),
  entries: [{ binding: 0, resource: { buffer: uniformBuffer } }],
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

function writeUniforms(): void {
  const buffer = new ArrayBuffer(160);
  const floats = new Float32Array(buffer);
  const uints = new Uint32Array(buffer);
  const aspect = previewCanvas.width / previewCanvas.height;
  const low = wind(state.lowWindSpeed, state.lowWindDirection);
  const high = wind(state.highWindSpeed, state.highWindDirection);
  floats.set([state.cameraX, state.cameraY, state.viewHeight * aspect, state.viewHeight], 0);
  floats.set([low[0], low[1], state.cover, state.rain], 4);
  floats.set([state.moisture, state.instability, state.highCloud, 0], 8);
  floats.set([...low, ...high], 12);
  const [seedLow, seedHigh] = splitSeed(state.seed);
  uints.set([
    seedLow,
    seedHigh,
    seedLow ^ 0x6174,
    seedHigh ^ 0x7079626f,
  ], 16);
  floats.set([...lowOffset, ...highOffset], 20);
  floats.set([state.cloudScale, state.cloudDetail, state.stratusLength, state.cirrusLength], 24);
  floats.set([state.opticalDepth, state.opacity, state.darkness, state.relief], 28);
  floats.set([state.terrainScale, state.elevationScale, state.cliffSlope, 0], 32);
  floats[36] = state.time;
  device.queue.writeBuffer(uniformBuffer, 0, buffer);
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

let previous = performance.now();
function frame(now: number): void {
  resize();
  const elapsed = Math.min((now - previous) / 1000, 0.1);
  previous = now;
  if (!state.paused) {
    const simulationElapsed = elapsed * state.timeRate;
    state.time += simulationElapsed;
    const low = wind(state.lowWindSpeed, state.lowWindDirection);
    const high = wind(state.highWindSpeed, state.highWindDirection);
    lowOffset = [
      lowOffset[0] + low[0] * simulationElapsed,
      lowOffset[1] + low[1] * simulationElapsed,
    ];
    highOffset = [
      highOffset[0] + high[0] * simulationElapsed,
      highOffset[1] + high[1] * simulationElapsed,
    ];
    syncControl("time");
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
  pass.setBindGroup(0, bindGroup);
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
