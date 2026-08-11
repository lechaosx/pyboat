# Product decisions

This file records intended player experience, scope, and product behavior.

- **[Explicit]** means the user directly requested or affirmatively confirmed the
  decision.
- **[Implicit]** means an agent selected the decision. Later acceptance of the
  resulting behavior does not make that underlying choice explicit.

## Core experience

- **[Explicit]** The product is a multiplayer, top-down naval combat game with the
  accessibility and shared-world appeal of Agar.io and semi-realistic sailing.
- **[Explicit]** The target experience preserves the enjoyable naval-combat
  qualities of *Assassin's Creed IV: Black Flag* while adapting them to a top-down
  multiplayer game.
- **[Explicit]** Matches are persistent rather than round-based. Players may join
  an existing match at any time while it has capacity.
- **[Explicit]** Combat should include ramming, cannons, exploding barrels, and
  other weapons.
- **[Explicit]** Weapons should mostly operate automatically. In addition to
  steering, the player should have only two custom actions, analogous to Agar.io's
  boost and split controls.

## Boats, progression, and matchmaking

- **[Explicit]** The product includes multiple boat and ship classes,
  customization, progression, and metaprogression.
- **[Explicit]** Matchmaking should account for skill, equipment tiers, or
  metaprogression.
- **[Explicit]** Advanced AI should use state-driven behavior, perform maneuvers,
  and understand the resources and capabilities of its ship.

## World and presentation

- **[Explicit]** Seas and islands are procedurally generated.
- **[Explicit]** Weather, currents, waves, wind, and storms should be procedural
  but deterministic from a match seed across both space and time.
- **[Explicit]** Graphics should be mostly procedural, including wave lines around
  islands and moving ships and clouds that correspond to weather.
- **[Explicit]** Clouds may conceal players. Authored sprites remain appropriate
  for details such as rocks, trees, and wreckage.

## Platforms and controls

- **[Explicit]** Keyboard, controller, and touchpad input participate in cross-play.
- **[Explicit]** Browser support is the primary platform target. Native desktop
  and mobile clients are desirable secondary targets rather than requirements for
  the first frontend.
- **[Explicit]** When the existing simulation can support a meaningful visual,
  establish a visible frontend slice before continuing deeper into gameplay
  mechanics.
- **[Explicit]** The browser slice includes one locally controlled boat from the
  Rust simulation. `W/S` controls boat-relative forward or reverse power, `A/D`
  turns, and the first connected gamepad's left stick controls world-relative
  movement without an added software deadzone.
- **[Implicit]** The browser connects to the Rust authoritative server, sends
  fixed-tick controls, predicts nearby physical interaction, reconciles local
  input, and extrapolates distant visible boats without intentional render delay.
- **[Explicit]** Boats move smoothly when authoritative snapshots arrive and when
  they enter or leave the nearby physics bubble.
- **[Implicit]** The browser HUD displays speed through water, apparent-wind
  speed, and point of sail from predicted and reconciled local state as immediate
  sailing feedback.
- **[Explicit]** The browser implementation language is not committed to pure
  JavaScript.
- **[Explicit]** The current steering and latency-compensated interaction were
  manually confirmed by the user as feeling great and form the accepted
  responsiveness baseline.

## Multiplayer awareness

- **[Explicit]** Players should see other nearby boats without intentional
  interpolation delay that would make aiming harder.
- **[Explicit]** Directional arrows to other ships are debug-only feedback and may
  be omitted from the browser and product UI.
- **[Implicit]** Wrecks and recently damaged boats use distinct colors, and the
  local client displays numerical health.

## Ramming, damage, and death

- **[Explicit]** Ramming causes damage, and a boat's resistance depends on where
  it is struck.
- **[Implicit]** The bow is the strongest zone, the middle and sides have normal
  strength, and the stern is weakest.
- **[Implicit]** Current tuning gives boats 100 health, ignores collision impulses
  at or below 100 N·s, converts stronger impulse at 8 N·s per damage point, and
  applies strength multipliers of 2× at the bow, 1× amidships, and 0.6× at the
  stern.
- **[Explicit]** When a player's boat is destroyed, that player's client exits
  instead of respawning.
- **[Explicit]** Other players continue to see the destroyed boat as a wreck. The
  wreck follows boat physics and collisions but has no controller.

## Open product questions (not decisions)

- How long wrecks remain and how they are eventually cleaned up.
- Whether current health, impact thresholds, damage conversion, and zone strength
  values remain appropriate after broader playtesting.
- Concrete rules for progression, matchmaking, ship classes, automatic weapons,
  and the two custom actions.
