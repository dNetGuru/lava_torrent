//! [Experimental] Module containing structs for tracker responses.
//!
//! These structs are provided only for user convenience. Since they
//! are experimental, they might be removed or replaced in the future.
//!
//! At the moment, `lava_torrent` does not handle communication
//! with trackers. Users will have to send requests themselves and
//! pass the received responses to `lava_torrent` for parsing.

use bencode::BencodeElem;
use error::*;
use itertools::Itertools;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use torrent::v1::{Dictionary, Integer};

/// Peer information returned in a tracker response.
///
/// Modeled after the specifications in
/// [BEP 3](http://bittorrent.org/beps/bep_0003.html) and
/// [BEP 23](http://www.bittorrent.org/beps/bep_0023.html).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Peer {
    /// A string of length 20 which this peer uses as its id.
    /// This field will be `None` for compact peer info.
    pub id: Option<String>,
    /// The IP/port this peer is listening on.
    pub addr: SocketAddr,
    /// Fields not listed above.
    pub extra_fields: Option<Dictionary>,
}

/// Everything found in a tracker response.
///
/// Modeled after the specifications in
/// [BEP 3](http://bittorrent.org/beps/bep_0003.html) and
/// [theory.org](https://wiki.theory.org/index.php/BitTorrentSpecification#Tracker_Response).
/// Unknown/extension fields will be placed in `extra_fields`. If you
/// need any of those extra fields you would have to parse it yourself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackerResponse {
    /// The number of seconds the downloader should wait between
    /// regular requests.
    pub interval: Integer,
    /// A list of dictionaries corresponding to `Peer`.
    pub peers: Vec<Peer>,
    /// Warning message.
    pub warning: Option<String>,
    /// Minimum announce interval. If present clients must not
    /// re-announce more frequently than this.
    pub min_interval: Option<Integer>,
    /// A string that the client should send back on its next
    /// announcements.
    pub tracker_id: Option<String>,
    /// Number of peers with the entire file, i.e. seeders.
    pub complete: Option<Integer>,
    /// Number of non-seeder peers, i.e. leechers.
    pub incomplete: Option<Integer>,
    /// Fields not listed above.
    pub extra_fields: Option<Dictionary>,
}

/// Swarm metadata returned in a tracker scrape response.
///
/// Modeled after the specifications in
/// [BEP 48](http://www.bittorrent.org/beps/bep_0048.html).
/// Unknown/extension fields will be placed in `extra_fields`. If you
/// need any of those extra fields you would have to parse it yourself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwarmMetadata {
    /// The number of active peers that have completed downloading.
    pub complete: Integer,
    /// The number of active peers that have not completed downloading.
    pub incomplete: Integer,
    /// The number of peers that have ever completed downloading.
    pub downloaded: Integer,
    /// Fields not listed above.
    pub extra_fields: Option<Dictionary>,
}

/// Everything found in a tracker scrape response.
///
/// Modeled after the specifications in
/// [BEP 48](http://www.bittorrent.org/beps/bep_0048.html) and
/// [theory.org](https://wiki.theory.org/index.php/BitTorrentSpecification#Tracker_.27scrape.27_Convention).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackerScrapeResponse {
    /// File info (info hash -> metadata).
    pub files: HashMap<Vec<u8>, SwarmMetadata>,
    /// Fields not listed above.
    pub extra_fields: Option<Dictionary>,
}

impl Peer {
    /// Go through `dict` and return the extracted `Peer`.
    ///
    /// If `dict` is missing any required field (e.g. `ip`),
    /// then `Err(error)` will be returned.
    fn from_dict(mut dict: HashMap<String, BencodeElem>) -> Result<Peer> {
        let id = match dict.remove("peer id") {
            Some(BencodeElem::String(string)) => Some(string),
            Some(BencodeElem::Bytes(bytes)) => Some(
                bytes
                    .iter()
                    .map(|b| format!("{:x}", b))
                    .format("")
                    .to_string(),
            ),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""peer id" maps to neither a utf8 string nor a string of bytes."#
            ))),
            None => None,
        };
        let ip = match dict.remove("ip") {
            Some(BencodeElem::String(ip)) => ip,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""ip" does not map to a string (or maps to invalid UTF8)."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""ip" does not exist."#
            ))),
        };
        let port = match dict.remove("port") {
            Some(BencodeElem::Integer(port)) => port,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""port" does not map to an integer."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""port" does not exist."#
            ))),
        };
        let extra_fields = if dict.is_empty() { None } else { Some(dict) };

        let ip = match ip.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""ip" is invalid."#
            ))),
        };

        Ok(Peer {
            id,
            addr: SocketAddr::from((ip, port as u16)),
            extra_fields,
        })
    }

    /// Parse `bytes` and return the extracted `Peer`.
    ///
    /// `bytes` must contain exactly 6 bytes.
    fn from_bytes<B>(bytes: B) -> Peer
        where
            B: AsRef<[u8]>,
    {
        let bytes = bytes.as_ref();
        if bytes.len() != 6 {
            panic!("Peer::from_bytes() expects 6 bytes, {} received.", bytes.len())
        }

        let ip = Ipv4Addr::from(u32::from_be_bytes(bytes[..4].try_into().unwrap()));
        let port = u16::from_be_bytes(bytes[4..].try_into().unwrap());

        Peer {
            id: None,
            addr: SocketAddr::from((ip, port)),
            extra_fields: None,
        }
    }
}

impl TrackerResponse {
    /// Parse `bytes` and return the extracted `TrackerResponse`.
    ///
    /// If `bytes` is missing any required field (e.g. `interval`), or if any other
    /// error is encountered (e.g. `IOError`), then `Err(error)` will be returned.
    pub fn from_bytes<B>(bytes: B) -> Result<TrackerResponse>
        where
            B: AsRef<[u8]>,
    {
        let mut parsed = BencodeElem::from_bytes(bytes)?;
        if parsed.len() != 1 {
            bail!(ErrorKind::MalformedTorrent(Cow::Owned(format!(
                "Tracker response should contain 1 and only 1 top-level element, {} found.",
                parsed.len()
            ))));
        }

        let mut parsed = match parsed.remove(0) {
            BencodeElem::Dictionary(dict) => dict,
            _ => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                "Tracker response doesn't contain a dictionary."
            ))),
        };

        match parsed.remove("failure reason") {
            Some(BencodeElem::String(reason)) => bail!(ErrorKind::TrackerErrorResponse(Cow::Owned(reason))),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""failure reason" does not map to a string (or maps to invalid UTF8)."#
            ))),
            None => (),
        }

        let interval = match parsed.remove("interval") {
            Some(BencodeElem::Integer(interval)) => interval,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""interval" does not map to an integer."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""interval" does not exist."#
            ))),
        };

        let peers = match parsed.remove("peers") {
            Some(BencodeElem::List(list)) => Self::extract_peers_from_list(list)?,
            Some(BencodeElem::Bytes(bytes)) => Self::extract_peers_from_bytes(bytes.as_ref())?,
            Some(BencodeElem::String(str)) => Self::extract_peers_from_bytes(str.as_bytes())?,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""peers" does not map to a dict or a string of bytes."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""peers" does not exist."#
            ))),
        };

        let warning = match parsed.remove("warning") {
            Some(BencodeElem::String(warning)) => Some(warning),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""warning" does not map to a string (or maps to invalid UTF8)."#
            ))),
            None => None,
        };

        let min_interval = match parsed.remove("min interval") {
            Some(BencodeElem::Integer(min_interval)) => Some(min_interval),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""min interval" does not map to an integer."#
            ))),
            None => None,
        };

        let tracker_id = match parsed.remove("tracker id") {
            Some(BencodeElem::String(tracker_id)) => Some(tracker_id),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""tracker id" does not map to a string (or maps to invalid UTF8)."#
            ))),
            None => None,
        };

        let complete = match parsed.remove("complete") {
            Some(BencodeElem::Integer(complete)) => Some(complete),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""complete" does not map to an integer."#
            ))),
            None => None,
        };
        let incomplete = match parsed.remove("incomplete") {
            Some(BencodeElem::Integer(incomplete)) => Some(incomplete),
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""incomplete" does not map to an integer."#
            ))),
            None => None,
        };

        let extra_fields = if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        };

        Ok(TrackerResponse {
            interval,
            peers,
            warning,
            min_interval,
            tracker_id,
            complete,
            incomplete,
            extra_fields,
        })
    }

    fn extract_peers_from_list(list: Vec<BencodeElem>) -> Result<Vec<Peer>> {
        list.into_iter()
            .map(|elem| match elem {
                BencodeElem::Dictionary(dict) => Ok(Peer::from_dict(dict)?),
                _ => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                    r#""peers" contains a non-dictionary element."#
                ))),
            })
            .collect()
    }

    fn extract_peers_from_bytes(bytes: &[u8]) -> Result<Vec<Peer>> {
        if (bytes.len() % 6) != 0 {
            bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#"Compact "peers" contains incorrect number of bytes"#
            )))
        }

        let n_peers = bytes.len() / 6;
        let mut peers = Vec::with_capacity(n_peers);
        for i in 0..(n_peers) {
            peers.push(Peer::from_bytes(bytes[(i * 6)..((i + 1) * 6)].as_ref()));
        }
        Ok(peers)
    }
}

impl SwarmMetadata {
    /// Go through `dict` and return the extracted `SwarmMetadata`.
    ///
    /// If `dict` is missing any required field (e.g. `complete`), then
    /// `Err(error)` will be returned.
    fn from_dict(mut dict: HashMap<String, BencodeElem>) -> Result<SwarmMetadata> {
        let complete = match dict.remove("complete") {
            Some(BencodeElem::Integer(complete)) => complete,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""complete" does not map to an integer."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""complete" does not exist."#
            ))),
        };
        let incomplete = match dict.remove("incomplete") {
            Some(BencodeElem::Integer(incomplete)) => incomplete,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""incomplete" does not map to an integer."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""incomplete" does not exist."#
            ))),
        };
        let downloaded = match dict.remove("downloaded") {
            Some(BencodeElem::Integer(downloaded)) => downloaded,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""downloaded" does not map to an integer."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""downloaded" does not exist."#
            ))),
        };
        let extra_fields = if dict.is_empty() { None } else { Some(dict) };

        Ok(SwarmMetadata {
            complete,
            incomplete,
            downloaded,
            extra_fields,
        })
    }
}

impl TrackerScrapeResponse {
    /// Parse `bytes` and return the extracted `TrackerScrapeResponse`.
    ///
    /// If `bytes` is missing any required field (e.g. `files`), or if any other
    /// error is encountered (e.g. `IOError`), then `Err(error)` will be returned.
    pub fn from_bytes<B>(bytes: B) -> Result<TrackerScrapeResponse>
        where
            B: AsRef<[u8]>,
    {
        let mut parsed = BencodeElem::from_bytes(bytes)?;
        if parsed.len() != 1 {
            bail!(ErrorKind::MalformedTorrent(Cow::Owned(format!(
                "Tracker scrape response should contain 1 and only 1 top-level element, {} found.",
                parsed.len()
            ))));
        }
        let mut parsed = match parsed.remove(0) {
            BencodeElem::Dictionary(dict) => dict,
            _ => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                "Tracker scrape response doesn't contain a dictionary."
            ))),
        };

        let files = match parsed.remove("files") {
            Some(BencodeElem::RawDictionary(dict)) => dict,
            Some(_) => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""files" does not map to a raw dict."#
            ))),
            None => bail!(ErrorKind::MalformedResponse(Cow::Borrowed(
                r#""files" does not exist."#
            ))),
        };
        let extra_fields = if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        };

        let files = files
            .into_iter()
            .map(|(k, v)| match v {
                BencodeElem::Dictionary(dict) => Ok((k, SwarmMetadata::from_dict(dict)?)),
                _ => bail!(ErrorKind::MalformedResponse(Cow::Owned(format!(
                    r#"swarm metadata for {} is not a dictionary."#,
                    k.iter().map(|b| format!("{:x}", b)).format("")
                )))),
            })
            .collect::<Result<HashMap<Vec<u8>, SwarmMetadata>>>()?;

        Ok(TrackerScrapeResponse {
            files,
            extra_fields,
        })
    }
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref id) = self.id {
            writeln!(f, "\t-id: {}", id)?;
        }
        writeln!(f, "\t-addr: {}", self.addr)?;

        if let Some(ref fields) = self.extra_fields {
            write!(
                f,
                "{}",
                fields
                    .iter()
                    .sorted_by_key(|&(key, _)| key.as_bytes())
                    .format_with("", |(k, v), f| f(&format_args!("-{}: {}\n", k, v)))
            )?;
        }

        writeln!(f, "\t========================================")
    }
}

impl fmt::Display for TrackerResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "-interval: {}", self.interval)?;
        if let Some(ref min_interval) = self.min_interval {
            writeln!(f, "-min_interval: {}", min_interval)?;
        }
        if let Some(ref warning) = self.warning {
            writeln!(f, "-warning: {}", warning)?;
        }
        if let Some(ref tracker_id) = self.tracker_id {
            writeln!(f, "-tracker_id: {}", tracker_id)?;
        }
        if let Some(ref complete) = self.complete {
            writeln!(f, "-complete: {}", complete)?;
        }
        if let Some(ref incomplete) = self.incomplete {
            writeln!(f, "-incomplete: {}", incomplete)?;
        }

        if let Some(ref fields) = self.extra_fields {
            write!(
                f,
                "{}",
                fields
                    .iter()
                    .sorted_by_key(|&(key, _)| key.as_bytes())
                    .format_with("", |(k, v), f| f(&format_args!("-{}: {}\n", k, v)))
            )?;
        }

        writeln!(f, "-peers ({}):\n{}", self.peers.len(), self.peers.iter().format(""))
    }
}

impl fmt::Display for SwarmMetadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "-complete: {}", self.complete)?;
        writeln!(f, "-incomplete: {}", self.incomplete)?;
        writeln!(f, "-downloaded: {}", self.downloaded)?;

        if let Some(ref fields) = self.extra_fields {
            write!(
                f,
                "{}",
                fields
                    .iter()
                    .sorted_by_key(|&(key, _)| key.as_bytes())
                    .format_with("", |(k, v), f| f(&format_args!("-{}: {}\n", k, v)))
            )?;
        }

        writeln!(f, "========================================")
    }
}

impl fmt::Display for TrackerScrapeResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "files:\n{}",
            self.files
                .iter()
                .format_with("", |(k, v), f| f(&format_args!(
                    "{}\n{}",
                    k.iter().map(|b| format!("{:x}", b)).format(""),
                    v
                )))
        )?;

        if let Some(ref fields) = self.extra_fields {
            write!(
                f,
                "{}",
                fields
                    .iter()
                    .sorted_by_key(|&(key, _)| key.as_bytes())
                    .format_with("", |(k, v), f| f(&format_args!("-{}: {}\n", k, v)))
            )?;
        }

        Ok(())
    }
}

// @todo: add unit tests
