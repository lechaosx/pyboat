import { BrowserControllerComparison } from "browser";
import "./controller.css";

function element<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

const canvas = element<HTMLCanvasElement>("comparison-canvas");
const preset = element<HTMLSelectElement>("preset");
const controlSide = element<HTMLSelectElement>("control-side");
const reset = element<HTMLButtonElement>("reset");
const pause = element<HTMLButtonElement>("pause");
const status = element<HTMLOutputElement>("status");
const clock = element<HTMLOutputElement>("clock");
const flow = element<HTMLParagraphElement>("flow");
const stick = element<HTMLDivElement>("stick");
const handle = element<HTMLSpanElement>("stick-handle");
const keys = new Set<string>();
const controlKeys = new Set(["KeyW", "KeyS", "KeyA", "KeyD"]);
let pointerId: number | null = null;
let pointerX = 0;
let pointerY = 0;
let playing = true;

const fields = ["Forward intent", "Turn intent", "Water speed", "Turn rate", "Rudder / target", "Heading"];
const readouts = ["current-stats", "candidate-stats"].map((id) => {
  const list = element<HTMLDListElement>(id);
  return fields.map((label) => {
    const row = document.createElement("div");
    const title = document.createElement("dt");
    title.textContent = label;
    const value = document.createElement("dd");
    value.textContent = "—";
    row.append(title, value);
    list.append(row);
    return value;
  });
});

function releasePointer(): void {
  pointerId = null;
  pointerX = 0;
  pointerY = 0;
  handle.style.left = "50%";
  handle.style.top = "50%";
}

function releaseInput(): void {
  keys.clear();
  releasePointer();
}

window.addEventListener("keydown", (event) => {
  if (!controlKeys.has(event.code) || event.target instanceof HTMLSelectElement) return;
  keys.add(event.code);
  event.preventDefault();
});
window.addEventListener("keyup", (event) => {
  if (!controlKeys.has(event.code)) return;
  keys.delete(event.code);
  event.preventDefault();
});
window.addEventListener("blur", releaseInput);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) releaseInput();
});
document.addEventListener("focusin", (event) => {
  if (event.target instanceof HTMLSelectElement) releaseInput();
});

function moveStick(event: PointerEvent): void {
  const rect = stick.getBoundingClientRect();
  const radius = Math.min(rect.width, rect.height) / 2;
  const x = (event.clientX - rect.left - rect.width / 2) / radius;
  const y = -(event.clientY - rect.top - rect.height / 2) / radius;
  const scale = Math.max(1, Math.hypot(x, y));
  pointerX = x / scale;
  pointerY = y / scale;
  handle.style.left = `${50 + pointerX * 40}%`;
  handle.style.top = `${50 - pointerY * 40}%`;
}

stick.addEventListener("pointerdown", (event) => {
  if (pointerId !== null || event.button !== 0) return;
  pointerId = event.pointerId;
  stick.setPointerCapture(event.pointerId);
  moveStick(event);
  event.preventDefault();
});
stick.addEventListener("pointermove", (event) => {
  if (event.pointerId === pointerId) moveStick(event);
});
stick.addEventListener("lostpointercapture", releasePointer);
for (const name of ["pointerup", "pointercancel"] as const) {
  stick.addEventListener(name, (event) => {
    if (event.pointerId !== pointerId) return;
    releasePointer();
  });
}

try {
  const game = await BrowserControllerComparison.create(canvas);
  let previousTime = performance.now();
  let stopped = false;
  document.addEventListener("visibilitychange", () => { previousTime = performance.now(); });
  reset.disabled = false;
  pause.disabled = false;
  status.textContent = "Ready · keyboard, gamepad, or the on-screen stick. Both boats receive released input until you steer.";

  function resetBoth(): void {
    releaseInput();
    game.reset(Number(preset.value));
    previousTime = performance.now();
  }
  reset.addEventListener("click", resetBoth);
  preset.addEventListener("change", resetBoth);
  controlSide.addEventListener("change", releaseInput);
  pause.addEventListener("click", () => {
    playing = !playing;
    pause.textContent = playing ? "Pause" : "Resume";
    previousTime = performance.now();
  });
  canvas.addEventListener("wheel", (event) => {
    const scale = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16
      : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? canvas.clientHeight : 1;
    game.zoom(event.deltaY * scale);
    event.preventDefault();
  }, { passive: false });

  function frame(now: number): void {
    if (stopped) return;
    try {
      const elapsed = document.hidden ? 0 : (now - previousTime) / 1000;
      previousTime = now;
      const pad = Array.from(navigator.getGamepads()).find((p) => p?.connected && p.axes.length >= 2);
      const padX = pad?.axes[0] ?? 0;
      const padY = -(pad?.axes[1] ?? 0);
      const padActive = padX !== 0 || padY !== 0;
      const focused = document.hasFocus();
      const worldX = focused ? (padActive ? padX : pointerX) : 0;
      const worldY = focused ? (padActive ? padY : pointerY) : 0;
      const forward = Number(keys.has("KeyW")) - Number(keys.has("KeyS"));
      const turn = Number(keys.has("KeyA")) - Number(keys.has("KeyD"));
      game.set_player_input(forward, turn, worldX, worldY, Number(controlSide.value));
      game.frame(elapsed, playing);
      for (let side = 0; side < 2; side += 1) {
        const values = game.stats(side);
        if (side === 0) {
          clock.textContent = `${values[0]!.toFixed(1)} s${playing ? "" : " · paused"}`;
          flow.textContent = `World wind (${values[10]!.toFixed(2)}, ${values[11]!.toFixed(2)}) m/s · Current (${values[12]!.toFixed(2)}, ${values[13]!.toFixed(2)}) m/s`;
        }
        const texts = [
          values[1]!.toFixed(3), values[2]!.toFixed(3), `${values[3]!.toFixed(2)} m/s`,
          `${values[4]!.toFixed(2)} °/s`, `${values[5]!.toFixed(2)}° / ${values[6]!.toFixed(2)}°`,
          `${values[7]!.toFixed(1)}°`,
        ];
        texts.forEach((text, index) => { readouts[side]![index]!.textContent = text; });
      }
      requestAnimationFrame(frame);
    } catch (error) {
      stopped = true;
      releaseInput();
      reset.disabled = true;
      pause.disabled = true;
      status.textContent = String(error);
      console.error(error);
    }
  }
  window.addEventListener("pagehide", () => { stopped = true; releaseInput(); });
  resetBoth();
  requestAnimationFrame(frame);
} catch (error) {
  status.textContent = String(error);
  console.error(error);
}
