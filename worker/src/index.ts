/**
 * Cloudflare Worker: GitHub App issue proxy + screenshot hosting for HYRR.
 *
 * Routes:
 *   POST /             — Create a GitHub issue (JSON: { title, body, labels?, email, cf-turnstile-response })
 *   POST /upload       — Upload a screenshot to R2 (binary body, returns URL)
 *   GET  /img/:key     — Serve a screenshot from R2
 */

interface Env {
  GITHUB_APP_ID: string;
  GITHUB_INSTALL_ID: string;
  GITHUB_APP_PRIVATE_KEY: string;
  TURNSTILE_SECRET: string;
  ALLOWED_ORIGIN: string;
  SCREENSHOTS: R2Bucket;
}

const REPO_OWNER = "exoma-ch";
const REPO_NAME = "hyrr";
const MAX_IMAGE_BYTES = 2 * 1024 * 1024; // 2 MB
const MAX_BUCKET_OBJECTS = 1000; // hard cap on total screenshots
const MAX_TITLE_LENGTH = 200;
const MAX_BODY_LENGTH = 10_000;
const ALLOWED_LABELS = ["bug"];
const ALLOWED_CONTENT_TYPES = ["image/jpeg", "image/png", "image/webp"];

// Magic bytes for image format validation
const MAGIC_BYTES: Record<string, number[]> = {
  "image/jpeg": [0xff, 0xd8, 0xff],
  "image/png": [0x89, 0x50, 0x4e, 0x47],
  "image/webp": [0x52, 0x49, 0x46, 0x46], // "RIFF" prefix
};

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const origin = request.headers.get("Origin") ?? "";
    const allowed =
      origin === env.ALLOWED_ORIGIN ||
      origin.startsWith("http://localhost:");

    // CORS preflight
    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: corsHeaders(origin, allowed),
      });
    }

    // Serve screenshots
    if (request.method === "GET" && url.pathname.startsWith("/img/")) {
      const key = url.pathname.slice(5);
      const obj = await env.SCREENSHOTS.get(key);
      if (!obj) return new Response("Not found", { status: 404 });
      return new Response(obj.body, {
        headers: {
          "Content-Type": obj.httpMetadata?.contentType ?? "image/png",
          "Content-Disposition": "inline",
          "Cache-Control": "public, max-age=31536000, immutable",
        },
      });
    }

    // ── Origin enforcement for mutation routes ──
    if (!allowed) {
      return json({ error: "Forbidden" }, 403, origin, false);
    }

    // Upload screenshot
    if (request.method === "POST" && url.pathname === "/upload") {
      return handleUpload(request, env, origin);
    }

    // Create issue
    if (request.method === "POST" && (url.pathname === "/" || url.pathname === "")) {
      return handleCreateIssue(request, env, origin);
    }

    return json({ error: "Not found" }, 404, origin, allowed);
  },
};

// ── Upload handler ──────────────────────────────────────────

async function handleUpload(
  request: Request,
  env: Env,
  origin: string,
): Promise<Response> {
  const contentType = request.headers.get("Content-Type") ?? "";
  if (!ALLOWED_CONTENT_TYPES.includes(contentType)) {
    return json(
      { error: `Unsupported content type. Allowed: ${ALLOWED_CONTENT_TYPES.join(", ")}` },
      415, origin, true,
    );
  }

  const contentLength = parseInt(request.headers.get("Content-Length") ?? "0", 10);
  if (contentLength > MAX_IMAGE_BYTES) {
    return json({ error: `Image too large (max ${MAX_IMAGE_BYTES / 1024 / 1024} MB)` }, 413, origin, true);
  }

  // Check bucket object count (soft cap)
  const countList = await env.SCREENSHOTS.list({ limit: MAX_BUCKET_OBJECTS + 1 });
  if (countList.objects.length >= MAX_BUCKET_OBJECTS) {
    return json({ error: "Screenshot storage is full" }, 507, origin, true);
  }

  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_IMAGE_BYTES) {
    return json({ error: `Image too large (max ${MAX_IMAGE_BYTES / 1024 / 1024} MB)` }, 413, origin, true);
  }

  // Validate magic bytes match declared content type
  const header = new Uint8Array(body.slice(0, 12));
  const expected = MAGIC_BYTES[contentType];
  if (expected && !expected.every((b, i) => header[i] === b)) {
    return json({ error: "File content does not match declared content type" }, 400, origin, true);
  }

  const ext = contentType === "image/jpeg" ? "jpg"
    : contentType === "image/webp" ? "webp"
    : "png";
  const key = `${Date.now()}-${crypto.randomUUID().slice(0, 8)}.${ext}`;

  await env.SCREENSHOTS.put(key, body, {
    httpMetadata: { contentType, contentDisposition: "inline" },
  });

  const imgUrl = `${new URL(request.url).origin}/img/${key}`;
  return json({ url: imgUrl, key }, 201, origin, true);
}

// ── Issue creation handler ──────────────────────────────────

async function handleCreateIssue(
  request: Request,
  env: Env,
  origin: string,
): Promise<Response> {
  try {
    const payload = (await request.json()) as {
      title: string;
      body: string;
      labels?: string[];
      email?: string;
      "cf-turnstile-response"?: string;
    };

    const { title, body, email } = payload;
    const turnstileToken = payload["cf-turnstile-response"];

    // Required fields
    if (!title || !body) {
      return json({ error: "title and body are required" }, 400, origin, true);
    }
    if (!email || !email.trim()) {
      return json({ error: "email is required" }, 400, origin, true);
    }

    // Input limits
    if (title.length > MAX_TITLE_LENGTH) {
      return json({ error: `title must be ${MAX_TITLE_LENGTH} chars or fewer` }, 400, origin, true);
    }
    if (body.length > MAX_BODY_LENGTH) {
      return json({ error: `body must be ${MAX_BODY_LENGTH} chars or fewer` }, 400, origin, true);
    }

    // Restrict labels to allowlist
    const labels = (payload.labels ?? ["bug"]).filter((l) => ALLOWED_LABELS.includes(l));

    // Turnstile verification
    if (!turnstileToken) {
      return json({ error: "Turnstile verification required" }, 400, origin, true);
    }

    const tsRes = await fetch("https://challenges.cloudflare.com/turnstile/v0/siteverify", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        secret: env.TURNSTILE_SECRET,
        response: turnstileToken,
        remoteip: request.headers.get("CF-Connecting-IP") ?? "",
      }),
    });

    const tsData = (await tsRes.json()) as { success: boolean };
    if (!tsData.success) {
      return json({ error: "Turnstile verification failed" }, 403, origin, true);
    }

    // GitHub App auth
    const jwt = await createAppJwt(env.GITHUB_APP_ID, env.GITHUB_APP_PRIVATE_KEY);

    const tokenRes = await fetch(
      `https://api.github.com/app/installations/${env.GITHUB_INSTALL_ID}/access_tokens`,
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${jwt}`,
          Accept: "application/vnd.github+json",
          "User-Agent": "hyrr-issue-worker",
        },
      }
    );

    if (!tokenRes.ok) {
      console.error("Token exchange failed:", await tokenRes.text());
      return json({ error: "GitHub auth failed" }, 502, origin, true);
    }

    const { token } = (await tokenRes.json()) as { token: string };

    const issueRes = await fetch(
      `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/issues`,
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          Accept: "application/vnd.github+json",
          "User-Agent": "hyrr-issue-worker",
        },
        body: JSON.stringify({
          title,
          body,
          labels,
        }),
      }
    );

    if (!issueRes.ok) {
      console.error("Issue creation failed:", await issueRes.text());
      return json({ error: "Failed to create issue" }, 502, origin, true);
    }

    const issue = (await issueRes.json()) as { html_url: string; number: number };
    return json({ url: issue.html_url, number: issue.number }, 201, origin, true);
  } catch (e) {
    console.error("Worker error:", e);
    return json({ error: "Internal error" }, 500, origin, true);
  }
}

// ── JWT generation (RS256) ──────────────────────────────────

async function createAppJwt(appId: string, pemKey: string): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  const payload = { iat: now - 60, exp: now + 600, iss: appId };
  const key = await importPkcs8(pemKey);

  const header = b64url(JSON.stringify({ alg: "RS256", typ: "JWT" }));
  const body = b64url(JSON.stringify(payload));
  const signingInput = new TextEncoder().encode(`${header}.${body}`);
  const sig = await crypto.subtle.sign(
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    key,
    signingInput
  );

  return `${header}.${body}.${b64url(sig)}`;
}

async function importPkcs8(pem: string): Promise<CryptoKey> {
  const lines = pem
    .replace(/-----BEGIN RSA PRIVATE KEY-----/, "")
    .replace(/-----END RSA PRIVATE KEY-----/, "")
    .replace(/-----BEGIN PRIVATE KEY-----/, "")
    .replace(/-----END PRIVATE KEY-----/, "")
    .replace(/\s/g, "");

  const der = Uint8Array.from(atob(lines), (c) => c.charCodeAt(0));
  return crypto.subtle.importKey(
    "pkcs8",
    der,
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    false,
    ["sign"]
  );
}

function b64url(input: string | ArrayBuffer): string {
  const bytes =
    typeof input === "string"
      ? new TextEncoder().encode(input)
      : new Uint8Array(input);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

// ── Helpers ─────────────────────────────────────────────────

function corsHeaders(origin: string, allowed: boolean): Record<string, string> {
  return {
    "Access-Control-Allow-Origin": allowed ? origin : "",
    "Access-Control-Allow-Methods": "POST, GET, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type",
    "Access-Control-Max-Age": "86400",
  };
}

function json(
  data: Record<string, unknown>,
  status: number,
  origin: string,
  allowed: boolean
): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json", ...corsHeaders(origin, allowed) },
  });
}
