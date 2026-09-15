#!/usr/bin/env python3
"""Static file server for developing Bims.

Three things the stock `python3 -m http.server` gets wrong here:

* It sends no cache headers. Files served out of the Nix store carry an mtime
  of 1970, so a browser's heuristic freshness check — roughly a tenth of the
  file's apparent age — concludes the page is good for decades and stops
  asking for it. You rebuild, reload, and are still looking at the old game.
* It gives up if the port is taken, which it often is after an earlier run.
* `.wasm` is missing from the MIME table on some platforms. The page decodes
  the module from an ArrayBuffer so it does not depend on this, but serving
  the right type keeps devtools honest.
"""

import argparse
import errno
import functools
import hashlib
import http.server
import os
import threading
import urllib.error
import urllib.request
import webbrowser

DEFAULT_PORT = 8080
# How many ports past the requested one to try before giving up.
PORT_SEARCH = 20


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
    }

    def end_headers(self):
        self.send_header("Cache-Control", "no-store, must-revalidate")
        super().end_headers()


def build_id(data):
    return hashlib.sha256(data).hexdigest()[:12]


def build_on_disk(directory):
    with open(os.path.join(directory, "bims.wasm"), "rb") as f:
        return build_id(f.read())


def build_being_served(port):
    """Which build of Bims is already on `port`, or None if what is listening
    there is not Bims at all. Identified by the served wasm, so an unrelated
    server on the same port cannot be mistaken for one of ours."""
    try:
        with urllib.request.urlopen(
            f"http://localhost:{port}/bims.wasm", timeout=2
        ) as response:
            data = response.read()
    except (urllib.error.URLError, OSError, ValueError):
        return None
    return build_id(data) if data.startswith(b"\0asm") else None


def open_browser(url, background=True):
    # Deliberately the plain URL, with no cache-busting query: a unique URL each
    # run means the browser can never reuse the tab, so every run leaves another
    # copy of the game running in an old tab. Freshness is the server's job
    # (no-store) and the page's (it versions its own script and wasm).
    #
    # `webbrowser.open` can block for a second or two, so the serving path hands
    # it to a thread. A caller that is about to exit must not: the thread is a
    # daemon and would be killed before it ever launched anything.
    if background:
        threading.Thread(target=webbrowser.open, args=(url,), daemon=True).start()
    else:
        webbrowser.open(url)


def bind(port, search, handler):
    """Bind `port`, walking up to the next free one when `search` is set."""
    last = port + PORT_SEARCH if search else port
    for candidate in range(port, last + 1):
        try:
            return http.server.ThreadingHTTPServer(("", candidate), handler)
        except OSError as exc:
            if exc.errno != errno.EADDRINUSE or candidate == last:
                raise
    raise AssertionError("unreachable")


def main():
    parser = argparse.ArgumentParser(
        # Without this, --help shows the Nix store path the script was run from.
        prog="bims-serve",
        description="Serve Bims for local development.",
    )
    parser.add_argument(
        "port",
        nargs="?",
        type=int,
        help=f"port to serve on (default {DEFAULT_PORT})",
    )
    parser.add_argument("--directory", default="web")
    parser.add_argument(
        "--open",
        action=argparse.BooleanOptionalAction,
        default=None,
        help="open the page in a browser (default: only when starting a server)",
    )
    args = parser.parse_args()

    port = args.port or DEFAULT_PORT
    mine = build_on_disk(args.directory)

    # Running the command twice should take you to the game, not start a second
    # copy of it on another port and open a second browser tab.
    running = build_being_served(port)
    if running == mine:
        url = f"http://localhost:{port}/"
        print(f"Bims is already running at {url}", flush=True)
        # Not opening a tab here on purpose: you already have one. Opening on
        # every invocation is what left several copies of the game running.
        if args.open is True:
            open_browser(url, background=False)
        return 0
    if running is not None:
        print(
            f"A different build of Bims is already on port {port} "
            f"(serving {running}, this one is {mine}).\n"
            f"Stop it with Ctrl+C in its terminal and run again, "
            f"or pass a port to run alongside it.",
            flush=True,
        )
        return 1

    # Nothing of ours there. If the port is taken by something unrelated and the
    # caller did not insist on it, move along rather than failing.
    handler = functools.partial(Handler, directory=args.directory)
    httpd = bind(port, search=args.port is None, handler=handler)
    url = f"http://localhost:{httpd.server_address[1]}/"

    # flush: stdout is block-buffered when piped, and the URL is the one thing
    # you want to see immediately.
    print(f"Bims is at {url}", flush=True)
    print("Press Ctrl+C to stop.", flush=True)

    if args.open is not False:
        open_browser(url)

    with httpd:
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
