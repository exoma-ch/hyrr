/**
 * Plotly entry for the viewer build (ADR 0008).
 *
 * Two jobs:
 *
 * 1. **Swap the bundle.** The result plots use only `scatter` and `bar`, so the
 *    viewer builds against `plotly.js-basic-dist-min` (1.1 MB) instead of the
 *    full `plotly.js-dist-min` (4.8 MB). That single swap is most of the
 *    artifact's size budget.
 *
 * 2. **Normalise the namespace.** Both packages are UMD bundles with only a
 *    `main` field. The app build reaches them through a code-split dynamic
 *    import, where the bundler's CJS interop hoists named exports; the viewer
 *    builds with `codeSplitting: false`, where the module is inlined and the
 *    namespace can instead arrive as `{ default: … }`. The shared components do
 *    `Plotly = await import(...)` and then call `Plotly.react(...)`, so they
 *    need the methods on the namespace either way.
 *
 * Components use exactly two methods — keep this surface that small; anything
 * wider is a signal the viewer is drifting toward being the app.
 */
import PlotlyDefault from "plotly.js-basic-dist-min";

interface PlotlyApi {
  react: (...args: unknown[]) => unknown;
  purge: (...args: unknown[]) => unknown;
}

// Depending on how the CJS module is interop'd, the callable object is either
// the imported binding itself or its `.default`.
const candidate = PlotlyDefault as unknown as Partial<PlotlyApi> & { default?: PlotlyApi };
const P: PlotlyApi = (typeof candidate?.react === "function"
  ? candidate
  : candidate?.default) as PlotlyApi;

if (!P || typeof P.react !== "function") {
  throw new Error("plotly-shim: could not resolve the Plotly API from plotly.js-basic-dist-min");
}

export const react: PlotlyApi["react"] = (...args) => P.react(...args);
export const purge: PlotlyApi["purge"] = (...args) => P.purge(...args);
export default P;
