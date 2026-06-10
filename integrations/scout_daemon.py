#!/usr/bin/env python3
"""
Engram Scout Daemon — Phase 4 Companion Service
================================================
Runs on http://127.0.0.1:8088

Provides a web search + LLM synthesis API for the Engram server.
Runs as a SEPARATE process (no CUDA/fork conflict).

Endpoints:
  GET /search?q={query}&max={n}   — DDG search + Gemma synthesis
  GET /health                      — health check

Config (env vars):
  ENGRAM_SCOUT_PORT       — listen port (default: 8088)
  ENGRAM_SCOUT_LLM_URL    — LLM base URL (default: http://localhost:11434)
  ENGRAM_SCOUT_LLM_MODEL  — model name  (default: gemma4:e4b-nemo)
  ENGRAM_SCOUT_MAX_TOKENS — synthesis token cap (default: 512)

Usage:
  python3 integrations/scout_daemon.py
  # or daemonize:
  nohup python3 integrations/scout_daemon.py > ~/.engram/scout.log 2>&1 &
"""

import json
import os
import re
import sys
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer

# ── Config ────────────────────────────────────────────────────────────────────

PORT       = int(os.environ.get("ENGRAM_SCOUT_PORT",       "8088"))
LLM_URL    = os.environ.get("ENGRAM_SCOUT_LLM_URL",    "http://localhost:11434")
LLM_MODEL  = os.environ.get("ENGRAM_SCOUT_LLM_MODEL",  "gemma4:e4b-nemo")
MAX_TOKENS = int(os.environ.get("ENGRAM_SCOUT_MAX_TOKENS", "512"))

# ── HTML cleanup ──────────────────────────────────────────────────────────────

HTML_TAG_RE    = re.compile(r'<[^>]+>')
SNIPPET_CLASS  = re.compile(r'class="result-snippet"[^>]*>(.*?)</td>', re.DOTALL)
TITLE_CLASS    = re.compile(r'class="result-link"[^>]*>(.*?)</a>', re.DOTALL)

def strip_html(s: str) -> str:
    s = HTML_TAG_RE.sub('', s)
    for entity, char in [('&amp;','&'),('&lt;','<'),('&gt;','>'),
                          ('&quot;','"'),('&#39;',"'"),('&nbsp;',' ')]:
        s = s.replace(entity, char)
    return ' '.join(s.split())

# ── DDG search ────────────────────────────────────────────────────────────────

def search_ddg(query: str, max_results: int) -> list[dict]:
    url = "https://lite.duckduckgo.com/lite/?q=" + urllib.parse.quote_plus(query)
    req = urllib.request.Request(
        url,
        headers={"User-Agent": "Mozilla/5.0 (compatible; EngramScout/0.4)"}
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            html = resp.read().decode("utf-8", errors="replace")
    except Exception as e:
        print(f"[scout] DDG fetch error: {e}", file=sys.stderr)
        return []

    titles   = [strip_html(m.group(1)) for m in TITLE_CLASS.finditer(html)   if strip_html(m.group(1))][:max_results]
    snippets = [strip_html(m.group(1)) for m in SNIPPET_CLASS.finditer(html) if strip_html(m.group(1))][:max_results]

    results = [{"title": t, "snippet": s} for t, s in zip(titles, snippets)]
    print(f"[scout] DDG: {len(results)} snippets for {query!r}", file=sys.stderr)
    return results

# ── LLM synthesis via ollama ──────────────────────────────────────────────────

def synthesize(query: str, snippets: list[dict]) -> str:
    snippet_block = "\n\n".join(
        f"[{i+1}] {s['title']}\n   {s['snippet']}" for i, s in enumerate(snippets)
    )
    system = ("You are a precise knowledge extraction agent. "
              "Given web search snippets, produce a dense factual summary. "
              "Be concise. Prioritize verifiable facts. Avoid filler.")
    user   = (f"Query: \"{query}\"\n\nWeb snippets:\n{snippet_block}\n\n"
              "Write a concise factual summary (3-5 sentences):")

    body = json.dumps({
        "model": LLM_MODEL,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user}
        ],
        "max_tokens": MAX_TOKENS,
        "temperature": 0.3
    }).encode()

    url = f"{LLM_URL}/v1/chat/completions"
    req = urllib.request.Request(url, data=body,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=90) as resp:
            data = json.loads(resp.read())
        return data["choices"][0]["message"]["content"].strip()
    except Exception as e:
        print(f"[scout] LLM error: {e}", file=sys.stderr)
        return f"[LLM synthesis failed: {e}]"

# ── HTTP handler ──────────────────────────────────────────────────────────────

class ScoutHandler(BaseHTTPRequestHandler):

    def log_message(self, fmt, *args):  # suppress default access log to stderr
        print(f"[scout] {self.address_string()} {fmt % args}", file=sys.stderr)

    def send_json(self, code: int, obj: object):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        params = dict(urllib.parse.parse_qsl(parsed.query))

        if parsed.path == "/health":
            self.send_json(200, {"status": "ok", "model": LLM_MODEL, "port": PORT})
            return

        if parsed.path == "/search":
            query = params.get("q", "").strip()
            max_r = int(params.get("max", "5"))
            if not query:
                self.send_json(400, {"error": "q param required"})
                return

            snippets = search_ddg(query, max_r)
            if not snippets:
                self.send_json(404, {"error": f"No results for {query!r}"})
                return

            summary  = synthesize(query, snippets)
            self.send_json(200, {
                "query":    query,
                "summary":  summary,
                "snippets": snippets,
            })
            return

    def do_POST(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/shutdown":
            self.send_json(200, {"status": "shutting_down"})
            import threading
            threading.Thread(target=self.server.shutdown).start()
            return
        self.send_json(404, {"error": f"Unknown path: {parsed.path}"})


# ── Entry point ───────────────────────────────────────────────────────────────

def shutdown_existing(port: int):
    url = f"http://127.0.0.1:{port}/shutdown"
    try:
        req = urllib.request.Request(url, method="POST")
        with urllib.request.urlopen(req, timeout=1.0) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            if data.get("status") == "shutting_down":
                print(f"[scout] Sent shutdown signal to existing daemon on port {port}.", file=sys.stderr)
                import time
                time.sleep(0.5)
    except Exception:
        pass

if __name__ == "__main__":
    shutdown_existing(PORT)
    server = HTTPServer(("127.0.0.1", PORT), ScoutHandler)
    print(f"[scout] Engram Scout Daemon online — http://127.0.0.1:{PORT}", file=sys.stderr)
    print(f"[scout] LLM: {LLM_URL} model={LLM_MODEL}", file=sys.stderr)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("[scout] Shutting down.", file=sys.stderr)
