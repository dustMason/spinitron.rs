#!/usr/bin/env python3
"""
Simple script to get a Spotify refresh token.
Run this once to get your refresh token, then use it in the Rust app.
"""

import base64
import json
import urllib.parse
import urllib.request
import webbrowser
from http.server import HTTPServer, BaseHTTPRequestHandler
import threading
import os
import time

# Configuration
CLIENT_ID = os.getenv("SPOTIFY_CLIENT_ID")
CLIENT_SECRET = os.getenv("SPOTIFY_CLIENT_SECRET")
REDIRECT_URI = "http://127.0.0.1:8888/callback"
SCOPE = "playlist-modify-public playlist-modify-private playlist-read-private"

# Global variable to store the authorization code
auth_code = None

class CallbackHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        global auth_code
        if self.path.startswith('/callback'):
            # Parse the authorization code from the callback
            query = urllib.parse.urlparse(self.path).query
            params = urllib.parse.parse_qs(query)
            
            if 'code' in params:
                auth_code = params['code'][0]
                self.send_response(200)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(b'<h1>Success!</h1><p>You can close this window and return to the terminal.</p>')
            else:
                self.send_response(400)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(b'<h1>Error!</h1><p>No authorization code received.</p>')
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, format, *args):
        # Suppress default logging
        pass

def get_refresh_token():
    # Step 1: Start local server for callback
    server = HTTPServer(('localhost', 8888), CallbackHandler)
    server_thread = threading.Thread(target=server.serve_forever)
    server_thread.daemon = True
    server_thread.start()
    
    # Step 2: Build authorization URL
    auth_params = {
        'client_id': CLIENT_ID,
        'response_type': 'code',
        'redirect_uri': REDIRECT_URI,
        'scope': SCOPE,
        'show_dialog': 'true'
    }
    
    auth_url = 'https://accounts.spotify.com/authorize?' + urllib.parse.urlencode(auth_params)
    
    print(f"\n1. Opening browser to authorize the application...")
    print(f"   If it doesn't open automatically, visit: {auth_url}")
    
    webbrowser.open(auth_url)
    
    # Step 3: Wait for callback
    print("2. Waiting for authorization callback...")
    timeout = 60  # 60 seconds timeout
    start_time = time.time()
    
    while auth_code is None and (time.time() - start_time) < timeout:
        time.sleep(0.1)
    
    server.shutdown()
    
    if auth_code is None:
        print("❌ Timeout waiting for authorization. Please try again.")
        return None
    
    print("3. Authorization code received! Exchanging for tokens...")
    
    # Step 4: Exchange authorization code for tokens
    token_data = {
        'grant_type': 'authorization_code',
        'code': auth_code,
        'redirect_uri': REDIRECT_URI
    }
    
    credentials = base64.b64encode(f"{CLIENT_ID}:{CLIENT_SECRET}".encode()).decode()
    
    req = urllib.request.Request(
        'https://accounts.spotify.com/api/token',
        data=urllib.parse.urlencode(token_data).encode(),
        headers={
            'Authorization': f'Basic {credentials}',
            'Content-Type': 'application/x-www-form-urlencoded'
        }
    )
    
    try:
        with urllib.request.urlopen(req) as response:
            token_response = json.loads(response.read().decode())
            
        if 'refresh_token' in token_response:
            print("✅ Success! Here are your tokens:")
            print(f"\nSPOTIFY_CLIENT_ID={CLIENT_ID}")
            print(f"SPOTIFY_CLIENT_SECRET={CLIENT_SECRET}")
            print(f"SPOTIFY_REFRESH_TOKEN={token_response['refresh_token']}")
            
            print(f"\n💡 Add these to your environment:")
            print(f"export SPOTIFY_CLIENT_ID='{CLIENT_ID}'")
            print(f"export SPOTIFY_CLIENT_SECRET='{CLIENT_SECRET}'")
            print(f"export SPOTIFY_REFRESH_TOKEN='{token_response['refresh_token']}'")
            
            return token_response['refresh_token']
        else:
            print("❌ Error: No refresh token in response")
            print(json.dumps(token_response, indent=2))
            return None
            
    except Exception as e:
        print(f"❌ Error getting tokens: {e}")
        return None

if __name__ == "__main__":
    print("🎵 Spotify Refresh Token Generator")
    print("==================================")
    print("\nThis script will help you get a refresh token for the Spinitron scraper.")
    print("Make sure you've set your Spotify app's redirect URI to: http://localhost:8888/callback\n")
    
    refresh_token = get_refresh_token()
    
    if refresh_token:
        print(f"\n🎉 Setup complete! You can now use the Spinitron scraper with --spotify")
    else:
        print(f"\n❌ Setup failed. Please check your Spotify app configuration and try again.")