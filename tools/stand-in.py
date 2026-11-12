#!/usr/bin/env python3
"""A stand-in for the outside services Turbine Hub reads, so the build's checks never depend on them being up. It
copies nobody's pages: it answers at the same paths with small made-up data in the same shape (tools/stand-in-data,
which the engine's own parsers are tested against in engine/tests/standin.rs).

  python3 tools/stand-in.py --port 47900 [--log stand-in.log]
  python3 tools/stand-in.py --self-test

Serves on 127.0.0.1 only:
  /saleaction/ajaxgetdeckappcompatibilityreport?nAppID=<id>   Valve's rating for one game
  /AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json      the anti-cheat list
  /Ravenor907/TurbineHub/main/engine/data/fixes.json          Turbine Hub's own fixes
  /Ravenor907/TurbineHub/releases.atom                        the release feed (0.2, and 0.3 as a test build)
  /Ravenor907/TurbineHub/releases/latest                      redirects to the newest full release's page
  /Ravenor907/TurbineHub/releases/download/<tag>/TurbineHub.flatpak and .sha256   a small file and its checksum
  /down                                                       answers 503, for "the service is down" screens
Every request is logged with its time. Anything else is a 404.
"""
import argparse
import hashlib
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "stand-in-data")
REPO = "/Ravenor907/TurbineHub"
BUNDLE = b"stand-in bundle, not a real Flatpak\n" * 64
LOCK = threading.Lock()


def data(name):
    with open(os.path.join(DATA, name), "rb") as f:
        return f.read()


def feed():
    entries = "".join(
        f'<entry><id>tag:github.com,2008:Repository/1/{tag}</id><updated>{when}</updated>'
        f'<link rel="alternate" type="text/html" href="https://github.com{REPO}/releases/tag/{tag}"/><title>{title}</title></entry>'
        for tag, when, title in [("v0.3", "2026-11-10T10:00:00Z", "Turbine Hub 0.3 (test build)"), ("v0.2", "2026-11-01T10:00:00Z", "Turbine Hub 0.2")])
    return f'<?xml version="1.0" encoding="UTF-8"?>\n<feed xmlns="http://www.w3.org/2005/Atom"><title>Release notes from TurbineHub</title>{entries}</feed>\n'.encode()


def answer(path, query):
    """(status, content type, body, extra headers) for one request."""
    if path == "/saleaction/ajaxgetdeckappcompatibilityreport":
        reports = json.loads(data("valve.json"))
        app = (query.get("nAppID") or [""])[0]
        return 200, "application/json", json.dumps(reports.get(app, {"success": 1, "results": []})).encode(), {}
    if path == "/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json":
        return 200, "application/json", data("anticheat.json"), {}
    if path == REPO + "/main/engine/data/fixes.json":
        return 200, "application/json", data("fixes.json"), {}
    if path == REPO + "/releases.atom":
        return 200, "application/atom+xml", feed(), {}
    if path == REPO + "/releases/latest":
        return 302, "text/plain", b"", {"Location": REPO + "/releases/tag/v0.2"}
    if path.startswith(REPO + "/releases/tag/v"):
        return 200, "text/html", b"<html><body>Release</body></html>", {}
    if path.startswith(REPO + "/releases/download/v"):
        name = path.rsplit("/", 1)[1]
        if name == "TurbineHub.flatpak":
            return 200, "application/octet-stream", BUNDLE, {}
        if name == "TurbineHub.flatpak.sha256":
            return 200, "text/plain", (hashlib.sha256(BUNDLE).hexdigest() + "  TurbineHub.flatpak\n").encode(), {}
    if path == "/down":
        return 503, "text/plain", b"down for the check\n", {}
    return 404, "text/plain", b"not here\n", {}


def handler(log_path):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            url = urllib.parse.urlsplit(self.path)
            status, kind, body, extra = answer(url.path, urllib.parse.parse_qs(url.query))
            if log_path:
                with LOCK, open(log_path, "a") as f:
                    f.write(f"{time.time():.3f} {status} {self.path}\n")
            self.send_response(status)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            for k, v in extra.items():
                self.send_header(k, v)
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    return Handler


def serve(port, log_path):
    server = ThreadingHTTPServer(("127.0.0.1", port), handler(log_path))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def self_test():
    server = serve(0, None)
    base = f"http://127.0.0.1:{server.server_address[1]}"

    def get(path):
        try:
            with urllib.request.urlopen(base + path, timeout=10) as r:
                return r.status, r.read(), r.geturl()
        except urllib.error.HTTPError as e:
            return e.code, e.read(), base + path

    problems = []

    def check(what, ok):
        if not ok:
            problems.append(what)

    status, body, _ = get("/saleaction/ajaxgetdeckappcompatibilityreport?nAppID=1245620")
    check("a rated game", status == 200 and json.loads(body)["results"]["steamos_resolved_category"] == 3)
    status, body, _ = get("/saleaction/ajaxgetdeckappcompatibilityreport?nAppID=1")
    check("a game without a rating", status == 200 and json.loads(body)["results"] == [])
    status, body, _ = get("/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json")
    check("the anti-cheat list", status == 200 and any(g["storeIds"].get("steam") == "1172470" for g in json.loads(body)))
    status, body, _ = get(REPO + "/main/engine/data/fixes.json")
    check("the fixes", status == 200 and json.loads(body)["1245620"]["kind"] == "switch")
    status, body, _ = get(REPO + "/releases.atom")
    check("the feed", status == 200 and b"/releases/tag/v0.3" in body and body.index(b"v0.3") < body.index(b"v0.2"))
    status, _, url = get(REPO + "/releases/latest")
    check("the newest release", status == 200 and url.endswith("/releases/tag/v0.2"))
    _, bundle, _ = get(REPO + "/releases/download/v0.2/TurbineHub.flatpak")
    _, sums, _ = get(REPO + "/releases/download/v0.2/TurbineHub.flatpak.sha256")
    check("the bundle's checksum", sums.split()[0].decode() == hashlib.sha256(bundle).hexdigest())
    check("a service that is down", get("/down")[0] == 503)
    check("an unknown path", get("/etc/passwd")[0] == 404)
    server.shutdown()
    for p in problems:
        print(f"::error title=Stand-in server::{p} isn't answered right")
    print(f"Stand-in server: {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=47900)
    ap.add_argument("--log", default="")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        sys.exit(self_test())
    serve(a.port, a.log)
    print(f"stand-in on http://127.0.0.1:{a.port}", flush=True)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass
