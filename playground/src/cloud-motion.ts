export type CloudWorld = {
  seed: string;
  widthM: number;
  heightM: number;
};

export class CloudMotion {
  private worldKey: string | undefined;
  private previousTimeSeconds: number | undefined;
  private readonly lowOffsetM = [0, 0];
  private readonly upperOffsetM = [0, 0];
  private previousLowWindMS = [0, 0];
  private previousUpperWindMS = [0, 0];

  advance(
    world: CloudWorld,
    timeSeconds: number,
    lowWindMS: number[],
    upperWindMS: number[],
  ): number[] {
    const worldKey = `${world.seed}:${world.widthM}:${world.heightM}`;
    if (this.worldKey !== worldKey
      || this.previousTimeSeconds === undefined
      || timeSeconds < this.previousTimeSeconds) {
      this.lowOffsetM.fill(0);
      this.upperOffsetM.fill(0);
    } else {
      const elapsedSeconds = timeSeconds - this.previousTimeSeconds;
      for (let axis = 0; axis < 2; axis += 1) {
        this.lowOffsetM[axis] += 0.5 * elapsedSeconds
          * (this.previousLowWindMS[axis] + lowWindMS[axis]);
        this.upperOffsetM[axis] += 0.5 * elapsedSeconds
          * (this.previousUpperWindMS[axis] + upperWindMS[axis]);
      }
    }
    this.worldKey = worldKey;
    this.previousTimeSeconds = timeSeconds;
    this.previousLowWindMS = lowWindMS;
    this.previousUpperWindMS = upperWindMS;
    return [...this.lowOffsetM, ...this.upperOffsetM];
  }
}
