import { vision_shader_source } from "browser/playground";

export class VisionPass {
  private readonly device: GPUDevice;
  private readonly inputLayout: GPUBindGroupLayout;
  private readonly stateLayout: GPUBindGroupLayout;
  private readonly state: GPUBindGroup;
  private readonly sampler: GPUSampler;
  private readonly shader: GPUShaderModule;
  private readonly layout: GPUPipelineLayout;
  private readonly mipPipeline: GPURenderPipeline;
  private readonly luminancePipeline: GPURenderPipeline;
  private readonly outputs = new Map<GPUTextureFormat, GPURenderPipeline>();
  private targets: {
    size: [number, number]; scene: [number, number];
    color: GPUTexture; luminance: GPUTexture; sceneLuminance?: GPUTexture;
    colorViews: GPUTextureView[]; luminanceViews: GPUTextureView[];
    inputs: GPUBindGroup[]; all: GPUBindGroup; sceneInput?: GPUBindGroup;
  } | undefined;

  constructor(device: GPUDevice, scene: GPUBuffer, adaptation: GPUTextureView) {
    this.device = device;
    this.shader = device.createShaderModule({ code: vision_shader_source() });
    this.stateLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 2, buffer: { type: "uniform" } },
      { binding: 7, visibility: 2, texture: { sampleType: "unfilterable-float" } },
    ] });
    this.state = device.createBindGroup({ layout: this.stateLayout, entries: [
      { binding: 0, resource: { buffer: scene } }, { binding: 7, resource: adaptation },
    ] });
    this.inputLayout = device.createBindGroupLayout({ entries: [
      { binding: 0, visibility: 2, texture: { sampleType: "float" } },
      { binding: 1, visibility: 2, texture: { sampleType: "float" } },
      { binding: 2, visibility: 2, sampler: { type: "filtering" } },
    ] });
    this.sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
    this.layout = device.createPipelineLayout({ bindGroupLayouts: [this.stateLayout, this.inputLayout] });
    this.mipPipeline = this.pipeline("vision_downsample", ["rgba16float", "rg16float"]);
    this.luminancePipeline = this.pipeline("vision_upscale_luminance", ["rg16float"]);
  }

  private pipeline(entryPoint: string, formats: GPUTextureFormat[]): GPURenderPipeline {
    return this.device.createRenderPipeline({
      layout: this.layout,
      vertex: { module: this.shader, entryPoint: "vision_vertex" },
      fragment: { module: this.shader, entryPoint, targets: formats.map((format) => ({ format })) },
    });
  }

  releaseTargets(): void {
    if (this.targets) {
      this.targets.color.destroy(); this.targets.luminance.destroy(); this.targets.sceneLuminance?.destroy();
      this.targets = undefined;
    }
  }

  size(size: [number, number], scene: [number, number]): void {
    const old = this.targets;
    if (old && old.size.every((v, i) => v === size[i]) && old.scene.every((v, i) => v === scene[i])) return;
    this.releaseTargets();
    const levels = Math.floor(Math.log2(Math.max(...size))) + 1;
    const texture = (extent: [number, number], mipLevelCount: number, format: GPUTextureFormat) =>
      this.device.createTexture({ size: extent, mipLevelCount, format, usage: 0x10 | 0x04 });
    const color = texture(size, levels, "rgba16float"), luminance = texture(size, levels, "rg16float");
    const colorViews = Array.from({ length: levels }, (_, baseMipLevel) => color.createView({ baseMipLevel, mipLevelCount: 1 }));
    const luminanceViews = Array.from({ length: levels }, (_, baseMipLevel) => luminance.createView({ baseMipLevel, mipLevelCount: 1 }));
    const input = (color: GPUTextureView, luminance: GPUTextureView) => this.device.createBindGroup({
      layout: this.inputLayout, entries: [{ binding: 0, resource: color }, { binding: 1, resource: luminance }, { binding: 2, resource: this.sampler }],
    });
    const sceneLuminance = scene.some((v, i) => v !== size[i]) ? texture(scene, 1, "rg16float") : undefined;
    this.targets = {
      size, scene, color, luminance, colorViews, luminanceViews,
      inputs: colorViews.map((view, i) => input(view, luminanceViews[i]!)),
      all: input(color.createView(), luminance.createView()),
      ...(sceneLuminance ? { sceneLuminance, sceneInput: input(colorViews[0]!, sceneLuminance.createView()) } : {}),
    };
  }

  get colorView(): GPUTextureView { return this.targets!.colorViews[0]!; }
  get luminanceView(): GPUTextureView { return this.targets!.luminanceViews[0]!; }
  get sceneLuminanceView(): GPUTextureView { return this.targets!.sceneLuminance?.createView() ?? this.luminanceView; }

  upscaleLuminance(encoder: GPUCommandEncoder): void {
    if (!this.targets?.sceneInput) return;
    const pass = encoder.beginRenderPass({ colorAttachments: [{ view: this.luminanceView, loadOp: "clear", storeOp: "store" }] });
    pass.setPipeline(this.luminancePipeline); pass.setBindGroup(0, this.state); pass.setBindGroup(1, this.targets.sceneInput); pass.draw(3); pass.end();
  }

  encode(encoder: GPUCommandEncoder, output: GPUTextureView, format: GPUTextureFormat, timestampWrites?: GPURenderPassTimestampWrites): void {
    const targets = this.targets!;
    for (let level = 1; level < targets.colorViews.length; level++) {
      const pass = encoder.beginRenderPass({ colorAttachments: [targets.colorViews[level]!, targets.luminanceViews[level]!].map((view) => ({ view, loadOp: "clear", storeOp: "store" })) });
      pass.setPipeline(this.mipPipeline); pass.setBindGroup(0, this.state); pass.setBindGroup(1, targets.inputs[level - 1]!); pass.draw(3); pass.end();
    }
    let pipeline = this.outputs.get(format);
    if (!pipeline) { pipeline = this.pipeline("vision_finish", [format]); this.outputs.set(format, pipeline); }
    const pass = encoder.beginRenderPass({ colorAttachments: [{ view: output, loadOp: "clear", storeOp: "store" }], ...(timestampWrites ? { timestampWrites } : {}) });
    pass.setPipeline(pipeline); pass.setBindGroup(0, this.state); pass.setBindGroup(1, targets.all); pass.draw(3); pass.end();
  }
}
