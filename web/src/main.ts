import { BrowserGame } from "browser";
import "./style.css";

const canvas = document.querySelector<HTMLCanvasElement>("#game");
const status = document.querySelector<HTMLElement>("#status");
const speed = document.querySelector<HTMLElement>("#speed");
const apparentWind = document.querySelector<HTMLElement>("#apparent-wind");
const pointOfSail = document.querySelector<HTMLElement>("#point-of-sail");
const health = document.querySelector<HTMLElement>("#health");
const waterDepth = document.querySelector<HTMLElement>("#water-depth");
const bottomClearance = document.querySelector<HTMLElement>("#bottom-clearance");
const bottomContact = document.querySelector<HTMLElement>("#bottom-contact");
const scaleLabel = document.querySelector<HTMLElement>("#scale-label");
const scaleLine = document.querySelector<HTMLElement>("#scale-line");
const pressedKeys = new Set<string>();
const controlKeys = new Set(["KeyW", "KeyS", "KeyA", "KeyD"]);

if (
  !canvas ||
  !status ||
  !speed ||
  !apparentWind ||
  !pointOfSail ||
  !health ||
  !waterDepth ||
  !bottomClearance ||
  !bottomContact ||
  !scaleLabel ||
  !scaleLine
) {
  throw new Error("The game page is incomplete");
}
const statusElement = status;
const speedElement = speed;
const apparentWindElement = apparentWind;
const pointOfSailElement = pointOfSail;
const healthElement = health;
const waterDepthElement = waterDepth;
const bottomClearanceElement = bottomClearance;
const bottomContactElement = bottomContact;
const scaleLabelElement = scaleLabel;
const scaleLineElement = scaleLine;

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
  let previousTimeMs = performance.now();

  canvas.addEventListener(
    "wheel",
    (event) => {
      const deltaScale =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? 16
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? canvas.clientHeight
            : 1;
      game.zoom(event.deltaY * deltaScale);
      event.preventDefault();
    },
    { passive: false },
  );
  canvas.addEventListener("mousedown", (event) => {
    if (event.button === 1) {
      game.reset_zoom();
      event.preventDefault();
    }
  });
  canvas.addEventListener("auxclick", (event) => {
    if (event.button === 1) {
      event.preventDefault();
    }
  });

  function frame(timeMs: number): void {
    const gamepad = firstGamepad();
    const worldX = gamepad?.axes[0] ?? 0;
    const worldY = -(gamepad?.axes[1] ?? 0);
    game.set_player_input(
      keyboardAxis("KeyW", "KeyS"),
      keyboardAxis("KeyD", "KeyA"),
      worldX,
      worldY,
    );
    game.frame((timeMs - previousTimeMs) / 1000);
    statusElement.textContent = game.status();
    speedElement.textContent = game.speed_m_s().toFixed(1);
    apparentWindElement.textContent = game.apparent_wind_speed_m_s().toFixed(1);
    pointOfSailElement.textContent = game.point_of_sail();
    healthElement.textContent = game.health().toFixed(0);
    waterDepthElement.textContent = game.water_depth_m().toFixed(1);
    bottomClearanceElement.textContent = game.bottom_clearance_m().toFixed(1);
    const bottomContactFraction = game.bottom_contact_fraction();
    bottomContactElement.textContent =
      bottomContactFraction >= 1
        ? "grounded"
        : bottomContactFraction > 0
          ? `dragging ${(bottomContactFraction * 100).toFixed(0)}%`
          : "clear";
    bottomContactElement.classList.toggle("active", bottomContactFraction > 0);
    scaleLabelElement.textContent = game.scale_bar_label();
    scaleLineElement.style.width = `${game.scale_bar_width_px()}px`;
    previousTimeMs = timeMs;
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
} catch (error) {
  statusElement.textContent = "The game could not start in this browser";
  throw error;
}
