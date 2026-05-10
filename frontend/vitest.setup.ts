/**
 * vitest setup for the jsdom (component-render) project (#169).
 *
 * Wires `@testing-library/jest-dom`'s custom matchers (`toBeInTheDocument`,
 * `toBeDisabled`, `toHaveAttribute`, …) into vitest's `expect`. Only loaded
 * for `*.svelte.test.ts` files — the fast `node` project skips this.
 */
import "@testing-library/jest-dom/vitest";
