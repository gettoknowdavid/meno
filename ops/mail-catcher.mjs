#!/usr/bin/env node
// Local catcher for outgoing transactional mail.
//
// Why this exists
// ---------------
// The API sends mail over an HTTPS API (Brevo), not SMTP. That is the right transport
// in production, but it means the usual local SMTP catcher (Mailpit, in
// `docker-compose.yml`) is never spoken to — it would sit there accepting connections
// that nothing makes. So the local story is "run something that speaks the mail API",
// and this is that thing.
//
// Point the API at it and the *production* adapter, the production rendering and the
// production request path all run unchanged; only the hostname differs:
//
//     SMTP_ENDPOINT=http://127.0.0.1:1026
//
// (the full provider path, `http://127.0.0.1:1026/v3/smtp/email`, works too)
//
// Consequences worth knowing: no provider account, no quota, no 401 from a placeholder
// key, and a message you can actually read. Nothing about a send here is real, so do
// not wire this into a deployed environment — the URL is the only thing distinguishing
// it, and `ops/docker-compose.yml` does not start it.
//
// Usage
// -----
//     node ops/mail-catcher.mjs            # listens on 127.0.0.1:1026
//     node ops/mail-catcher.mjs 9000       # or pick a port
//
// Then open http://127.0.0.1:1026/ for the inbox, or watch the console.
//
// Deliberately dependency-free: no `npm install`, no lockfile, nothing to drift. The
// provider's send endpoint accepts JSON and answers `{"messageId": "..."}`, so that is
// the whole contract to reproduce.

import { createServer } from 'node:http';

const port = Number(process.argv[2] ?? 1026);
const host = '127.0.0.1';

/** Newest first, capped so a long session cannot grow without bound. */
const MAX_MESSAGES = 100;

/** @type {{at: string, to: string, from: string, subject: string, html: string, raw: unknown}[]} */
let inbox = [];

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

/** Pull the code out of the rendered body so it is visible without reading HTML. */
function extractCode(html) {
  const sixDigits = html.match(/\b\d{6}\b/);
  return sixDigits ? sixDigits[0] : null;
}

function inboxPage() {
  const rows = inbox
    .map((message) => {
      const code = extractCode(message.html);
      return `<article>
  <header><strong>${escapeHtml(message.to)}</strong> &larr; ${escapeHtml(message.from)}</header>
  <div class="meta">${escapeHtml(message.at)}${code ? ` &middot; code <code>${code}</code>` : ''}</div>
  <h2>${escapeHtml(message.subject)}</h2>
  <iframe srcdoc="${escapeHtml(message.html)}"></iframe>
</article>`;
    })
    .join('\n');

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Mail catcher</title>
<style>
  body { font: 15px/1.5 system-ui, sans-serif; margin: 2rem auto; max-width: 46rem; color: #1a1a1a; }
  h1 { font-size: 1.3rem; }
  article { border: 1px solid #d8d8d8; border-radius: 8px; padding: 1rem; margin: 1rem 0; }
  header { font-weight: 600; }
  .meta { color: #666; font-size: 0.85rem; margin: 0.25rem 0 0.75rem; }
  h2 { font-size: 1rem; margin: 0 0 0.5rem; }
  code { background: #f2f2f2; padding: 0.1rem 0.35rem; border-radius: 4px; font-size: 1.05rem; }
  iframe { width: 100%; height: 14rem; border: 1px solid #eee; border-radius: 4px; }
  .empty { color: #666; }
</style>
</head>
<body>
<h1>Mail catcher <small>(${inbox.length} message${inbox.length === 1 ? '' : 's'})</small></h1>
${
  inbox.length === 0
    ? '<p class="empty">Nothing yet. Register a user, or call <code>POST /auth/resend-otp</code>.</p>'
    : rows
}
</body>
</html>`;
}

function readBody(request) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    request.on('data', (chunk) => chunks.push(chunk));
    request.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    request.on('error', reject);
  });
}

const server = createServer(async (request, response) => {
  const url = new URL(request.url ?? '/', `http://${request.headers.host ?? host}`);

  if (request.method === 'GET' && (url.pathname === '/' || url.pathname === '/inbox')) {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end(inboxPage());
    return;
  }

  // The provider's send endpoint. Any path is accepted, because `SMTP_ENDPOINT` is a
  // base URL a developer may well set without the provider's `/v3/smtp/email` suffix —
  // rejecting that with a 404 would look like a broken catcher rather than a URL that
  // needs completing. The path is logged so a wrong one is still visible.
  if (request.method === 'POST') {
    let payload;
    try {
      payload = JSON.parse(await readBody(request));
    } catch {
      response.writeHead(400, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ message: 'body is not JSON', code: 'invalid_parameter' }));
      return;
    }

    const sender = Array.isArray(payload.sender) ? payload.sender[0] : payload.sender;
    const to = Array.isArray(payload.to) ? payload.to[0] : (payload.to ?? {});
    const subject = payload.subject ?? '(no subject)';
    const html = payload.htmlContent ?? payload.textContent ?? '';

    inbox.unshift({
      at: new Date().toISOString(),
      to: to.email ?? '(no recipient)',
      from: sender?.email ?? '(no sender)',
      subject,
      html,
      raw: payload,
    });
    inbox.length = Math.min(inbox.length, MAX_MESSAGES);

    const code = extractCode(html);
    console.log(`\n--- ${new Date().toISOString()}`);
    console.log(`path:    ${url.pathname}`);
    console.log(`to:      ${to.email ?? '(none)'}`);
    console.log(`from:    ${sender?.email ?? '(none)'}`);
    console.log(`subject: ${subject}`);
    if (code) console.log(`code:    ${code}`);
    console.log(`api-key: ${request.headers['api-key'] ? 'present' : 'absent'}`);

    // The shape the adapter treats as success.
    response.writeHead(201, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ messageId: `<${Date.now()}@mail-catcher>` }));
    return;
  }

  response.writeHead(404, { 'content-type': 'application/json' });
  response.end(JSON.stringify({ message: 'not found', code: 'document_not_found' }));
});

server.listen(port, host, () => {
  console.log(`mail catcher listening on http://${host}:${port}`);
  console.log(`inbox: http://${host}:${port}/`);
  console.log('point the API at it with SMTP_ENDPOINT=http://%s:%d', host, port);
});