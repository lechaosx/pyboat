import { describe, expect, test } from "bun:test";
import {
  deriveIntermediateVisibility,
  type FragmentContributionState,
} from "../src/feature-visibility";

const disabledContributions: FragmentContributionState = {
  landSun: false,
  landMoon: false,
  landClearSkyAmbient: false,
  landCanopyAmbient: false,
  waterSun: false,
  waterMoon: false,
  waterClearSkyAmbient: false,
  waterCanopyAmbient: false,
  iceSun: false,
  iceMoon: false,
  iceClearSkyAmbient: false,
  iceCanopyAmbient: false,
  lowCloudSun: false,
  lowCloudMoon: false,
  lowCloudClearSkyAmbient: false,
  lowCloudCanopyAmbient: false,
};

describe("playground intermediate feature visibility", () => {
  test("disables the scene pipeline when no fragment contribution is visible", () => {
    expect(deriveIntermediateVisibility(disabledContributions, false)).toEqual({
      pressure: false,
      temperature: false,
      moisture: false,
      weather: false,
      materials: false,
      oceanForcing: false,
      waveSpectrum: false,
    });
  });

  test("keeps full inputs available to one visible land-light product", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      landSun: true,
    }, false)).toEqual({
      pressure: true,
      temperature: true,
      moisture: true,
      weather: true,
      materials: true,
      oceanForcing: true,
      waveSpectrum: true,
    });
  });

  test("keeps full upstream input available to one visible cloud-light product", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      lowCloudCanopyAmbient: true,
    }, false)).toEqual({
      pressure: true,
      temperature: true,
      moisture: true,
      weather: true,
      materials: true,
      oceanForcing: true,
      waveSpectrum: true,
    });
  });

  test("keeps all fields available to diagnostic outputs", () => {
    expect(deriveIntermediateVisibility(disabledContributions, true)).toEqual({
      pressure: true,
      temperature: true,
      moisture: true,
      weather: true,
      materials: true,
      oceanForcing: true,
      waveSpectrum: true,
    });
  });
});
