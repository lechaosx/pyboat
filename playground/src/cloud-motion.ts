export class CloudMotion {
  private readonly lowOffsetM = [0, 0];
  private readonly upperOffsetM = [0, 0];

  advance(
    elapsedSeconds: number,
    lowWindMS: number[],
    upperWindMS: number[],
  ): number[] {
    for (let axis = 0; axis < 2; axis += 1) {
      this.lowOffsetM[axis] += elapsedSeconds * lowWindMS[axis];
      this.upperOffsetM[axis] += elapsedSeconds * upperWindMS[axis];
    }
    return [...this.lowOffsetM, ...this.upperOffsetM];
  }
}
