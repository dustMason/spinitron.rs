#!/usr/bin/env python3
"""
Delete duplicate KALX playlists from Spotify.
Keeps the most recent playlist for each show name.
"""

import os
import sys
import requests
import json
from collections import defaultdict
from datetime import datetime

def get_access_token():
    """Get Spotify access token using refresh token."""
    client_id = os.environ.get('SPOTIFY_CLIENT_ID')
    client_secret = os.environ.get('SPOTIFY_CLIENT_SECRET')
    refresh_token = os.environ.get('SPOTIFY_REFRESH_TOKEN')
    
    if not all([client_id, client_secret, refresh_token]):
        print("❌ Missing environment variables:")
        print("   SPOTIFY_CLIENT_ID, SPOTIFY_CLIENT_SECRET, SPOTIFY_REFRESH_TOKEN")
        sys.exit(1)
    
    # Get access token
    import base64
    auth_header = base64.b64encode(f"{client_id}:{client_secret}".encode()).decode()
    
    response = requests.post('https://accounts.spotify.com/api/token', 
        headers={'Authorization': f'Basic {auth_header}'},
        data={
            'grant_type': 'refresh_token',
            'refresh_token': refresh_token
        }
    )
    
    if response.status_code != 200:
        print(f"❌ Failed to get access token: {response.text}")
        sys.exit(1)
        
    return response.json()['access_token']

def get_user_playlists(access_token):
    """Get all user playlists."""
    headers = {'Authorization': f'Bearer {access_token}'}
    playlists = []
    url = 'https://api.spotify.com/v1/me/playlists?limit=50'
    
    while url:
        response = requests.get(url, headers=headers)
        if response.status_code != 200:
            print(f"❌ Failed to get playlists: {response.text}")
            sys.exit(1)
            
        data = response.json()
        playlists.extend(data['items'])
        url = data.get('next')
    
    return playlists

def delete_playlist(access_token, playlist_id):
    """Delete a playlist (actually unfollows it)."""
    headers = {'Authorization': f'Bearer {access_token}'}
    response = requests.delete(f'https://api.spotify.com/v1/playlists/{playlist_id}/followers', 
                              headers=headers)
    return response.status_code == 200

def main():
    print("🎵 KALX Duplicate Playlist Cleaner")
    print("=" * 40)
    
    # Get access token
    print("🔐 Getting Spotify access token...")
    access_token = get_access_token()
    
    # Get all playlists
    print("📋 Fetching user playlists...")
    all_playlists = get_user_playlists(access_token)
    
    # Find KALX playlists
    kalx_playlists = [p for p in all_playlists if p['name'].startswith('KALX -')]
    print(f"🎧 Found {len(kalx_playlists)} KALX playlists")
    
    if not kalx_playlists:
        print("✅ No KALX playlists found!")
        return
    
    # Group by name to find duplicates
    playlist_groups = defaultdict(list)
    for playlist in kalx_playlists:
        playlist_groups[playlist['name']].append(playlist)
    
    # Find duplicates
    duplicates_found = 0
    to_delete = []
    
    for name, playlists in playlist_groups.items():
        if len(playlists) > 1:
            duplicates_found += len(playlists) - 1
            # Sort by creation date (most recent first)
            playlists.sort(key=lambda p: p.get('snapshot_id', ''), reverse=True)
            
            print(f"\n📂 '{name}' has {len(playlists)} duplicates:")
            for i, playlist in enumerate(playlists):
                track_count = playlist['tracks']['total']
                status = "🟢 KEEP (most recent)" if i == 0 else "🔴 DELETE"
                print(f"   {status} - {track_count} tracks - ID: {playlist['id']}")
                
                if i > 0:  # Delete all but the first (most recent)
                    to_delete.append(playlist)
    
    if not to_delete:
        print("✅ No duplicates found! All playlists are unique.")
        return
    
    print(f"\n⚠️  Found {duplicates_found} duplicate playlists to delete")
    
    # Confirm deletion
    confirm = input("\nDo you want to delete the duplicate playlists? (yes/no): ").strip().lower()
    if confirm not in ['yes', 'y']:
        print("❌ Deletion cancelled.")
        return
    
    # Delete duplicates
    print(f"\n🗑️  Deleting {len(to_delete)} duplicate playlists...")
    deleted_count = 0
    
    for playlist in to_delete:
        if delete_playlist(access_token, playlist['id']):
            print(f"   ✅ Deleted: {playlist['name']} ({playlist['tracks']['total']} tracks)")
            deleted_count += 1
        else:
            print(f"   ❌ Failed to delete: {playlist['name']}")
    
    print(f"\n✅ Successfully deleted {deleted_count} duplicate playlists!")
    print(f"🎵 {len(playlist_groups)} unique KALX playlists remain")

if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        print("\n❌ Cancelled by user")
    except Exception as e:
        print(f"❌ Error: {e}")
        sys.exit(1)