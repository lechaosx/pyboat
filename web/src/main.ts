import { pointerVector, worldSteering } from "./steering";
import "./style.css";
import { SailingAudio } from "./audio";

const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
const status = document.querySelector<HTMLElement>("#status")!;
const welcome = document.querySelector<HTMLElement>("#welcome")!;
const welcomeTitle = document.querySelector<HTMLElement>("#welcome-title")!;
const home = document.querySelector<HTMLElement>("#menu-home")!;
const settings = document.querySelector<HTMLElement>("#settings")!;
const joinForm = document.querySelector<HTMLFormElement>("#join-form")!;
const nickname = document.querySelector<HTMLInputElement>("#nickname")!;
const joinButton = document.querySelector<HTMLButtonElement>("#join")!;
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
      if (event.shiftKey && document.activeElement === first) { last?.focus(); event.preventDefault(); }
      else if (!event.shiftKey && document.activeElement === last) { first?.focus(); event.preventDefault(); }
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
function showFailure(title: string, message: string, detail: unknown): void {
  welcome.hidden = false; home.hidden = false; settings.hidden = true;
  welcomeTitle.textContent = title; status.textContent = message;
  loading.hidden = true;
  errorMessage.textContent = String(detail); errorDetails.hidden = false;
  nickname.disabled = true; nickname.required = false;
  joinButton.disabled = false; joinButton.textContent = "Reload game";
  liveFleet.hidden = true; fps.textContent = "— FPS";
  audio.setActive(false);
  document.querySelector<HTMLFieldSetElement>("#graphics-controls")!.disabled = true;
}
let engineReady = false;
window.addEventListener("keydown", event => { if (!engineReady && event.code === "Escape" && !settings.hidden) openSettings(false); });
let openSettings = (open: boolean): void => {
  home.hidden = open; settings.hidden = !open;
  if (open) document.querySelector<HTMLButtonElement>("#graphics-close")!.focus();
  else document.querySelector<HTMLButtonElement>("#graphics-open")!.focus();
  welcome.scrollTop = 0;
};
document.querySelector("#graphics-open")!.addEventListener("click", () => openSettings(true));
document.querySelector("#graphics-close")!.addEventListener("click", () => openSettings(false));

try {
  stage("Downloading and starting the game engine");
  const { BrowserGame } = await import("browser");
  const game = await BrowserGame.create(canvas, savedGraphics.toString(), async (label: string) => {
    stage(label);
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  });
  engineReady = true;
  document.querySelector<HTMLFieldSetElement>("#graphics-controls")!.disabled = false;
  let previousTimeMs = performance.now();
  let fpsStartMs = previousTimeMs;
  let fpsStartFrames = game.presented_frames();
  let currentState = "";
  let menuOpen = true;
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
    return currentState === "playing" && !menuOpen && document.hasFocus() && !document.hidden;
  }
  function setMenu(open: boolean): void {
    clearInput(); menuOpen = open; welcome.hidden = !open;
    liveFleet.hidden = open || currentState !== "playing";
    if (open) { showSettings(false); joinButton.focus(); }
  }
  function showSettings(open: boolean): void {
    clearInput();
    if (open) updateGraphicsForm();
    home.hidden = open; settings.hidden = !open;
    (document.querySelector(open ? "#graphics-close" : "#graphics-open") as HTMLButtonElement).focus();
    welcome.scrollTop = 0;
  }
  openSettings = showSettings;
  document.querySelector("#menu-open")!.addEventListener("click", () => setMenu(true));

  function updateGraphicsForm(): void {
    const values = new URLSearchParams(game.graphics_settings());
    for (const control of graphicsForm.elements) {
      if (!(control instanceof HTMLInputElement || control instanceof HTMLSelectElement)) continue;
      const value = values.get(control.name);
      if (value === null) continue;
      if (control instanceof HTMLInputElement && control.type === "checkbox") control.checked = value === "1";
      else {
        if (control instanceof HTMLSelectElement && ![...control.options].some(option => option.value === value)) control.add(new Option(value, value));
        control.value = value;
      }
    }
    updateGraphicsResolution(values);
  }
  function updateGraphicsResolution(values: URLSearchParams): void {
    graphicsResolution.textContent = `Scene resolution: ${Math.round(Number(values.get("effectiveRenderScale")) * 100)}% of output. Boats stay at display resolution.`;
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
    if (next !== currentState) {
      clearInput();
      const wasPlaying = currentState === "playing";
      currentState = next;
      if (next === "playing" && !wasPlaying) setMenu(false);
      else if (next !== "playing") { menuOpen = true; welcome.hidden = false; liveFleet.hidden = true; }
      if (["dead", "disconnected"].includes(next)) showSettings(false);
      lastRoster = "";
    }
    const disconnected = currentState === "disconnected";
    const ready = currentState === "ready" || currentState === "dead";
    const playing = currentState === "playing";
    const busy = !ready && !playing && !disconnected;
    loading.hidden = !busy;
    welcomeTitle.textContent = playing ? "At the helm" : currentState === "dead" ? "Ship lost" : disconnected ? "Connection lost" : "Under sail";
    status.textContent = playing ? "Your boat is still at sea." : currentState === "dead" ? "Your boat was destroyed. Join again when you’re ready." : disconnected ? game.status() : ready ? "The sea is yours." : currentState === "joining" ? "Launching your boat…" : currentState === "connecting" ? "Establishing a secure connection…" : "Waiting for the first view of the sea…";
    if (currentState === "connecting") stage("Connecting to the match");
    else if (currentState === "loading") stage("Receiving the world and drawing the first frame");
    nickname.disabled = playing || currentState === "joining" || disconnected;
    nickname.required = !nickname.disabled;
    joinButton.textContent = playing ? "Resume" : disconnected ? "Reconnect" : currentState === "dead" ? "Join again" : ready ? "Join" : currentState === "joining" ? "Launching…" : "Loading…";
    joinButton.disabled = busy;
    spectatorSection.hidden = playing || disconnected || spectatorList.childElementCount === 0;
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
    spectatorSection.hidden = currentState === "playing" || currentState === "disconnected" || !players.length;
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
    } catch (error) { status.textContent = "That did not work. Please try again."; errorDetails.hidden = false; errorMessage.textContent = String(error); }
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
    if (event.code === "Escape") {
      event.preventDefault();
      if (!settings.hidden) showSettings(false);
      else if (currentState === "playing") setMenu(!menuOpen);
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
      if (timeMs - fpsStartMs >= 1000) {
        const frames = game.presented_frames();
        fps.textContent = `${Math.round((frames - fpsStartFrames) * 1000 / (timeMs - fpsStartMs))} FPS`;
        fpsStartMs = timeMs; fpsStartFrames = frames;
        if (!settings.hidden) updateGraphicsResolution(new URLSearchParams(game.graphics_settings()));
      }
      // The rudder uses counterclockwise world angles; CSS rotates clockwise.
      wheel.style.transform = `rotate(${-game.rudder_angle_rad()}rad)`;
      if (["ready", "playing", "dead", "joining"].includes(currentState)) {
        const width = game.scale_bar_width_px();
        if (Number.isFinite(width) && width > 0) {
          scale.hidden = !settings.hidden; scaleLabel.textContent = game.scale_bar_label(); scaleLine.style.width = `${width}px`;
        }
      }
    } catch (error) {
      console.error(error);
      clearInput(); game.dispose(); renderingStopped = true; currentState = "disconnected"; menuOpen = true;
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
