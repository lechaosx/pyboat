export function startRenderLoop(
  advance: (now: number) => void,
  render: (now: number) => Promise<void>,
  isActive: () => boolean,
  onError: (error: unknown) => void,
): void {
  let pending = 0;
  async function renderFrame(now: number): Promise<void> {
    pending += 1;
    try {
      await render(now);
    } catch (error) {
      onError(error);
    } finally {
      pending -= 1;
    }
  }
  function frame(now: number): void {
    if (!isActive()) return;
    try {
      advance(now);
      // Two slots overlap CPU/GPU work while bounding stale submitted views.
      if (pending < 2) void renderFrame(now);
    } catch (error) {
      onError(error);
    } finally {
      if (isActive()) requestAnimationFrame(frame);
    }
  }
  requestAnimationFrame(frame);
}
