import { describe, expect, test } from "bun:test";
import { diagnosticGroups } from "../src/diagnostics";

describe("diagnostic output organization", () => {
  test("presents primary and derived world fields before rendered consequences", () => {
    const weather = diagnosticGroups.weather;
    expect(weather.map(({ label }) => label)).toEqual([
      "Primary world fields",
      "Derived conditions",
      "Transport",
      "Rendered consequences",
      "Overview",
    ]);
    expect(weather[0].options.map(([id]) => id)).toContain("atmosphere_water_vapor");
    expect(weather[1].options.map(([id]) => id)).toEqual(expect.arrayContaining([
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
    ]));
  });
});
