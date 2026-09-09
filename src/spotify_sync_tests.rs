use super::*;
use crate::models::{Show, ShowEpisode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct Exchange {
    method: &'static str,
    path: String,
    status: u16,
    reply: Value,
}

fn exchange(method: &'static str, path: &str, reply: Value) -> Exchange {
    Exchange {
        method,
        path: path.into(),
        status: 200,
        reply,
    }
}

fn track_page(uris: &[String], next: Option<String>) -> Value {
    serde_json::json!({"items":uris.iter().map(|uri| serde_json::json!({"track":{"uri":uri}})).collect::<Vec<_>>(),"next":next})
}

// Exercise the real HTTP update path on loopback with cached track matches.
// The server accepts only the expected requests; an extra write fails the test.
async fn mock_client(
    plan: impl FnOnce(&str) -> Vec<Exchange>,
) -> (SpotifyClient, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let exchanges = plan(&base);
    let server = tokio::spawn(async move {
        let mut bodies = Vec::new();
        for expected in exchanges {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                assert_ne!(stream.read_buf(&mut request).await.unwrap(), 0);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
            assert_eq!(
                headers.lines().next().unwrap(),
                format!("{} {} HTTP/1.1", expected.method, expected.path)
            );
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while request.len() < header_end + length {
                assert_ne!(stream.read_buf(&mut request).await.unwrap(), 0);
            }
            bodies.push(if length == 0 {
                Value::Null
            } else {
                serde_json::from_slice(&request[header_end..header_end + length]).unwrap()
            });
            let body = expected.reply.to_string();
            let response = format!("HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", expected.status, body.len(), body);
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        bodies
    });
    let mut client = super::tests::offline_client();
    client.client = Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();
    client.api_base_url = base;
    (client, server)
}

async fn requests(server: tokio::task::JoinHandle<Vec<Value>>) -> Vec<Value> {
    tokio::time::timeout(std::time::Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap()
}

fn seed_show(client: &mut SpotifyClient, uris: &[String], existing: bool) -> ShowGroup {
    let tracks: Vec<_> = uris
        .iter()
        .enumerate()
        .map(|(i, uri)| {
            let track = Track {
                artist: "Artist".into(),
                song: format!("Song {i}"),
                album: String::new(),
                label: None,
                time: None,
            };
            client.track_cache.entries.insert(
                track.cache_key(),
                CachedTrackEntry {
                    track: (!uri.is_empty()).then(|| SpotifyTrack {
                        id: uri.clone(),
                        uri: uri.clone(),
                        name: track.song.clone(),
                        artists: vec![],
                    }),
                    expires_at: u64::MAX,
                },
            );
            track
        })
        .collect();
    let show = ShowGroup {
        station: "KALX".into(),
        show_name: "Test".into(),
        episodes: vec![ShowEpisode {
            show: Show {
                id: 2,
                title: "Test".into(),
                url: String::new(),
                start_time: String::new(),
                end_time: String::new(),
            },
            tracks,
        }],
    };
    if existing {
        client.playlist_cache.playlists.insert(
            "1".into(),
            SpotifyPlaylist {
                id: "existing".into(),
                name: show.playlist_name(),
                description: Some("Last updated: 2020-01-01 00:00 UTC".into()),
                uri: "spotify:playlist:existing".into(),
                external_url: None,
                track_count: 42,
            },
        );
    }
    show
}

fn uris(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .map(|name| format!("spotify:track:{name}"))
        .collect()
}

#[tokio::test]
async fn identical_paginated_contents_do_not_write_or_change_the_date() {
    let contents = uris(&["a", "b", "a"]);
    let (mut client, server) = mock_client(|base| {
        vec![
            exchange(
                "GET",
                "/v1/playlists/existing/tracks?limit=100",
                track_page(&contents[..2], Some(format!("{base}/page2"))),
            ),
            exchange("GET", "/v1/page2", track_page(&contents[2..], None)),
        ]
    })
    .await;
    let show = seed_show(&mut client, &contents, true);
    let before = serde_json::to_value(&client.playlist_cache).unwrap();
    let result = client
        .create_or_update_show_playlist(&show)
        .await
        .unwrap()
        .unwrap();
    let PlaylistUpdate::Unchanged(playlist) = result else {
        panic!("Expected unchanged contents")
    };
    assert_eq!(
        playlist.description.as_deref(),
        Some("Last updated: 2020-01-01 00:00 UTC")
    );
    assert_eq!(
        serde_json::to_value(&client.playlist_cache).unwrap(),
        before
    );
    assert_eq!(requests(server).await, vec![Value::Null, Value::Null]);
}

#[tokio::test]
async fn additions_removals_reordering_and_duplicate_counts_trigger_updates() {
    for (current, desired) in [
        (uris(&["a"]), uris(&["a", "b"])),
        (uris(&["a", "b"]), uris(&["a"])),
        (uris(&["a", "b"]), uris(&["b", "a"])),
        (uris(&["a", "b", "a"]), uris(&["a", "b"])),
        (vec![], uris(&["a"])),
    ] {
        let (mut client, server) = mock_client(|_| {
            let mut plan = vec![exchange(
                "GET",
                "/v1/playlists/existing/tracks?limit=100",
                track_page(&current, None),
            )];
            if !current.is_empty() {
                plan.push(exchange(
                    "DELETE",
                    "/v1/playlists/existing/tracks",
                    serde_json::json!({}),
                ));
            }
            plan.push(exchange(
                "POST",
                "/v1/playlists/existing/tracks",
                serde_json::json!({}),
            ));
            plan.push(exchange(
                "PUT",
                "/v1/playlists/existing",
                serde_json::json!({}),
            ));
            plan
        })
        .await;
        let show = seed_show(&mut client, &desired, true);
        let result = client
            .create_or_update_show_playlist(&show)
            .await
            .unwrap()
            .unwrap();
        let PlaylistUpdate::Updated(playlist) = result else {
            panic!("Expected changed contents")
        };
        let bodies = requests(server).await;
        assert_eq!(
            bodies[bodies.len() - 2],
            serde_json::json!({"uris":desired})
        );
        assert_eq!(
            bodies.last().unwrap()["description"],
            playlist.description.clone().unwrap()
        );
        assert!(playlist
            .description
            .as_ref()
            .unwrap()
            .contains("Latest ID: 2 Last updated:"));
        assert!(!playlist
            .description
            .as_ref()
            .unwrap()
            .contains("2020-01-01"));
        assert_eq!(playlist.track_count, desired.len() as u32);
        assert_eq!(
            client.playlist_cache.playlists["2"].description,
            playlist.description
        );
    }
}

#[tokio::test]
async fn compares_tracks_beyond_the_first_hundred_and_writes_in_batches() {
    let current: Vec<_> = (0..101).map(|i| format!("spotify:track:{i}")).collect();
    let mut desired = current.clone();
    desired[100] = "spotify:track:new".into();
    let (mut client, server) = mock_client(|base| {
        vec![
            exchange(
                "GET",
                "/v1/playlists/existing/tracks?limit=100",
                track_page(&current[..100], Some(format!("{base}/page2"))),
            ),
            exchange("GET", "/v1/page2", track_page(&current[100..], None)),
            exchange(
                "DELETE",
                "/v1/playlists/existing/tracks",
                serde_json::json!({}),
            ),
            exchange(
                "DELETE",
                "/v1/playlists/existing/tracks",
                serde_json::json!({}),
            ),
            exchange(
                "POST",
                "/v1/playlists/existing/tracks",
                serde_json::json!({}),
            ),
            exchange(
                "POST",
                "/v1/playlists/existing/tracks",
                serde_json::json!({}),
            ),
            exchange("PUT", "/v1/playlists/existing", serde_json::json!({})),
        ]
    })
    .await;
    let show = seed_show(&mut client, &desired, true);
    assert!(matches!(
        client.create_or_update_show_playlist(&show).await.unwrap(),
        Some(PlaylistUpdate::Updated(_))
    ));
    let bodies = requests(server).await;
    assert_eq!(bodies[4], serde_json::json!({"uris":desired[..100]}));
    assert_eq!(bodies[5], serde_json::json!({"uris":desired[100..]}));
}

#[tokio::test]
async fn incomplete_or_failed_playlist_reads_never_write() {
    for (status, reply) in [
        (500, serde_json::json!({"error":"unavailable"})),
        (200, serde_json::json!({"next":null})),
        (
            200,
            serde_json::json!({"items":[{"track":null}],"next":null}),
        ),
        (200, serde_json::json!({"items":[]})),
    ] {
        let (mut client, server) = mock_client(|base| {
            vec![
                exchange(
                    "GET",
                    "/v1/playlists/existing/tracks?limit=100",
                    track_page(&uris(&["a"]), Some(format!("{base}/page2"))),
                ),
                Exchange {
                    method: "GET",
                    path: "/v1/page2".into(),
                    status,
                    reply,
                },
            ]
        })
        .await;
        let show = seed_show(&mut client, &uris(&["a", "b"]), true);
        let before = serde_json::to_value(&client.playlist_cache).unwrap();
        assert!(client.create_or_update_show_playlist(&show).await.is_err());
        assert_eq!(
            serde_json::to_value(&client.playlist_cache).unwrap(),
            before
        );
        assert_eq!(requests(server).await, vec![Value::Null, Value::Null]);
    }
}

#[tokio::test]
async fn zero_matches_never_clear_or_create_a_playlist() {
    for existing in [true, false] {
        let mut client = super::tests::offline_client();
        let show = seed_show(&mut client, &[String::new()], existing);
        let before = serde_json::to_value(&client.playlist_cache).unwrap();
        let error = client
            .create_or_update_show_playlist(&show)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("No Spotify tracks matched"));
        assert_eq!(
            serde_json::to_value(&client.playlist_cache).unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn new_playlists_still_get_created_with_the_matched_track_count() {
    let contents = uris(&["a", "b"]);
    let (mut client, server) = mock_client(|_| {
        vec![
            exchange(
                "POST",
                "/v1/users/test-user/playlists",
                serde_json::json!({"id":"new","name":"KALX - Test"}),
            ),
            exchange("POST", "/v1/playlists/new/tracks", serde_json::json!({})),
        ]
    })
    .await;
    let mut scraped = contents.clone();
    scraped.push(String::new()); // An unmatched source song does not inflate the count.
    let show = seed_show(&mut client, &scraped, false);
    let result = client
        .create_or_update_show_playlist(&show)
        .await
        .unwrap()
        .unwrap();
    let PlaylistUpdate::Created(playlist) = result else {
        panic!("Expected a new playlist")
    };
    assert_eq!(playlist.track_count, 2);
    let bodies = requests(server).await;
    assert_eq!(bodies[0]["name"], "KALX - Test");
    assert_eq!(bodies[1], serde_json::json!({"uris":contents}));
}
