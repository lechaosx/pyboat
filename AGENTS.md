# Engineering Guidelines

## Project Documentation

- Read [PRODUCT.md](PRODUCT.md), [ARCHITECTURE.md](ARCHITECTURE.md), and
  [MAINTAINERS.md](MAINTAINERS.md) before planning or implementing changes.
- Follow the documentation ownership map in
  [README.md](README.md#project-guide).
- Transform the canonical decision passage in the same change that introduces,
  changes, or removes a decision. Revise it in place so it positively describes
  the current snapshot; do not append migration history or frame the decision
  against removed or otherwise irrelevant alternatives. Retain a contrast only
  when rejecting that alternative is enduring rationale needed to apply the
  decision correctly.
- Mark a decision **[Explicit]** when the user requests or clearly endorses it,
  including when that endorsement happens after an agent first selected it. Mark
  agent-selected decisions **[Implicit]** until then; passive acceptance alone
  does not reclassify them.
- Flag a proposed decision change to the user before making it. An agent may
  change an **[Implicit]** decision after flagging it, but must receive the
  user's explicit acknowledgement before changing an **[Explicit]** decision.
  Human-authored decisions are always **[Explicit]**.
- Keep unresolved matters as open questions rather than decisions.
- Keep delivery status out of decision records. `README.md` is the sole concise
  summary of current playable behavior; transform its existing summary instead
  of accumulating parallel status passages.
- Apply a staleness test before adding documentation: identify its canonical
  owner and the event that requires an update. If correctness depends on
  periodically comparing prose with code, and the prose adds no durable decision,
  rationale, or indispensable workflow, do not add it.
- Prefer sources that stay correct by construction: manifests for requirements,
  CLI help for options, tests for accepted behavior, and generator metadata on
  generated artifacts.
- Put non-obvious rationale, invariants, calibration provenance, and cross-file
  synchronization constraints beside the relevant code. Do not add task context,
  reasoning history, absence justifications, or restatements as comments.
- Avoid duplicated instructions, implementation inventories, delivery matrices,
  and explanations of behavior that code states plainly.
- Treat unimplemented target behavior as future scope, not a contradiction. When
  implemented behavior contradicts a recorded decision, resolve both together.

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
- Do not test thin delivery adapters, command wrappers, or presentation formatting merely to prove that they forward to already-testable behavior. Test the underlying stable contract directly; verify the adapter with the smallest appropriate build, smoke check, or manual exercise.
- For rendering and other changes whose meaningful result is visual, run the strongest practical automated checks, such as parser validation, compilation, and build verification. State when visual inspection is still required.
- Documentation, formatting, metadata, and compile-only changes do not require a contrived failing test. Run the smallest relevant verification after the change.
- Do not duplicate production logic in a test solely to make an implementation detail testable.
