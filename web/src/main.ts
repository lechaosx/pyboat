import { BrowserGame } from "@pyboat/browser";
import "./style.css";

const canvas = document.querySelector<HTMLCanvasElement>("#game");
const status = document.querySelector<HTMLElement>("#status");

if (!canvas || !status) {
  throw new Error("The game canvas is missing");
}

try {
  const game = await BrowserGame.create(canvas);
  status.textContent = "Rapier physics · WebGPU rendering";
  let previousTimeMs = performance.now();

  function frame(timeMs: number): void {
    game.frame((timeMs - previousTimeMs) / 1000);
    previousTimeMs = timeMs;
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
} catch (error) {
  status.textContent = "WebGPU could not start in this browser";
  throw error;
}
