export type DiagnosticId =
  | "cloud_low_latent_texture"
  | "cloud_low_amount"
  | "cloud_low_optical_depth"
  | "cloud_low_normals"
  | "cloud_weather_canopy_optical_depth"
  | "lighting_projected_low_shadow"
  | "lighting_projected_upper_shadow"
  | "lighting_combined_transmittance"
  | "lighting_solar_elevation"
  | "lighting_aerosol_optical_depth"
  | "lighting_solar_eclipse_coverage"
  | "temperature_gradient"
  | "surface_air_temperature"
  | "pressure_gradient"
  | "pressure_broad_anomaly"
  | "pressure_full_anomaly"
  | "pressure_synoptic_anomaly"
  | "pressure_local_anomaly"
  | "wind_surface"
  | "wind_upper"
  | "wind_shear"
  | "wind_terrain_shelter"
  | "wind_surface_gust"
  | "wind_resolved_surface"
  | "wind_broad_synoptic"
  | "terrain_elevation"
  | "terrain_slope"
  | "terrain_normals"
  | "terrain_island_clusters"
  | "terrain_island_detail"
  | "terrain_coastal_reef"
  | "material_vegetation"
  | "material_coastal_character"
  | "climate_solid_ice"
  | "climate_autumn_vegetation"
  | "climate_annual_temperature"
  | "climate_spatial_snow"
  | "climate_spatial_ice"
  | "climate_composition"
  | "water_tide_height"
  | "water_ocean_current"
  | "water_visual_normals"
  | "water_physical_wave_normals"
  | "water_significant_wave_height"
  | "water_precipitation_contact_normals";

export type DiagnosticSelection = "scene" | DiagnosticId;

export type DiagnosticDomain = "weather" | "landscape" | "water";

export type DiagnosticGroup = {
  label: string;
  options: Array<[DiagnosticSelection, string]>;
};

export type DiagnosticShader = {
  name: string;
  constants?: Record<string, number>;
};

export const diagnosticShaders: Record<DiagnosticId, DiagnosticShader> = {
  cloud_low_latent_texture: { name: "cloud_low_latent_texture" },
  cloud_low_amount: { name: "cloud_low_sample", constants: { CLOUD_LOW_OUTPUT: 0 } },
  cloud_low_optical_depth: { name: "cloud_low_sample", constants: { CLOUD_LOW_OUTPUT: 1 } },
  cloud_low_normals: { name: "cloud_low_sample", constants: { CLOUD_LOW_OUTPUT: 2 } },
  cloud_weather_canopy_optical_depth: { name: "cloud_weather_canopy_optical_depth" },
  lighting_projected_low_shadow: {
    name: "lighting_cloud_shadow",
    constants: { CLOUD_SHADOW_OUTPUT: 0 },
  },
  lighting_projected_upper_shadow: {
    name: "lighting_cloud_shadow",
    constants: { CLOUD_SHADOW_OUTPUT: 1 },
  },
  lighting_combined_transmittance: {
    name: "lighting_cloud_shadow",
    constants: { CLOUD_SHADOW_OUTPUT: 2 },
  },
  lighting_solar_elevation: {
    name: "lighting_solar",
    constants: { LIGHTING_SOLAR_OUTPUT: 0 },
  },
  lighting_solar_eclipse_coverage: {
    name: "lighting_solar",
    constants: { LIGHTING_SOLAR_OUTPUT: 1 },
  },
  lighting_aerosol_optical_depth: { name: "lighting_aerosol_optical_depth" },
  temperature_gradient: { name: "temperature_gradient" },
  surface_air_temperature: { name: "surface_air_temperature" },
  pressure_gradient: { name: "pressure_gradient" },
  pressure_broad_anomaly: {
    name: "pressure_anomaly",
    constants: { PRESSURE_ANOMALY_OUTPUT: 0 },
  },
  pressure_full_anomaly: {
    name: "pressure_anomaly",
    constants: { PRESSURE_ANOMALY_OUTPUT: 1 },
  },
  pressure_synoptic_anomaly: {
    name: "pressure_anomaly",
    constants: { PRESSURE_ANOMALY_OUTPUT: 2 },
  },
  pressure_local_anomaly: {
    name: "pressure_anomaly",
    constants: { PRESSURE_ANOMALY_OUTPUT: 3 },
  },
  wind_surface: { name: "wind_surface_motion", constants: { WIND_SURFACE_OUTPUT: 0 } },
  wind_shear: { name: "wind_surface_motion", constants: { WIND_SURFACE_OUTPUT: 1 } },
  wind_surface_gust: { name: "wind_surface_motion", constants: { WIND_SURFACE_OUTPUT: 2 } },
  wind_upper: { name: "wind_flow", constants: { WIND_FLOW_OUTPUT: 0 } },
  wind_resolved_surface: { name: "wind_flow", constants: { WIND_FLOW_OUTPUT: 1 } },
  wind_broad_synoptic: { name: "wind_flow", constants: { WIND_FLOW_OUTPUT: 2 } },
  wind_terrain_shelter: { name: "wind_terrain_shelter" },
  terrain_elevation: { name: "terrain_geometry", constants: { TERRAIN_GEOMETRY_OUTPUT: 0 } },
  terrain_slope: { name: "terrain_geometry", constants: { TERRAIN_GEOMETRY_OUTPUT: 1 } },
  terrain_normals: { name: "terrain_geometry", constants: { TERRAIN_GEOMETRY_OUTPUT: 2 } },
  terrain_island_clusters: { name: "terrain_island_clusters" },
  terrain_island_detail: { name: "terrain_island_detail" },
  terrain_coastal_reef: { name: "terrain_coastal_reef" },
  material_vegetation: { name: "material_vegetation" },
  material_coastal_character: { name: "material_coastal_character" },
  climate_solid_ice: { name: "climate_surface", constants: { CLIMATE_SURFACE_OUTPUT: 0 } },
  climate_autumn_vegetation: {
    name: "climate_surface",
    constants: { CLIMATE_SURFACE_OUTPUT: 1 },
  },
  climate_spatial_snow: { name: "climate_surface", constants: { CLIMATE_SURFACE_OUTPUT: 2 } },
  climate_composition: { name: "climate_surface", constants: { CLIMATE_SURFACE_OUTPUT: 3 } },
  climate_annual_temperature: { name: "climate_annual_temperature" },
  climate_spatial_ice: { name: "climate_spatial_ice" },
  water_tide_height: { name: "water_tide_height" },
  water_ocean_current: { name: "water_ocean_forcing", constants: { OCEAN_FORCING_OUTPUT: 0 } },
  water_significant_wave_height: {
    name: "water_ocean_forcing",
    constants: { OCEAN_FORCING_OUTPUT: 1 },
  },
  water_visual_normals: { name: "water_visual_normals" },
  water_physical_wave_normals: { name: "water_physical_wave_normals" },
  water_precipitation_contact_normals: { name: "water_precipitation_contact_normals" },
};

export const diagnosticGroups: Record<DiagnosticDomain, DiagnosticGroup[]> = {
  weather: [
    { label: "Overview", options: [["scene", "Final composition"]] },
    { label: "Cloud structure", options: [
      ["cloud_low_latent_texture", "Low latent texture"],
      ["cloud_low_amount", "Low cloud amount"],
      ["cloud_low_optical_depth", "Low optical depth"],
      ["cloud_low_normals", "Low normals"],
      ["cloud_weather_canopy_optical_depth", "Canopy optical depth"],
    ] },
    { label: "Lighting and shadows", options: [
      ["lighting_projected_low_shadow", "Projected low shadow"],
      ["lighting_projected_upper_shadow", "Projected upper shadow"],
      ["lighting_combined_transmittance", "Combined transmittance"],
      ["lighting_solar_elevation", "Solar elevation"],
      ["lighting_aerosol_optical_depth", "Aerosol optical depth"],
      ["lighting_solar_eclipse_coverage", "Solar eclipse coverage"],
    ] },
    { label: "Thermodynamics", options: [
      ["temperature_gradient", "Local temperature-gradient vector · RGB"],
      ["surface_air_temperature", "Surface air temperature"],
    ] },
    { label: "Pressure", options: [
      ["pressure_gradient", "Pressure gradient vector · RGB"],
      ["pressure_broad_anomaly", "Broad pressure anomaly · ±8 hPa"],
      ["pressure_full_anomaly", "Full pressure anomaly · ±8 hPa"],
      ["pressure_synoptic_anomaly", "Synoptic pressure anomaly · ±3 hPa"],
      ["pressure_local_anomaly", "Local pressure anomaly · ±0.5 hPa"],
    ] },
    { label: "Wind", options: [
      ["wind_surface", "Surface wind vector · RGB"],
      ["wind_upper", "Upper wind vector · RGB"],
      ["wind_shear", "Vertical wind shear · RGB"],
      ["wind_terrain_shelter", "Terrain shelter"],
      ["wind_surface_gust", "Surface gust vector · RGB"],
      ["wind_resolved_surface", "Resolved surface wind vector · RGB"],
      ["wind_broad_synoptic", "Broad / synoptic circulation · RGB"],
    ] },
  ],
  landscape: [
    { label: "Overview", options: [["scene", "Final composition"]] },
    { label: "Landform geometry", options: [
      ["terrain_elevation", "Elevation"],
      ["terrain_slope", "Slope"],
      ["terrain_normals", "Normals"],
      ["terrain_island_clusters", "Island clusters"],
      ["terrain_island_detail", "Island detail"],
      ["terrain_coastal_reef", "Coastal reef field"],
    ] },
    { label: "Surface materials", options: [
      ["material_vegetation", "Vegetation field"],
      ["material_coastal_character", "Coastal character · RGB"],
    ] },
    { label: "Climate", options: [
      ["climate_solid_ice", "Solid ice"],
      ["climate_autumn_vegetation", "Autumn vegetation"],
      ["climate_annual_temperature", "Annual climate"],
      ["climate_spatial_snow", "Spatial snow"],
      ["climate_spatial_ice", "Spatial ice climate"],
      ["climate_composition", "Climate / snow / ice · RGB"],
    ] },
  ],
  water: [
    { label: "Overview", options: [["scene", "Final composition"]] },
    { label: "Water level and flow", options: [
      ["water_tide_height", "Tide height"],
      ["water_ocean_current", "Ocean current vector · RGB"],
    ] },
    { label: "Wave surface", options: [
      ["water_visual_normals", "Visual water normals"],
      ["water_physical_wave_normals", "Gameplay-driven wave normals"],
      ["water_significant_wave_height", "Authoritative significant wave height"],
    ] },
    { label: "Precipitation", options: [
      ["water_precipitation_contact_normals", "Precipitation contact normals"],
    ] },
  ],
};
