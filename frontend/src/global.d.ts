declare const __APP_VERSION__: string;

declare module "*.svg?url" {
  const src: string;
  export default src;
}

interface ImportMetaEnv {
  readonly VITE_ISSUE_WORKER_URL?: string;
  readonly VITE_TURNSTILE_SITE_KEY?: string;
}

interface TurnstileInstance {
  render(container: HTMLElement, options: {
    sitekey: string;
    size?: "normal" | "compact" | "invisible";
    callback?: (token: string) => void;
    "error-callback"?: () => void;
  }): string;
  reset(widgetId: string): void;
  remove(widgetId: string): void;
  execute(widgetId: string): void;
}

interface Window {
  turnstile: TurnstileInstance;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module "plotly.js-dist-min" {
  const Plotly: any;
  export default Plotly;
  export function newPlot(
    root: HTMLElement,
    data: any[],
    layout?: any,
    config?: any,
  ): Promise<any>;
}

declare module "hyparquet" {
  export type Compressors = Record<string, (input: Uint8Array, outputLength: number) => Uint8Array>;
  export function parquetRead(options: {
    file: ArrayBuffer;
    compressors?: Compressors;
    rowFormat?: "object" | "array";
    onComplete: (data: any[]) => void;
    columns?: string[];
    rowStart?: number;
    rowEnd?: number;
  }): Promise<void>;
}

declare module "hyparquet-compressors" {
  import type { Compressors } from "hyparquet";
  export const compressors: Compressors;
}
