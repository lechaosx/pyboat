import { describe, expect, test } from "bun:test";
import {
  deriveIntermediateVisibility,
  type FragmentContributionState,
} from "../src/feature-visibility";

const disabledContributions: FragmentContributionState = {
  land: false,
  water: false,
  lowCloud: false,
  sun: false,
  moon: false,
  clearSkyAmbient: false,
  canopyAmbient: false,
  lowCloudShadow: false,
  canopyShadow: false,
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

  test("keeps full material inputs available to a visible surface contribution", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      water: true,
      clearSkyAmbient: true,
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

  test("keeps full upstream input available to a visible cloud contribution", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      lowCloud: true,
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

  test("does not activate a shadow without a visible direct-light contribution", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      land: true,
      lowCloudShadow: true,
    }, false)).toEqual({
      pressure: false,
      temperature: false,
      moisture: false,
      weather: false,
      materials: false,
      oceanForcing: false,
      waveSpectrum: false,
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
