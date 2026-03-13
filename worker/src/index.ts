/**
 * Cloudflare Worker: GitHub App issue proxy + screenshot hosting for HYRR.
 *
 * Routes:
 *   POST /             — Create a GitHub issue (JSON: { title, body, labels?, screenshot? })
 *   POST /upload       — Upload a screenshot to R2 (binary body, returns URL)
 *   GET  /img/:key     — Serve a screenshot from R2
 */

interface Env {
  GITHUB_APP_ID: string;
  GITHUB_INSTALL_ID: string;
  GITHUB_APP_PRIVATE_KEY: string;
  ALLOWED_ORIGIN: string;
  SCREENSHOTS: R2Bucket;
}

const REPO_OWNER = "exoma-ch";
const REPO_NAME = "hyrr";
const MAX_IMAGE_BYTES = 2 * 1024 * 1024; // 2 MB
const MAX_BUCKET_OBJECTS = 1000; // hard cap on total screenshots

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const origin = request.headers.get("Origin") ?? "";
    const allowed =
      origin === env.ALLOWED_ORIGIN ||
      origin === "http://localhost:5173" ||
      origin === "http://localhost:4173";

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
          "Cache-Control": "public, max-age=31536000, immutable",
        },
      });
    }

    // Upload screenshot
    if (request.method === "POST" && url.pathname === "/upload") {
      return handleUpload(request, env, origin, allowed);
    }

    // Create issue
    if (request.method === "POST" && (url.pathname === "/" || url.pathname === "")) {
      return handleCreateIssue(request, env, origin, allowed);
    }

    return json({ error: "Not found" }, 404, origin, allowed);
  },
};

// ── Upload handler ──────────────────────────────────────────

async function handleUpload(
  request: Request,
  env: Env,
  origin: string,
  allowed: boolean
): Promise<Response> {
  const contentLength = parseInt(request.headers.get("Content-Length") ?? "0", 10);
  if (contentLength > MAX_IMAGE_BYTES) {
    return json({ error: `Image too large (max ${MAX_IMAGE_BYTES / 1024 / 1024} MB)` }, 413, origin, allowed);
  }

  // Check bucket object count (soft cap)
  const list = await env.SCREENSHOTS.list({ limit: 1, include: ["httpMetadata"] });
  // R2 list doesn't return total count directly; use a prefix count approach
  const countList = await env.SCREENSHOTS.list({ limit: MAX_BUCKET_OBJECTS + 1 });
  if (countList.objects.length >= MAX_BUCKET_OBJECTS) {
    return json({ error: "Screenshot storage is full" }, 507, origin, allowed);
  }

  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_IMAGE_BYTES) {
    return json({ error: `Image too large (max ${MAX_IMAGE_BYTES / 1024 / 1024} MB)` }, 413, origin, allowed);
  }

  const contentType = request.headers.get("Content-Type") ?? "image/png";
  const ext = contentType.includes("jpeg") || contentType.includes("jpg") ? "jpg" : "png";
  const key = `${Date.now()}-${crypto.randomUUID().slice(0, 8)}.${ext}`;

  await env.SCREENSHOTS.put(key, body, {
    httpMetadata: { contentType },
  });

  const imgUrl = `${new URL(request.url).origin}/img/${key}`;
  return json({ url: imgUrl, key }, 201, origin, allowed);
}

// ── Issue creation handler ──────────────────────────────────

async function handleCreateIssue(
  request: Request,
  env: Env,
  origin: string,
  allowed: boolean
): Promise<Response> {
  try {
    const { title, body, labels } = (await request.json()) as {
      title: string;
      body: string;
      labels?: string[];
    };

    if (!title || !body) {
      return json({ error: "title and body are required" }, 400, origin, allowed);
    }

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
      return json({ error: "GitHub auth failed" }, 502, origin, allowed);
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
          labels: labels ?? ["bug"],
        }),
      }
    );

    if (!issueRes.ok) {
      console.error("Issue creation failed:", await issueRes.text());
      return json({ error: "Failed to create issue" }, 502, origin, allowed);
    }

    const issue = (await issueRes.json()) as { html_url: string; number: number };
    return json({ url: issue.html_url, number: issue.number }, 201, origin, allowed);
  } catch (e) {
    console.error("Worker error:", e);
    return json({ error: "Internal error" }, 500, origin, allowed);
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
