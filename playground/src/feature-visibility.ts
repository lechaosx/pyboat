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
  if (diagnosticActive) {
    return {
      pressure: true,
      temperature: true,
      moisture: true,
      weather: true,
      materials: true,
      oceanForcing: true,
      waveSpectrum: true,
    };
  }

  const colorPipeline = contributions.colorLayer
    && (contributions.sun
      || contributions.moon
      || contributions.clearSkyAmbient
      || contributions.canopyAmbient);

  return {
    pressure: colorPipeline,
    temperature: colorPipeline,
    moisture: colorPipeline,
    weather: colorPipeline,
    materials: colorPipeline,
    oceanForcing: colorPipeline,
    waveSpectrum: colorPipeline,
  };
}
