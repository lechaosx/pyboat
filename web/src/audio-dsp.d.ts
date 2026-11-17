export type SoundBoat = { id: number; x: number; y: number; speed: number; angular: number; load: number; flutter: number; heave: number };
export type SoundState = { wind: number; waves: number; rain: number; water: number; boats: SoundBoat[]; shores: { x: number; y: number; strength: number }[] };
export function spatialGain(x: number, y: number): { gain: number; pan: number; cutoff: number };
export class SeaSoundDSP {
  constructor(rate: number, seed?: number);
  layers: number[];
  setState(state: SoundState, elapsedSeconds?: number): void;
  render(left: Float32Array, right: Float32Array): void;
}
