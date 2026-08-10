import { BrowserGame } from "@pyboat/browser";
import "./style.css";

const canvas = document.querySelector<HTMLCanvasElement>("#game");
const status = document.querySelector<HTMLElement>("#status");
const pressedKeys = new Set<string>();
const controlKeys = new Set(["KeyW", "KeyS", "KeyA", "KeyD"]);

if (!canvas || !status) {
  throw new Error("The game canvas is missing");
}

window.addEventListener("keydown", (event) => {
  if (controlKeys.has(event.code)) {
    pressedKeys.add(event.code);
    event.preventDefault();
  }
});
window.addEventListener("keyup", (event) => {
  if (controlKeys.has(event.code)) {
    pressedKeys.delete(event.code);
    event.preventDefault();
  }
});
window.addEventListener("blur", () => pressedKeys.clear());

function keyboardAxis(positive: string, negative: string): number {
  return Number(pressedKeys.has(positive)) - Number(pressedKeys.has(negative));
}

function firstGamepad(): Gamepad | null {
  for (const gamepad of navigator.getGamepads()) {
    if (gamepad?.connected && gamepad.axes.length >= 2) {
      return gamepad;
    }
  }
  return null;
}

try {
  const game = await BrowserGame.create(canvas);
  status.textContent = "Rapier physics · WebGPU rendering";
  let previousTimeMs = performance.now();

  function frame(timeMs: number): void {
    const gamepad = firstGamepad();
    game.set_player_input(
      keyboardAxis("KeyW", "KeyS"),
      keyboardAxis("KeyD", "KeyA"),
      gamepad?.axes[0] ?? 0,
      gamepad?.axes[1] ?? 0,
    );
    game.frame((timeMs - previousTimeMs) / 1000);
    previousTimeMs = timeMs;
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
} catch (error) {
  status.textContent = "WebGPU could not start in this browser";
  throw error;
}
