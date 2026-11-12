#!/usr/bin/env python3
"""A stand-in for the outside services Turbine Hub reads, so the build's checks never depend on them being up. It
copies nobody's pages: it answers at the same paths with small made-up data in the same shape (tools/stand-in-data,
which the engine's own parsers are tested against in engine/tests/standin.rs).

  python3 tools/stand-in.py --port 47900 [--log stand-in.log]     (--port 0: any free port; the first line says which)
  python3 tools/stand-in.py --self-test

Serves on 127.0.0.1 only:
  /saleaction/ajaxgetdeckappcompatibilityreport?nAppID=<id>   Valve's rating for one game
  /AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json      the anti-cheat list
  /Ravenor907/TurbineHub/main/engine/data/fixes.json          Turbine Hub's own fixes
  /Ravenor907/TurbineHub/releases.atom                        the release feed (0.2, and 0.3 as a test build)
  /Ravenor907/TurbineHub/releases/latest                      redirects to the newest full release's page
  /Ravenor907/TurbineHub/releases/download/<tag>/TurbineHub.flatpak and .sha256   a small file and its checksum
  /repos/GloriousEggroll/proton-ge-custom/releases/latest     GitHub's API answer for a made-up GE-Proton release
  /GloriousEggroll/proton-ge-custom/releases/latest           redirects to that release's page
  /GloriousEggroll/proton-ge-custom/releases/download/GE-Proton11-99/GE-Proton11-99-x86_64.tar.gz and .sha512sum
                                                              a small made-up Proton build and its checksum (one for
                                                              aarch64 too, as releases have had since GE-Proton 11)
  /GloriousEggroll/proton-ge-custom/releases/download/GE-Proton10-98/GE-Proton10-98.tar.gz and .sha512sum
                                                              an older release, named the way they were before that
  /api/v1/reports/summaries/<id>.json                         what players say about one game (ProtonDB's shape)
  /down                                                       answers 503, for "the service is down" screens
Every request is logged with its time. Anything else is a 404.
"""
import argparse
import gzip
import hashlib
import io
import json
import os
import sys
import tarfile
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
GE = "/GloriousEggroll/proton-ge-custom"
GE_TAG = "GE-Proton11-99"
GE_OLD = "GE-Proton10-98"


def data(name):
    with open(os.path.join(DATA, name), "rb") as f:
        return f.read()


def feed():
    entries = "".join(
        f'<entry><id>tag:github.com,2008:Repository/1/{tag}</id><updated>{when}</updated>'
        f'<link rel="alternate" type="text/html" href="https://github.com{REPO}/releases/tag/{tag}"/><title>{title}</title></entry>'
        for tag, when, title in [("v0.3", "2026-11-10T10:00:00Z", "Turbine Hub 0.3 (test build)"), ("v0.2", "2026-11-01T10:00:00Z", "Turbine Hub 0.2")])
    return f'<?xml version="1.0" encoding="UTF-8"?>\n<feed xmlns="http://www.w3.org/2005/Atom"><title>Release notes from TurbineHub</title>{entries}</feed>\n'.encode()


def ge_build(name):
    """A small made-up Proton build as a .tar.gz, the same bytes every time: one folder called `name` with the files
    Steam and Turbine Hub look for, a link inside it, and a file whose name is too long for a plain tar header."""
    tool = '"compatibilitytools"\n{\n  "compat_tools"\n  {\n    "%s" // Internal name of this tool\n    {\n      "install_path" "."\n      "display_name" "%s"\n      "from_oslist"  "windows"\n      "to_oslist"    "linux"\n    }\n  }\n}\n' % (name, name)
    files = [
        ("proton", b"#!/usr/bin/env python3\nprint('a stand-in, not a real Proton')\n", 0o755),
        ("toolmanifest.vdf", b'"manifest"\n{\n  "version" "2"\n  "commandline" "/proton %verb%"\n}\n', 0o644),
        ("compatibilitytool.vdf", tool.encode(), 0o644),
        ("version", ("1700000000 %s\n" % name).encode(), 0o644),
        ("files/lib/wine/x86_64-unix/ntdll.so", b"stand-in library\n" * 400, 0o755),
        ("files/share/" + "deep/" * 24 + "readme.txt", b"found by its long name\n", 0o644),
    ]
    buf = io.BytesIO()
    with gzip.GzipFile(fileobj=buf, mode="wb", mtime=0) as gz, tarfile.open(fileobj=gz, mode="w", format=tarfile.GNU_FORMAT) as tar:
        def add(path, kind, data=b"", mode=0o755, link=""):
            info = tarfile.TarInfo(name + "/" + path if path else name)
            info.type, info.size, info.mode, info.linkname = kind, len(data), mode, link
            info.mtime, info.uid, info.gid, info.uname, info.gname = 1700000000, 0, 0, "root", "root"
            tar.addfile(info, io.BytesIO(data))
        add("", tarfile.DIRTYPE)
        for path, data, mode in files:
            add(path, tarfile.REGTYPE, data, mode)
        add("files/lib64", tarfile.SYMTYPE, link="lib")
        add("files/share/default_pfx/drive_c/windows/system32/ntdll.dll", tarfile.SYMTYPE, link="../../../../../lib/wine/x86_64-unix/ntdll.so")
    return buf.getvalue()


# Each build by its name, with the release it belongs to: the newest release has one for each kind of processor.
GE_BUILDS = {name: (tag, ge_build(name)) for tag, name in [(GE_TAG, GE_TAG + "-x86_64"), (GE_TAG, GE_TAG + "-aarch64"), (GE_OLD, GE_OLD)]}


def ge_release():
    """GitHub's API answer for the newest made-up release, with the addresses its files have on GitHub."""
    assets = []
    for name in sorted(n for n, (tag, _) in GE_BUILDS.items() if tag == GE_TAG):
        base = f"https://github.com{GE}/releases/download/{GE_TAG}/{name}"
        assets += [{"name": name + ".sha512sum", "size": 130 + len(name) + 7, "browser_download_url": base + ".sha512sum"}, {"name": name + ".tar.gz", "size": len(GE_BUILDS[name][1]), "browser_download_url": base + ".tar.gz"}]
    return json.dumps({"tag_name": GE_TAG, "name": GE_TAG + " Released", "draft": False, "prerelease": False, "html_url": f"https://github.com{GE}/releases/tag/{GE_TAG}", "assets": assets}).encode()


def ge_file(path):
    """A build's archive or its checksum file, by the path GitHub gives them. None for anything else."""
    for name, (tag, build) in GE_BUILDS.items():
        if path == f"{GE}/releases/download/{tag}/{name}.tar.gz":
            return 200, "application/octet-stream", build, {}
        if path == f"{GE}/releases/download/{tag}/{name}.sha512sum":
            return 200, "text/plain", (hashlib.sha512(build).hexdigest() + f"  {name}.tar.gz\n").encode(), {}
    return None


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
    if path == "/repos" + GE + "/releases/latest":
        return 200, "application/json", ge_release(), {}
    if path == GE + "/releases/latest":
        return 302, "text/plain", b"", {"Location": GE + "/releases/tag/" + GE_TAG}
    if path == GE + "/releases/tag/" + GE_TAG:
        return 200, "text/html", b"<html><body>Release</body></html>", {}
    if ge_file(path):
        return ge_file(path)
    if path.startswith("/api/v1/reports/summaries/") and path.endswith(".json"):
        summary = json.loads(data("protondb.json")).get(path.rsplit("/", 1)[1][:-5])
        if summary:
            return 200, "application/json", json.dumps(summary).encode(), {}
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
    status, body, _ = get("/repos" + GE + "/releases/latest")
    release = json.loads(body) if status == 200 else {}
    mine = GE_TAG + "-x86_64"
    check("the newest GE-Proton", release.get("tag_name") == GE_TAG and [a["name"] for a in release["assets"]] == [GE_TAG + "-aarch64.sha512sum", GE_TAG + "-aarch64.tar.gz", mine + ".sha512sum", mine + ".tar.gz"])
    status, _, url = get(GE + "/releases/latest")
    check("the newest GE-Proton's page", status == 200 and url.endswith("/releases/tag/" + GE_TAG))
    for tag, name in [(GE_TAG, mine), (GE_TAG, GE_TAG + "-aarch64"), (GE_OLD, GE_OLD)]:
        _, build, _ = get(f"{GE}/releases/download/{tag}/{name}.tar.gz")
        status, sums, _ = get(f"{GE}/releases/download/{tag}/{name}.sha512sum")
        check(f"the checksum of {name}", status == 200 and sums.decode().split() == [hashlib.sha512(build).hexdigest(), name + ".tar.gz"])
        check(f"{name} is the same bytes each time", build == ge_build(name))
        with tarfile.open(fileobj=io.BytesIO(build), mode="r:gz") as tar:
            names = tar.getnames()
            check(f"the files of {name}", all(name + "/" + n in names for n in ["proton", "toolmanifest.vdf", "compatibilitytool.vdf", "files/lib64"]) and all(n == name or n.startswith(name + "/") for n in names))
    check("a release under the name it never had", get(f"{GE}/releases/download/{GE_TAG}/{GE_TAG}.tar.gz")[0] == 404 and get(f"{GE}/releases/download/{GE_OLD}/{GE_OLD}-x86_64.sha512sum")[0] == 404)
    status, body, _ = get("/api/v1/reports/summaries/1245620.json")
    check("what players say about a game", status == 200 and json.loads(body)["tier"] == "gold")
    check("a game without reports", get("/api/v1/reports/summaries/1.json")[0] == 404)
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
    # Port 0 lets the system pick a free one; the line below says which it is.
    server = serve(a.port, a.log)
    print(f"stand-in on http://127.0.0.1:{server.server_address[1]}", flush=True)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass
