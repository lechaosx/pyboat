import {
  PlaygroundBoatDriver, PlaygroundBoatImage, playground_boat_image_count,
  playground_boat_shader_source, playground_boat_composite_shader_source, playground_boat_shadow_shader_source,
} from "browser/playground";

export type BoatWorld = { width: number; height: number; seed: bigint; tick: bigint };
export type BoatView = { x: number; y: number; height: number; cameraHeight: number; outputWidth: number; outputHeight: number; sceneHeight: number; time: number };

export class DrivingBoat {
  driver: PlaygroundBoatDriver | undefined;
  readonly element = document.createElement("section");
  readonly follow: HTMLInputElement;
  readonly readout: HTMLElement;
  readonly message: HTMLElement;
  private readonly sailor: HTMLSelectElement;
  private readonly helm: HTMLInputElement;
  private readonly restart: HTMLButtonElement;
  private readonly remove: HTMLButtonElement;
  private readonly pause: HTMLButtonElement;
  private readonly keys = new Set<string>();
  private pointer: { id: number; x: number; y: number; dx: number; dy: number } | undefined;
  private readonly stick = document.createElement("div");
  private world: BoatWorld | undefined;
  private start: { world: BoatWorld; center: [number, number] } | undefined;

  private readonly getWorld: () => BoatWorld;
  private readonly getCenter: () => [number, number];
  private readonly spawned: (position: [number, number], tick: bigint) => void;
  private readonly playback: (active: boolean, paused: boolean) => void;
  constructor(
    canvas: HTMLCanvasElement,
    getWorld: () => BoatWorld,
    getCenter: () => [number, number],
    spawned: (position: [number, number], tick: bigint) => void,
    playback: (active: boolean, paused: boolean) => void,
  ) {
    this.getWorld = getWorld; this.getCenter = getCenter; this.spawned = spawned; this.playback = playback;
    this.element.className = "panel-section boat-controls";
    this.element.innerHTML = `<h2>Drive a boat</h2>
      <div class="boat-actions"><button id="boat-spawn">Spawn / respawn</button><button id="boat-restart" disabled>Repeat start</button><button id="boat-remove" disabled>Remove</button><button id="boat-pause" disabled>Pause</button></div>
      <label for="boat-sailor">Sailor <select id="boat-sailor"><option value="simple">Simple sailor</option><option value="trim">Force trim</option></select></label>
      <label><input id="boat-loaded-helm" type="checkbox"> Load-aware helm</label>
      <p>Force trim meters sail force. Strong forward input can bring the boom across. Load-aware helm limits rudder travel under load.</p>
      <label><input id="boat-follow" type="checkbox" checked> Follow boat / drive with pointer</label>
      <p>W/S power · A/D turn · gamepad or drag to steer toward a world direction. Disable follow to pan.</p>
      <p>Driving uses real time; pause remains available.</p>
      <output id="boat-readout"></output><p id="boat-message" role="status"></p>`;
    this.follow = this.element.querySelector<HTMLInputElement>("#boat-follow")!;
    this.readout = this.element.querySelector<HTMLElement>("#boat-readout")!;
    this.message = this.element.querySelector<HTMLElement>("#boat-message")!;
    this.sailor = this.element.querySelector<HTMLSelectElement>("#boat-sailor")!;
    this.helm = this.element.querySelector<HTMLInputElement>("#boat-loaded-helm")!;
    this.restart = this.element.querySelector<HTMLButtonElement>("#boat-restart")!;
    this.remove = this.element.querySelector<HTMLButtonElement>("#boat-remove")!;
    this.pause = this.element.querySelector<HTMLButtonElement>("#boat-pause")!;
    this.sailor.addEventListener("change", () => this.applySailorOptions());
    this.helm.addEventListener("change", () => this.applySailorOptions());
    this.element.querySelector("#boat-spawn")!.addEventListener("click", () => {
      this.spawn(this.getWorld(), this.getCenter(), false);
    });
    this.restart.addEventListener("click", () => {
      if (this.start) this.spawn(this.start.world, this.start.center, this.pause.textContent === "Resume");
    });
    this.remove.addEventListener("click", () => {
      this.driver?.free(); this.driver = undefined; this.clearInput();
      this.start = undefined;
      this.restart.disabled = this.remove.disabled = this.pause.disabled = true;
      this.readout.textContent = ""; this.message.textContent = "";
      this.playback(false, false);
    });
    this.pause.addEventListener("click", () => {
      this.setPaused(this.pause.textContent === "Pause");
    });
    window.addEventListener("keydown", (event) => {
      if (!this.driver || (event.target instanceof HTMLElement && event.target.closest("input, select, textarea"))) return;
      if (["KeyW", "KeyS", "KeyA", "KeyD"].includes(event.code)) {
        this.keys.add(event.code); event.preventDefault();
      }
    });
    window.addEventListener("keyup", (event) => this.keys.delete(event.code));
    window.addEventListener("blur", () => this.clearInput());
    document.addEventListener("visibilitychange", () => { if (document.hidden) this.clearInput(); });
    window.addEventListener("focusin", (event) => { if (event.target instanceof HTMLElement && event.target.closest("input, select, textarea")) this.clearInput(); });
    this.follow.addEventListener("change", () => this.clearInput());
    this.stick.className = "boat-stick"; this.stick.hidden = true;
    this.stick.innerHTML = "<span></span>";
    canvas.parentElement!.append(this.stick);
    canvas.addEventListener("pointerdown", (event) => {
      if (!this.driving || event.button !== 0 || this.pointer) return;
      this.pointer = { id: event.pointerId, x: event.clientX, y: event.clientY, dx: 0, dy: 0 };
      canvas.setPointerCapture(event.pointerId);
      this.stick.hidden = false;
      const rect = canvas.parentElement!.getBoundingClientRect();
      this.stick.style.left = `${event.clientX - rect.left}px`;
      this.stick.style.top = `${event.clientY - rect.top}px`;
    });
    canvas.addEventListener("pointermove", (event) => {
      if (this.pointer?.id !== event.pointerId) return;
      const dx = (event.clientX - this.pointer.x) / 64;
      const dy = (this.pointer.y - event.clientY) / 64;
      const scale = Math.max(1, Math.hypot(dx, dy));
      this.pointer.dx = dx / scale; this.pointer.dy = dy / scale;
      this.stick.firstElementChild!.setAttribute("style", `transform: translate(${this.pointer.dx * 48}px, ${-this.pointer.dy * 48}px)`);
    });
    const release = (event: PointerEvent) => { if (this.pointer?.id === event.pointerId) { this.pointer = undefined; this.stick.hidden = true; } };
    canvas.addEventListener("pointerup", release);
    canvas.addEventListener("pointercancel", release);
    canvas.addEventListener("lostpointercapture", release);
  }
  get driving(): boolean { return Boolean(this.driver && this.follow.checked); }
  setPaused(paused: boolean): void {
    this.pause.textContent = paused ? "Resume" : "Pause";
    this.playback(true, paused);
  }
  private clearInput(): void { this.keys.clear(); this.pointer = undefined; this.stick.hidden = true; }
  private applySailorOptions(): void {
    this.driver?.set_sailor_options(this.sailor.value === "trim", this.helm.checked);
  }
  private spawn(world: BoatWorld, center: [number, number], paused: boolean): void {
    try {
      const driver = new PlaygroundBoatDriver(world.width, world.height, world.seed, world.tick, ...center);
      this.driver?.free(); this.driver = driver;
      this.applySailorOptions();
      this.start = { world, center };
      this.world = world; this.clearInput(); this.follow.checked = true;
      this.restart.disabled = this.remove.disabled = this.pause.disabled = false;
      this.setPaused(paused);
      const state = driver.state(); this.spawned([state[0]!, state[1]!], world.tick);
      this.message.textContent = "";
    } catch (error) { this.message.textContent = String(error); }
  }
  synchronizeWorld(): void {
    if (!this.driver) return;
    const world = this.getWorld();
    if (!this.world || world.width !== this.world.width || world.height !== this.world.height || world.seed !== this.world.seed || world.tick !== this.world.tick) {
      this.driver.rebase(world.width, world.height, world.seed, world.tick);
      this.world = world;
      this.start = undefined;
      this.restart.disabled = true;
      this.message.textContent = "World changed. Respawn to save a new start.";
    }
  }
  step(): void {
    if (!this.driver) return;
    const pads = document.hasFocus() && document.visibilityState === "visible" ? navigator.getGamepads?.() : []; const pad = Array.from(pads ?? []).find((pad) => pad && pad.connected && pad.axes.length >= 2);
    let x = pad?.axes[0] ?? 0; let y = -(pad?.axes[1] ?? 0);
    if (x === 0 && y === 0) { x = this.pointer?.dx ?? 0; y = this.pointer?.dy ?? 0; }
    this.driver.step(Number(this.keys.has("KeyW")) - Number(this.keys.has("KeyS")), Number(this.keys.has("KeyA")) - Number(this.keys.has("KeyD")), x, y);
    if (this.world) this.world = { ...this.world, tick: this.world.tick + 1n };
  }
  state(): Float64Array | undefined {
    const state = this.driver?.state();
    if (state) this.readout.textContent = `${state[3]!.toFixed(2)} m/s · yaw ${(state[4]! * 180 / Math.PI).toFixed(1)}°/s · depth ${state[10]!.toFixed(1)} m · intent ${state[5]!.toFixed(2)}, ${state[6]!.toFixed(2)}`;
    return state;
  }
}

export class BoatPass {
  private readonly shader: GPUShaderModule;
  private readonly sceneLayout: GPUBindGroupLayout;
  private readonly imageLayout: GPUBindGroupLayout;
  private readonly heightLayout: GPUBindGroupLayout;
  private scene: GPUBindGroup;
  private shadowTexture: GPUTexture;
  private shadowView: GPUTextureView;
  private readonly shadowFrame: GPUBuffer;
  private readonly shadowProjections: GPUBuffer;
  private readonly shadowInput: GPUBindGroup;
  private readonly shadowPipeline: GPURenderPipeline;
  private shadowOccupied = false;
  private vertices: Uint8Array = new Uint8Array();
  private batches: Uint32Array = new Uint32Array();
  private readonly sceneEntries: GPUBindGroupEntry[];
  private readonly images: GPUBindGroup[] = [];
  private readonly pipelines = new Map<string, [GPURenderPipeline, GPURenderPipeline, GPURenderPipeline, GPURenderPipeline]>();
  private readonly bounds: GPUBuffer;
  private readonly compositeLayout: GPUBindGroupLayout;
  private readonly compositeShader: GPUShaderModule;
  private vertex: GPUBuffer;
  private vertexBytes = 1024;
  private targets: { size: [number, number]; format: GPUTextureFormat; color: GPUTexture; resolved: GPUTexture; luminance?: GPUTexture; resolvedLuminance?: GPUTexture; depth: GPUTexture; fold: GPUTexture; composite: GPUBindGroup } | undefined;
  private readonly device: GPUDevice;
  constructor(device: GPUDevice, sceneBuffer: GPUBuffer, adaptation: GPUTextureView, environment: GPUTextureView, occluders: GPUTextureView, occluderSampler: GPUSampler) {
    this.device = device;
    this.shader = device.createShaderModule({ code: playground_boat_shader_source() });
    this.sceneLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 3, buffer: { type: "uniform" } },
      { binding: 7, visibility: 2, texture: { sampleType: "unfilterable-float" } },
      { binding: 1, visibility: 2, texture: { sampleType: "unfilterable-float", viewDimension: "2d-array" } },
      { binding: 2, visibility: 2, texture: { viewDimension: "2d-array" } },
      { binding: 3, visibility: 2, sampler: {} },
      { binding: 9, visibility: 2, texture: { sampleType: "depth", viewDimension: "2d-array" } },
      { binding: 10, visibility: 2, buffer: { type: "uniform" } },
    ] });
    this.imageLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 2, texture: { sampleType: "float" } },
      { binding: 1, visibility: 2, sampler: { type: "filtering" } },
    ] });
    this.heightLayout = device.createBindGroupLayout({ entries: [
      ...[0].map((binding) => ({ binding, visibility: 2, texture: { sampleType: "unfilterable-float" as GPUTextureSampleType } })),
      { binding: 2, visibility: 3, buffer: { type: "uniform" } },
      { binding: 3, visibility: 2, texture: { sampleType: "depth", multisampled: true } },
    ] });
    this.bounds = device.createBuffer({ size: 32, usage: 0x40 | 0x08 });
    this.compositeLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 2, texture: { sampleType: "unfilterable-float" } },
      { binding: 1, visibility: 2, buffer: { type: "uniform" } },
      { binding: 2, visibility: 2, texture: { sampleType: "unfilterable-float" } },
    ] });
    this.compositeShader = device.createShaderModule({ code: playground_boat_composite_shader_source() });
    this.shadowFrame = device.createBuffer({ size: 480, usage: 0x40 | 0x08 });
    this.shadowProjections = device.createBuffer({ size: 6 * 256, usage: 0x40 | 0x08 });
    this.shadowTexture = device.createTexture({ size: [1, 1, 6], format: "depth32float", usage: 0x10 | 0x04 });
    this.shadowView = this.shadowTexture.createView({ dimension: "2d-array" });
    const shadowLayout = device.createBindGroupLayout({ entries: [{ binding: 0, visibility: 1, buffer: { type: "uniform", hasDynamicOffset: true } }] });
    const shadowShader = device.createShaderModule({ code: playground_boat_shadow_shader_source() });
    this.shadowInput = device.createBindGroup({ layout: shadowLayout, entries: [{ binding: 0, resource: { buffer: this.shadowProjections, size: 80 } }] });
    this.shadowPipeline = device.createRenderPipeline({
      layout: device.createPipelineLayout({ bindGroupLayouts: [shadowLayout, this.imageLayout] }),
      vertex: { module: shadowShader, entryPoint: "vertex", buffers: [{ arrayStride: 28, attributes: [
        { shaderLocation: 0, offset: 0, format: "float32x3" }, { shaderLocation: 1, offset: 12, format: "float32x3" }, { shaderLocation: 2, offset: 24, format: "float32" },
      ] }] }, fragment: { module: shadowShader, entryPoint: "fragment", targets: [] },
      depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less-equal", depthBias: 2, depthBiasSlopeScale: 1 },
    });
    this.sceneEntries = [{ binding: 0, resource: { buffer: sceneBuffer } }, { binding: 7, resource: adaptation },
      { binding: 1, resource: environment }, { binding: 2, resource: occluders }, { binding: 3, resource: occluderSampler }];
    this.scene = device.createBindGroup({ layout: this.sceneLayout, entries: [...this.sceneEntries, ...this.shadowEntries()] });
    this.vertex = device.createBuffer({ size: this.vertexBytes, usage: 0x20 | 0x08 });
    const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear", mipmapFilter: "linear", maxAnisotropy: 8 });
    for (let index = 0; index < playground_boat_image_count(); index++) {
      const image = new PlaygroundBoatImage(index);
      const texture = device.createTexture({ size: [image.width(0), image.height(0)], mipLevelCount: image.levels(), format: "rgba8unorm-srgb", usage: 0x04 | 0x02 });
      for (let level = 0; level < image.levels(); level++) {
        const width = image.width(level); const height = image.height(level);
        device.queue.writeTexture({ texture, mipLevel: level }, image.pixels(level), { bytesPerRow: width * 4, rowsPerImage: height }, [width, height]);
      }
      this.images.push(device.createBindGroup({ layout: this.imageLayout, entries: [{ binding: 0, resource: texture.createView() }, { binding: 1, resource: sampler }] }));
      image.free();
    }
  }
  private forFormat(format: GPUTextureFormat, vision: boolean): [GPURenderPipeline, GPURenderPipeline, GPURenderPipeline, GPURenderPipeline] {
    const key = `${format}:${vision}`;
    let result = this.pipelines.get(key); if (result) return result;
    const layout = this.device.createPipelineLayout({ bindGroupLayouts: [this.sceneLayout, this.imageLayout, this.heightLayout] });
    const pipeline = (sail: boolean, depth: boolean): GPURenderPipeline => this.device.createRenderPipeline({
      layout,
      vertex: { module: this.shader, entryPoint: "vertex_boat_sprite", buffers: [{ arrayStride: 28, attributes: [
        { shaderLocation: 0, offset: 0, format: "float32x3" }, { shaderLocation: 1, offset: 12, format: "float32x3" }, { shaderLocation: 2, offset: 24, format: "float32" },
      ] }] },
      fragment: { module: this.shader, entryPoint: depth ? "fragment_sail_depth" : sail ? (vision ? "fragment_sail_vision" : "fragment_sail") : (vision ? "fragment_boat_vision" : "fragment_boat_sprite"), constants: { IS_SAIL: Number(sail), VISION_INTERMEDIATE: Number(vision) }, targets: depth ? [] : [{ format }, ...(vision ? [{ format: "rg16float" as GPUTextureFormat }] : [])] },
      primitive: { topology: "triangle-list" },
      depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less-equal" },
      multisample: { count: 4 },
    });
    const composite = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({ bindGroupLayouts: [this.compositeLayout] }),
      vertex: { module: this.compositeShader, entryPoint: "vertex" },
      fragment: { module: this.compositeShader, entryPoint: vision ? "fragment_vision" : "fragment", targets: [format, ...(vision ? ["rg16float" as GPUTextureFormat] : [])].map((format) => ({ format, blend: {
        color: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
        alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
      } })) },
    });
    result = [pipeline(false, false), pipeline(true, true), pipeline(true, false), composite];
    this.pipelines.set(key, result); return result;
  }
  releaseTargets(): void {
    const t = this.targets;
    if (t) for (const texture of [t.color, t.resolved, t.depth, t.fold, t.luminance, t.resolvedLuminance]) texture?.destroy();
    this.targets = undefined;
  }
  shadowEntries(): GPUBindGroupEntry[] { return [{ binding: 9, resource: this.shadowView }, { binding: 10, resource: { buffer: this.shadowFrame } }]; }
  prepareShadows(encoder: GPUCommandEncoder, driver: PlaygroundBoatDriver | undefined, view: BoatView): boolean {
    let frame: Float32Array = new Float32Array(120);
    if (driver) {
      driver.prepare_mesh(view.x, view.y, view.outputWidth, view.outputHeight, view.sceneHeight, view.height, view.time);
      this.vertices = driver.vertices(); this.batches = driver.batches();
      frame = driver.shadow_frame(view.x, view.y, view.time);
    } else { this.vertices = new Uint8Array(); this.batches = new Uint32Array(); }
    const occupied = Array.from({ length: 6 }, (_, i) => frame[96 + i * 4]! > 0).some(Boolean);
    const changed = occupied !== this.shadowOccupied;
    if (changed) {
      this.shadowTexture.destroy();
      this.shadowTexture = this.device.createTexture({ size: [occupied ? 1024 : 1, occupied ? 1024 : 1, 6], format: "depth32float", usage: 0x10 | 0x04 });
      this.shadowView = this.shadowTexture.createView({ dimension: "2d-array" }); this.shadowOccupied = occupied;
      this.scene = this.device.createBindGroup({ layout: this.sceneLayout, entries: [...this.sceneEntries, ...this.shadowEntries()] });
    }
    this.device.queue.writeBuffer(this.shadowFrame, 0, frame);
    if (this.vertices.byteLength > this.vertexBytes) {
      this.vertex.destroy(); this.vertexBytes = this.vertices.byteLength;
      this.vertex = this.device.createBuffer({ size: this.vertexBytes, usage: 0x20 | 0x08 });
    }
    if (this.vertices.byteLength) this.device.queue.writeBuffer(this.vertex, 0, this.vertices);
    const projections = new Float32Array(6 * 64);
    for (let i = 0; i < 6; i++) { projections.set(frame.subarray(i * 16, i * 16 + 16), i * 64); projections.set(frame.subarray(96 + i * 4, 100 + i * 4), i * 64 + 16); }
    this.device.queue.writeBuffer(this.shadowProjections, 0, projections);
    for (let light = 0; light < 6; light++) {
      if (!frame[96 + light * 4]) continue;
      const pass = encoder.beginRenderPass({ colorAttachments: [], depthStencilAttachment: {
        view: this.shadowTexture.createView({ dimension: "2d", baseArrayLayer: light, arrayLayerCount: 1 }), depthLoadOp: "clear", depthStoreOp: "store", depthClearValue: 1,
      } });
      pass.setPipeline(this.shadowPipeline); pass.setBindGroup(0, this.shadowInput, [light * 256]); pass.setVertexBuffer(0, this.vertex);
      for (let i = 0; i < this.batches.length; i += 4) { pass.setBindGroup(1, this.images[this.batches[i]!]!); pass.draw(this.batches[i + 3]! - this.batches[i + 2]!, light >= 2 ? 3 : 1, this.batches[i + 2]!); }
      pass.end();
    }
    return changed;
  }
  encode(encoder: GPUCommandEncoder, target: GPUTextureView, format: GPUTextureFormat, luminance: GPUTextureView | undefined, surface: GPUTextureView, size: [number, number], view: BoatView): void {
    const vision = Boolean(luminance);
    const vertices = this.vertices; const batches = this.batches;
    let left = size[0]; let top = size[1]; let right = 0; let bottom = 0;
    const mesh = new DataView(vertices.buffer, vertices.byteOffset, vertices.byteLength);
    const viewWidth = view.height * size[0] / size[1];
    for (let i = 0; i < vertices.byteLength; i += 28) {
      const scale = 1 - mesh.getFloat32(i + 8, true) / view.cameraHeight;
      if (scale <= 0) { left = top = 0; [right, bottom] = size; break; }
      const x = (mesh.getFloat32(i, true) / (viewWidth * scale) + 0.5) * size[0];
      const y = (0.5 - mesh.getFloat32(i + 4, true) / (view.height * scale)) * size[1];
      left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y);
    }
    left = Math.max(0, Math.floor(left)); top = Math.max(0, Math.floor(top));
    right = Math.min(size[0], Math.ceil(right)); bottom = Math.min(size[1], Math.ceil(bottom));
    if (right <= left || bottom <= top) { this.releaseTargets(); return; }
    const required: [number, number] = [right - left, bottom - top];
    const allocation: [number, number] = required.map((v, i) => Math.min(size[i]!, Math.ceil(v / 64) * 64)) as [number, number];
    if (!this.targets || this.targets.format !== format || Boolean(this.targets.luminance) !== vision || allocation.some((v, i) => required[i]! > this.targets!.size[i]! || v < this.targets!.size[i]! / 2 || this.targets!.size[i]! > size[i]!)) {
      this.releaseTargets();
      const texture = (format: GPUTextureFormat, sampleCount: number, usage: GPUTextureUsageFlags) => this.device.createTexture({ size: allocation, format, sampleCount, usage });
      const color = texture(format, 4, 0x10);
      const resolved = texture(format, 1, 0x10 | 0x04);
      const depth = texture("depth32float", 4, 0x10 | 0x04);
      const fold = texture("depth32float", 4, 0x10 | 0x04);
      const luminance = vision ? texture("rg16float", 4, 0x10) : undefined;
      const resolvedLuminance = vision ? texture("rg16float", 1, 0x10 | 0x04) : undefined;
      const composite = this.device.createBindGroup({ layout: this.compositeLayout, entries: [
        { binding: 0, resource: resolved.createView() }, { binding: 1, resource: { buffer: this.bounds } },
        { binding: 2, resource: (resolvedLuminance ?? resolved).createView() },
      ] });
      this.targets = { size: allocation, format, color, resolved, depth, fold, composite, ...(luminance && resolvedLuminance ? { luminance, resolvedLuminance } : {}) };
    }
    const t = this.targets; const [width, height] = t.size;
    this.device.queue.writeBuffer(this.bounds, 0, new Float32Array([
      size[0] / width, size[1] / height, (size[0] - 2 * left) / width - 1, 1 - (size[1] - 2 * top) / height,
      left, top, width, height,
    ]));
    const surfaces = (fold: GPUTexture) => this.device.createBindGroup({ layout: this.heightLayout, entries: [
      { binding: 0, resource: surface },
      { binding: 2, resource: { buffer: this.bounds } }, { binding: 3, resource: fold.createView() },
    ] });
    const colorSurfaces = surfaces(t.fold); const foldSurfaces = surfaces(t.depth);
    const pipelines = this.forFormat(format, vision);
    const bind = (pass: GPURenderPassEncoder, surfaces: GPUBindGroup) => {
      pass.setBindGroup(0, this.scene); pass.setBindGroup(2, surfaces); pass.setVertexBuffer(0, this.vertex);
    };
    let pass: GPURenderPassEncoder | undefined;
    let first = true;
    for (let offset = 0; offset < batches.length; offset += 4) {
      const image = batches[offset]!; const sail = batches[offset + 1] !== 0;
      const start = batches[offset + 2]!; const count = batches[offset + 3]! - start;
      if (sail) {
        pass?.end(); pass = undefined;
        const fold = encoder.beginRenderPass({ colorAttachments: [],
          depthStencilAttachment: { view: t.fold.createView(), depthClearValue: 1, depthLoadOp: "clear", depthStoreOp: "store" } });
        bind(fold, foldSurfaces); fold.setBindGroup(1, this.images[image]!); fold.setPipeline(pipelines[1]); fold.draw(count, 1, start); fold.end();
      }
      if (!pass) {
        const final = sail ? offset + 4 === batches.length : !Array.from(batches).some((value, index) => index >= offset && index % 4 === 1 && value !== 0);
        pass = encoder.beginRenderPass({ colorAttachments: [{ view: t.color.createView(), ...(final ? { resolveTarget: t.resolved.createView() } : {}), loadOp: first ? "clear" : "load", storeOp: "store", clearValue: [0, 0, 0, 0] }, ...(t.luminance && t.resolvedLuminance ? [{ view: t.luminance.createView(), ...(final ? { resolveTarget: t.resolvedLuminance.createView() } : {}), loadOp: first ? "clear" as GPULoadOp : "load" as GPULoadOp, storeOp: "store" as GPUStoreOp, clearValue: [0, 0, 0, 0] as GPUColor }] : [])],
          depthStencilAttachment: { view: t.depth.createView(), depthClearValue: 1, depthLoadOp: first ? "clear" : "load", depthStoreOp: "store" } });
        bind(pass, colorSurfaces); first = false;
      }
      pass.setBindGroup(1, this.images[image]!); pass.setPipeline(pipelines[sail ? 2 : 0]); pass.draw(count, 1, start);
      if (sail) { pass.end(); pass = undefined; }
    }
    pass?.end();
    const composite = encoder.beginRenderPass({ colorAttachments: [{ view: target, loadOp: "load", storeOp: "store" }, ...(luminance ? [{ view: luminance, loadOp: "load" as GPULoadOp, storeOp: "store" as GPUStoreOp }] : [])] });
    composite.setScissorRect(left, top, required[0], required[1]); composite.setPipeline(pipelines[3]); composite.setBindGroup(0, t.composite); composite.draw(3); composite.end();
  }
}
