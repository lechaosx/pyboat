import {
  PlaygroundBoatDriver, PlaygroundBoatImage, playground_boat_image_count,
  playground_boat_shader_source,
} from "browser/playground";

export type BoatWorld = { width: number; height: number; seed: bigint; tick: bigint };
export type BoatView = { x: number; y: number; height: number; outputHeight: number; time: number };

export class DrivingBoat {
  driver: PlaygroundBoatDriver | undefined;
  readonly element = document.createElement("section");
  readonly follow: HTMLInputElement;
  readonly readout: HTMLElement;
  readonly message: HTMLElement;
  private readonly select: HTMLSelectElement;
  private readonly remove: HTMLButtonElement;
  private readonly pause: HTMLButtonElement;
  private readonly keys = new Set<string>();
  private pointer: { id: number; x: number; y: number; dx: number; dy: number } | undefined;
  private readonly stick = document.createElement("div");
  private world: BoatWorld | undefined;

  private readonly getWorld: () => BoatWorld;
  private readonly getCenter: () => [number, number];
  private readonly spawned: (position: [number, number]) => void;
  private readonly playback: (active: boolean, paused: boolean) => void;
  constructor(
    canvas: HTMLCanvasElement,
    getWorld: () => BoatWorld,
    getCenter: () => [number, number],
    spawned: (position: [number, number]) => void,
    playback: (active: boolean, paused: boolean) => void,
  ) {
    this.getWorld = getWorld; this.getCenter = getCenter; this.spawned = spawned; this.playback = playback;
    this.element.className = "panel-section boat-controls";
    this.element.innerHTML = `<h2>Drive a boat</h2>
      <div class="boat-actions"><button id="boat-spawn">Spawn / respawn</button><button id="boat-remove" disabled>Remove</button><button id="boat-pause" disabled>Pause</button></div>
      <label>Controller <select id="boat-controller"><option value="0">Current · reference</option><option value="1">Physical rudder allocation</option><option value="5">Current refined</option><option value="2">Classical PID</option><option value="4">Neural · residual pilot</option><option value="6">Neural · standalone pilot (failed)</option><option value="7">Simple sailor · guide rules</option><option value="8">Advanced sailor · experimental</option></select></label>
      <label><input id="boat-follow" type="checkbox" checked> Follow boat / drive with pointer</label>
      <p>W/S power · A/D turn · gamepad or drag to steer toward a world direction. Disable follow to pan.</p>
      <p>Switching preserves boat motion and resets the new controller’s memory. Driving uses real time; pause remains available.</p>
      <output id="boat-readout"></output><p id="boat-message" role="status"></p>`;
    this.follow = this.element.querySelector<HTMLInputElement>("#boat-follow")!;
    this.readout = this.element.querySelector<HTMLElement>("#boat-readout")!;
    this.message = this.element.querySelector<HTMLElement>("#boat-message")!;
    this.select = this.element.querySelector<HTMLSelectElement>("#boat-controller")!;
    this.remove = this.element.querySelector<HTMLButtonElement>("#boat-remove")!;
    this.pause = this.element.querySelector<HTMLButtonElement>("#boat-pause")!;
    this.element.querySelector("#boat-spawn")!.addEventListener("click", () => this.spawn());
    this.remove.addEventListener("click", () => {
      this.driver?.free(); this.driver = undefined; this.clearInput();
      this.remove.disabled = this.pause.disabled = true;
      this.readout.textContent = ""; this.message.textContent = "";
      this.playback(false, false);
    });
    this.pause.addEventListener("click", () => {
      this.setPaused(this.pause.textContent === "Pause");
    });
    this.select.addEventListener("change", () => {
      this.driver?.select(Number(this.select.value)); this.clearInput();
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
  private spawn(): void {
    const world = this.getWorld(); const [x, y] = this.getCenter();
    try {
      const driver = new PlaygroundBoatDriver(world.width, world.height, world.seed, world.tick, x, y);
      driver.select(Number(this.select.value)); this.driver?.free(); this.driver = driver;
      this.world = world; this.clearInput(); this.follow.checked = true;
      this.remove.disabled = this.pause.disabled = false; this.pause.textContent = "Pause";
      this.playback(true, false);
      const state = driver.state(); this.spawned([state[0]!, state[1]!]);
      this.message.textContent = "";
    } catch (error) { this.message.textContent = String(error); }
  }
  synchronizeWorld(): void {
    if (!this.driver) return;
    const world = this.getWorld();
    if (!this.world || world.width !== this.world.width || world.height !== this.world.height || world.seed !== this.world.seed || world.tick !== this.world.tick) {
      this.driver.rebase(world.width, world.height, world.seed, world.tick);
      this.world = world;
      this.message.textContent = "World changed; controller memory reset.";
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
  private readonly scene: GPUBindGroup;
  private readonly images: GPUBindGroup[] = [];
  private readonly pipelines = new Map<GPUTextureFormat, [GPURenderPipeline, GPURenderPipeline, GPURenderPipeline]>();
  private vertex: GPUBuffer;
  private vertexBytes = 1024;
  private depth: GPUTexture | undefined;
  private size = "";
  private readonly device: GPUDevice;
  constructor(device: GPUDevice, sceneBuffer: GPUBuffer, adaptation: GPUTextureView) {
    this.device = device;
    this.shader = device.createShaderModule({ code: playground_boat_shader_source() });
    this.sceneLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 3, buffer: { type: "uniform" } },
      { binding: 7, visibility: 2, texture: { sampleType: "unfilterable-float" } },
    ] });
    this.imageLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 2, texture: { sampleType: "float" } },
      { binding: 1, visibility: 2, sampler: { type: "filtering" } },
    ] });
    this.heightLayout = device.createBindGroupLayout({ entries: [{ binding: 0, visibility: 2, texture: { sampleType: "unfilterable-float" } }] });
    this.scene = device.createBindGroup({ layout: this.sceneLayout, entries: [
      { binding: 0, resource: { buffer: sceneBuffer } }, { binding: 7, resource: adaptation },
    ] });
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
  private forFormat(format: GPUTextureFormat): [GPURenderPipeline, GPURenderPipeline, GPURenderPipeline] {
    let result = this.pipelines.get(format); if (result) return result;
    const layout = this.device.createPipelineLayout({ bindGroupLayouts: [this.sceneLayout, this.imageLayout, this.heightLayout] });
    const pipeline = (sail: boolean, depth: boolean): GPURenderPipeline => this.device.createRenderPipeline({
      layout,
      vertex: { module: this.shader, entryPoint: "vertex_boat_sprite", buffers: [{ arrayStride: 28, attributes: [
        { shaderLocation: 0, offset: 0, format: "float32x3" }, { shaderLocation: 1, offset: 12, format: "float32x3" }, { shaderLocation: 2, offset: 24, format: "float32" },
      ] }] },
      fragment: { module: this.shader, entryPoint: depth ? "fragment_sail_depth" : "fragment_boat_sprite", constants: { IS_SAIL: Number(sail) }, targets: [{ format, writeMask: depth ? 0 : 15, blend: { color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" }, alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" } } }] },
      primitive: { topology: "triangle-list" },
      ...(sail ? { depthStencil: { format: "depth32float", depthWriteEnabled: depth, depthCompare: depth ? "less-equal" : "equal" } } : {}),
    });
    result = [pipeline(false, false), pipeline(true, true), pipeline(true, false)];
    this.pipelines.set(format, result); return result;
  }
  encode(encoder: GPUCommandEncoder, target: GPUTextureView, format: GPUTextureFormat, surface: GPUTextureView, size: [number, number], driver: PlaygroundBoatDriver, view: BoatView): void {
    driver.prepare_mesh(view.x, view.y, view.outputHeight, view.height, view.time);
    const vertices = driver.vertices(); const batches = driver.batches();
    if (vertices.byteLength > this.vertexBytes) {
      this.vertex.destroy(); this.vertexBytes = vertices.byteLength;
      this.vertex = this.device.createBuffer({ size: this.vertexBytes, usage: 0x20 | 0x08 });
    }
    this.device.queue.writeBuffer(this.vertex, 0, vertices);
    if (this.size !== size.join()) {
      this.depth?.destroy(); this.depth = this.device.createTexture({ size, format: "depth32float", usage: 0x10 }); this.size = size.join();
    }
    const height = this.device.createBindGroup({ layout: this.heightLayout, entries: [{ binding: 0, resource: surface }] });
    const pipelines = this.forFormat(format);
    let pass: GPURenderPassEncoder | undefined;
    for (let offset = 0; offset < batches.length; offset += 4) {
      const image = batches[offset]!; const sail = batches[offset + 1] !== 0;
      if (sail || !pass) {
        pass?.end();
        pass = encoder.beginRenderPass({ colorAttachments: [{ view: target, loadOp: "load", storeOp: "store" }], ...(sail ? { depthStencilAttachment: { view: this.depth!.createView(), depthClearValue: 1, depthLoadOp: "clear", depthStoreOp: "discard" } } : {}) });
        pass.setBindGroup(0, this.scene); pass.setBindGroup(2, height); pass.setVertexBuffer(0, this.vertex);
      }
      pass.setBindGroup(1, this.images[image]!);
      const start = batches[offset + 2]!; const count = batches[offset + 3]! - start;
      if (sail) { pass.setPipeline(pipelines[1]); pass.draw(count, 1, start); }
      pass.setPipeline(pipelines[sail ? 2 : 0]); pass.draw(count, 1, start);
      if (sail) { pass.end(); pass = undefined; }
    }
    pass?.end();
  }
}
