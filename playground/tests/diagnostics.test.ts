import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { diagnosticGroups } from "../src/diagnostics.ts";

describe("diagnostic output organization", () => {
  test("presents primary and derived world fields before rendered consequences", () => {
    const weather = diagnosticGroups.weather;
    assert.deepEqual(weather.map(({ label }) => label), [
      "Primary world fields",
      "Derived conditions",
      "Transport",
      "Rendered consequences",
      "Overview",
    ]);
    assert.ok(weather[0].options.some(([id]) => id === "atmosphere_water_vapor"));
    for (const id of [
      "weather_relative_humidity",
      "weather_instability",
      "weather_lift",
      "weather_low_coverage",
      "weather_regional_condensation",
      "weather_broad_pressure_lift",
      "weather_storm_potential",
      "weather_sky_fraction",
      "weather_canopy_optical_thickness",
      "weather_rain",
    ]) {
      assert.ok(weather[1].options.some(([optionId]) => optionId === id));
    }
  });
});
