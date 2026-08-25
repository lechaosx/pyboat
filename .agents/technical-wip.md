# Technical work in progress

This local, unversioned note contains only unresolved product and technical context.
Canonical decisions belong in `PRODUCT.md` and `ARCHITECTURE.md`; playable behavior
belongs in `README.md`.

## Delivery queue

1. **Active — spatial world rendering.** Finish per-position climate, tide,
   snow/ice, and celestial lighting; remove the normal-climate full-width polar
   ice cutoff; make moon intensity respond to elevation; keep center sampling
   cloud-only; keep the playground guide and live spatial model always enabled.
   Verify the complete unit, then wait for user acknowledgement before commit.
2. **World topology decision.** Reconsider periodic north-south topology versus
   a longitude-wrapped cylinder with two bounded polar ice regions. Do not alter
   topology until the user selects it. This precedes further field and ocean work.
3. **Shared cloud field and thickness.** Implement the endorsed stateless model
   below. Include the reproducible near-origin flat cloud cutoff as an acceptance
   failure; diagnose it through raw-detail/density diagnostics rather than mixing
   it into the active rendering unit.
4. **Continuous terrain covers.** Replace categorical material zoning with the
   competing-cover model below.
5. **Sun/sky lighting and finite-disc shadows.** Separate direct and diffuse
   illumination and make shadow softness physically bounded.
6. **Ocean dynamics.** Add deterministic shared currents, wave state, and rogue
   interference.
7. **Sea rendering.** Communicate the physical ocean through reflection, wave,
   breaker, and foam rendering.

Only the active unit receives production edits. New requests are recorded and
prioritized here unless they block its deployability.

## Current foundation

- `2fc5e8f0bb0c40b73d194e80b0f06d7efb1e7a93` implements the rectangular,
  centered toroidal world, seamless procedural fields, cross-boundary physics,
  and playground world-size diagnostics.
- `296c0abecc7f97c47d6ca475197bb9b6f2fee55f` adds position-aware climate and
  world sampling.
- The world is rectangular with a square default. `(0, 0)` is the equator and
  reference meridian. The identified `y = +/-H/2` loop is the signed-latitude
  discontinuity; opposite-hemisphere seasons are intentional.
- Spatial procedural fields repeat across both world periods. Simulation,
  interest, prediction, collision, and combat operate across the identified
  boundaries.
- Broad terrain and weather fields use 4D circle-embedded noise where preserving
  local wavelength matters. Fine material/cloud detail uses cheaper periodic 2D
  simplex. Pressure remains an integer-mode toroidal Fourier field.

## Spatial rendering correction

- Climate, latitude, local solar time, surface temperature, snow/ice, tide, and
  surface lighting must be evaluated across the rendered world, not frozen to a
  single view-center sample. A full-world view must show ice caps and opposing
  seasons.
- Seed, geometry, tick, global season, and lunar phase are global inputs.
- View-center measurements are diagnostics only.
- Current client-side clouds may temporarily use center-sampled wind. The shared
  cloud field should eventually make cloud forcing spatial as well.
- Seam guidelines are always visible. Live spatial world sampling is always on;
  there is no toggle for either. Manual scenarios remain explicit primary-cause
  overrides.
- Normal polar climate must not trigger an all-width binary sea-ice closure at
  one latitude. Full deep-ocean closure is reserved for more extreme persistent
  cold; ordinary cap edges retain bathymetric/coastal structure.
- Moon illumination must vary with elevation like sun illumination instead of
  reaching a flat plateau immediately above the horizon.

## World topology open choice

- The current torus identifies north and south at one polar loop, so signed
  latitude and opposite seasons jump there.
- The user is considering making y nonperiodic because the visible seasonal and
  day/night jump is jarring.
- Smallest alternative: wrap x only; bound y inside two permanent, impassable
  polar ice regions and constrain the camera to the world rectangle. This gives
  two caps but removes north-south sailing laps.
- A sphere-like pole quotient is more physically faithful but materially
  complicates Rapier charts, distances, collisions, rendering, and procedural
  coordinates. A folded periodic latitude is cheap but duplicates equators and
  poles. No alternative is selected yet.

## Terrain covers

- Avoid categorical material zoning, boundary perturbation, and
  distance-to-boundary fields.
- Rock, sediment, vegetation, canopy, and snow should emerge as continuous,
  competing covers from physical causes. Preserve stylized material identity
  without abrupt low-frequency material swaps.
- Classification-scale relief may use existing 256-to-8 m terrain relief, but
  metre-scale relief must not create aliased speckle.

## Shared clouds

- The desired field is deterministic, periodic, stateless, and directly
  queryable as `cloud(seed, geometry, layer, x, y, tick)`.
- Use neighboring temporal anchor realizations, bounded wind pullbacks, quintic
  temporal phase, and variance-preserving cosine/sine blending. Keep integer
  epoch and remainder on the host so time precision does not degrade.
- Current pressure/moisture/temperature provide formation forcing. Do not
  backtrace the weather envelope in the first version; avoid advecting both the
  support and cloud texture in a way that doubles apparent motion.
- A continuous signed condensate field determines cloud presence and thickness:
  zero or negative means clear sky; positive thickness drives visible body
  relief, optical depth, opacity, and both low- and upper-cloud shadows.
- Preserve attractive cloud boundaries while replacing flat, uniformly
  translucent interiors.
- At the default playground start near the origin, a low cloud can exhibit a
  short, perfectly horizontal cutoff. It is not aligned with the equator or a
  world seam. Raw-detail, gate, density, and normal captures are available as
  the first diagnostic split; avoid a speculative fix if the discontinuity
  cannot be reproduced in the underlying field.

## Lighting

- Model independent RGB direct sun and non-directional sky light. Both affect
  land and visible clouds.
- The sun is a finite disc, so even clear-atmosphere direct shadows have a
  penumbra. Cloud thickness modulates direct-light transmission rather than
  producing binary shadows.

## Ocean

- Gameplay currents and macro waves must be deterministic and shared. They
  should respond through plausible causal paths to weather, wind, islands,
  bathymetry, latitude, and tide without a fluid simulation.
- Candidate architecture: analytic/stateless mean currents, wind sea and swell
  state, plus a canonical phase-resolved spectral sampler. Rogue waves emerge
  through severe-sea interference rather than a random event flag.
- Rendering should communicate the physical state through reflections, finite-disc
  sun glitter, wave direction, current-carried foam, shoaling, breakers, and
  material-dependent coastal whitewater. Fine visual detail may remain local.
- Open gameplay choices include the first wave effects beyond water-relative
  loads and under-keel depth, hazardous-wave calibration, and whether the first
  coast model may omit full refraction and diffraction.
