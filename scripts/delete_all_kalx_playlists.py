#!/usr/bin/env python3
"""
Quick script to delete all existing KALX playlists from Spotify.
"""

import os
import requests
import base64
import json
import sys

def get_access_token():
    """Get access token using refresh token"""
    client_id = os.environ.get('SPOTIFY_CLIENT_ID')
    client_secret = os.environ.get('SPOTIFY_CLIENT_SECRET') 
    refresh_token = os.environ.get('SPOTIFY_REFRESH_TOKEN')
    
    if not all([client_id, client_secret, refresh_token]):
        print("❌ Missing environment variables:")
        print("   SPOTIFY_CLIENT_ID, SPOTIFY_CLIENT_SECRET, SPOTIFY_REFRESH_TOKEN")
        sys.exit(1)
    
    # Encode credentials
    credentials = base64.b64encode(f"{client_id}:{client_secret}".encode()).decode()
    
    # Request access token
    response = requests.post(
        'https://accounts.spotify.com/api/token',
        headers={'Authorization': f'Basic {credentials}'},
        data={
            'grant_type': 'refresh_token',
            'refresh_token': refresh_token
        }
    )
    
    if response.status_code != 200:
        print(f"❌ Failed to get access token: {response.text}")
        sys.exit(1)
    
    return response.json()['access_token']

def get_all_playlists(access_token):
    """Get all user's playlists"""
    headers = {'Authorization': f'Bearer {access_token}'}
    playlists = []
    url = 'https://api.spotify.com/v1/me/playlists?limit=50'
    
    while url:
        response = requests.get(url, headers=headers)
        if response.status_code != 200:
            print(f"❌ Failed to get playlists: {response.text}")
            return []
        
        data = response.json()
        playlists.extend(data['items'])
        url = data.get('next')
    
    return playlists

def delete_playlist(playlist_id, access_token):
    """Delete a playlist by unfollowing it"""
    headers = {'Authorization': f'Bearer {access_token}'}
    
    response = requests.delete(
        f'https://api.spotify.com/v1/playlists/{playlist_id}/followers',
        headers=headers
    )
    
    return response.status_code == 200

def main():
    print("🎵 KALX Playlist Cleanup Tool")
    print("=" * 40)
    
    # Get access token
    print("Getting access token...")
    access_token = get_access_token()
    print("✅ Got access token")
    
    # Get all playlists
    print("Fetching all playlists...")
    all_playlists = get_all_playlists(access_token)
    print(f"✅ Found {len(all_playlists)} total playlists")
    
    # Filter KALX playlists
    kalx_playlists = []
    for playlist in all_playlists:
        name = playlist.get('name', '')
        description = playlist.get('description', '')
        
        # Look for KALX playlists or Spinitron-generated playlists
        is_kalx = (
            name.startswith('KALX -') or 
            'Generated from Spinitron' in description or
            'Spinítron ID:' in description or
            'Latest ID:' in description
        )
        
        if is_kalx:
            kalx_playlists.append({
                'id': playlist['id'],
                'name': name,
                'description': description
            })
    
    if not kalx_playlists:
        print("✅ No KALX playlists found to delete")
        return
    
    print(f"\n🎯 Found {len(kalx_playlists)} KALX playlists:")
    for i, playlist in enumerate(kalx_playlists, 1):
        print(f"   {i:2d}. {playlist['name']}")
    
    # Confirm deletion
    print(f"\n⚠️  This will DELETE all {len(kalx_playlists)} KALX playlists!")
    confirmation = input("Type 'DELETE' to confirm: ")
    
    if confirmation != 'DELETE':
        print("❌ Cancelled - no playlists deleted")
        return
    
    # Delete playlists
    print(f"\n🗑️  Deleting {len(kalx_playlists)} playlists...")
    deleted_count = 0
    failed_count = 0
    
    for playlist in kalx_playlists:
        print(f"   Deleting: {playlist['name'][:50]}...")
        
        if delete_playlist(playlist['id'], access_token):
            deleted_count += 1
            print("     ✅ Deleted")
        else:
            failed_count += 1
            print("     ❌ Failed")
    
    print(f"\n📊 Results:")
    print(f"   ✅ Deleted: {deleted_count}")
    print(f"   ❌ Failed:  {failed_count}")
    
    if deleted_count > 0:
        print(f"\n🎉 Successfully cleaned up {deleted_count} KALX playlists!")

if __name__ == '__main__':
    main()