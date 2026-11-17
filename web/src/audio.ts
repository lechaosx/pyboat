import workletUrl from './audio-worklet.js?worker&url';
import type { SoundState } from './audio-dsp.js';

const storageKey = 'sound-settings';
export const soundLayers = ['Wind', 'Sea and shore', 'Hull water', 'Wooden creaks', 'Sail flutter', 'Rain'] as const;

export function decodeSoundScene(data: Float32Array): SoundState {
  const state: SoundState = { wind: data[0] ?? 0, waves: data[1] ?? 0, rain: data[2] ?? 0, water: data[3] ?? 0, boats: [], shores: [] };
  for (let i = 0; i < 4; i++) {
    const start = 8 + i * 9;
    if (data[start]) state.boats.push({ id: data[start]!, x: data[start + 1]!, y: data[start + 2]!, speed: data[start + 3]!, angular: data[start + 4]!, load: data[start + 5]!, flutter: data[start + 6]!, heave: data[start + 7]! });
    const shore = 44 + i * 3;
    if (data[shore + 2]) state.shores.push({ x: data[shore]!, y: data[shore + 1]!, strength: data[shore + 2]! });
  }
  return state;
}

export class SailingAudio {
  private context: AudioContext | undefined;
  private processor: AudioWorkletNode | undefined;
  private master: GainNode | undefined;
  private starting: Promise<void> | undefined;
  private active = true;
  private nextUpdate = 0;
  private lastUpdate: number | undefined;
  private elapsedSeconds = 0.05;
  private volume = 0.6;
  private muted = false;
  private enabled = false;
  private pauseTimer: number | undefined;
  private readonly layers = soundLayers.map(() => 1);
  private readonly status = document.createElement('p');
  private readonly enable = document.createElement('button');
  readonly element = document.createElement('section');
  constructor(audition = false) {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(storageKey) ?? '{}');
      if (saved && typeof saved === 'object') {
        if ('volume' in saved && typeof saved.volume === 'number' && Number.isFinite(saved.volume)) this.volume = Math.min(1, Math.max(0, saved.volume));
        if ('muted' in saved && typeof saved.muted === 'boolean') this.muted = saved.muted;
      }
    } catch { /* Device storage is optional. */ }
    this.element.className = 'panel-section sound-controls';
    this.element.innerHTML = '<h3>Sound</h3>';
    this.enable.type = 'button'; this.enable.textContent = 'Enable sound';
    this.enable.addEventListener('click', () => { void this.start(); });
    const volumeLabel = document.createElement('label'); volumeLabel.textContent = 'Volume';
    const volume = document.createElement('input'); volume.type = 'range'; volume.min = '0'; volume.max = '1'; volume.step = '0.01'; volume.value = String(this.volume); volume.setAttribute('aria-label', 'Sound volume');
    volume.addEventListener('input', () => { this.volume = Number(volume.value); this.save(); this.applyGain(); });
    volumeLabel.append(volume);
    const muteLabel = document.createElement('label'); const mute = document.createElement('input'); mute.type = 'checkbox'; mute.checked = this.muted;
    mute.addEventListener('change', () => { this.muted = mute.checked; this.save(); this.applyGain(); if (!this.muted) void this.start(); });
    muteLabel.append(mute, ' Mute');
    this.status.setAttribute('role', 'status'); this.status.textContent = 'Sound starts when you set sail or enable it here.';
    this.element.append(this.enable, volumeLabel, muteLabel, this.status);
    if (audition) {
      for (let i = 0; i < soundLayers.length; i++) {
        const label = document.createElement('label'); const input = document.createElement('input'); input.type = 'checkbox'; input.checked = true;
        input.addEventListener('change', () => { this.layers[i] = Number(input.checked); this.processor?.port.postMessage({ layers: this.layers }); });
        label.append(input, ` ${soundLayers[i]}`); this.element.append(label);
      }
      const note = document.createElement('p'); note.textContent = 'Audition the current world. Spawn and drive a boat for hull, sail and creaks; pan near a coast for surf.'; this.element.append(note);
    }
    window.addEventListener('blur', () => this.syncPlayback());
    window.addEventListener('focus', () => this.syncPlayback());
    document.addEventListener('visibilitychange', () => this.syncPlayback());
    window.addEventListener('pagehide', event => { if (event.persisted) void this.context?.suspend(); else void this.context?.close(); });
    window.addEventListener('pageshow', () => this.syncPlayback());
  }
  private save(): void { try { localStorage.setItem(storageKey, JSON.stringify({ volume: this.volume, muted: this.muted })); } catch { /* Changes still apply this visit. */ } }
  private applyGain(): void {
    if (!this.master || !this.context) return;
    this.master.gain.setTargetAtTime(this.muted ? 0 : this.volume, this.context.currentTime, 0.03);
    window.clearTimeout(this.pauseTimer);
    if (this.muted || this.volume === 0) this.pauseTimer = window.setTimeout(() => this.syncPlayback(), 180);
    else this.syncPlayback();
  }
  async start(): Promise<void> {
    if (this.starting) return this.starting;
    if (this.enabled) { this.syncPlayback(); return; }
    if (this.muted) return;
    this.starting = this.initialize();
    await this.starting; this.starting = undefined;
  }
  private async initialize(): Promise<void> {
    try {
      const context = new AudioContext({ latencyHint: 'interactive' }); this.context = context;
      // Resume inside the initiating gesture, before fetching the worklet.
      const resume = context.resume();
      await context.audioWorklet.addModule(workletUrl); await resume;
      this.processor = new AudioWorkletNode(context, 'sailing-sea', { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2] });
      this.master = context.createGain(); this.master.gain.value = 0;
      this.processor.connect(this.master); this.master.connect(context.destination);
      this.processor.port.postMessage({ layers: this.layers }); this.enabled = true;
      this.applyGain(); this.syncPlayback(); this.enable.hidden = true;
      this.status.textContent = 'Sound enabled. Pauses when this window is inactive.';
    } catch (error) {
      await this.context?.close().catch(() => {}); this.context = undefined;
      this.status.textContent = `Sound could not start. You can retry: ${String(error)}`;
    }
  }
  setActive(active: boolean): void {
    if (this.active === active) return;
    this.active = active; this.syncPlayback();
  }
  private syncPlayback(): void {
    if (!this.context || !this.enabled) return;
    if (this.active && !this.muted && this.volume > 0 && !document.hidden && document.hasFocus()) void this.context.resume().catch(() => { this.enable.hidden = false; });
    else void this.context.suspend().catch(() => {});
  }
  due(now: number): boolean {
    if (!this.enabled || this.muted || this.volume === 0 || !this.active || document.hidden || !document.hasFocus() || this.context?.state !== 'running' || now < this.nextUpdate) return false;
    this.elapsedSeconds = this.lastUpdate === undefined ? 0.05 : (now - this.lastUpdate) / 1000;
    this.lastUpdate = now; this.nextUpdate = now + 50; return true;
  }
  update(scene: Float32Array): void { this.processor?.port.postMessage({ state: decodeSoundScene(scene), elapsedSeconds: this.elapsedSeconds }); }
}
