#!/usr/bin/env python3
"""A stand-in for github.com's release pages, so install.sh can be tested
offline against real archives (docs/ops/release.md).

    python3 scripts/release/release_server.py DIST TAG [INSTALLER] [ART]

Serves DIST's files at /releases/download/TAG/<name>, redirects
/releases/latest to /releases/tag/TAG the way GitHub does (the installer and
the app's update check both read the tag off that redirect), answers
/releases/tag/TAG with a stub page, and — given INSTALLER, a path to
install.sh — serves it at /install.sh, where `zeroth update` fetches it
from a non-GitHub base (docs/ops/update.md). Given ART, a directory, its
files are served at /art/<name>: where the installer reads its banner
artwork from when ZEROTH_INSTALL_ART_BASE_URL points here, so a preview can
be tested without the network. Binds an ephemeral port on 127.0.0.1 and
prints it on the first line of stdout; the selftest and the smoke suite
point ZEROTH_INSTALL_BASE_URL / ZEROTH_UPDATE_URL at it, and
`scripts/release.sh download --from`.
"""
import http.server
import os
import socketserver
import sys

DIST, TAG = sys.argv[1], sys.argv[2]
INSTALLER = sys.argv[3] if len(sys.argv) > 3 else None
ART = sys.argv[4] if len(sys.argv) > 4 else None


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):  # quiet
        pass

    def _send(self, status, body=b"", ctype="application/octet-stream", head=False, location=None):
        self.send_response(status)
        if location:
            self.send_header("Location", location)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if not head:
            self.wfile.write(body)

    def _route(self, head):
        path = self.path.split("?", 1)[0]
        if path == "/releases/latest":
            return self._send(302, head=head, location=f"/releases/tag/{TAG}")
        if path == f"/releases/tag/{TAG}":
            return self._send(200, b"<html>release</html>", "text/html", head)
        if path == "/install.sh" and INSTALLER and os.path.isfile(INSTALLER):
            with open(INSTALLER, "rb") as fh:
                return self._send(200, fh.read(), "text/x-shellscript", head)
        if path.startswith("/art/") and ART:
            name = os.path.basename(path[len("/art/"):])
            full = os.path.join(ART, name)
            if os.path.isfile(full):
                with open(full, "rb") as fh:
                    return self._send(200, fh.read(), "image/png", head)
        prefix = f"/releases/download/{TAG}/"
        if path.startswith(prefix):
            name = os.path.basename(path[len(prefix):])
            full = os.path.join(DIST, name)
            if os.path.isfile(full):
                with open(full, "rb") as fh:
                    return self._send(200, fh.read(), head=head)
        return self._send(404, b"not found", "text/plain", head)

    def do_GET(self):
        self._route(False)

    def do_HEAD(self):
        self._route(True)


class Server(socketserver.TCPServer):
    allow_reuse_address = True


with Server(("127.0.0.1", 0), Handler) as server:
    print(server.server_address[1], flush=True)
    server.serve_forever()
