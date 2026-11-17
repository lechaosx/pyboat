// Perceptual source models; these envelopes and resonances exert no forces.
// The 8 m near field bounds finite hull/surf sources; the low-pass supplies
// a distance cue rather than calibrated atmospheric absorption.
export function spatialGain(x, y) {
  const distance = Math.hypot(x, y);
  return { gain: 1 / Math.sqrt(1 + (distance / 8) ** 2), pan: x / Math.max(4, distance), cutoff: 12000 / (1 + distance / 35) };
}

export class SeaSoundDSP {
  constructor(rate, seed = 1234567) {
    this.rate = rate; this.seed = seed >>> 0 || 1;
    this.time = 0; this.wind = 0; this.waves = 0; this.rain = 0; this.water = 0;
    this.targets = [0, 0, 0, 0]; this.layers = Array(8).fill(1);
    this.windLow = 0; this.windBody = 0; this.seaLow = 0; this.seaBody = 0;
    this.gust = 0; this.gustTarget = 0.6; this.gustWait = 0;
    this.smooth = 1 - Math.exp(-1 / (0.2 * rate));
    this.low = 1 - Math.exp(-2 * Math.PI * 140 / rate);
    this.mid = 1 - Math.exp(-2 * Math.PI * 1500 / rate);
    this.waterAlpha = 1 - Math.exp(-2 * Math.PI * 650 / rate);
    this.voices = Array.from({ length: 8 }, (_, index) => ({ id: -1, target: 0, gain: 0, left: 0.707, right: 0.707, filter: 0, alpha: 0.1, speed: 0, flutter: 0, strain: 0, wood: 0, phase: 0, overtone: 0, rigPhase: 0, water: 1, lastLoad: 0, lastHeave: 0, wash: 0, body: 0, bass: 0, bubble: 0, bubblePhase: 0, bubbleStep: 0, surgeAge: index * 0.73, surgeDuration: 4 + index * 0.67, foam: 0 }));
    this.drops = Array.from({ length: 8 }, () => ({ value: 0, phase: 0, step: 0, pan: 0.5 }));
    this.laps = Array.from({ length: 2 }, () => ({ value: 0, phase: 0, step: 0 }));
    this.thunder = Array.from({ length: 12 }, () => ({ age: Infinity, duration: 0, gain: 0, left: 0, right: 0, low: 0, alpha: 0 }));
    this.impacts = Array.from({ length: 8 }, () => ({ age: Infinity, gain: 0, left: 0, right: 0, low: 0, filter: 0, alpha: 0 }));
    this.dropDecay = Math.exp(-1 / (0.012 * rate));
    this.woodDecay = Math.exp(-1 / (0.55 * rate));
    this.bubbleDecay = Math.exp(-1 / (0.045 * rate));
  }
  resetThunder() { for (const voice of this.thunder) voice.age = Infinity; }
  resetEvents() {
    this.resetThunder();
    for (const voice of this.impacts) voice.age = Infinity;
    for (const voice of this.voices) { voice.wood = 0; voice.bubble = 0; voice.strain = 0; voice.id = -1; }
    for (const drop of this.drops) drop.value = 0;
    for (const lap of this.laps) lap.value = 0;
  }
  queueImpact(event) {
    let voice = this.impacts.find(v => v.age > 1);
    if (!voice) voice = this.impacts.reduce((a, b) => a.age > b.age ? a : b);
    const spatial = spatialGain(event.x, event.y), angle = (spatial.pan + 1) * Math.PI / 4;
    Object.assign(voice, { age: 0, gain: event.strength * spatial.gain, left: Math.cos(angle), right: Math.sin(angle), low: 0, filter: 0, alpha: 1 - Math.exp(-2 * Math.PI * spatial.cutoff / this.rate) });
  }
  queueThunder(event) {
    const distance = Math.max(0, event.distance);
    const pan = event.x / Math.max(1000, Math.hypot(event.x, event.y));
    const angle = (pan + 1) * Math.PI / 4;
    // Channel patches spread the crack into a rumble as distance increases.
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
    // Match hulls by identity: nearest-source sorting must not move resonators.
    const boats = this.voices.slice(0, 4).map(v => state.boats.find(b => b.id === v.id));
    for (const source of state.boats.slice(0, 4)) {
      if (boats.includes(source)) continue;
      const slot = boats.findIndex(b => !b);
      if (slot >= 0) boats[slot] = source;
    }
    for (let i = 0; i < 8; i++) {
      const v = this.voices[i], source = i < 4 ? boats[i] : state.shores[i - 4];
      if (!source) { v.target = 0; v.strain = 0; continue; }
      const spatial = spatialGain(source.x, source.y);
      v.target = spatial.gain * (i < 4 ? 1 : Math.min(1, source.strength));
      const angle = (spatial.pan + 1) * Math.PI / 4;
      v.left = Math.cos(angle); v.right = Math.sin(angle);
      v.alpha = 1 - Math.exp(-2 * Math.PI * spatial.cutoff / this.rate);
      if (i < 4) {
        if (v.id !== source.id) {
          v.id = source.id; v.lastLoad = source.load; v.lastHeave = source.heave;
          v.wood = 0; v.bubble = 0; v.filter = 0; v.wash = 0; v.gain = 0;
        }
        // Normalized load/heave/yaw rates stand in for unresolved joint flex.
        // Square-root excitation keeps small real wave motions perceptible.
        v.strain = Math.min(1, Math.abs(source.load - v.lastLoad) / elapsedSeconds * 0.4 + Math.abs(source.heave - v.lastHeave) / elapsedSeconds * 0.3 + Math.abs(source.angular) * 0.25);
        v.lastLoad = source.load; v.lastHeave = source.heave;
        v.water = source.water ?? 1;
        v.speed = Math.min(10, Math.max(0, source.speed)); v.flutter = Math.min(1, Math.max(0, source.flutter));
      }
    }
  }
  render(left, right) {
    const dt = 1 / this.rate;
    for (let i = 0; i < left.length; i++) {
      this.wind += (this.targets[0] - this.wind) * this.smooth;
      this.waves += (this.targets[1] - this.waves) * this.smooth;
      this.rain += (this.targets[2] - this.rain) * this.smooth;
      this.water += (this.targets[3] - this.water) * this.smooth;
      const noise = this.random() * 2 - 1, other = this.random() * 2 - 1;
      this.windLow += (noise - this.windLow) * this.low;
      this.windBody += (noise - this.windBody) * this.mid;
      this.seaLow += (other - this.seaLow) * this.low;
      this.seaBody += (other - this.seaBody) * this.waterAlpha;
      if ((this.gustWait -= dt) <= 0) { this.gustWait = 0.6 + this.random() * 2; this.gustTarget = 0.25 + this.random() * 0.75; }
      this.gust += (this.gustTarget - this.gust) * dt * 2;
      const air = this.wind / (this.wind + 10);
      const wind = (this.windLow * 0.5 + this.windBody * (0.1 + 0.5 * this.gust)) * air * air * this.layers[0];
      // Long swell does not imply breaking foam offshore. The quiet water bed
      // stays dark; resolved local laps/surf supply the bright transients.
      const lap = 0.5 + 0.5 * Math.sin(this.time * 1.1 + Math.sin(this.time * 0.37));
      const wash = this.seaLow * (0.025 + Math.min(0.035, this.waves * 0.008)) * (0.2 + lap * lap) * this.water * this.layers[1];
      const rain = (noise - this.windBody) * this.rain ** 1.5 * 0.045 * this.layers[5];
      let l = wind + wash + rain, r = (this.seaLow * 0.5 + this.seaBody * (0.1 + 0.5 * this.gust)) * air * air * this.layers[0] + this.windLow * (0.025 + Math.min(0.035, this.waves * 0.008)) * (1.2 - lap * lap) * this.water * this.layers[1] + (other - this.seaBody) * this.rain ** 1.5 * 0.045 * this.layers[5];
      for (let channel = 0; channel < 2; channel++) {
        const pulse = this.laps[channel];
        if (this.random() < this.water * (0.8 + Math.min(2, this.waves)) * dt) {
          pulse.value = 0.02 + this.random() * 0.045;
          pulse.phase = 0; pulse.step = 2 * Math.PI * (180 + this.random() * 350) * dt;
        }
        if (pulse.value > 0.000001) {
          pulse.value *= this.bubbleDecay; pulse.phase += pulse.step; pulse.step *= 1 + 2 * dt;
          const value = Math.sin(pulse.phase) * pulse.value * this.water * this.layers[1];
          if (channel === 0) l += value; else r += value;
        }
      }
      for (let j = 0; j < 8; j++) {
        const v = this.voices[j]; v.gain += (v.target - v.gain) * this.smooth;
        if (v.gain < 0.00001) continue;
        const localNoise = this.random() * 2 - 1;
        v.body += (localNoise - v.body) * this.waterAlpha;
        v.bass += (localNoise - v.bass) * this.low;
        let value, bubbleRate, bubbleGain;
        if (j < 4) {
          v.wash += (v.speed - v.wash) * this.smooth;
          if (v.strain > 0.001 && this.random() < (0.6 + v.strain * 9) * dt) v.wood = Math.max(v.wood, Math.sqrt(v.strain) * (0.55 + this.random() * 0.45));
          v.wood *= this.woodDecay;
          v.phase += 2 * Math.PI * (80 + v.wood * 170 + j * 13) * dt;
          if (v.phase > Math.PI * 2) v.phase -= Math.PI * 2;
          v.overtone = (v.overtone + 2 * Math.PI * (80 + v.wood * 170 + j * 13) * 2.71 * dt) % (2 * Math.PI);
          v.rigPhase = (v.rigPhase + 2 * Math.PI * (420 + this.wind * 18 + this.gust * 80) * dt) % (2 * Math.PI);
          const slip = 0.35 + 0.65 * Math.max(0, Math.sin(this.time * (35 + j * 3) + Math.sin(this.time * 13)));
          const creak = v.wood * slip * (Math.sin(v.phase) + 0.35 * Math.sin(v.overtone) + localNoise * 0.15) * 0.32 * this.layers[3];
          const flutter = (localNoise - v.body) * v.flutter * (0.1 + 0.9 * Math.max(0, Math.sin(this.time * (22 + j) + Math.sin(this.time * 3)))) * 0.1 * this.layers[4];
          const rigging = Math.sin(v.rigPhase) * v.flutter * air * this.gust * 0.012 * this.layers[0];
          const flow = Math.min(1, v.wash / 4);
          value = creak + flutter + rigging + (v.bass * 0.8 + v.body * 0.25) * flow * (0.25 + lap) * 0.65 * this.layers[2];
          bubbleRate = (0.8 + flow * 14) * v.water; bubbleGain = (0.02 + flow * 0.09) * v.water * this.layers[2];
        } else {
          v.surgeAge += dt;
          if (v.surgeAge > v.surgeDuration) { v.surgeAge = 0; v.surgeDuration = 3.5 + this.random() * 5; }
          const t = v.surgeAge / v.surgeDuration;
          // Fast breaking front, then a darker, slower draining backwash.
          const front = Math.max(0, 1 - Math.abs(t - 0.22) / 0.2);
          const backwash = Math.sin(Math.PI * t) ** 2;
          v.foam += (front - v.foam) * dt * 5;
          value = (v.bass * backwash * 0.8 + (localNoise - v.body) * v.foam * 0.16 + v.body * backwash * 0.3) * this.layers[1];
          bubbleRate = backwash * 18; bubbleGain = backwash * 0.065 * this.layers[1];
        }
        if (this.random() < bubbleRate * dt) {
          v.bubble = bubbleGain * (0.4 + this.random() * 0.6); v.bubblePhase = 0;
          v.bubbleStep = 2 * Math.PI * (220 + this.random() * 650) * dt;
        }
        if (v.bubble > 0.000001) {
          v.bubble *= this.bubbleDecay; v.bubbleStep *= 1 + 2 * dt; v.bubblePhase += v.bubbleStep;
          value += Math.sin(v.bubblePhase) * v.bubble;
        }
        v.filter += (value - v.filter) * v.alpha;
        l += v.filter * v.gain * v.left; r += v.filter * v.gain * v.right;
      }
      for (const drop of this.drops) {
        if (this.rain > 0.001 && this.random() < this.rain * 180 * dt / this.drops.length) {
          drop.value = (0.25 + this.random() * 0.75) * Math.sqrt(this.rain) * 0.17;
          drop.phase = 0; drop.step = 2 * Math.PI * (700 + this.random() * 3000) * dt; drop.pan = this.random();
        }
        drop.value *= this.dropDecay; drop.phase += drop.step;
        const value = (Math.sin(drop.phase) + noise * 0.4) * drop.value * this.layers[5];
        l += value * (1 - drop.pan); r += value * drop.pan;
      }
      for (const voice of this.impacts) {
        if (voice.age > 1) continue;
        voice.age += dt;
        const t = voice.age, n = this.random() * 2 - 1;
        voice.low += (n - voice.low) * this.waterAlpha;
        const thud = (Math.sin(t * 2 * Math.PI * 75) + 0.35 * Math.sin(t * 2 * Math.PI * 193)) * Math.exp(-t * 16);
        const crunch = n * Math.exp(-t * 65) * 0.6;
        const splash = voice.low * (1 - Math.exp(-t * 25)) * Math.exp(-t * 6) * 1.5;
        const value = (thud + crunch + splash) * (1 - Math.exp(-t * 1000)) * voice.gain * (this.layers[7] ?? 0) * 0.9;
        voice.filter += (value - voice.filter) * voice.alpha;
        l += voice.filter * voice.left; r += voice.filter * voice.right;
      }
      for (const voice of this.thunder) {
        if (voice.age > voice.duration) continue;
        voice.age += dt;
        if (voice.age < 0) continue;
        voice.low += (this.random() * 2 - 1 - voice.low) * voice.alpha;
        const envelope = (1 - Math.exp(-voice.age / 0.012)) * Math.exp(-voice.age / (voice.duration / 4));
        const value = voice.low * envelope * voice.gain * (this.layers[6] ?? 0);
        l += value * voice.left; r += value * voice.right;
      }
      left[i] = l / (1 + Math.abs(l)); right[i] = r / (1 + Math.abs(r));
      this.time += dt;
    }
  }
}
