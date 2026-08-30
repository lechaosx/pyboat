import { describe, expect, test } from "bun:test";
import {
  deriveIntermediateVisibility,
  type FragmentContributionState,
} from "../src/feature-visibility";

const disabledContributions: FragmentContributionState = {
  colorLayer: false,
  sun: false,
  moon: false,
  clearSkyAmbient: false,
  canopyAmbient: false,
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

  test("keeps full inputs available to one visible row-column product", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      colorLayer: true,
      sun: true,
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

  test("keeps full upstream input available to an ambient row-column product", () => {
    expect(deriveIntermediateVisibility({
      ...disabledContributions,
      colorLayer: true,
      canopyAmbient: true,
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
