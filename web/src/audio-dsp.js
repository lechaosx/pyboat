// Noise envelopes approximate the sound, not the fluid acoustics. The world
// supplies wind, waves and relative motion; this DSP never feeds simulation.
// Finite-source distance proxy: an 8 m near field prevents a singularity
// beside a hull or surf patch. Beyond it, amplitude tends to 1 / distance.
// The low-pass is a distance cue, not a calibrated atmospheric absorption law.
export function spatialGain(x, y) {
  const distance = Math.hypot(x, y);
  return { gain: 1 / Math.sqrt(1 + (distance / 8) ** 2), pan: x / Math.max(4, distance), cutoff: 12000 / (1 + distance / 35) };
}

export class SeaSoundDSP {
  constructor(rate, seed = 1234567) {
    this.rate = rate; this.seed = seed >>> 0 || 1;
    this.time = 0; this.wind = 0; this.waves = 0; this.rain = 0; this.water = 0;
    this.targets = [0, 0, 0, 0]; this.layers = [1, 1, 1, 1, 1, 1, 1];
    this.windLow = 0; this.windBody = 0; this.seaLow = 0; this.seaBody = 0; this.surge = 0;
    this.smooth = 1 - Math.exp(-1 / (0.2 * rate));
    this.low = 1 - Math.exp(-2 * Math.PI * 180 / rate);
    this.mid = 1 - Math.exp(-2 * Math.PI * 1800 / rate);
    this.voices = Array.from({ length: 8 }, () => ({ id: -1, target: 0, gain: 0, left: 0.707, right: 0.707, filter: 0, alpha: 0.1, speed: 0, flutter: 0, strain: 0, wood: 0, phase: 0, phaseStep: 0.02, lastLoad: 0, lastHeave: 0, wash: 0 }));
    this.drops = Array.from({ length: 6 }, () => ({ value: 0, phase: 0, step: 0, pan: 0.5 }));
    this.thunder = Array.from({ length: 12 }, () => ({ age: Infinity, duration: 0, gain: 0, left: 0, right: 0, low: 0, alpha: 0 }));
    this.dropDecay = Math.exp(-1 / (0.008 * rate));
    this.woodDecay = Math.exp(-1 / (0.18 * rate));
  }
  resetThunder() { for (const voice of this.thunder) voice.age = Infinity; }
  queueThunder(event) {
    const distance = Math.max(0, event.distance);
    const pan = event.x / Math.max(1000, Math.hypot(event.x, event.y));
    const angle = (pan + 1) * Math.PI / 4;
    // Three delayed channel patches give a crack followed by a spatially
    // extended rumble. Distance removes high frequencies and spreads arrivals;
    // this is a finite-channel acoustic proxy, not propagation through weather.
    for (let patch = 0; patch < 3; patch++) {
      let voice = this.thunder.find(v => v.age > v.duration);
      if (!voice) voice = this.thunder.reduce((a,b) => a.age > b.age ? a : b);
      Object.assign(voice, {
        age: -patch * (0.13 + distance / 16000), duration: 3 + distance / 5000,
        gain: event.strength / Math.sqrt(1 + (distance / 1200) ** 2) * (0.8 - patch * 0.18),
        left: Math.cos(angle), right: Math.sin(angle), low: 0,
        alpha: 1 - Math.exp(-2 * Math.PI * (1800 / (1 + distance / 1800)) / this.rate),
      });
    }
  }
  random() {
    let x = this.seed; x ^= x << 13; x ^= x >>> 17; x ^= x << 5;
    this.seed = x >>> 0; return this.seed / 4294967296;
  }
  setState(state, elapsedSeconds = 0.05) {
    this.targets[0] = Math.min(40, Math.max(0, state.wind));
    this.targets[1] = Math.min(12, Math.max(0, state.waves));
    this.targets[2] = Math.min(1, Math.max(0, state.rain));
    this.targets[3] = Math.min(1, Math.max(0, state.water));
    for (let i = 0; i < 8; i++) {
      const v = this.voices[i], source = i < 4 ? state.boats[i] : state.shores[i - 4];
      if (!source) { v.target = 0; v.strain = 0; continue; }
      const spatial = spatialGain(source.x, source.y);
      v.target = spatial.gain * (i < 4 ? 1 : Math.min(1, source.strength));
      const angle = (spatial.pan + 1) * Math.PI / 4;
      v.left = Math.cos(angle); v.right = Math.sin(angle);
      v.alpha = 1 - Math.exp(-2 * Math.PI * spatial.cutoff / this.rate);
      if (i < 4) {
        if (v.id !== source.id) { v.id = source.id; v.lastLoad = source.load; v.lastHeave = source.heave; v.wood = 0; }
        // Changing rig load and hull flex excite the wooden resonator. A held
        // key cannot produce creaks on a boat that is not moving or loading.
        // Rates keep flex excitation independent of the visual/control cadence.
        // Perceptual gains: 0.15 s for normalized load, 0.1 s/m for heave,
        // and 0.08 s/rad for yaw. They set audibility, not mechanical forces.
        v.strain = Math.min(1, Math.abs(source.load - v.lastLoad) / elapsedSeconds * 0.15 + Math.abs(source.heave - v.lastHeave) / elapsedSeconds * 0.1 + Math.abs(source.angular) * 0.08);
        v.lastLoad = source.load; v.lastHeave = source.heave;
        v.speed = Math.min(10, Math.max(0, source.speed)); v.flutter = Math.min(1, Math.max(0, source.flutter));
        v.phaseStep = 2 * Math.PI * (100 + i * 29) / this.rate;
      }
    }
  }
  render(left, right) {
    for (let i = 0; i < left.length; i++) {
      this.wind += (this.targets[0] - this.wind) * this.smooth;
      this.waves += (this.targets[1] - this.waves) * this.smooth;
      this.rain += (this.targets[2] - this.rain) * this.smooth;
      this.water += (this.targets[3] - this.water) * this.smooth;
      const noise = this.random() * 2 - 1, other = this.random() * 2 - 1;
      this.windLow += (noise - this.windLow) * this.low;
      this.windBody += (noise - this.windBody) * this.mid;
      this.seaLow += (other - this.seaLow) * this.low;
      this.seaBody += (other - this.seaBody) * this.mid;
      this.surge += (Math.abs(this.seaLow) - this.surge) * (1 / this.rate);
      const wind = (this.windBody - this.windLow * 0.65) * (this.wind / 40) ** 1.4 * 0.65 * this.layers[0];
      const wash = this.seaBody * Math.min(1, this.waves / 3) * this.water * (0.1 + Math.min(0.15, this.surge * 2)) * this.layers[1];
      const rain = (noise - this.windLow) * this.rain * 0.11 * this.layers[5];
      let l = wind + wash + rain, r = wind * 0.88 + wash * 0.85 + rain * 0.9;
      for (let j = 0; j < 8; j++) {
        const v = this.voices[j]; v.gain += (v.target - v.gain) * this.smooth;
        if (v.gain < 0.00001) continue;
        let value;
        if (j < 4) {
          v.wash += (v.speed - v.wash) * this.smooth;
          if (v.strain > 0 && this.random() < v.strain * 2 / this.rate) v.wood = Math.max(v.wood, v.strain * (0.3 + this.random() * 0.7));
          v.wood *= this.woodDecay;
          v.phase += v.phaseStep;
          if (v.phase > Math.PI * 2) v.phase -= Math.PI * 2;
          const creak = v.wood * (Math.sin(v.phase) + 0.25 * Math.sin(v.phase * 2.07)) * 0.22 * this.layers[3];
          const flutter = this.windBody * v.flutter * (0.25 + 0.75 * Math.abs(Math.sin(this.time * (17 + j)))) * 0.13 * this.layers[4];
          value = creak + flutter + this.seaBody * Math.min(1, v.wash / 5) * 0.25 * this.layers[2];
        } else {
          // Independent slow surge phases avoid synchronized coast emitters.
          value = this.seaBody * (0.35 + 0.65 * (0.5 + 0.5 * Math.sin(this.time * (0.7 + j * 0.09) + j)) ** 2) * 0.75 * this.layers[1];
        }
        v.filter += (value - v.filter) * v.alpha;
        l += v.filter * v.gain * v.left; r += v.filter * v.gain * v.right;
      }
      for (let j = 0; j < this.drops.length; j++) {
        const drop = this.drops[j];
        if (this.rain > 0.001 && this.random() < this.rain * 100 / this.rate / this.drops.length) {
          drop.value = (0.2 + this.random() * 0.8) * this.rain * 0.18;
          drop.phase = 0; drop.step = 2 * Math.PI * (900 + this.random() * 2500) / this.rate; drop.pan = this.random();
        }
        drop.value *= this.dropDecay; drop.phase += drop.step;
        const value = Math.sin(drop.phase) * drop.value * this.layers[5];
        l += value * (1 - drop.pan); r += value * drop.pan;
      }
      for (const voice of this.thunder) {
        if (voice.age > voice.duration) continue;
        voice.age += 1 / this.rate;
        if (voice.age < 0) continue;
        const noise = this.random() * 2 - 1;
        voice.low += (noise - voice.low) * voice.alpha;
        const envelope = (1 - Math.exp(-voice.age / 0.012)) * Math.exp(-voice.age / (voice.duration / 4));
        const value = voice.low * envelope * voice.gain * (this.layers[6] ?? 0);
        l += value * voice.left; r += value * voice.right;
      }
      // Soft saturation bounds coincident stochastic transients without a
      // main-thread compressor or an unbounded peak at a crowded shoreline.
      left[i] = l / (1 + Math.abs(l)); right[i] = r / (1 + Math.abs(r));
      this.time += 1 / this.rate;
    }
  }
}
