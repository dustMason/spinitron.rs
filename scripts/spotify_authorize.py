"""Capture a Spotify refresh token through a validated loopback OAuth callback.

Credentials are returned to the caller in memory, never printed or written here.
"""
import base64
import hmac
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import secrets
import threading
import urllib.error
import urllib.parse
import urllib.request


REDIRECT_URI = "http://127.0.0.1:8888/callback"
SCOPES = "playlist-modify-public playlist-modify-private playlist-read-private"


def authorize(client_id, client_secret, timeout=300):
    if not client_id or not client_secret:
        raise ValueError("Spotify client ID and secret are required")
    state = secrets.token_urlsafe(32)
    completed = threading.Event()
    result = {}

    class Callback(BaseHTTPRequestHandler):
        def do_GET(self):
            parsed = urllib.parse.urlparse(self.path)
            if parsed.path != "/callback":
                self.send_error(404)
                return
            params = urllib.parse.parse_qs(parsed.query)
            received_state = params.get("state", [""])[0]
            if not hmac.compare_digest(received_state, state):
                result["error"] = "Spotify callback state did not match; authorization stopped"
                self.send_error(400, "Invalid authorization state")
            elif params.get("code"):
                result["code"] = params["code"][0]
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.end_headers()
                self.wfile.write(b"<!doctype html><meta charset='utf-8'><title>Spotify authorization received</title><script>history.replaceState(null, '', '/callback');</script><h1>Spotify authorization received</h1><p>You can return to Codex. Verification will continue automatically.</p>")
            else:
                result["error"] = "Spotify authorization was declined or returned no code"
                self.send_error(400, "Authorization was not completed")
            completed.set()

        def log_message(self, *_):
            pass

    with HTTPServer(("127.0.0.1", 8888), Callback) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = "https://accounts.spotify.com/authorize?" + urllib.parse.urlencode({
            "client_id": client_id, "response_type": "code", "redirect_uri": REDIRECT_URI,
            "scope": SCOPES, "state": state,
        })
        print("Open this Spotify authorization page (no values need copying):", flush=True)
        print(url, flush=True)
        try:
            if not completed.wait(timeout):
                raise RuntimeError("Spotify authorization timed out; start the command again")
        finally:
            server.shutdown()
            thread.join()
    if "error" in result:
        raise RuntimeError(result["error"])
    request = urllib.request.Request("https://accounts.spotify.com/api/token", data=urllib.parse.urlencode({
        "grant_type": "authorization_code", "code": result["code"], "redirect_uri": REDIRECT_URI,
    }).encode(), headers={
        "Authorization": "Basic " + base64.b64encode((client_id + ":" + client_secret).encode()).decode(),
    })
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"Spotify token exchange failed (HTTP {error.code})") from None
    if not payload.get("refresh_token"):
        raise RuntimeError("Spotify did not return a refresh token")
    print("Spotify token received securely; continuing verification.", flush=True)
    return payload["refresh_token"]
