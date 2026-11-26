//! Spotify links: catalog metadata from Spotify, audio from a matching YouTube upload.
//!
//! Spotify's own audio is DRM-protected and is never downloaded. DarkLoad reads public track
//! metadata (title, artists, album, duration, cover art), searches YouTube for the same
//! recording, downloads that audio with yt-dlp and writes Spotify's metadata into the file.
//!
//! Metadata comes from Spotify's public embed player by default, which needs no account but
//! lists at most 100 songs of an album or playlist. When the user adds their own Spotify Web API
//! credentials, the official API is used instead and long playlists are read in full.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::extractor::{self, Analysis, MediaInfo, NetOptions, ToolPaths};
use crate::platform::{self, DetectedUrl, Platform, UrlKind};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36";
const EMBED_LIMIT: usize = 100;
const MAX_TRACKS: usize = 5000;
const MAX_TEXT: usize = 512;
/// Lowest match score accepted as the same recording.
const MIN_SCORE: f64 = 55.0;

/// Song metadata from Spotify, plus the YouTube upload chosen as its audio source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpotifyTrack {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    pub release_date: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
    pub cover: Option<String>,
    pub isrc: Option<String>,
    pub explicit: bool,
    pub playable: bool,
    /// YouTube video used as the audio source, once matched.
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub source_channel: Option<String>,
}

impl SpotifyTrack {
    pub fn url(&self) -> String {
        platform::spotify_url(UrlKind::Track, &self.id)
    }

    pub fn artist_line(&self) -> String {
        self.artists.join(", ")
    }

    /// Checks metadata that came back from the interface before it is used for a download.
    pub fn validate(&mut self) -> AppResult<()> {
        if !platform::is_spotify_id(&self.id) {
            return Err(AppError::invalid_input("Invalid Spotify track."));
        }
        let text_ok = |s: &str| s.len() <= MAX_TEXT && !s.chars().any(char::is_control);
        let fields_ok = text_ok(&self.title)
            && self.artists.len() <= 50
            && self.artists.iter().all(|a| text_ok(a))
            && [
                &self.album,
                &self.album_artist,
                &self.release_date,
                &self.isrc,
                &self.source_title,
                &self.source_channel,
            ]
            .into_iter()
            .flatten()
            .all(|s| text_ok(s));
        if !fields_ok || self.title.trim().is_empty() {
            return Err(AppError::invalid_input("Invalid Spotify track metadata."));
        }
        if self.cover.as_deref().is_some_and(|c| !is_cover_url(c)) {
            self.cover = None;
        }
        if let Some(source) = &self.source_url {
            match platform::detect(source) {
                Ok(d) if d.platform == Platform::Youtube && d.kind == UrlKind::Video => {
                    self.source_url = Some(d.url);
                }
                _ => {
                    self.source_url = None;
                    self.source_title = None;
                    self.source_channel = None;
                }
            }
        }
        Ok(())
    }

    /// Fills fields missing here from another description of the same song.
    fn merge_missing(&mut self, other: SpotifyTrack) {
        if self.artists.is_empty() {
            self.artists = other.artists;
        }
        self.album = self.album.take().or(other.album);
        self.album_artist = self.album_artist.take().or(other.album_artist);
        self.track_number = self.track_number.or(other.track_number);
        self.release_date = self.release_date.take().or(other.release_date);
        self.duration = self.duration.or(other.duration);
        self.cover = self.cover.take().or(other.cover);
        self.isrc = self.isrc.take().or(other.isrc);
    }
}

fn is_cover_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str().is_some_and(|h| {
            h == "i.scdn.co"
                || h == "mosaic.scdn.co"
                || h == "image-cdn-ak.spotifycdn.com"
                || h == "image-cdn-fa.spotifycdn.com"
                || h.ends_with(".spotifycdn.com")
        })
}

/// An album or playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    pub kind: UrlKind,
    pub id: String,
    pub title: String,
    pub owner: Option<String>,
    pub cover: Option<String>,
    pub tracks: Vec<SpotifyTrack>,
    pub notice: Option<String>,
}

/// User-supplied Spotify Web API application credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

impl Credentials {
    pub fn from_settings(s: &crate::settings::Settings) -> Option<Self> {
        let client_id = s.spotify_client_id.trim();
        let client_secret = s.spotify_client_secret.trim();
        (!client_id.is_empty() && !client_secret.is_empty()).then(|| Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        })
    }
}

struct Token {
    value: String,
    expires: Instant,
    client_id: String,
}

pub struct SpotifyClient {
    http: reqwest::Client,
    token: tokio::sync::Mutex<Option<Token>>,
    /// Songs already matched on YouTube, keyed by Spotify track ID.
    matches: Mutex<HashMap<String, (Instant, SpotifyTrack)>>,
}

impl SpotifyClient {
    const MATCH_TTL: Duration = Duration::from_secs(12 * 3600);

    pub fn new() -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self {
            http,
            token: tokio::sync::Mutex::new(None),
            matches: Mutex::new(HashMap::new()),
        })
    }

    fn remember(&self, track: &SpotifyTrack) {
        if track.source_url.is_none() {
            return;
        }
        let mut map = self.matches.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 2000 {
            map.retain(|_, (t, _)| t.elapsed() < Self::MATCH_TTL);
        }
        map.insert(track.id.clone(), (Instant::now(), track.clone()));
    }

    fn remembered(&self, id: &str) -> Option<SpotifyTrack> {
        let map = self.matches.lock().unwrap_or_else(|e| e.into_inner());
        map.get(id)
            .filter(|(t, _)| t.elapsed() < Self::MATCH_TTL)
            .map(|(_, track)| track.clone())
    }

    /// Follows a `spotify.link` short link to the song, album or playlist it points to.
    pub async fn resolve_shortlink(&self, detected: &DetectedUrl) -> AppResult<DetectedUrl> {
        if detected.kind != UrlKind::Shortlink {
            return Ok(detected.clone());
        }
        let mut current = detected.url.clone();
        for _ in 0..4 {
            let response = self.http.get(&current).send().await?;
            let Some(location) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            else {
                break;
            };
            let next = url::Url::parse(&current)
                .and_then(|base| base.join(location))
                .map_err(|_| short_link_error())?;
            match platform::detect(next.as_str()) {
                Ok(d) if d.platform == Platform::Spotify && d.kind != UrlKind::Shortlink => {
                    return Ok(d)
                }
                Ok(d) if d.platform == Platform::Spotify => current = d.url,
                _ => return Err(short_link_error()),
            }
        }
        Err(short_link_error())
    }

    /// Reads one song's metadata.
    pub async fn track(&self, id: &str, creds: Option<&Credentials>) -> AppResult<SpotifyTrack> {
        if let Some(creds) = creds {
            match self.api_get(creds, &format!("tracks/{id}")).await {
                Ok(v) => {
                    if let Some(track) = parse_api_track(&v, None) {
                        return Ok(track);
                    }
                }
                Err(e) if e.code == ErrorCode::LoginRequired => return Err(e),
                Err(e) => tracing::warn!("Spotify API track lookup failed: {}", e.message),
            }
        }
        let entity = self.embed_entity("track", id).await?;
        parse_embed_track(&entity).ok_or_else(unreadable)
    }

    /// Reads an album or playlist and its songs.
    pub async fn collection(
        &self,
        detected: &DetectedUrl,
        creds: Option<&Credentials>,
    ) -> AppResult<Collection> {
        let id = detected.id.clone().unwrap_or_default();
        let mut api_notice = None;
        if let Some(creds) = creds {
            match self.api_collection(creds, detected.kind, &id).await {
                Ok(collection) => return Ok(collection),
                Err(e) if e.code == ErrorCode::LoginRequired => return Err(e),
                Err(e) => {
                    tracing::warn!("Spotify API collection lookup failed: {}", e.message);
                    api_notice = Some(
                        "Spotify's API did not return this list (Spotify does not share its own editorial playlists with third-party apps), so it was read from the public player."
                            .to_string(),
                    );
                }
            }
        }
        let path = if detected.kind == UrlKind::Album {
            "album"
        } else {
            "playlist"
        };
        let entity = self.embed_entity(path, &id).await?;
        let mut collection =
            parse_embed_collection(&entity, detected.kind).ok_or_else(unreadable)?;
        if collection.tracks.len() >= EMBED_LIMIT {
            let limit = format!(
                "Spotify's public player lists only the first {EMBED_LIMIT} songs. Add your own Spotify API credentials in Settings → Downloads to read the whole list."
            );
            collection.notice = Some(match (api_notice, creds) {
                (Some(api), _) => format!("{api} {limit}"),
                (None, _) => limit,
            });
        } else {
            collection.notice = api_notice;
        }
        Ok(collection)
    }

    async fn embed_entity(&self, kind: &str, id: &str) -> AppResult<Value> {
        let url = format!("https://open.spotify.com/embed/{kind}/{id}");
        let response = self
            .http
            .get(&url)
            .header(reqwest::header::ACCEPT_LANGUAGE, "en")
            .send()
            .await?;
        match response.status().as_u16() {
            200 => {}
            404 | 400 => {
                return Err(AppError::new(
                    ErrorCode::Unavailable,
                    "Spotify could not find this song, album or playlist. It may be private or removed.",
                ))
            }
            429 => {
                return Err(AppError::new(
                    ErrorCode::RateLimited,
                    "Spotify is limiting requests. Wait a moment and try again.",
                ))
            }
            code => {
                return Err(AppError::new(
                    ErrorCode::Network,
                    format!("Spotify answered with an unexpected status ({code})."),
                ))
            }
        }
        let html = response.text().await?;
        let data = next_data(&html).ok_or_else(unreadable)?;
        data.pointer("/props/pageProps/state/data/entity")
            .filter(|e| e.is_object())
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Unavailable,
                    "Spotify did not return this song, album or playlist. It may be private or unavailable in your region.",
                )
            })
    }

    async fn access_token(&self, creds: &Credentials) -> AppResult<String> {
        let mut guard = self.token.lock().await;
        if let Some(token) = guard.as_ref() {
            if token.client_id == creds.client_id && token.expires > Instant::now() {
                return Ok(token.value.clone());
            }
        }
        let response = self
            .http
            .post("https://accounts.spotify.com/api/token")
            .basic_auth(&creds.client_id, Some(&creds.client_secret))
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body("grant_type=client_credentials")
            .send()
            .await?;
        if matches!(response.status().as_u16(), 400 | 401 | 403) {
            return Err(AppError::new(
                ErrorCode::LoginRequired,
                "Spotify rejected the API Client ID or Client Secret in Settings → Downloads.",
            ));
        }
        if !response.status().is_success() {
            return Err(AppError::new(
                ErrorCode::Network,
                "Could not connect to the Spotify API.",
            ));
        }
        let v: Value = response.json().await?;
        let value = v
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(unreadable)?
            .to_string();
        let lifetime = v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
        *guard = Some(Token {
            value: value.clone(),
            expires: Instant::now() + Duration::from_secs(lifetime.saturating_sub(60)),
            client_id: creds.client_id.clone(),
        });
        Ok(value)
    }

    async fn api_get(&self, creds: &Credentials, path: &str) -> AppResult<Value> {
        self.api_url(creds, &format!("https://api.spotify.com/v1/{path}"))
            .await
    }

    async fn api_url(&self, creds: &Credentials, url: &str) -> AppResult<Value> {
        // Pagination links come from the API response; only follow them to the API itself.
        if !url.starts_with("https://api.spotify.com/v1/") {
            return Err(unreadable());
        }
        let token = self.access_token(creds).await?;
        let response = self.http.get(url).bearer_auth(token).send().await?;
        match response.status().as_u16() {
            200 => Ok(response.json().await?),
            401 => {
                *self.token.lock().await = None;
                Err(AppError::new(
                    ErrorCode::Network,
                    "The Spotify API session expired.",
                ))
            }
            429 => Err(AppError::new(
                ErrorCode::RateLimited,
                "Spotify is limiting requests. Wait a moment and try again.",
            )),
            code => Err(AppError::new(
                ErrorCode::Unavailable,
                format!("The Spotify API answered with status {code}."),
            )),
        }
    }

    async fn api_collection(
        &self,
        creds: &Credentials,
        kind: UrlKind,
        id: &str,
    ) -> AppResult<Collection> {
        let is_album = kind == UrlKind::Album;
        let root = self
            .api_get(
                creds,
                &format!("{}/{id}", if is_album { "albums" } else { "playlists" }),
            )
            .await?;
        let album = is_album.then(|| AlbumContext::from_api(&root));
        let mut collection = Collection {
            kind,
            id: id.to_string(),
            title: text(&root, "name").unwrap_or_else(|| "Spotify list".into()),
            owner: if is_album {
                names(root.get("artists")).first().cloned()
            } else {
                root.pointer("/owner/display_name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            },
            cover: largest_image(root.get("images")),
            tracks: Vec::new(),
            notice: None,
        };
        // Playlists list their entries under `tracks` (older API) or `items`.
        let mut page = root
            .get("tracks")
            .or_else(|| root.get("items"))
            .cloned()
            .unwrap_or(Value::Null);
        loop {
            for item in page
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let track = if is_album {
                    item
                } else {
                    item.get("track")
                        .or_else(|| item.get("item"))
                        .unwrap_or(&Value::Null)
                };
                if let Some(t) = parse_api_track(track, album.as_ref()) {
                    collection.tracks.push(t);
                }
            }
            if collection.tracks.len() >= MAX_TRACKS {
                break;
            }
            match page.get("next").and_then(Value::as_str) {
                Some(next) => page = self.api_url(creds, next).await?,
                None => break,
            }
        }
        if collection.tracks.is_empty() {
            return Err(no_songs());
        }
        Ok(collection)
    }
}

fn short_link_error() -> AppError {
    AppError::new(
        ErrorCode::InvalidUrl,
        "This Spotify short link does not lead to a song, album or playlist.",
    )
}

fn unreadable() -> AppError {
    AppError::new(
        ErrorCode::ProcessFailed,
        "Spotify returned data DarkLoad could not read.",
    )
}

fn no_songs() -> AppError {
    AppError::new(
        ErrorCode::FormatUnavailable,
        "This album or playlist has no songs DarkLoad can download.",
    )
}

// ----- Parsing ------------------------------------------------------------------------------

fn clean(s: &str) -> Option<String> {
    let s: String = s
        .replace('\u{a0}', " ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect();
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).and_then(clean)
}

fn names(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|list| list.iter().filter_map(|a| text(a, "name")).collect())
        .unwrap_or_default()
}

fn largest_image(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_array)?
        .iter()
        .filter_map(|img| {
            let url = img.get("url").and_then(Value::as_str)?;
            let width = img
                .get("width")
                .or_else(|| img.get("maxWidth"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            is_cover_url(url).then(|| (width, url.to_string()))
        })
        .max_by_key(|(w, _)| *w)
        .map(|(_, url)| url)
}

fn track_id_from_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("spotify:track:")
        .filter(|id| platform::is_spotify_id(id))
        .map(str::to_string)
}

fn millis(v: &Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|ms| *ms > 0.0 && ms.is_finite())
        .map(|ms| ms / 1000.0)
}

/// Extracts the Next.js page data embedded in Spotify's player page.
fn next_data(html: &str) -> Option<Value> {
    let marker = html.find("id=\"__NEXT_DATA__\"")?;
    let start = marker + html[marker..].find('>')? + 1;
    let end = start + html[start..].find("</script>")?;
    serde_json::from_str(&html[start..end]).ok()
}

fn parse_embed_track(e: &Value) -> Option<SpotifyTrack> {
    let id = text(e, "id")
        .filter(|id| platform::is_spotify_id(id))
        .or_else(|| text(e, "uri").and_then(|u| track_id_from_uri(&u)))?;
    Some(SpotifyTrack {
        id,
        title: text(e, "name").or_else(|| text(e, "title"))?,
        artists: names(e.get("artists")),
        release_date: e
            .pointer("/releaseDate/isoString")
            .and_then(Value::as_str)
            .and_then(|s| s.get(..10))
            .map(str::to_string),
        duration: millis(e, "duration"),
        cover: largest_image(e.pointer("/visualIdentity/image"))
            .or_else(|| largest_image(e.pointer("/coverArt/sources"))),
        explicit: e
            .get("isExplicit")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        playable: e.get("isPlayable").and_then(Value::as_bool).unwrap_or(true),
        ..SpotifyTrack::default()
    })
}

fn parse_embed_collection(e: &Value, kind: UrlKind) -> Option<Collection> {
    let title = text(e, "name").or_else(|| text(e, "title"))?;
    let owner = text(e, "subtitle");
    let cover = largest_image(e.pointer("/coverArt/sources"))
        .or_else(|| largest_image(e.pointer("/visualIdentity/image")));
    let is_album = kind == UrlKind::Album;
    let tracks: Vec<SpotifyTrack> = e
        .get("trackList")
        .and_then(Value::as_array)?
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let id = text(t, "uri").and_then(|u| track_id_from_uri(&u))?;
            let artists = text(t, "subtitle")
                .map(|s| s.split(',').filter_map(clean).collect())
                .unwrap_or_default();
            Some(SpotifyTrack {
                id,
                title: text(t, "title")?,
                artists,
                album: is_album.then(|| title.clone()),
                album_artist: if is_album { owner.clone() } else { None },
                track_number: is_album.then(|| (i + 1) as u32),
                duration: millis(t, "duration"),
                cover: if is_album { cover.clone() } else { None },
                explicit: t
                    .get("isExplicit")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                playable: t.get("isPlayable").and_then(Value::as_bool).unwrap_or(true),
                ..SpotifyTrack::default()
            })
        })
        .collect();
    Some(Collection {
        kind,
        id: text(e, "id").unwrap_or_default(),
        title,
        owner,
        cover,
        tracks,
        notice: None,
    })
}

/// Album details shared by every track of an album response, which omits them per track.
struct AlbumContext {
    name: Option<String>,
    artist: Option<String>,
    release_date: Option<String>,
    cover: Option<String>,
}

impl AlbumContext {
    fn from_api(v: &Value) -> Self {
        Self {
            name: text(v, "name"),
            artist: names(v.get("artists")).first().cloned(),
            release_date: text(v, "release_date"),
            cover: largest_image(v.get("images")),
        }
    }
}

fn parse_api_track(v: &Value, album: Option<&AlbumContext>) -> Option<SpotifyTrack> {
    if v.get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t != "track")
        || v.get("is_local").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let id = text(v, "id").filter(|id| platform::is_spotify_id(id))?;
    let own = v.get("album").map(AlbumContext::from_api);
    let album = own.as_ref().or(album);
    Some(SpotifyTrack {
        id,
        title: text(v, "name")?,
        artists: names(v.get("artists")),
        album: album.and_then(|a| a.name.clone()),
        album_artist: album.and_then(|a| a.artist.clone()),
        track_number: v
            .get("track_number")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        release_date: album.and_then(|a| a.release_date.clone()),
        duration: millis(v, "duration_ms"),
        cover: album.and_then(|a| a.cover.clone()),
        isrc: v
            .pointer("/external_ids/isrc")
            .and_then(Value::as_str)
            .and_then(clean),
        explicit: v.get("explicit").and_then(Value::as_bool).unwrap_or(false),
        playable: v
            .get("is_playable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        ..SpotifyTrack::default()
    })
}

// ----- YouTube matching ---------------------------------------------------------------------

/// A YouTube search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub channel: Option<String>,
    pub duration: Option<f64>,
    pub verified: bool,
}

fn parse_search(v: &Value) -> Vec<Candidate> {
    v.get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = text(e, "id")?;
            let url = platform::detect(&format!("https://www.youtube.com/watch?v={id}")).ok()?;
            if e.get("live_status")
                .and_then(Value::as_str)
                .is_some_and(|s| s == "is_live" || s == "is_upcoming")
            {
                return None;
            }
            Some(Candidate {
                url: url.url,
                title: text(e, "title")?,
                channel: text(e, "channel").or_else(|| text(e, "uploader")),
                duration: e.get("duration").and_then(Value::as_f64),
                verified: e
                    .get("channel_is_verified")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// Lowercase words with accents and punctuation removed, for fuzzy comparison.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            '&' => ' ',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The song title without Spotify's version suffixes ("- 2009 Remaster", "(feat. X)").
fn core_title(title: &str) -> String {
    let mut t = title.to_string();
    if let Some(i) = t.find(" - ") {
        let suffix = t[i + 3..].to_lowercase();
        if [
            "remaster", "version", "edit", "mix", "mono", "stereo", "live", "from ", "bonus",
        ]
        .iter()
        .any(|k| suffix.contains(k))
        {
            t.truncate(i);
        }
    }
    for open in ['(', '['] {
        if let Some(i) = t.find(open) {
            let inner = t[i..].to_lowercase();
            if inner.contains("feat") || inner.contains("with ") || inner.contains("remaster") {
                t.truncate(i);
            }
        }
    }
    t.trim().to_string()
}

/// Share of `needle` words that appear in `haystack`.
fn coverage(needle: &[String], haystack: &[String]) -> f64 {
    if needle.is_empty() {
        return 0.0;
    }
    let hits = needle.iter().filter(|w| haystack.contains(w)).count();
    hits as f64 / needle.len() as f64
}

/// Words that mark a different performance or edit of a song.
const VARIANT_WORDS: &[&str] = &[
    "live",
    "cover",
    "remix",
    "karaoke",
    "instrumental",
    "sped",
    "slowed",
    "reverb",
    "nightcore",
    "8d",
    "acoustic",
    "reaction",
    "tutorial",
    "lesson",
    "parody",
    "concert",
    "bass",
    "boosted",
    "loop",
    "hour",
    "hours",
    "mashup",
    "piano",
    "edit",
    "version",
];

/// Scores how likely a search result is the same recording (0–100), or `None` to reject it.
pub fn score(track: &SpotifyTrack, c: &Candidate) -> Option<f64> {
    let spotify_title = words(&track.title);
    let wanted_title = words(&core_title(&track.title));
    let result_title = words(&c.title);
    let channel = c
        .channel
        .as_deref()
        .map(|ch| ch.trim_end_matches(" - Topic").replace("VEVO", ""))
        .unwrap_or_default();
    let channel_words = words(&channel);
    let mut haystack = result_title.clone();
    haystack.extend(channel_words.iter().cloned());

    let title_score = coverage(&wanted_title, &result_title);
    if title_score < 0.5 {
        return None;
    }
    let artist_score = track
        .artists
        .first()
        .map(|a| coverage(&words(a), &haystack))
        .unwrap_or(0.5);

    let duration_score = match (track.duration, c.duration) {
        (Some(want), Some(got)) => {
            let diff = (want - got).abs();
            if diff > 20.0 && diff > want * 0.1 {
                return None;
            }
            (1.0 - (diff - 2.0).max(0.0) / 18.0).max(0.0)
        }
        _ => 0.4,
    };

    let mut total = title_score * 40.0 + artist_score * 25.0 + duration_score * 25.0;
    if c.channel
        .as_deref()
        .is_some_and(|ch| ch.ends_with(" - Topic"))
    {
        total += 10.0;
    } else if !channel_words.is_empty()
        && track
            .artists
            .iter()
            .any(|a| coverage(&words(a), &channel_words) >= 0.99)
    {
        total += 6.0;
    }
    if c.verified {
        total += 2.0;
    }
    for word in VARIANT_WORDS {
        let word = (*word).to_string();
        if result_title.contains(&word) && !spotify_title.contains(&word) {
            total -= 30.0;
        }
    }
    // Fan uploads with lyrics or subtitles usually carry the right audio, but an official
    // upload is preferred when both exist.
    if [
        "lyrics",
        "lyric",
        "letra",
        "testo",
        "vietsub",
        "sub",
        "subtitulada",
    ]
    .iter()
    .any(|w| result_title.contains(&(*w).to_string()))
    {
        total -= 8.0;
    }
    Some(total.clamp(0.0, 100.0))
}

pub fn best_match(track: &SpotifyTrack, candidates: Vec<Candidate>) -> Option<(Candidate, f64)> {
    // Among equally good results, YouTube's own ranking decides: earlier results get a small
    // bonus, and ties keep the earlier result.
    candidates
        .into_iter()
        .enumerate()
        .filter_map(|(rank, c)| score(track, &c).map(|s| (rank, c, s)))
        .filter(|(_, _, s)| *s >= MIN_SCORE)
        .map(|(rank, c, s)| (rank, c, s + (4.0 - rank as f64 * 0.5).max(0.0)))
        .min_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)))
        .map(|(_, c, s)| (c, s.min(100.0)))
}

async fn search(
    tools: &ToolPaths,
    net: NetOptions,
    query: &str,
    cancel: &CancellationToken,
) -> AppResult<Vec<Candidate>> {
    let query: String = query
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let json = extractor::dump_json(
        tools,
        &format!("ytsearch8:{query}"),
        net,
        cancel,
        &["--flat-playlist"],
    )
    .await?;
    Ok(parse_search(&json))
}

/// Finds the YouTube upload that is the same recording as `track`.
pub async fn find_match(
    tools: &ToolPaths,
    net: NetOptions,
    track: &SpotifyTrack,
    cancel: &CancellationToken,
) -> AppResult<Candidate> {
    let artist = track.artists.first().cloned().unwrap_or_default();
    let title = core_title(&track.title);
    let queries = [
        format!("{artist} - {title}"),
        format!("{} {title} audio", track.artist_line()),
    ];
    let mut best: Option<(Candidate, f64)> = None;
    for query in queries {
        let found = best_match(track, search(tools, net, &query, cancel).await?);
        if let Some((c, s)) = found {
            if best.as_ref().is_none_or(|(_, b)| s > *b) {
                best = Some((c, s));
            }
        }
        if best.as_ref().is_some_and(|(_, s)| *s >= 80.0) {
            break;
        }
    }
    best.map(|(c, _)| c).ok_or_else(|| {
        AppError::new(
            ErrorCode::Unavailable,
            format!(
                "No reliable match for “{}” was found on YouTube, so nothing was downloaded.",
                track.title
            ),
        )
    })
}

// ----- Download preparation -----------------------------------------------------------------

/// A Spotify song resolved to YouTube media, with metadata ready for the download.
pub struct Prepared {
    pub track: SpotifyTrack,
    pub media: MediaInfo,
    /// yt-dlp info JSON of the matched video with Spotify's metadata applied.
    pub info: Value,
}

/// Rewrites the matched video's metadata so yt-dlp tags the file with Spotify's details.
pub fn apply_metadata(info: &mut Value, track: &SpotifyTrack) {
    let Some(obj) = info.as_object_mut() else {
        return;
    };
    let artist = track.artist_line();
    obj.insert("title".into(), track.title.clone().into());
    obj.insert("fulltitle".into(), track.title.clone().into());
    obj.insert("track".into(), track.title.clone().into());
    if !track.artists.is_empty() {
        obj.insert("artist".into(), artist.clone().into());
        obj.insert("artists".into(), track.artists.clone().into());
        obj.insert("creator".into(), artist.clone().into());
        obj.insert("creators".into(), track.artists.clone().into());
        obj.insert("uploader".into(), artist.clone().into());
    }
    for (key, value) in [
        ("album", &track.album),
        ("album_artist", &track.album_artist),
    ] {
        match value {
            Some(v) => obj.insert(key.into(), v.clone().into()),
            None => obj.remove(key),
        };
    }
    if let Some(album_artist) = &track.album_artist {
        obj.insert("album_artists".into(), vec![album_artist.clone()].into());
    } else {
        obj.remove("album_artists");
    }
    match track.track_number {
        Some(n) => obj.insert("track_number".into(), n.into()),
        None => obj.remove("track_number"),
    };
    if let Some(date) = &track.release_date {
        let digits: String = date.chars().filter(char::is_ascii_digit).collect();
        if digits.len() >= 4 {
            if let Ok(year) = digits[..4].parse::<u32>() {
                obj.insert("release_year".into(), year.into());
            }
        }
        if digits.len() == 8 {
            obj.insert("release_date".into(), digits.clone().into());
            obj.insert("upload_date".into(), digits.into());
        }
    }
    // The video description is not the song's; keep a plain pointer to the source instead.
    obj.insert(
        "description".into(),
        format!("Spotify: {}", track.url()).into(),
    );
    if let Some(cover) = &track.cover {
        obj.insert("thumbnail".into(), cover.clone().into());
        obj.insert(
            "thumbnails".into(),
            serde_json::json!([{ "url": cover, "id": "spotify", "preference": 1 }]),
        );
    }
}

/// Matches a song on YouTube (unless already matched) and reads the matched video's formats.
pub async fn prepare(
    client: &SpotifyClient,
    tools: &ToolPaths,
    net: NetOptions,
    mut track: SpotifyTrack,
    creds: Option<&Credentials>,
    cancel: &CancellationToken,
) -> AppResult<Prepared> {
    if track.source_url.is_none() {
        if let Some(known) = client.remembered(&track.id) {
            track.source_url = known.source_url;
            track.source_title = known.source_title;
            track.source_channel = known.source_channel;
        }
    }
    if track.cover.is_none() || track.release_date.is_none() || track.artists.is_empty() {
        match client.track(&track.id, creds).await {
            Ok(full) => track.merge_missing(full),
            Err(e) => tracing::debug!("could not complete Spotify metadata: {}", e.message),
        }
    }
    if track.source_url.is_none() {
        let found = find_match(tools, net, &track, cancel).await?;
        track.source_url = Some(found.url);
        track.source_title = Some(found.title);
        track.source_channel = found.channel;
    }
    let source = track.source_url.clone().unwrap_or_default();
    let detected = platform::detect(&source)?;
    let mut info =
        extractor::dump_json(tools, &detected.url, net, cancel, &["--no-playlist"]).await?;
    let mut media = match extractor::parse_analysis(&info, &detected)? {
        Analysis::Media(media) => *media,
        Analysis::Playlist(_) => return Err(unreadable()),
    };
    if !media.summary.has_audio {
        return Err(AppError::new(
            ErrorCode::FormatUnavailable,
            "The matching YouTube video has no audio.",
        ));
    }
    apply_metadata(&mut info, &track);
    media.id = track.id.clone();
    media.url = track.url();
    media.title = track.title.clone();
    media.uploader = Some(track.artist_line()).filter(|a| !a.is_empty());
    media.platform = Platform::Spotify;
    media.playlist_id = None;
    media.subtitles.clear();
    if let Some(cover) = &track.cover {
        media.thumbnail = Some(cover.clone());
        media.thumbnail_local = None;
    }
    media.spotify = Some(track.clone());
    client.remember(&track);
    Ok(Prepared { track, media, info })
}

/// Builds the analysis shown for a Spotify link.
pub async fn analyze(
    client: &SpotifyClient,
    tools: &ToolPaths,
    net: NetOptions,
    detected: &DetectedUrl,
    creds: Option<&Credentials>,
    cancel: &CancellationToken,
) -> AppResult<Analysis> {
    let detected = client.resolve_shortlink(detected).await?;
    let id = detected.id.clone().unwrap_or_default();
    if detected.kind == UrlKind::Track {
        let track = client.track(&id, creds).await?;
        let prepared = prepare(client, tools, net, track, creds, cancel).await?;
        return Ok(Analysis::Media(Box::new(prepared.media)));
    }
    let collection = client.collection(&detected, creds).await?;
    if collection.tracks.is_empty() {
        return Err(no_songs());
    }
    let entries = collection
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| extractor::PlaylistEntry {
            id: t.id.clone(),
            url: t.url(),
            title: t.title.clone(),
            uploader: Some(t.artist_line()).filter(|a| !a.is_empty()),
            duration: t.duration,
            thumbnail: t.cover.clone().or_else(|| collection.cover.clone()),
            index: (i + 1) as u32,
            playlist_item: None,
            available: t.playable,
            media: None,
            spotify: Some(t.clone()),
        })
        .collect();
    Ok(Analysis::Playlist(extractor::PlaylistInfo {
        id: collection.id.clone(),
        url: detected.url.clone(),
        title: collection.title,
        uploader: collection.owner,
        platform: Platform::Spotify,
        thumbnail: collection.cover,
        entries,
        notice: collection.notice,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artist: &str, duration: f64) -> SpotifyTrack {
        SpotifyTrack {
            id: "4PTG3Z6ehGkBFwjybzWkR8".into(),
            title: title.into(),
            artists: vec![artist.into()],
            duration: Some(duration),
            playable: true,
            ..SpotifyTrack::default()
        }
    }

    fn cand(title: &str, channel: &str, duration: f64) -> Candidate {
        Candidate {
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            title: title.into(),
            channel: Some(channel.into()),
            duration: Some(duration),
            verified: false,
        }
    }

    #[test]
    fn parses_embed_track() {
        let html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"type":"track","name":"Never Gonna Give You Up","id":"4PTG3Z6ehGkBFwjybzWkR8","artists":[{"name":"Rick Astley"}],"releaseDate":{"isoString":"1987-11-16T00:00:00Z"},"duration":213573,"isPlayable":true,"isExplicit":false,"visualIdentity":{"image":[{"url":"https://image-cdn-fa.spotifycdn.com/image/a300","maxWidth":300},{"url":"https://image-cdn-fa.spotifycdn.com/image/a640","maxWidth":640}]}}}}}}}</script></html>"#;
        let data = next_data(html).expect("next data");
        let entity = data.pointer("/props/pageProps/state/data/entity").unwrap();
        let t = parse_embed_track(entity).expect("track");
        assert_eq!(t.title, "Never Gonna Give You Up");
        assert_eq!(t.artists, vec!["Rick Astley"]);
        assert_eq!(t.release_date.as_deref(), Some("1987-11-16"));
        assert_eq!(t.duration, Some(213.573));
        assert_eq!(
            t.cover.as_deref(),
            Some("https://image-cdn-fa.spotifycdn.com/image/a640")
        );
    }

    #[test]
    fn parses_embed_album() {
        let entity: Value = serde_json::from_str(r#"{"type":"album","name":"Whenever You Need Somebody","id":"6N9PS4QXF1D0OWPk0Sxtb4","subtitle":"Rick Astley","coverArt":{"sources":[{"url":"https://i.scdn.co/image/abc","width":640}]},"trackList":[{"uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC","title":"Never Gonna Give You Up","subtitle":"Rick Astley","duration":213573,"isPlayable":true},{"uri":"spotify:local:xyz","title":"Local file","subtitle":"Someone","duration":1000},{"uri":"spotify:track:6PCUP3dWmTjcTtXY02oFdT","title":"Together Forever","subtitle":"Rick Astley, Guest Artist","duration":205000,"isPlayable":false}]}"#).unwrap();
        let c = parse_embed_collection(&entity, UrlKind::Album).expect("album");
        assert_eq!(c.title, "Whenever You Need Somebody");
        assert_eq!(c.tracks.len(), 2);
        assert_eq!(
            c.tracks[0].album.as_deref(),
            Some("Whenever You Need Somebody")
        );
        assert_eq!(c.tracks[0].track_number, Some(1));
        assert_eq!(
            c.tracks[0].cover.as_deref(),
            Some("https://i.scdn.co/image/abc")
        );
        assert_eq!(c.tracks[1].artists, vec!["Rick Astley", "Guest Artist"]);
        assert!(!c.tracks[1].playable);
    }

    #[test]
    fn parses_api_playlist_track() {
        let v: Value = serde_json::from_str(r#"{"type":"track","id":"4PTG3Z6ehGkBFwjybzWkR8","name":"Never Gonna Give You Up","artists":[{"name":"Rick Astley"}],"album":{"name":"Whenever You Need Somebody","artists":[{"name":"Rick Astley"}],"release_date":"1987-11-12","images":[{"url":"https://i.scdn.co/image/big","width":640},{"url":"https://i.scdn.co/image/small","width":64}]},"track_number":1,"duration_ms":213573,"external_ids":{"isrc":"GBARL9300135"},"explicit":false}"#).unwrap();
        let t = parse_api_track(&v, None).expect("track");
        assert_eq!(t.album.as_deref(), Some("Whenever You Need Somebody"));
        assert_eq!(t.isrc.as_deref(), Some("GBARL9300135"));
        assert_eq!(t.cover.as_deref(), Some("https://i.scdn.co/image/big"));
        assert_eq!(t.track_number, Some(1));
        let episode: Value = serde_json::from_str(
            r#"{"type":"episode","id":"4PTG3Z6ehGkBFwjybzWkR8","name":"Podcast"}"#,
        )
        .unwrap();
        assert!(parse_api_track(&episode, None).is_none());
    }

    #[test]
    fn strips_version_suffixes() {
        assert_eq!(core_title("Dreams - 2004 Remaster"), "Dreams");
        assert_eq!(core_title("Stay (with Justin Bieber)"), "Stay");
        assert_eq!(
            core_title("Flower In A Hurricane (feat. SZA) [From GTAVI]"),
            "Flower In A Hurricane"
        );
        assert_eq!(core_title("Hey - You"), "Hey - You");
    }

    #[test]
    fn prefers_the_same_recording() {
        let t = track("Never Gonna Give You Up", "Rick Astley", 213.6);
        let official = cand(
            "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)",
            "Rick Astley",
            214.0,
        );
        let topic = cand("Never Gonna Give You Up", "Rick Astley - Topic", 213.0);
        let live = cand(
            "Never Gonna Give You Up (Live at Glastonbury)",
            "BBC",
            230.0,
        );
        let cover = cand(
            "Never Gonna Give You Up - Piano Cover",
            "Some Pianist",
            213.0,
        );
        let (best, _) = best_match(
            &t,
            vec![live.clone(), official, cover.clone(), topic.clone()],
        )
        .unwrap();
        assert_eq!(best, topic);
        assert!(score(&t, &live).is_none_or(|s| s < MIN_SCORE));
        assert!(score(&t, &cover).is_none_or(|s| s < MIN_SCORE));
    }

    #[test]
    fn prefers_official_uploads_over_lyrics_videos() {
        let t = track("Never Gonna Give You Up", "Rick Astley", 213.6);
        let mut official = cand(
            "Rick Astley - Never Gonna Give You Up (Official Video)",
            "Rick Astley",
            214.0,
        );
        official.verified = true;
        let mut lyrics = cand(
            "Vietsub | Never Gonna Give You Up - Rick Astley | Lyrics Video",
            "",
            213.0,
        );
        lyrics.channel = None;
        let (best, _) = best_match(&t, vec![lyrics.clone(), official.clone()]).unwrap();
        assert_eq!(best, official);
        // Equal results keep YouTube's order.
        let (first, _) = best_match(&t, vec![official.clone(), official.clone()]).unwrap();
        assert_eq!(first, official);
    }

    #[test]
    fn rejects_wrong_duration_and_title() {
        let t = track("Bohemian Rhapsody", "Queen", 354.0);
        assert!(score(
            &t,
            &cand("Queen - Bohemian Rhapsody (10 hours)", "Loops", 36000.0)
        )
        .is_none());
        assert!(score(&t, &cand("Queen - Don't Stop Me Now", "Queen", 354.0)).is_none());
        assert!(best_match(&t, vec![]).is_none());
    }

    #[test]
    fn keeps_variants_spotify_names() {
        let t = track("Hallelujah - Live", "Jeff Buckley", 400.0);
        let live = cand("Jeff Buckley - Hallelujah (Live)", "Jeff Buckley", 401.0);
        assert!(score(&t, &live).unwrap() >= MIN_SCORE);
    }

    #[test]
    fn writes_spotify_metadata() {
        let mut info = serde_json::json!({"title": "Video title", "description": "Long text", "album": "Wrong"});
        let mut t = track("Song", "Artist", 200.0);
        t.artists.push("Guest".into());
        t.album = Some("Album".into());
        t.track_number = Some(3);
        t.release_date = Some("2020-05-01".into());
        t.cover = Some("https://i.scdn.co/image/x".into());
        apply_metadata(&mut info, &t);
        assert_eq!(info["title"], "Song");
        assert_eq!(info["artist"], "Artist, Guest");
        assert_eq!(info["album"], "Album");
        assert_eq!(info["track_number"], 3);
        assert_eq!(info["release_date"], "20200501");
        assert_eq!(info["release_year"], 2020);
        assert_eq!(info["thumbnails"][0]["url"], "https://i.scdn.co/image/x");
    }

    #[test]
    fn validates_interface_metadata() {
        let mut t = track("Song", "Artist", 200.0);
        t.cover = Some("https://evil.example/x.jpg".into());
        t.source_url = Some("https://example.com/watch?v=dQw4w9WgXcQ".into());
        t.validate().unwrap();
        assert!(t.cover.is_none());
        assert!(t.source_url.is_none());
        t.source_url = Some("https://youtu.be/dQw4w9WgXcQ".into());
        t.validate().unwrap();
        assert_eq!(
            t.source_url.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        t.id = "bad".into();
        assert!(t.validate().is_err());
    }
}
