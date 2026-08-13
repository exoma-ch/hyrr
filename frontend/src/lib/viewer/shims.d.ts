/**
 * Ambient declarations for the viewer build (ADR 0008).
 *
 * `tsconfig.json` pins `types` to an explicit list, which suppresses the
 * ambient declarations a bundler would normally contribute — so the two module
 * shapes the viewer entry relies on have to be declared here.
 */

/** Side-effect CSS imports (`import "./lib/styles/tokens.css"`). */
declare module "*.css";

/**
 * The basic Plotly distribution ships no types (neither does the full one the
 * app uses — components hold it as `any`). The viewer only ever calls `react`
 * and `purge`; see `plotly-shim.ts`, which narrows to exactly that surface.
 */
declare module "plotly.js-basic-dist-min" {
  const Plotly: unknown;
  export default Plotly;
}
