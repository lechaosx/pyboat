# Engineering Guidelines

## Defer Abstraction

- Start at the lowest practical level of abstraction and keep code concrete and local while requirements are still changing.
- Let patterns emerge before introducing abstractions. An abstraction must have demonstrated semantic merit, not merely make the current code look organized.
- Make architectural decisions at the latest practical moment, when concrete constraints justify them.

## Prefer WET Until Reuse Is Established

- Keep an implementation local until the same reusable need appears more than twice.
- Extract shared code only when the functionality is semantically general and valuable on its own, not merely because two places currently look alike.
- Account for the coupling introduced by shared code. Common components should represent stable concepts that are unlikely to change with any one caller.
