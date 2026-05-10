/**
 * Parse a thrown / returned data-fetch error into a typed
 * {@link FetchErrorPayload}.
 *
 * Mirrors the Rust-side `hyrr_core::data_fetch::FetchErrorPayload` chain
 * (see #118 / #157). The Tauri `ensure_data` and
 * `install_from_local_tarball` commands return their `Err(_)` arm as a
 * JSON-encoded payload string; this parser handles three shapes:
 *
 * 1. Structured object — `{kind: "FetchError", variant, ...}`.
 * 2. JSON string — `JSON.parse` into shape 1.
 * 3. Anything else — opaque legacy/unknown error; we surface the raw
 *    text under `kind: "unknown"` so the recovery card can still
 *    render a generic message + the bug-report fallback.
 */

export type FetchErrorPayload =
  | {
      kind: "FetchError";
      variant: "HttpStatus";
      status: number;
      url: string;
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "Network";
      detail: string;
      url: string;
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "Decompress";
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "Extract";
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "UnsafeTarballEntry";
      entry_kind: string;
      entry_path: string;
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "Io";
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "NoHome";
      message: string;
    };

export type ParsedFetchError =
  | FetchErrorPayload
  | { kind: "unknown"; message: string };

export function parseFetchError(raw: unknown): ParsedFetchError {
  const fromObj = tryStructured(raw);
  if (fromObj) return fromObj;

  if (typeof raw === "string") {
    try {
      const parsed = JSON.parse(raw);
      const fromString = tryStructured(parsed);
      if (fromString) return fromString;
    } catch {
      // not JSON — fall through to unknown
    }
    return { kind: "unknown", message: raw };
  }

  if (raw instanceof Error) {
    try {
      const parsed = JSON.parse(raw.message);
      const fromString = tryStructured(parsed);
      if (fromString) return fromString;
    } catch {
      // not JSON
    }
    return { kind: "unknown", message: raw.message };
  }

  if (raw && typeof raw === "object") {
    const m = (raw as { message?: unknown }).message;
    return {
      kind: "unknown",
      message: m != null ? String(m) : JSON.stringify(raw),
    };
  }

  return { kind: "unknown", message: String(raw ?? "Unknown fetch error") };
}

function tryStructured(raw: unknown): FetchErrorPayload | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  if (o.kind !== "FetchError") return null;
  const variant = o.variant;
  const message = str(o.message);

  if (variant === "HttpStatus") {
    return {
      kind: "FetchError",
      variant: "HttpStatus",
      status: num(o.status),
      url: str(o.url),
      cache_dir: str(o.cache_dir),
      message,
    };
  }
  if (variant === "Network") {
    return {
      kind: "FetchError",
      variant: "Network",
      detail: str(o.detail),
      url: str(o.url),
      cache_dir: str(o.cache_dir),
      message,
    };
  }
  if (variant === "Decompress") {
    return {
      kind: "FetchError",
      variant: "Decompress",
      cache_dir: str(o.cache_dir),
      message,
    };
  }
  if (variant === "Extract") {
    return {
      kind: "FetchError",
      variant: "Extract",
      cache_dir: str(o.cache_dir),
      message,
    };
  }
  if (variant === "UnsafeTarballEntry") {
    return {
      kind: "FetchError",
      variant: "UnsafeTarballEntry",
      entry_kind: str(o.entry_kind),
      entry_path: str(o.entry_path),
      cache_dir: str(o.cache_dir),
      message,
    };
  }
  if (variant === "Io") {
    return {
      kind: "FetchError",
      variant: "Io",
      cache_dir: str(o.cache_dir),
      message,
    };
  }
  if (variant === "NoHome") {
    return {
      kind: "FetchError",
      variant: "NoHome",
      message,
    };
  }
  return null;
}

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}
function num(v: unknown): number {
  return typeof v === "number" ? v : Number(v) || 0;
}

/** Title for the recovery-card header — variant-specific for support
 *  triage but bounded to "Couldn't download nuclear data — …". */
export function fetchErrorTitle(err: ParsedFetchError): string {
  if (err.kind === "unknown") {
    return "Couldn't download nuclear data";
  }
  switch (err.variant) {
    case "HttpStatus":
      return `Couldn't download nuclear data — HTTP ${err.status}`;
    case "Network":
      return "Couldn't reach the nuclear-data server";
    case "Decompress":
      return "Downloaded archive failed to decompress";
    case "Extract":
      return "Downloaded archive failed to extract";
    case "UnsafeTarballEntry":
      return "Downloaded archive contained an unsafe entry";
    case "Io":
      return "Couldn't write nuclear data to disk";
    case "NoHome":
      return "Couldn't locate user home directory";
  }
}
