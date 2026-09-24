# Cooling illustration

Generated using the built-in imagegen tool. Asset: `frontend/public/cooling-interior-v2.png`.
This is an original illustrative background, not a photograph or a mechanically exact M3 board layout. The user's visual reference informed the composition; its image was not copied into the app.

## Airflow accuracy

Apple's [M3 fan repair guide](https://support.apple.com/en-us/103921) and [logic board guide](https://support.apple.com/en-us/103920) show the fan, thermal duct and heatsink. The overlay represents the general intake-to-impeller and exhaust-through-heatsink principle. It is not a measured airflow field or a CFD simulation. In particular, intake vent routes, airflow volume, air temperature and physical rotation direction are not measured. Orange exhaust cues cross the illustrated radiator toward the hinge; long speculative intake routes have been removed.

Only RPM and CPU temperature come from live sensors. Displayed rotation is slowed 60 times and proportional to RPM. Moving overlays pause when data is stale, the tab or component is hidden, reduced motion is enabled, or the user pauses animation.

## Generation prompt

```text
Use case: product-mockup.
Asset type: photorealistic background plate for an interactive local MacBook cooling dashboard, not a finished UI.
Primary request: an overhead cinematic cutaway of the inside of a modern compact aluminum laptop, bottom cover removed. Broad horizontal composition, near-orthographic camera straight down, no perspective distortion. Show the upper internal assembly across nearly the full width: silver-gray chassis rim, hinge at top, very detailed dark motherboard with tiny chips, solder, connectors and shielding, a broad dark curved heat pipe running from the central processor toward ONE SINGLE cooling blower on the right, fin stack at the top-right hinge. Partial dark battery cells at bottom fading naturally into black.
Composition: 1536x768 landscape approximately 2:1. Hardware occupies x 8%-92%, y 15%-85%. The SINGLE circular fan opening is centered precisely at x=76%, y=50% with outer radius about 10% of image width. Its center is a dark matte round cavity with a small metallic center hub; keep the interior dark because animated impeller blades will be composited in code. Do not put any second fan on the left. The left half shows a dense motherboard and connectors. Fan exhaust goes upward toward the hinge.
Style: convincingly photographic premium hardware teardown, realistic tiny electronics, brushed aluminum edges, black graphite internals, soft moody directional studio lighting, fine sharp material detail. Similar composition to a MacBook interior thermal airflow explainer, but an original illustrative hardware layout, not a certified physical blueprint.
Constraints: no text, logos, symbols, interface, numbers, watermarks, arrows, air trails, glow rings, or airflow drawn into the image. The air trails and luminous fan ring will be animated overlays added later. Black #0b0e12 background with subtle vignette around the edges. Keep all hardware readable, not pitch black.
```
