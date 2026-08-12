# Engineering Guidelines

## Project Documentation

- Read [PRODUCT.md](PRODUCT.md), [ARCHITECTURE.md](ARCHITECTURE.md), and
  [MAINTAINERS.md](MAINTAINERS.md) before planning or implementing changes.
- Keep [README.md](README.md) as the user-facing repository signpost: what the product is,
  current gameplay, controls, prerequisites, and how to run it. Do not move
  internal implementation guidance into it.
- Keep [MAINTAINERS.md](MAINTAINERS.md) developer-facing. Update it when development setup,
  commands, repository layout, tests, tools, or implementation guidance changes.
- Treat [PRODUCT.md](PRODUCT.md) and [ARCHITECTURE.md](ARCHITECTURE.md) as living
  decision records. Update them in the same change that introduces, changes, or
  removes a decision.
- Put player experience, scope, content, controls, balance, and product constraints
  in [PRODUCT.md](PRODUCT.md).
- Put implementation, simulation, networking, protocol, persistence, and technical
  boundaries in [ARCHITECTURE.md](ARCHITECTURE.md).
- Mark every decision as **[Explicit]** when the user directly requested or
  affirmatively confirmed that specific decision.
- Mark every decision as **[Implicit]** when an agent selected it. User acceptance
  of the resulting behavior does not retroactively make the underlying choice
  explicit.
- Keep unresolved matters under an open-questions section rather than presenting
  them as decisions.
- If implementation and a decision record disagree, surface the discrepancy and
  resolve both together.
- Update links among these documents when files move or their roles change.

## Defer Abstraction

- Start at the lowest practical level of abstraction and keep code concrete and local while requirements are still changing.
- Let patterns emerge before introducing abstractions. An abstraction must have demonstrated semantic merit, not merely make the current code look organized.
- Make architectural decisions at the latest practical moment, when concrete constraints justify them.

## Prefer WET Until Reuse Is Established

- Keep an implementation local until the same reusable need appears more than twice.
- Extract shared code only when the functionality is semantically general and valuable on its own, not merely because two places currently look alike.
- Account for the coupling introduced by shared code. Common components should represent stable concepts that are unlikely to change with any one caller.

## Test Observable Risk

- Use red-green TDD for bug fixes and for behavior or contracts that can be exercised through a stable interface at reasonable cost. Confirm that the new test fails for the intended reason before implementing the change.
- Add tests for observable behavior, compatibility boundaries, deterministic calculations, and regressions that are likely to recur.
- Do not add tests whose primary assertion searches source text, pins private names or structure, or encodes subjective visual tuning. Such tests obstruct implementation changes without protecting product behavior.
- For rendering and other changes whose meaningful result is visual, run the strongest practical automated checks, such as parser validation, compilation, and build verification. State when visual inspection is still required.
- Documentation, formatting, metadata, and compile-only changes do not require a contrived failing test. Run the smallest relevant verification after the change.
- Do not duplicate production logic in a test solely to make an implementation detail testable.
