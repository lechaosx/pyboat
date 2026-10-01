export type FragmentContributionState = {
  colorLayer: boolean;
  sun: boolean;
  moon: boolean;
  clearSkyAmbient: boolean;
  canopyAmbient: boolean;
};

export type IntermediateVisibility = {
  pressure: boolean;
  temperature: boolean;
  moisture: boolean;
  weather: boolean;
  materials: boolean;
  oceanForcing: boolean;
  waveSpectrum: boolean;
};

export function deriveIntermediateVisibility(
  contributions: FragmentContributionState,
  diagnosticActive: boolean,
): IntermediateVisibility {
  const enabled = diagnosticActive || (contributions.colorLayer
    && (contributions.sun
      || contributions.moon
      || contributions.clearSkyAmbient
      || contributions.canopyAmbient));

  return {
    pressure: enabled,
    temperature: enabled,
    moisture: enabled,
    weather: enabled,
    materials: enabled,
    oceanForcing: enabled,
    waveSpectrum: enabled,
  };
}
