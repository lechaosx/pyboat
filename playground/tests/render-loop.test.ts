import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
import { test, type TestContext } from "node:test";
import { startRenderLoop } from "../src/render-loop.ts";
import { advanceFixedClock, fixedClockFraction } from "../src/fixed-clock.ts";
import { CloudMotion } from "../src/cloud-motion.ts";

type Frame = (now: number) => void;
const animationHost = globalThis as unknown as {
  requestAnimationFrame: (frame: Frame) => number;
};
animationHost.requestAnimationFrame = () => { throw new Error("Unexpected animation frame"); };

function animationFrames(t: TestContext): Frame[] {
  const frames: Frame[] = [];
  t.mock.method(animationHost, "requestAnimationFrame", (frame: Frame) => {
    frames.push(frame);
    return frames.length;
  });
  return frames;
}

test("bounds GPU scenes while clocks advance and the next scene uses latest input", async (t) => {
  const frames = animationFrames(t);
  const clock = { accumulatorSeconds: 0 };
  const motion = new CloudMotion();
  let previous = 0, ticks = 0, cameraX = 0;
  let cloudOffset = [0, 0, 0, 0];
  const rendered: Array<{ now: number; cameraX: number; ticks: number; cloudOffset: number[] }> = [];
  const complete: (() => void)[] = [];
  startRenderLoop((now) => {
    const elapsed = (now - previous) / 1000;
    previous = now;
    advanceFixedClock(clock, elapsed, (count) => { ticks += count; });
    cloudOffset = motion.advance(elapsed, [10, 0], [0, 20]);
  }, (now) => {
    rendered.push({ now, cameraX, ticks, cloudOffset });
    return new Promise<void>((resolve) => complete.push(resolve));
  }, () => true, assert.fail);

  frames.shift()!(0);
  cameraX = 1;
  frames.shift()!(1000 / 60);
  for (let input = 2; input <= 60; input += 1) {
    cameraX = input;
    frames.shift()!(input * 1000 / 60);
  }
  assert.equal(rendered.length, 2, "slow GPU work must prevent a third outstanding scene");
  assert.ok(Math.abs(ticks + fixedClockFraction(clock) - 60) < 1e-9);
  assert.ok(Math.abs(cloudOffset[0]! - 10) < 1e-9);
  assert.ok(Math.abs(cloudOffset[3]! - 20) < 1e-9);

  complete.shift()!();
  await setImmediate();
  frames.shift()!(1050);
  assert.equal(rendered.length, 3);
  assert.equal(rendered[2]!.now, 1050);
  assert.equal(rendered[2]!.cameraX, 60);
  assert.equal(rendered[2]!.ticks, ticks);
  assert.ok(Math.abs(ticks + fixedClockFraction(clock) - 63) < 1e-9);
  assert.ok(Math.abs(rendered[2]!.cloudOffset[0]! - 10.5) < 1e-9);
  assert.ok(Math.abs(rendered[2]!.cloudOffset[3]! - 21) < 1e-9);
});

for (const rate of [60, 120]) {
  test(`uses every ${rate} Hz display opportunity when GPU completion keeps up`, async (t) => {
    const frames = animationFrames(t);
    const rendered: number[] = [];
    startRenderLoop(() => {}, async (now) => { rendered.push(now); }, () => true, assert.fail);
    for (let frame = 0; frame < rate; frame++) {
      frames.shift()!(frame * 1000 / rate);
      await setImmediate();
    }
    assert.equal(rendered.length, rate);
    assert.equal(rendered.at(-1), (rate - 1) * 1000 / rate);
  });
}

test("delayed completion notifications bound submissions without stopping state advancement", async (t) => {
  const frames = animationFrames(t);
  let advances = 0, renders = 0;
  const notify: (() => void)[] = [];
  startRenderLoop(() => { advances++; }, () => {
    renders++;
    return new Promise<void>((resolve) => notify.push(resolve));
  }, () => true, assert.fail);
  for (let frame = 0; frame <= 12; frame++) frames.shift()!(frame * 1000 / 120);
  assert.equal(advances, 13);
  assert.equal(renders, 2);
  for (const resolve of notify) resolve();
  await setImmediate();
  frames.shift()!(110);
  assert.equal(advances, 14);
  assert.equal(renders, 3);
});

test("pending completions cannot revive a stopped loop", async (t) => {
  const frames = animationFrames(t);
  let active = true;
  const complete: (() => void)[] = [];
  startRenderLoop(() => {}, () => new Promise<void>((resolve) => complete.push(resolve)),
    () => active, assert.fail);
  frames.shift()!(0);
  frames.shift()!(16);
  active = false;
  for (const resolve of complete) resolve();
  await setImmediate();
  frames.shift()!(1000);
  assert.equal(frames.length, 0);
});

test("stopping before a scheduled frame prevents both advancement and rendering", async (t) => {
  const frames = animationFrames(t);
  let active = true;
  startRenderLoop(assert.fail, async () => assert.fail(), () => active, assert.fail);
  active = false;
  frames.shift()!(0);
  await setImmediate();
  assert.equal(frames.length, 0);
});

test("advancement, encoding and completion failures release slots and remain retryable", async (t) => {
  const frames = animationFrames(t);
  const errors: unknown[] = [];
  let advances = 0, renders = 0;
  const clockError = new Error("State advancement failed");
  const encodeError = new Error("Encoding failed");
  const queueError = new Error("GPU completion failed");
  startRenderLoop(() => {
    if (++advances === 1) throw clockError;
  }, () => {
    renders++;
    if (renders === 1) throw encodeError;
    if (renders === 2) return Promise.reject(queueError);
    return Promise.resolve();
  }, () => true, (error) => { errors.push(error); });
  for (const now of [0, 16, 32, 48]) {
    frames.shift()!(now);
    await setImmediate();
  }
  assert.deepEqual(errors, [clockError, encodeError, queueError]);
  assert.equal(renders, 3);
});
