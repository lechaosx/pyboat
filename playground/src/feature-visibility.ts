export type FragmentContributionState = {
  landSun: boolean;
  landMoon: boolean;
  landClearSkyAmbient: boolean;
  landCanopyAmbient: boolean;
  waterSun: boolean;
  waterMoon: boolean;
  waterClearSkyAmbient: boolean;
  waterCanopyAmbient: boolean;
  iceSun: boolean;
  iceMoon: boolean;
  iceClearSkyAmbient: boolean;
  iceCanopyAmbient: boolean;
  lowCloudSun: boolean;
  lowCloudMoon: boolean;
  lowCloudClearSkyAmbient: boolean;
  lowCloudCanopyAmbient: boolean;
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

  const colorPipeline = Object.values(contributions).some(Boolean);

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
