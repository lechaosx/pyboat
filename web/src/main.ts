import { pointerVector, worldSteering } from "./steering";
import "./style.css";
import { SailingAudio } from "./audio";

const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
const status = document.querySelector<HTMLElement>("#status")!;
const welcome = document.querySelector<HTMLElement>("#welcome")!;
const welcomeTitle = document.querySelector<HTMLElement>("#welcome-title")!;
const home = document.querySelector<HTMLElement>("#menu-home")!;
const settings = document.querySelector<HTMLElement>("#settings")!;
const graphicsOpen = document.querySelector<HTMLButtonElement>("#graphics-open")!;
const graphicsClose = document.querySelector<HTMLButtonElement>("#graphics-close")!;
const liveGraphicsOpen = document.querySelector<HTMLButtonElement>("#live-graphics-open")!;
const menuButton = document.querySelector<HTMLButtonElement>("#menu-open")!;
const joinForm = document.querySelector<HTMLFormElement>("#join-form")!;
const nickname = document.querySelector<HTMLInputElement>("#nickname")!;
const joinButton = document.querySelector<HTMLButtonElement>("#join")!;
const leaveAction = document.querySelector<HTMLElement>("#leave-action")!;
const leaveButton = document.querySelector<HTMLButtonElement>("#leave")!;
const fps = document.querySelector<HTMLElement>("#fps")!;
const graphicsForm = document.querySelector<HTMLFormElement>("#graphics-form")!;
const graphicsNote = document.querySelector<HTMLElement>("#graphics-note")!;
const graphicsResolution = document.querySelector<HTMLElement>("#graphics-resolution")!;
const liveFleet = document.querySelector<HTMLElement>("#live-fleet")!;
const spectatorSection = document.querySelector<HTMLElement>("#spectators")!;
const spectatorList = document.querySelector<HTMLElement>("#spectator-list")!;
const livePlayerList = document.querySelector<HTMLElement>("#live-player-list")!;
const loading = document.querySelector<HTMLDetailsElement>("#loading")!;
const loadStage = document.querySelector<HTMLElement>("#load-stage")!;
const loadLog = document.querySelector<HTMLElement>("#load-log")!;
const errorDetails = document.querySelector<HTMLDetailsElement>("#error-details")!;
const errorMessage = document.querySelector<HTMLElement>("#error-message")!;
const scale = document.querySelector<HTMLElement>("#scale")!;
const scaleLabel = document.querySelector<HTMLElement>("#scale-label")!;
const scaleLine = document.querySelector<HTMLElement>("#scale-line")!;
const graphicsStorageKey = "graphics-settings";
let savedGraphics = new URLSearchParams();
try { savedGraphics = new URLSearchParams(localStorage.getItem(graphicsStorageKey) ?? ""); } catch { /* Session settings still work without storage. */ }
const graphicsOverrides = new URLSearchParams(location.search);
const pressedKeys = new Set<string>();
const controlKeys = new Set(["KeyW", "KeyS", "KeyA", "KeyD"]);
const gameCanvas = canvas;
const audio = new SailingAudio();
audio.setActive(false);
document.querySelector("#sound-settings")!.append(audio.element);
const names = ["Swift", "Quiet", "Brave", "Wandering", "Lucky", "Salty"];
const ships = ["Gull", "Tern", "Albatross", "Mariner", "Petrel", "Heron"];
nickname.value = `${names[Math.floor(Math.random() * names.length)]} ${ships[Math.floor(Math.random() * ships.length)]} ${Math.floor(Math.random() * 90) + 10}`;

function firstInteraction(event: Event): void {
  if (event.target instanceof Node && audio.element.contains(event.target)) return;
  void audio.start();
}
window.addEventListener("keydown", event => {
  if (!welcome.hidden && event.code === "Tab") {
    const controls = [...welcome.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), summary')].filter(control => control.getClientRects().length);
    const first = controls[0], last = controls.at(-1);
    if (!controls.some(control => control === document.activeElement) || document.activeElement === (event.shiftKey ? first : last)) {
      (event.shiftKey ? last : first)?.focus();
      event.preventDefault();
    }
  }
});
window.addEventListener("click", firstInteraction);
window.addEventListener("keydown", firstInteraction);
window.addEventListener("pageshow", event => { if (event.persisted) location.reload(); });
function keyboardAxis(positive: string, negative: string): number {
  return Number(pressedKeys.has(positive)) - Number(pressedKeys.has(negative));
}
function firstGamepad(): Gamepad | null {
  if (!document.hasFocus() || document.hidden) return null;
  return [...navigator.getGamepads()].find(gamepad => gamepad?.connected && gamepad.axes.length >= 2) ?? null;
}
function stage(label: string): void {
  if (loadStage.textContent === label) return;
  const item = document.createElement("li"); item.textContent = label;
  loadLog.append(item);
  loadStage.textContent = label;
}
function showPanel(panel: "home" | "settings" | "play"): void {
  welcome.hidden = panel === "play";
  home.hidden = panel !== "home";
  settings.hidden = panel !== "settings";
  liveFleet.hidden = panel !== "play";
  welcome.setAttribute("aria-labelledby", panel === "settings" ? "settings-title" : "welcome-title");
  welcome.scrollTop = 0;
}
function showFailure(title: string, message: string, detail: unknown): void {
  showPanel("home");
  leaveAction.hidden = true;
  welcomeTitle.textContent = title; status.textContent = message; status.hidden = false;
  loading.hidden = true;
  errorMessage.textContent = String(detail); errorDetails.hidden = false;
  nickname.disabled = true; nickname.required = false;
  joinButton.disabled = false; joinButton.textContent = "Reload game";
  fps.hidden = true;
  audio.setActive(false);
  document.querySelector<HTMLFieldSetElement>("#graphics-controls")!.disabled = true;
  joinButton.focus();
}
let settingsOpener = graphicsOpen;
let prepareSettings = (): void => {};
function openSettings(opener: HTMLButtonElement): void {
  settingsOpener = opener;
  prepareSettings();
  graphicsClose.textContent = opener === liveGraphicsOpen ? "Done" : "Back";
  showPanel("settings");
  graphicsClose.focus();
}
function closeSettings(): void {
  showPanel(settingsOpener === liveGraphicsOpen ? "play" : "home");
  settingsOpener.focus();
}
graphicsOpen.addEventListener("click", () => openSettings(graphicsOpen));
liveGraphicsOpen.addEventListener("click", () => openSettings(liveGraphicsOpen));
graphicsClose.addEventListener("click", closeSettings);
window.addEventListener("keydown", event => {
  if (event.code !== "Escape" || settings.hidden) return;
  event.preventDefault();
  if (!event.repeat) closeSettings();
});

try {
  stage("Downloading and starting the game engine");
  const { BrowserGame } = await import("browser");
  const game = await BrowserGame.create(canvas, savedGraphics.toString(), async (label: string) => {
    stage(label);
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  });
  document.querySelector<HTMLFieldSetElement>("#graphics-controls")!.disabled = false;
  let previousTimeMs = performance.now();
  let lastPresentedFrames = game.presented_frames();
  let fpsStart: { timeMs: number; frames: number } | undefined;
  let nextGraphicsUpdateMs = 0;
  let currentState = "";
  let renderingStopped = false;
  let pageEnded = false;
  let nextRosterMs = 0;
  let lastRoster = "";
  let selectedPlayer: string | undefined;

  function endSession(): void {
    if (pageEnded) return;
    pageEnded = true; clearInput(); audio.setActive(false); game.dispose();
    window.removeEventListener("click", firstInteraction);
    window.removeEventListener("keydown", firstInteraction);
  }
  window.addEventListener("pagehide", endSession, { once: true });
  if (import.meta.hot) import.meta.hot.dispose(endSession);
  function inputEnabled(): boolean {
    return currentState === "playing" && welcome.hidden && document.hasFocus() && !document.hidden;
  }
  function setMenu(open: boolean): void {
    clearInput();
    showPanel(open ? "home" : "play");
    (open ? joinButton : menuButton).focus();
  }
  prepareSettings = (): void => {
    if (renderingStopped) return;
    clearInput();
    updateGraphicsForm();
  };
  menuButton.addEventListener("click", () => setMenu(true));

  function updateGraphicsForm(): void {
    const values = new URLSearchParams(game.graphics_settings());
    for (const control of graphicsForm.elements) {
      if (!(control instanceof HTMLInputElement || control instanceof HTMLSelectElement)) continue;
      const value = values.get(control.name);
      if (value === null) continue;
      if (control instanceof HTMLInputElement && control.type === "checkbox") control.checked = value === "1";
      else {
        if (control instanceof HTMLSelectElement && ![...control.options].some(option => option.value === value)) {
          const label = control.name === "fps" ? `${value} FPS · custom` : `${Math.round(Number(value) * 100)}% · custom`;
          control.add(new Option(label, value));
        }
        control.value = value;
      }
    }
    updateGraphicsResolution(values);
  }
  function updateGraphicsResolution(values: URLSearchParams): void {
    graphicsResolution.textContent = `Current scene resolution is ${Math.round(Number(values.get("effectiveRenderScale")) * 100)}% of output. Boats render at full output resolution.`;
  }
  function applyGraphics(): void {
    game.set_graphics_settings(savedGraphics.toString(), graphicsOverrides.toString());
    try {
      if (savedGraphics.size) localStorage.setItem(graphicsStorageKey, savedGraphics.toString());
      else localStorage.removeItem(graphicsStorageKey);
      const overridden = [...graphicsForm.elements].some(control => (control instanceof HTMLInputElement || control instanceof HTMLSelectElement) && new URLSearchParams(location.search).has(control.name));
      graphicsNote.textContent = `Saved on this device.${overridden ? " URL options apply again on reload." : ""}`;
    } catch { graphicsNote.textContent = "Applied for this visit. Device storage is unavailable."; }
    updateGraphicsForm();
  }
  updateGraphicsForm();
  graphicsForm.addEventListener("submit", event => event.preventDefault());
  graphicsForm.addEventListener("change", event => {
    const control = event.target;
    if (!(control instanceof HTMLInputElement || control instanceof HTMLSelectElement) || !graphicsForm.reportValidity()) return;
    const value = control instanceof HTMLInputElement ? control.type === "checkbox" ? (control.checked ? "1" : "0") : control.valueAsNumber.toString().replace("e+", "e") : control.value;
    graphicsOverrides.delete(control.name);
    if (value === "") savedGraphics.delete(control.name); else savedGraphics.set(control.name, value);
    applyGraphics();
  });
  document.querySelector("#graphics-reset")!.addEventListener("click", () => {
    for (const control of graphicsForm.elements) if (control instanceof HTMLInputElement || control instanceof HTMLSelectElement) graphicsOverrides.delete(control.name);
    savedGraphics = new URLSearchParams(); applyGraphics();
  });

  function updateSession(): void {
    const next = game.session_state();
    if (next === currentState) return;
    const wasLeaving = currentState === "leaving";
    currentState = next;
    lastRoster = "";
    const disconnected = currentState === "disconnected";
    const ready = currentState === "ready" || currentState === "dead";
    const playing = currentState === "playing";
    const leaving = currentState === "leaving";
    const busy = !ready && !playing && !disconnected;
    loading.hidden = !busy || leaving;
    welcomeTitle.textContent = playing ? "At the helm" : currentState === "dead" ? "Ship lost" : disconnected ? "Connection lost" : "Under sail";
    status.textContent = playing ? "Your boat is still at sea." : leaving ? "Leaving your boat…" : currentState === "dead" ? "Your boat was destroyed. Join again when you’re ready." : disconnected ? game.status() : ready ? "" : currentState === "joining" ? "Launching your boat…" : currentState === "connecting" ? "Establishing a secure connection…" : "Waiting for the first view of the sea…";
    status.hidden = currentState === "ready";
    if (currentState === "connecting") stage("Connecting to the match");
    else if (currentState === "loading") stage("Receiving the world and drawing the first frame");
    nickname.disabled = playing || leaving || currentState === "joining" || disconnected;
    nickname.required = !nickname.disabled;
    joinButton.textContent = playing ? "Resume" : leaving ? "Leaving…" : disconnected ? "Reconnect" : currentState === "dead" ? "Join again" : ready ? "Join" : currentState === "joining" ? "Launching…" : "Loading…";
    joinButton.disabled = busy;
    leaveAction.hidden = !playing || !game.supports_leaving_boat();
    spectatorSection.hidden = playing || leaving || disconnected || spectatorList.childElementCount === 0;
    if (playing) setMenu(false);
    else if (currentState === "dead" || disconnected) setMenu(true);
    else {
      clearInput();
      if (ready && wasLeaving && !home.hidden) joinButton.focus();
    }
  }
  function updatePlayers(): void {
    const own = game.controlled_boat_id();
    const players = game.players() as [string, string][];
    if (selectedPlayer && !players.some(([id]) => id === selectedPlayer)) selectedPlayer = undefined;
    const key = JSON.stringify([players, own, currentState, selectedPlayer]);
    if (key === lastRoster) return;
    lastRoster = key;
    spectatorList.replaceChildren(); livePlayerList.replaceChildren();
    document.querySelector("#fleet-count")!.textContent = String(players.length);
    for (const [id, name] of players) {
      const button = document.createElement("button"); button.type = "button"; button.textContent = name;
      button.setAttribute("aria-pressed", String(id === selectedPlayer));
      button.addEventListener("click", () => { selectedPlayer = id; game.spectate(id); lastRoster = ""; updatePlayers(); });
      spectatorList.append(button);
      if (id !== own) { const row = document.createElement("span"); row.textContent = name; livePlayerList.append(row); }
    }
    spectatorSection.hidden = currentState === "playing" || currentState === "leaving" || currentState === "disconnected" || !players.length;
  }
  joinForm.addEventListener("submit", event => {
    event.preventDefault();
    try {
      clearInput();
      if (renderingStopped) { location.reload(); return; }
      if (currentState === "playing") { setMenu(false); return; }
      if (currentState === "disconnected") game.reconnect();
      else {
        const name = nickname.value.trim();
        nickname.setCustomValidity(!name ? "Enter a captain’s name." : [...name].length > 24 ? "Use at most 24 characters." : "");
        if (!nickname.reportValidity()) return;
        game.join(name);
      }
      updateSession();
    } catch (error) { status.textContent = "That did not work. Please try again."; status.hidden = false; errorDetails.hidden = false; errorMessage.textContent = String(error); }
  });
  leaveButton.addEventListener("click", () => {
    try {
      clearInput();
      game.leave_boat();
      updateSession();
    } catch (error) { status.textContent = "That did not work. Please try again."; status.hidden = false; errorDetails.hidden = false; errorMessage.textContent = String(error); }
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
    if (event.code === "Escape" && currentState === "playing" && !event.defaultPrevented) {
      event.preventDefault();
      if (!event.repeat) setMenu(Boolean(welcome.hidden));
      return;
    }
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
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) return;
    clearInput();
    fpsStart = undefined;
    fps.hidden = true;
  });
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

  function frame(timeMs: number): void {
    if (pageEnded) return;
    const elapsedSeconds = (timeMs - previousTimeMs) / 1000;
    previousTimeMs = timeMs;
    try {
      const gamepad = inputEnabled() ? firstGamepad() : null;
      const [worldX, worldY] = worldSteering(gamepad?.axes, pointer?.vector ?? [0, 0]);
      game.set_player_input(keyboardAxis("KeyW", "KeyS"), keyboardAxis("KeyA", "KeyD"), worldX, worldY);
      game.frame(elapsedSeconds);
      updateSession();
      audio.setActive(!["connecting", "loading", "disconnected"].includes(currentState));
      if (audio.due(timeMs)) audio.update(game.sound_scene(timeMs / 1000, audio.playbackEpoch));
      if (timeMs >= nextRosterMs) { updatePlayers(); nextRosterMs = timeMs + 250; }
      if (!settings.hidden && timeMs >= nextGraphicsUpdateMs) {
        updateGraphicsResolution(new URLSearchParams(game.graphics_settings()));
        nextGraphicsUpdateMs = timeMs + 1000;
      }
      // The rudder uses counterclockwise world angles; CSS rotates clockwise.
      wheel.style.transform = `rotate(${-game.rudder_angle_rad()}rad)`;
      const frames = game.presented_frames();
      if (["ready", "playing", "dead", "joining", "leaving"].includes(currentState)) {
        if (!document.hidden && frames > lastPresentedFrames) {
          if (!fpsStart) fpsStart = { timeMs, frames };
          else if (timeMs - fpsStart.timeMs >= 1000) {
            const rate = (frames - fpsStart.frames) * 1000 / (timeMs - fpsStart.timeMs);
            fps.textContent = `${rate < 1 ? rate.toPrecision(1) : Math.round(rate)} FPS`;
            fps.hidden = false;
            fpsStart = { timeMs, frames };
          }
        }
        const width = game.scale_bar_width_px();
        if (Number.isFinite(width) && width > 0) {
          scale.hidden = false; scaleLabel.textContent = game.scale_bar_label(); scaleLine.style.width = `${width}px`;
        }
      } else {
        fpsStart = undefined;
        fps.hidden = true;
      }
      lastPresentedFrames = frames;
    } catch (error) {
      console.error(error);
      clearInput(); game.dispose(); renderingStopped = true; currentState = "disconnected";
      showFailure("Graphics stopped", "The graphics device became unavailable. Reload to return to sea.", error);
      return;
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
} catch (error) {
  const memoryLimited = /not enough memory|out of memory/i.test(String(error));
  showFailure("Could not set sail", memoryLimited ? "The browser could not allocate graphics memory. Close this tab and open the game again." : "The game could not start. Reload to try again.", error);
  joinForm.addEventListener("submit", event => { event.preventDefault(); location.reload(); });
  console.error(error);
}
