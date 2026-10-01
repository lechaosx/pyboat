import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  deriveIntermediateVisibility,
  type FragmentContributionState,
} from "../src/feature-visibility.ts";

const disabledContributions: FragmentContributionState = {
  colorLayer: false,
  sun: false,
  moon: false,
  clearSkyAmbient: false,
  canopyAmbient: false,
};

describe("playground intermediate feature visibility", () => {
  test("disables the scene pipeline when no fragment contribution is visible", () => {
    assert.deepEqual(deriveIntermediateVisibility(disabledContributions, false), {
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
    assert.deepEqual(deriveIntermediateVisibility({
      ...disabledContributions,
      colorLayer: true,
      sun: true,
    }, false), {
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
    assert.deepEqual(deriveIntermediateVisibility({
      ...disabledContributions,
      colorLayer: true,
      canopyAmbient: true,
    }, false), {
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
    assert.deepEqual(deriveIntermediateVisibility(disabledContributions, true), {
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
