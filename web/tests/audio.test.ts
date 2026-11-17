import test from 'node:test';
import assert from 'node:assert/strict';
import { SeaSoundDSP, spatialGain } from '../src/audio-dsp.js';

test('local sound falls with distance and preserves left/right', () => {
  const near = spatialGain(2, 0), far = spatialGain(40, 0);
  assert.ok(near.gain > far.gain);
  assert.equal(spatialGain(-2, 0).pan, -near.pan);
  assert.equal(spatialGain(0, 0).pan, 0);
  assert.ok(far.cutoff < near.cutoff);
});
test('procedural stereo is finite and bounded during a storm and state changes', () => {
  const dsp = new SeaSoundDSP(48000, 42);
  const left = new Float32Array(128), right = new Float32Array(128);
  dsp.setState({ wind: 28, waves: 6, rain: 1, water: 1, boats: [{ id: 1, x: 0, y: 0, speed: 5, angular: 1, load: 0.8, flutter: 0.8, heave: 0.5 }], shores: [{ x: -15, y: 5, strength: 1 }] });
  let energy = 0;
  for (let block = 0; block < 1500; block++) {
    if (block === 700) dsp.setState({ wind: 0, waves: 0, rain: 0, water: 0, boats: [], shores: [] });
    dsp.render(left, right);
    for (let i = 0; i < left.length; i++) {
      assert.ok(Number.isFinite(left[i]) && Number.isFinite(right[i]));
      assert.ok(Math.abs(left[i]) <= 1 && Math.abs(right[i]) <= 1);
      energy += left[i] ** 2 + right[i] ** 2;
    }
  }
  assert.ok(energy > 0);
});
test('an uninitialized scene stays silent', () => {
  const dsp = new SeaSoundDSP(48000, 7);
  const left = new Float32Array(128), right = new Float32Array(128);
  dsp.render(left, right);
  assert.ok(left.every(x => x === 0) && right.every(x => x === 0));
});
test('boat wash is audible to a listener on dry land', () => {
  const dsp = new SeaSoundDSP(48000, 5);
  dsp.layers = [0, 0, 1, 0, 0, 0];
  dsp.setState({ wind: 0, waves: 0, rain: 0, water: 0, boats: [{ id: 1, x: 5, y: 0, speed: 3, angular: 0, load: 0, flutter: 0, heave: 0 }], shores: [] });
  const l = new Float32Array(128), r = new Float32Array(128);
  let energy = 0;
  for (let block = 0; block < 100; block++) { dsp.render(l, r); energy += l.reduce((sum, x) => sum + x*x, 0); }
  assert.ok(energy > 0);
});
test('the same load-change rate produces the same creak activity at different control rates', () => {
  function creaks(interval: number): Float32Array {
    const dsp = new SeaSoundDSP(48000, 200);
    dsp.layers = [0, 0, 0, 1, 0, 0];
    const state = { wind: 0, waves: 0, rain: 0, water: 0, boats: [{ id: 1, x: 0, y: 0, speed: 0, angular: 0, load: 0, flutter: 0, heave: 0 }], shores: [] };
    dsp.setState(state, interval);
    state.boats[0]!.load = 2 * interval;
    dsp.setState(state, interval);
    const left = new Float32Array(480000), right = new Float32Array(480000);
    dsp.render(left, right);
    return left;
  }
  const at20 = creaks(0.05), at10 = creaks(0.1);
  assert.ok(at20.some(value => value !== 0));
  assert.ok(at20.every((value, index) => value === at10[index]), 'equal physical load-change rates should produce equal creak excitation');
});

test('thunder is a bounded transient and cancellation removes queued channels', () => {
  const dsp = new SeaSoundDSP(48000, 42);
  dsp.layers = [0, 0, 0, 0, 0, 0, 1];
  dsp.queueThunder({ x: 3000, y: 0, strength: 1, distance: 3600 });
  const l = new Float32Array(48000), r = new Float32Array(48000);
  dsp.render(l, r);
  assert.ok(l.some(value => value !== 0));
  assert.ok(l.every(value => Number.isFinite(value) && Math.abs(value) <= 1));
  assert.ok(r.reduce((a,x) => a+x*x,0) > l.reduce((a,x) => a+x*x,0));
  dsp.resetThunder(); dsp.render(l,r);
  assert.ok(l.every(value => value === 0) && r.every(value => value === 0));
});
