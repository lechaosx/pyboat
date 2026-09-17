export function boundedCanvasDimensions(
  cssWidthPx: number,
  cssHeightPx: number,
  deviceScale: number,
  maximumDimension: number,
): [number, number] {
  const scale = Number.isFinite(deviceScale) && deviceScale > 0 ? deviceScale : 1;
  const requestedWidth = Math.max(1, Math.floor(Math.max(1, cssWidthPx) * scale));
  const requestedHeight = Math.max(1, Math.floor(Math.max(1, cssHeightPx) * scale));
  const limit = Math.max(1, Math.floor(maximumDimension));
  const reduction = Math.min(1, limit / Math.max(requestedWidth, requestedHeight));
  return [
    Math.min(limit, Math.max(1, Math.floor(requestedWidth * reduction))),
    Math.min(limit, Math.max(1, Math.floor(requestedHeight * reduction))),
  ];
}
