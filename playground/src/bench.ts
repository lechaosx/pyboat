export type BenchScenario = {
  id: string;
  exercises: string;
  seed: string;
  cameraM: [number, number];
  viewHeightM: number;
  cloudTimeSeconds: number;
  worldTimeSeconds: number;
};

export type BenchSettings = {
  width: number;
  height: number;
  warmupFrames: number;
  timedFrames: number;
};

export type PassTimes = {
  discoveryMs: number;
  fillMs: number;
  sceneMs: number;
  totalMs: number;
};

export type FrameSample = {
  submittedToDoneMs: number;
  passes?: PassTimes | undefined;
};

export type ScenarioSummary = {
  id: string;
  submittedToDoneMs: number;
  passes?: PassTimes;
  nsPerPixel: number;
};

export function benchSettings(search: string): BenchSettings | undefined {
  const parameters = new URLSearchParams(search);
  if (!parameters.has("bench")) return undefined;
  const integer = (name: string, fallback: number, minimum: number): number => {
    const value = Number(parameters.get(name) ?? fallback);
    if (!Number.isInteger(value) || value < minimum) {
      throw new Error(`bench parameter ${name} must be an integer of at least ${minimum}`);
    }
    return value;
  };
  return {
    width: integer("width", 1920, 1),
    height: integer("height", 1080, 1),
    warmupFrames: integer("warmup", 5, 0),
    timedFrames: integer("frames", 20, 1),
  };
}

export function median(values: readonly number[]): number {
  if (values.length === 0) throw new Error("median needs at least one value");
  const sorted = [...values].sort((first, second) => first - second);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1]! + sorted[middle]!) / 2
    : sorted[middle]!;
}

/**
 * Medians per pass. GPU timestamps, when every frame has them, set the
 * per-pixel cost; otherwise submit-to-completion time does.
 */
export function summarizeScenario(
  id: string,
  samples: readonly FrameSample[],
  pixels: number,
): ScenarioSummary {
  const submittedToDoneMs = median(samples.map((sample) => sample.submittedToDoneMs));
  const timed = samples.flatMap((sample) => sample.passes ? [sample.passes] : []);
  if (timed.length !== samples.length) {
    return { id, submittedToDoneMs, nsPerPixel: submittedToDoneMs * 1e6 / pixels };
  }
  const passes = {
    discoveryMs: median(timed.map((pass) => pass.discoveryMs)),
    fillMs: median(timed.map((pass) => pass.fillMs)),
    sceneMs: median(timed.map((pass) => pass.sceneMs)),
    totalMs: median(timed.map((pass) => pass.totalMs)),
  };
  return { id, submittedToDoneMs, passes, nsPerPixel: passes.totalMs * 1e6 / pixels };
}
