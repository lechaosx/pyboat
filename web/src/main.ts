import { BrowserGame } from "browser";
import { pointerVector, worldSteering } from "./steering";
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
const welcome = document.querySelector<HTMLElement>("#welcome")!;
const welcomeTitle = document.querySelector<HTMLElement>("#welcome-title")!;
const joinForm = document.querySelector<HTMLFormElement>("#join-form")!;
const nickname = document.querySelector<HTMLInputElement>("#nickname")!;
const joinButton = document.querySelector<HTMLButtonElement>("#join")!;
const hud = document.querySelector<HTMLElement>("#hud")!;
const captain = document.querySelector<HTMLElement>("#captain")!;
const fps = document.querySelector<HTMLElement>("#fps")!;
const graphicsOpen = document.querySelector<HTMLButtonElement>("#graphics-open")!;
const graphicsDialog = document.querySelector<HTMLDialogElement>("#graphics-dialog")!;
const graphicsForm = document.querySelector<HTMLFormElement>("#graphics-form")!;
const graphicsNote = document.querySelector<HTMLElement>("#graphics-note")!;
const graphicsResolution = document.querySelector<HTMLElement>("#graphics-resolution")!;
const graphicsStorageKey = "graphics-settings";
let savedGraphics = new URLSearchParams();
try { savedGraphics = new URLSearchParams(localStorage.getItem(graphicsStorageKey) ?? ""); } catch { /* Session settings still work without storage. */ }
const graphicsOverrides = new URLSearchParams(location.search);
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
const gameCanvas = canvas;

function keyboardAxis(positive: string, negative: string): number {
  return Number(pressedKeys.has(positive)) - Number(pressedKeys.has(negative));
}

function firstGamepad(): Gamepad | null {
  if (!document.hasFocus() || document.hidden) return null;
  for (const gamepad of navigator.getGamepads()) {
    if (gamepad?.connected && gamepad.axes.length >= 2) {
      return gamepad;
    }
  }
  return null;
}

try {
  const game = await BrowserGame.create(canvas, savedGraphics.toString());
  let previousTimeMs = performance.now();
  let fpsStartMs = previousTimeMs;
  let fpsStartFrames = game.presented_frames();
  let currentState = "loading";
  let renderingStopped = false;

  function inputEnabled(): boolean {
    return currentState === "playing" && !graphicsDialog.open && document.hasFocus() && !document.hidden;
  }

  function updateGraphicsForm(): void {
    const settings = new URLSearchParams(game.graphics_settings());
    for (const control of graphicsForm.elements) {
      if (!(control instanceof HTMLInputElement || control instanceof HTMLSelectElement)) continue;
      const value = settings.get(control.name);
      if (value === null) continue;
      if (control instanceof HTMLInputElement && control.type === "checkbox") {
        control.checked = value === "1";
      } else {
        if (control instanceof HTMLSelectElement && ![...control.options].some(option => option.value === value)) {
          control.add(new Option(value, value));
        }
        control.value = value;
      }
    }
    updateGraphicsResolution(settings);
  }

  function updateGraphicsResolution(settings: URLSearchParams): void {
    graphicsResolution.textContent = `Current scene: ${Math.round(Number(settings.get("effectiveRenderScale")) * 100)}% of output. Automatic saves detail on high-density screens. Boats stay sharp.`;
  }

  function applyGraphics(): void {
    game.set_graphics_settings(savedGraphics.toString(), graphicsOverrides.toString());
    try {
      if (savedGraphics.size) localStorage.setItem(graphicsStorageKey, savedGraphics.toString());
      else localStorage.removeItem(graphicsStorageKey);
      const urlOverrides = [...graphicsForm.elements].some(control =>
        (control instanceof HTMLInputElement || control instanceof HTMLSelectElement) && new URLSearchParams(location.search).has(control.name));
      graphicsNote.textContent = `Saved on this device.${urlOverrides ? " URL graphics options apply again on reload." : ""}`;
    } catch {
      graphicsNote.textContent = "Applied for this visit. Browser storage is unavailable.";
    }
    updateGraphicsForm();
  }

  graphicsOpen.disabled = false;
  graphicsOpen.addEventListener("click", () => {
    clearInput();
    updateGraphicsForm();
    graphicsDialog.showModal();
  });
  graphicsForm.addEventListener("submit", event => event.preventDefault());
  graphicsForm.addEventListener("change", event => {
    const control = event.target;
    if (!(control instanceof HTMLInputElement || control instanceof HTMLSelectElement) || !graphicsForm.reportValidity()) return;
    const value = control instanceof HTMLInputElement
      ? control.type === "checkbox" ? (control.checked ? "1" : "0") : control.valueAsNumber.toString().replace("e+", "e")
      : control.value;
    graphicsOverrides.delete(control.name);
    if (value === "") savedGraphics.delete(control.name);
    else savedGraphics.set(control.name, value);
    applyGraphics();
  });
  document.querySelector("#graphics-reset")!.addEventListener("click", () => {
    for (const control of graphicsForm.elements) {
      if (control instanceof HTMLInputElement || control instanceof HTMLSelectElement) graphicsOverrides.delete(control.name);
    }
    savedGraphics = new URLSearchParams();
    applyGraphics();
  });
  document.querySelector("#graphics-close")!.addEventListener("click", () => graphicsDialog.close());
  graphicsDialog.addEventListener("close", clearInput);

  function updateSession(): void {
    const nextState = game.session_state();
    if (nextState !== currentState) clearInput();
    currentState = nextState;
    welcome.hidden = currentState === "playing";
    hud.hidden = currentState !== "playing";
    const disconnected = currentState === "disconnected";
    const ready = currentState === "ready" || currentState === "dead";
    welcomeTitle.textContent = currentState === "dead" ? "Ship lost" : disconnected ? "Disconnected" : "Under sail";
    statusElement.textContent = currentState === "dead" ? "Your boat was destroyed. Its wreck remains at sea." : ready ? "The sea is ready. Choose a name and set sail." : currentState === "joining" ? "Launching your boat…" : game.status();
    nickname.disabled = currentState === "joining" || disconnected;
    nickname.required = !disconnected;
    joinButton.textContent = disconnected ? "Reconnect" : currentState === "dead" ? "Set sail again" : ready ? "Set sail" : currentState === "joining" ? "Launching…" : "Loading…";
    joinButton.disabled = !disconnected && !ready;
    captain.textContent = game.nickname();
  }

  joinForm.addEventListener("submit", (event) => {
    event.preventDefault();
    try {
      clearInput();
      if (renderingStopped) { location.reload(); return; }
      if (currentState === "disconnected") {
        game.reconnect();
      } else {
        const name = nickname.value.trim();
        nickname.setCustomValidity(!name ? "Enter a nickname." : [...name].length > 24 ? "Use at most 24 characters." : "");
        if (!nickname.reportValidity()) return;
        game.join(name);
      }
      updateSession();
    } catch (error) {
      statusElement.textContent = String(error);
    }
  });
  nickname.addEventListener("input", () => nickname.setCustomValidity(""));
  let pointer: { id: number; x: number; y: number; vector: [number, number] } | undefined;
  const pointerRadiusPx = 64;
  const helm = document.createElement("div");
  helm.className = "pointer-helm";
  helm.hidden = true;
  helm.setAttribute("aria-hidden", "true");
  helm.innerHTML = `<svg class="helm-wheel" viewBox="0 0 140 140">
    <defs>
      <linearGradient id="helm-wood" x2="1" y2="1"><stop stop-color="#f3c47a"/><stop offset=".45" stop-color="#aa652e"/><stop offset="1" stop-color="#633517"/></linearGradient>
      <radialGradient id="helm-brass"><stop stop-color="#ffe3a0"/><stop offset=".6" stop-color="#c99d50"/><stop offset="1" stop-color="#775326"/></radialGradient>
    </defs>
    <g stroke="#4c2a18" stroke-width="10" stroke-linecap="round">
      <path d="M70 12V128 M12 70H128 M29 29L111 111 M29 111L111 29"/>
    </g>
    <g stroke="url(#helm-wood)" stroke-width="7" stroke-linecap="round">
      <path d="M70 12V128 M12 70H128 M29 29L111 111 M29 111L111 29"/>
    </g>
    <circle cx="70" cy="70" r="42" fill="none" stroke="#4c2a18" stroke-width="15"/>
    <circle cx="70" cy="70" r="42" fill="none" stroke="url(#helm-wood)" stroke-width="11"/>
    <circle cx="70" cy="70" r="47" fill="none" stroke="#ddb566" stroke-width="1.5"/>
    <circle cx="70" cy="70" r="37" fill="none" stroke="#ddb566" stroke-width="1.5"/>
    <circle cx="70" cy="70" r="12" fill="url(#helm-brass)" stroke="#61401f" stroke-width="2"/>
    <circle cx="70" cy="70" r="4" fill="#835a2d"/>
    <path d="M67 10H73V22H67Z" fill="#f7d896"/>
  </svg><span class="helm-direction"></span>`;
  canvas.parentElement!.append(helm);
  const wheel = helm.querySelector<SVGElement>(".helm-wheel")!;
  const direction = helm.querySelector<HTMLElement>(".helm-direction")!;

  function releasePointer(): void {
    const id = pointer?.id;
    pointer = undefined;
    helm.hidden = true;
    if (id !== undefined && gameCanvas.hasPointerCapture(id)) gameCanvas.releasePointerCapture(id);
  }

  function clearInput(): void {
    pressedKeys.clear();
    releasePointer();
    game.set_player_input(0, 0, 0, 0);
  }

  window.addEventListener("keydown", (event) => {
    if (inputEnabled() && controlKeys.has(event.code)) {
      pressedKeys.add(event.code);
      event.preventDefault();
    }
  });
  window.addEventListener("keyup", (event) => {
    if (controlKeys.has(event.code)) {
      pressedKeys.delete(event.code);
      if (inputEnabled()) event.preventDefault();
    }
  });
  window.addEventListener("blur", clearInput);
  window.addEventListener("resize", clearInput);
  document.addEventListener("visibilitychange", () => { if (document.hidden) clearInput(); });
  canvas.addEventListener("pointerdown", (event) => {
    if (!inputEnabled() || !event.isPrimary || event.button !== 0 || pointer) return;
    pointer = { id: event.pointerId, x: event.clientX, y: event.clientY, vector: [0, 0] };
    canvas.setPointerCapture(event.pointerId);
    helm.style.left = `${event.clientX}px`;
    helm.style.top = `${event.clientY}px`;
    direction.style.transform = "translate(0, 0)";
    helm.hidden = false;
    event.preventDefault();
  });
  canvas.addEventListener("pointermove", (event) => {
    if (pointer?.id !== event.pointerId) return;
    pointer.vector = pointerVector(
      (event.clientX - pointer.x) / pointerRadiusPx,
      (event.clientY - pointer.y) / pointerRadiusPx,
    );
    direction.style.transform = `translate(${pointer.vector[0] * pointerRadiusPx}px, ${-pointer.vector[1] * pointerRadiusPx}px)`;
    event.preventDefault();
  });
  const finishPointer = (event: PointerEvent): void => {
    if (pointer?.id === event.pointerId) releasePointer();
  };
  canvas.addEventListener("pointerup", finishPointer);
  canvas.addEventListener("pointercancel", finishPointer);
  canvas.addEventListener("lostpointercapture", finishPointer);

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
    const elapsedSeconds = (timeMs - previousTimeMs) / 1000;
    previousTimeMs = timeMs;
    try {
      const gamepad = inputEnabled() ? firstGamepad() : null;
      const [worldX, worldY] = worldSteering(gamepad?.axes, pointer?.vector ?? [0, 0]);
      game.set_player_input(
        keyboardAxis("KeyW", "KeyS"),
        keyboardAxis("KeyA", "KeyD"),
        worldX,
        worldY,
      );
      game.frame(elapsedSeconds);
      updateSession();
      if (timeMs - fpsStartMs >= 1000) {
        const frames = game.presented_frames();
        fps.textContent = `${Math.round((frames - fpsStartFrames) * 1000 / (timeMs - fpsStartMs))} FPS`;
        fpsStartMs = timeMs;
        fpsStartFrames = frames;
        if (graphicsDialog.open) updateGraphicsResolution(new URLSearchParams(game.graphics_settings()));
      }
      // The rudder uses counterclockwise world angles; CSS rotates clockwise.
      wheel.style.transform = `rotate(${-game.rudder_angle_rad()}rad)`;
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
    } catch (error) {
      console.error(error);
      if (!game.renderer_is_available()) {
        clearInput();
        game.shutdown();
        renderingStopped = true;
        currentState = "disconnected";
        welcome.hidden = false;
        hud.hidden = true;
        welcomeTitle.textContent = "Rendering stopped";
        statusElement.textContent = "The graphics device became unavailable. Reload to reconnect.";
        nickname.disabled = true;
        nickname.required = false;
        joinButton.textContent = "Reload game";
        joinButton.disabled = false;
        fps.textContent = "— FPS";
        return;
      }
      statusElement.textContent = "Rendering failed; retrying";
    }
    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
} catch (error) {
  welcomeTitle.textContent = "Could not start";
  statusElement.textContent = `The game could not start: ${String(error)}`;
  nickname.disabled = true;
  nickname.required = false;
  joinButton.textContent = "Reload game";
  joinButton.disabled = false;
  joinForm.addEventListener("submit", (event) => { event.preventDefault(); location.reload(); });
  console.error(error);
}
