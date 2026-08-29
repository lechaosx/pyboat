export type ShaderNodeKind = "input" | "physical" | "intermediate" | "visual" | "output";
export type ShaderDomain = "world" | "weather" | "landscape" | "water" | "lighting" | "composition";
export type ShaderSocketType = "state" | "geometry" | "field" | "vector" | "mask" | "color";

export type ShaderGraphControl =
  | {
      type: "range";
      key: string;
      label: string;
      min: number;
      max: number;
      step: number;
      displayScale?: number;
      suffix?: string;
    }
  | {
      type: "toggle";
      key: string;
      label: string;
    }
  | {
      type: "text";
      key: string;
      label: string;
    };

export type ShaderMuteReplacement = "zero" | "identity" | "fallback";

export type ShaderGraphMute = {
  key: string;
  replacement: ShaderMuteReplacement;
  label: string;
};

export type ShaderGraphDiagnostic = {
  id: string;
  label: string;
};

export type ShaderGraphNode = {
  id: string;
  label: string;
  summary: string;
  kind: ShaderNodeKind;
  domain: ShaderDomain;
  output: string;
  outputType: ShaderSocketType;
  mute?: ShaderGraphMute;
  controls?: readonly ShaderGraphControl[];
  diagnostics?: readonly ShaderGraphDiagnostic[];
};

export type ShaderGraphEdge = {
  id: string;
  source: string;
  target: string;
  input: string;
  type: ShaderSocketType;
};

const range = (
  key: string,
  label: string,
  min: number,
  max: number,
  step: number,
  displayScale?: number,
  suffix?: string,
): ShaderGraphControl => ({
  type: "range", key, label, min, max, step, displayScale, suffix,
});

const toggle = (key: string, label: string): ShaderGraphControl => ({
  type: "toggle",
  key,
  label,
});

const text = (key: string, label: string): ShaderGraphControl => ({
  type: "text",
  key,
  label,
});

const mute = (
  key: string,
  replacement: ShaderMuteReplacement,
  label: string,
): ShaderGraphMute => ({ key, replacement, label });

export const shaderGraphNodes: readonly ShaderGraphNode[] = [
  {
    id: "seed_input",
    label: "World seed",
    summary: "The stable match seed uploaded by the host and salted independently by procedural mechanisms.",
    kind: "input",
    domain: "world",
    output: "seed",
    outputType: "state",
    controls: [text("seed", "Seed")],
  },
  {
    id: "world_geometry_input",
    label: "World geometry",
    summary: "Longitude circumference and finite pole-to-pole span supplied by the authoritative match.",
    kind: "input",
    domain: "world",
    output: "world dimensions",
    outputType: "geometry",
    controls: [
      range("worldWidthM", "Longitude", 8000, 256000, 1000, 0.001, " km"),
      range("worldHeightM", "Pole to pole", 8000, 256000, 1000, 0.001, " km"),
    ],
  },
  {
    id: "field_clocks_input",
    label: "Procedural clocks",
    summary: "Independent fixed-tick coordinates for clouds, pressure, currents, and material evolution.",
    kind: "input",
    domain: "world",
    output: "field time",
    outputType: "state",
    controls: [
      range("cloudTimeScale", "Cloud speed", 0, 10, 0.1, 1, "×"),
      range("pressureTimeScale", "Pressure speed", 0, 10, 0.1, 1, "×"),
      range("currentTimeScale", "Current speed", 0, 10, 0.1, 1, "×"),
    ],
  },
  {
    id: "world_clock_input",
    label: "World clock",
    summary: "The global fixed-tick source for day, season, lunar position, tides, and eclipses.",
    kind: "input",
    domain: "world",
    output: "world phases",
    outputType: "state",
    controls: [range("worldTimeScale", "Clock speed", 0, 10, 0.1, 1, "×")],
  },
  {
    id: "view_input",
    label: "View input",
    summary: "Camera center, viewport size, zoom, and world-space fragment footprint.",
    kind: "input",
    domain: "world",
    output: "view state",
    outputType: "geometry",
  },
  {
    id: "fragment_input",
    label: "Fragment input",
    summary: "Interpolated world position, base vertex color, and terrain identity for this fragment.",
    kind: "input",
    domain: "world",
    output: "fragment",
    outputType: "geometry",
  },
  {
    id: "surface_coordinates",
    label: "Surface coordinates",
    summary: "Canonical longitude, finite latitude, local solar phase, and periodic circle embedding.",
    kind: "intermediate",
    domain: "world",
    output: "surface frame",
    outputType: "geometry",
  },
  {
    id: "terrain_geometry",
    label: "Terrain geometry",
    summary: "Authoritative elevation, gradients, geology, coastal character, and ice capacities.",
    kind: "physical",
    domain: "landscape",
    output: "terrain sample",
    outputType: "geometry",
    controls: [
      range("terrainScale", "Horizontal scale", 0.25, 4, 0.05),
      range("elevationScale", "Elevation scale", 0.1, 3, 0.05),
      range("cliffSlope", "Cliff slope", 0.01, 0.3, 0.005),
    ],
    diagnostics: [
      { id: "terrain_elevation", label: "Elevation" },
      { id: "terrain_slope", label: "Slope" },
      { id: "terrain_normals", label: "Normals" },
    ],
  },
  {
    id: "coastal_relief",
    label: "Coastal relief",
    summary: "Physical reef, shelf, outcrop, and fine signed coastal relief added to terrain geometry.",
    kind: "physical",
    domain: "landscape",
    output: "relief",
    outputType: "geometry",
    mute: mute("coastalReliefVisible", "zero", "Zero coastal displacement"),
    diagnostics: [{ id: "terrain_coastal_reef", label: "Coastal reef field" }],
  },
  {
    id: "tide",
    label: "Astronomical tide",
    summary: "Authoritative surface height derived from world clock, position, and basin response.",
    kind: "physical",
    domain: "water",
    output: "water height",
    outputType: "field",
    diagnostics: [{ id: "water_tide_height", label: "Tide height" }],
  },
  {
    id: "pressure",
    label: "Pressure field",
    summary: "Broad, synoptic, and local pressure potential sampled at the fragment position.",
    kind: "physical",
    domain: "weather",
    output: "pressure",
    outputType: "field",
    mute: mute("pressureFieldEnabled", "zero", "Zero pressure anomaly and gradient"),
    diagnostics: [
      { id: "pressure_full_anomaly", label: "Full anomaly" },
      { id: "pressure_gradient", label: "Gradient" },
    ],
  },
  {
    id: "temperature",
    label: "Temperature",
    summary: "Climate baseline plus local evolving temperature anomaly.",
    kind: "physical",
    domain: "weather",
    output: "temperature",
    outputType: "field",
    mute: mute("temperatureFieldEnabled", "fallback", "Climate baseline without local anomaly"),
    diagnostics: [{ id: "surface_air_temperature", label: "Surface air temperature" }],
  },
  {
    id: "moisture",
    label: "Moisture",
    summary: "Climatic saturation capacity plus seeded, evolving humidity structure.",
    kind: "physical",
    domain: "weather",
    output: "humidity",
    outputType: "field",
    mute: mute("moistureFieldEnabled", "zero", "Zero atmospheric water vapor"),
    diagnostics: [{ id: "climate_composition", label: "Climate composition" }],
  },
  {
    id: "surface_wind",
    label: "Resolved wind",
    summary: "Pressure circulation, cross-isobar flow, thermal shear, gusts, and terrain shelter.",
    kind: "physical",
    domain: "weather",
    output: "wind vectors",
    outputType: "vector",
    controls: [
      toggle("pressureCirculationEnabled", "Pressure circulation"),
      range("pressureWindScale", "Pressure response", 0, 2, 0.05),
      toggle("crossIsobarEnabled", "Cross-isobar flow"),
      range("crossIsobarScale", "Cross-isobar response", 0, 2, 0.05),
      toggle("thermalWindEnabled", "Thermal-wind shear"),
      range("thermalWindScale", "Thermal response", 0, 2, 0.05),
      toggle("shelterEnabled", "Terrain shelter"),
      range("shelterStrength", "Shelter strength", 0, 1, 0.05),
    ],
    diagnostics: [
      { id: "wind_resolved_surface", label: "Resolved surface wind" },
      { id: "wind_upper", label: "Upper wind" },
      { id: "wind_terrain_shelter", label: "Terrain shelter" },
    ],
  },
  {
    id: "surface_weather",
    label: "Surface weather",
    summary: "Humidity, instability, lift, rain, cloud type, coverage, and advection wind.",
    kind: "intermediate",
    domain: "weather",
    output: "weather sample",
    outputType: "state",
    mute: mute("weatherFormationEnabled", "fallback", "Clear, calm weather sample"),
    diagnostics: [{ id: "cloud_low_formation", label: "Low-cloud formation" }],
  },
  {
    id: "celestial_state",
    label: "Celestial state",
    summary: "Topocentric Sun and Moon directions, eclipses, horizon visibility, and source colors.",
    kind: "physical",
    domain: "lighting",
    output: "celestial light",
    outputType: "state",
    diagnostics: [
      { id: "lighting_solar_elevation", label: "Solar elevation" },
      { id: "lighting_solar_eclipse_coverage", label: "Solar eclipse" },
    ],
  },
  {
    id: "regional_canopy",
    label: "Regional canopy",
    summary: "Smooth visible-sky cloudiness and mean cloudy-sky optical thickness; never a local cloud pixel.",
    kind: "intermediate",
    domain: "weather",
    output: "canopy conditions",
    outputType: "field",
    mute: mute("upperCloudEnabled", "fallback", "Clear regional sky"),
    diagnostics: [{ id: "cloud_weather_canopy_optical_depth", label: "Canopy optical depth" }],
  },
  {
    id: "low_cloud_field",
    label: "Low-cloud field",
    summary: "Transported latent structure converted into cloud amount, optical depth, height, and gradients.",
    kind: "visual",
    domain: "weather",
    output: "low-cloud sample",
    outputType: "field",
    mute: mute("lowCloudEnabled", "zero", "Zero cloud amount and optical depth"),
    controls: [
      range("detailCharacter", "Billow shape", 0, 1, 0.05),
      range("detailBlobiness", "Cell separation", 0, 1, 0.05),
      range("detailContrast", "Detail contrast", 0.25, 2.5, 0.05),
      range("detailEdgeSoftness", "Body edge", 0.005, 0.25, 0.005),
      range("maxWispCoverage", "Maximum wisps", 0, 1, 0.01),
    ],
    diagnostics: [
      { id: "cloud_low_amount", label: "Cloud amount" },
      { id: "cloud_low_optical_depth", label: "Optical depth" },
      { id: "cloud_low_normals", label: "Normals" },
    ],
  },
  {
    id: "horizon_scattering",
    label: "Atmospheric scattering",
    summary: "Aerosol, moisture, elevation, and eclipse-aware horizon color response.",
    kind: "intermediate",
    domain: "lighting",
    output: "scattering",
    outputType: "color",
    diagnostics: [{ id: "lighting_aerosol_optical_depth", label: "Aerosol optical depth" }],
  },
  {
    id: "sun_direct",
    label: "Sun direct",
    summary: "Eclipse-attenuated, elevation-shaped direct sunlight and its atmospheric color.",
    kind: "visual",
    domain: "lighting",
    output: "Sun RGB",
    outputType: "color",
    mute: mute("sunDirectLightEnabled", "zero", "Zero direct-Sun RGB"),
  },
  {
    id: "moon_direct",
    label: "Moon direct",
    summary: "Phase-, eclipse-, elevation-, and atmosphere-shaped direct moonlight.",
    kind: "visual",
    domain: "lighting",
    output: "Moon RGB",
    outputType: "color",
    mute: mute("moonDirectLightEnabled", "zero", "Zero direct-Moon RGB"),
  },
  {
    id: "clear_sky_indirect",
    label: "Clear-sky indirect",
    summary: "Diffuse clear-sky endpoint from celestial light and regional atmospheric scattering.",
    kind: "visual",
    domain: "lighting",
    output: "sky RGB",
    outputType: "color",
    mute: mute("skyIndirectLightEnabled", "zero", "Zero clear-sky diffuse RGB"),
  },
  {
    id: "canopy_indirect",
    label: "Canopy indirect",
    summary: "Diffuse cloudy-sky endpoint filtered by regional canopy thickness and source color.",
    kind: "visual",
    domain: "lighting",
    output: "canopy RGB",
    outputType: "color",
    mute: mute("canopyIndirectLightEnabled", "zero", "Zero canopy diffuse RGB"),
  },
  {
    id: "light_mix",
    label: "Light composition",
    summary: "Combines direct Sun, direct Moon, and cloudiness-weighted indirect endpoints.",
    kind: "intermediate",
    domain: "lighting",
    output: "light contributions",
    outputType: "color",
  },
  {
    id: "low_cloud_shadow",
    label: "Low-cloud shadows",
    summary: "Projects the low layer toward each active celestial source and samples its optical depth.",
    kind: "visual",
    domain: "lighting",
    output: "low transmittance",
    outputType: "mask",
    mute: mute("lowCloudShadow", "identity", "Unit light transmittance"),
    diagnostics: [{ id: "lighting_projected_low_shadow", label: "Projected low shadow" }],
  },
  {
    id: "canopy_shadow",
    label: "Canopy shadows",
    summary: "Projects the high weather canopy toward each active celestial source.",
    kind: "visual",
    domain: "lighting",
    output: "canopy transmittance",
    outputType: "mask",
    mute: mute("upperCloudShadow", "identity", "Unit light transmittance"),
    diagnostics: [{ id: "lighting_projected_upper_shadow", label: "Projected canopy shadow" }],
  },
  {
    id: "canopy_on_low",
    label: "Canopy on low clouds",
    summary: "Filters direct light reaching the visible low-cloud body through the high canopy.",
    kind: "visual",
    domain: "lighting",
    output: "cloud transmittance",
    outputType: "mask",
    mute: mute("upperShadowOnLowCloud", "identity", "Unit light transmittance"),
  },
  {
    id: "surface_climate",
    label: "Surface climate",
    summary: "Ground and water temperatures, seasonal vegetation, snow, and authoritative ice closure.",
    kind: "physical",
    domain: "landscape",
    output: "climate masks",
    outputType: "mask",
    diagnostics: [
      { id: "climate_spatial_snow", label: "Snow cover" },
      { id: "climate_solid_ice", label: "Solid ice" },
    ],
  },
  {
    id: "surface_materials",
    label: "Surface materials",
    summary: "Continuous rock, sediment, wet ground, vegetation, snow, ice, and water surface material.",
    kind: "visual",
    domain: "landscape",
    output: "surface material",
    outputType: "color",
    mute: mute("terrainMaterialsEnabled", "fallback", "Flat land/water palette"),
    diagnostics: [{ id: "material_coastal_character", label: "Coastal character" }],
  },
  {
    id: "surface_texture",
    label: "Surface texture",
    summary: "Appearance-only detail gradient and roughness variation over the physical landform.",
    kind: "visual",
    domain: "landscape",
    output: "material + detail normal",
    outputType: "state",
    mute: mute("surfaceTextureVisible", "zero", "Zero appearance-detail gradient"),
  },
  {
    id: "vegetation",
    label: "Vegetation",
    summary: "Continuous vegetation cover and its seasonal color response.",
    kind: "visual",
    domain: "landscape",
    output: "vegetation layer",
    outputType: "color",
    mute: mute("vegetationVisible", "fallback", "Non-forested ground material"),
    controls: [toggle("seasonalVegetationVisible", "Seasonal color")],
    diagnostics: [{ id: "material_vegetation", label: "Vegetation field" }],
  },
  {
    id: "seasonal_snow",
    label: "Seasonal snow",
    summary: "Snow overlay from the physical climate masks and material exposure.",
    kind: "visual",
    domain: "landscape",
    output: "snow layer",
    outputType: "color",
    mute: mute("seasonalSnowVisible", "zero", "Zero seasonal snow overlay"),
  },
  {
    id: "ocean_forcing",
    label: "Ocean forcing",
    summary: "Surface and upper wind, fetch, depth, ice, and current response for physical waves.",
    kind: "physical",
    domain: "water",
    output: "ocean state",
    outputType: "state",
    mute: mute("oceanForcingEnabled", "zero", "Zero current and significant wave height"),
    diagnostics: [
      { id: "water_ocean_current", label: "Ocean current" },
      { id: "water_significant_wave_height", label: "Significant wave height" },
    ],
  },
  {
    id: "wave_spectrum",
    label: "Physical wave spectrum",
    summary: "Seeded multi-directional wind-sea and swell modes that alter the physical water surface.",
    kind: "physical",
    domain: "water",
    output: "wave normals",
    outputType: "vector",
    mute: mute("waveSpectrumEnabled", "fallback", "Flat physical wave normal"),
    diagnostics: [{ id: "water_physical_wave_normals", label: "Physical wave normals" }],
  },
  {
    id: "decorative_water",
    label: "Decorative water normals",
    summary: "Appearance-only high-frequency water normals below the physical wave scale.",
    kind: "visual",
    domain: "water",
    output: "detail normals",
    outputType: "vector",
    mute: mute("decorativeWaterEnabled", "zero", "Zero decorative normal"),
    diagnostics: [{ id: "water_visual_normals", label: "Visual normals" }],
  },
  {
    id: "precipitation_contacts",
    label: "Precipitation contacts",
    summary: "Filtered world-space rain and snow contact normals on open water.",
    kind: "visual",
    domain: "water",
    output: "contact normals",
    outputType: "vector",
    mute: mute("precipitationContactsEnabled", "zero", "Zero contact normal"),
    diagnostics: [{ id: "water_precipitation_contact_normals", label: "Contact normals" }],
  },
  {
    id: "shallow_bottom",
    label: "Shallow-bottom optics",
    summary: "Depth-dependent bottom visibility through water without changing physical depth.",
    kind: "visual",
    domain: "water",
    output: "bottom color",
    outputType: "color",
    mute: mute("shallowBottomEnabled", "zero", "Zero bottom optical contribution"),
    controls: [
      range("waterAttenuationLength", "Attenuation · m", 0, 1, 0.01),
      range("bottomVisibility", "Visibility", 0, 2, 0.02),
    ],
  },
  {
    id: "landform_relief",
    label: "Landform relief light",
    summary: "Normal response and roughness-aware highlight from the composed celestial lighting.",
    kind: "visual",
    domain: "lighting",
    output: "light grade",
    outputType: "color",
    mute: mute("landformReliefEnabled", "identity", "Neutral relief grade"),
    controls: [range("landformReliefStrength", "Strength", 0, 2, 0.05)],
  },
  {
    id: "landform_shadow",
    label: "Landform shadows",
    summary: "Finite-source terrain ray samples toward active Sun and Moon contributions.",
    kind: "visual",
    domain: "lighting",
    output: "terrain transmittance",
    outputType: "mask",
    mute: mute("landformShadowEnabled", "identity", "Unit light transmittance"),
    controls: [
      range("landformShadowStrength", "Strength", 0, 1, 0.05),
      range("landformShadowReach", "Reach · m", 32, 1024, 16),
    ],
  },
  {
    id: "surface_shading",
    label: "Surface shading",
    summary: "Combines material, water, relief, direct and indirect light, and all surface shadows.",
    kind: "intermediate",
    domain: "composition",
    output: "surface RGB",
    outputType: "color",
  },
  {
    id: "low_cloud_body",
    label: "Low-cloud body",
    summary: "Lights and composites the visible low-cloud material above the shaded surface.",
    kind: "visual",
    domain: "composition",
    output: "cloud RGBα",
    outputType: "color",
    mute: mute("lowCloudVisible", "zero", "Transparent cloud composite"),
    controls: [
      range("opticalDepth", "Optical depth", 0, 3, 0.05),
      range("opacity", "Opacity", 0, 1.5, 0.05),
      range("darkness", "Darkening", 0, 2, 0.05),
      range("relief", "Relief shading", 0, 2, 0.05),
    ],
  },
  {
    id: "final_color",
    label: "Final fragment color",
    summary: "Final terrain or boat RGBA returned by the production fragment entry point.",
    kind: "output",
    domain: "composition",
    output: "@location(0) RGBA",
    outputType: "color",
    diagnostics: [{ id: "scene", label: "Final composition" }],
  },
];

let nextEdgeId = 0;
const edge = (
  source: string,
  target: string,
  input: string,
  type: ShaderSocketType,
): ShaderGraphEdge => ({
  id: `shader-edge-${nextEdgeId++}`,
  source,
  target,
  input,
  type,
});

export const shaderGraphEdges: readonly ShaderGraphEdge[] = [
  edge("view_input", "fragment_input", "camera + viewport", "geometry"),
  edge("world_geometry_input", "surface_coordinates", "world dimensions", "geometry"),
  edge("world_clock_input", "surface_coordinates", "local solar phase", "state"),
  edge("fragment_input", "surface_coordinates", "world position", "geometry"),
  edge("surface_coordinates", "terrain_geometry", "surface frame", "geometry"),
  edge("seed_input", "terrain_geometry", "terrain seed", "state"),
  edge("surface_coordinates", "coastal_relief", "position", "geometry"),
  edge("seed_input", "coastal_relief", "relief seed", "state"),
  edge("coastal_relief", "terrain_geometry", "coastal relief", "geometry"),
  edge("surface_coordinates", "tide", "position", "geometry"),
  edge("world_clock_input", "tide", "astronomical phase", "state"),
  edge("seed_input", "tide", "basin seed", "state"),
  edge("surface_coordinates", "pressure", "position", "geometry"),
  edge("seed_input", "pressure", "pressure seed", "state"),
  edge("field_clocks_input", "pressure", "pressure time", "state"),
  edge("surface_coordinates", "temperature", "position + season", "geometry"),
  edge("seed_input", "temperature", "temperature seed", "state"),
  edge("field_clocks_input", "temperature", "weather time", "state"),
  edge("world_clock_input", "temperature", "day + season", "state"),
  edge("surface_coordinates", "moisture", "position", "geometry"),
  edge("seed_input", "moisture", "moisture seed", "state"),
  edge("field_clocks_input", "moisture", "weather time", "state"),
  edge("pressure", "surface_wind", "pressure gradient", "field"),
  edge("temperature", "surface_wind", "thermal gradient", "field"),
  edge("terrain_geometry", "surface_wind", "shelter", "geometry"),
  edge("pressure", "surface_weather", "lift", "field"),
  edge("temperature", "surface_weather", "instability", "field"),
  edge("moisture", "surface_weather", "humidity", "field"),
  edge("surface_wind", "surface_weather", "advection", "vector"),
  edge("world_clock_input", "celestial_state", "orbit + clock", "state"),
  edge("surface_coordinates", "celestial_state", "observer", "geometry"),
  edge("surface_weather", "regional_canopy", "cloud regime", "state"),
  edge("seed_input", "regional_canopy", "canopy seed", "state"),
  edge("field_clocks_input", "regional_canopy", "canopy anchors", "state"),
  edge("surface_weather", "low_cloud_field", "formation", "state"),
  edge("surface_wind", "low_cloud_field", "transport", "vector"),
  edge("seed_input", "low_cloud_field", "cloud seed", "state"),
  edge("field_clocks_input", "low_cloud_field", "cloud anchors", "state"),
  edge("surface_weather", "horizon_scattering", "aerosol + moisture", "state"),
  edge("celestial_state", "horizon_scattering", "source elevation", "state"),
  edge("celestial_state", "sun_direct", "Sun", "state"),
  edge("horizon_scattering", "sun_direct", "source color", "color"),
  edge("celestial_state", "moon_direct", "Moon", "state"),
  edge("horizon_scattering", "moon_direct", "source color", "color"),
  edge("celestial_state", "clear_sky_indirect", "celestial light", "state"),
  edge("horizon_scattering", "clear_sky_indirect", "sky scattering", "color"),
  edge("celestial_state", "canopy_indirect", "celestial light", "state"),
  edge("horizon_scattering", "canopy_indirect", "atmosphere", "color"),
  edge("regional_canopy", "canopy_indirect", "cloudiness + thickness", "field"),
  edge("sun_direct", "light_mix", "Sun direct", "color"),
  edge("moon_direct", "light_mix", "Moon direct", "color"),
  edge("clear_sky_indirect", "light_mix", "clear-sky indirect", "color"),
  edge("canopy_indirect", "light_mix", "canopy indirect", "color"),
  edge("low_cloud_field", "low_cloud_shadow", "optical depth", "field"),
  edge("sun_direct", "low_cloud_shadow", "Sun projection", "color"),
  edge("moon_direct", "low_cloud_shadow", "Moon projection", "color"),
  edge("regional_canopy", "canopy_shadow", "optical depth", "field"),
  edge("sun_direct", "canopy_shadow", "Sun projection", "color"),
  edge("moon_direct", "canopy_shadow", "Moon projection", "color"),
  edge("regional_canopy", "canopy_on_low", "canopy depth", "field"),
  edge("low_cloud_field", "canopy_on_low", "low-cloud height", "field"),
  edge("sun_direct", "canopy_on_low", "Sun", "color"),
  edge("moon_direct", "canopy_on_low", "Moon", "color"),
  edge("terrain_geometry", "surface_climate", "elevation + geology", "geometry"),
  edge("temperature", "surface_climate", "temperature", "field"),
  edge("moisture", "surface_climate", "moisture", "field"),
  edge("tide", "surface_climate", "water height", "field"),
  edge("world_clock_input", "surface_climate", "season", "state"),
  edge("terrain_geometry", "surface_materials", "terrain", "geometry"),
  edge("surface_climate", "surface_materials", "climate masks", "mask"),
  edge("tide", "surface_materials", "shoreline", "field"),
  edge("surface_materials", "surface_texture", "base material", "color"),
  edge("surface_climate", "vegetation", "season", "mask"),
  edge("terrain_geometry", "vegetation", "cover capacity", "geometry"),
  edge("surface_materials", "vegetation", "material", "color"),
  edge("surface_climate", "seasonal_snow", "snow cover", "mask"),
  edge("surface_materials", "seasonal_snow", "material", "color"),
  edge("surface_wind", "ocean_forcing", "wind", "vector"),
  edge("terrain_geometry", "ocean_forcing", "depth + fetch", "geometry"),
  edge("tide", "ocean_forcing", "water height", "field"),
  edge("surface_climate", "ocean_forcing", "ice", "mask"),
  edge("ocean_forcing", "wave_spectrum", "wave energy", "state"),
  edge("seed_input", "wave_spectrum", "wave seed", "state"),
  edge("field_clocks_input", "wave_spectrum", "current phase", "state"),
  edge("wave_spectrum", "decorative_water", "physical normal", "vector"),
  edge("seed_input", "decorative_water", "detail seed", "state"),
  edge("field_clocks_input", "decorative_water", "detail phase", "state"),
  edge("surface_weather", "precipitation_contacts", "rain + snow", "state"),
  edge("decorative_water", "precipitation_contacts", "surface normal", "vector"),
  edge("terrain_geometry", "shallow_bottom", "bottom", "geometry"),
  edge("tide", "shallow_bottom", "water depth", "field"),
  edge("light_mix", "landform_relief", "lighting", "color"),
  edge("terrain_geometry", "landform_relief", "gradient", "geometry"),
  edge("surface_texture", "landform_relief", "detail response", "state"),
  edge("sun_direct", "landform_shadow", "Sun", "color"),
  edge("moon_direct", "landform_shadow", "Moon", "color"),
  edge("terrain_geometry", "landform_shadow", "elevation", "geometry"),
  edge("surface_texture", "surface_shading", "textured surface", "state"),
  edge("vegetation", "surface_shading", "vegetation", "color"),
  edge("seasonal_snow", "surface_shading", "snow", "color"),
  edge("wave_spectrum", "surface_shading", "physical waves", "vector"),
  edge("decorative_water", "surface_shading", "water detail", "vector"),
  edge("precipitation_contacts", "surface_shading", "contacts", "vector"),
  edge("shallow_bottom", "surface_shading", "bottom", "color"),
  edge("light_mix", "surface_shading", "light", "color"),
  edge("low_cloud_shadow", "surface_shading", "low shadow", "mask"),
  edge("canopy_shadow", "surface_shading", "canopy shadow", "mask"),
  edge("landform_relief", "surface_shading", "relief grade", "color"),
  edge("landform_shadow", "surface_shading", "landform shadow", "mask"),
  edge("low_cloud_field", "low_cloud_body", "cloud sample", "field"),
  edge("light_mix", "low_cloud_body", "light", "color"),
  edge("canopy_on_low", "low_cloud_body", "canopy transmittance", "mask"),
  edge("surface_shading", "low_cloud_body", "surface behind", "color"),
  edge("fragment_input", "final_color", "boat/base fragment", "geometry"),
  edge("surface_shading", "final_color", "surface", "color"),
  edge("low_cloud_body", "final_color", "cloud composite", "color"),
];

export function validateShaderGraph(): void {
  const nodes = new Map(shaderGraphNodes.map((node) => [node.id, node]));
  if (nodes.size !== shaderGraphNodes.length) throw new Error("duplicate shader graph node");
  const incoming = new Map(shaderGraphNodes.map((node) => [node.id, [] as string[]]));
  const edgeIds = new Set<string>();
  for (const graphEdge of shaderGraphEdges) {
    if (edgeIds.has(graphEdge.id)) throw new Error("duplicate shader graph edge");
    edgeIds.add(graphEdge.id);
    if (!nodes.has(graphEdge.source) || !nodes.has(graphEdge.target)) {
      throw new Error(`missing shader graph endpoint: ${graphEdge.id}`);
    }
    if (nodes.get(graphEdge.source)!.outputType !== graphEdge.type) {
      throw new Error(`shader graph socket type mismatch: ${graphEdge.id}`);
    }
    incoming.get(graphEdge.target)!.push(graphEdge.source);
  }
  const visited = new Set<string>();
  const visiting = new Set<string>();
  const visit = (id: string): void => {
    if (visited.has(id)) return;
    if (visiting.has(id)) throw new Error(`shader graph cycle at: ${id}`);
    visiting.add(id);
    for (const dependency of incoming.get(id)!) visit(dependency);
    visiting.delete(id);
    visited.add(id);
  };
  for (const node of shaderGraphNodes) visit(node.id);
}
