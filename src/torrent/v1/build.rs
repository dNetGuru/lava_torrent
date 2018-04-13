use std::io::{BufReader, Read};
use std::path::Component;
use crypto::sha1::Sha1;
use crypto::digest::Digest;
use util;
use super::*;

impl TorrentBuilder {
    /// Create a new `TorrentBuilder` with required fields set.
    ///
    /// The caller has to ensure that the inputs are valid, as this method
    /// does not validate its inputs. If they turn out
    /// to be invalid, calling [`build()`] later will fail.
    ///
    /// # Notes
    /// - `path` must be absolute.
    ///
    /// - A valid `piece_length` is larger than `0` AND is a power of `2`.
    ///
    /// - Using a `path` containing hidden components will make `build()` return `Err`.
    /// This is \*nix-specific. Hidden components are those that start with `.`.
    ///
    /// - Paths with components exactly matching `..` are invalid.
    ///
    /// [`build()`]: #method.build
    pub fn new<P>(announce: String, path: P, piece_length: Integer) -> TorrentBuilder
    where
        P: AsRef<Path>,
    {
        TorrentBuilder {
            announce,
            path: path.as_ref().to_path_buf(),
            piece_length,
            ..Default::default()
        }
    }

    /// Build a `Torrent` from this `TorrentBuilder`.
    ///
    /// If `name` is not set, then the [last component] of `path`
    /// will be used as the `Torrent`'s `name` field.
    ///
    /// `build()` **does not** provide comprehensive validation of
    /// any input. Basic cases such as setting `announce` to
    /// an empty string will be detected and `Err` will be returned.
    /// But more complicated cases such as using an invalid url
    /// as `announce` won't be detected. Again, the caller
    /// has to ensure that the values given to a `TorrentBuilder`
    /// are valid.
    ///
    /// [last component]: https://doc.rust-lang.org/std/path/struct.Path.html#method.file_name
    pub fn build(self) -> Result<Torrent> {
        // delegate validation to other methods
        self.validate_announce()?;
        self.validate_announce_list()?;
        self.validate_name()?;
        self.validate_path()?;
        self.validate_piece_length()?;
        self.validate_extra_fields()?;
        self.validate_extra_info_fields()?;

        // if `name` is not yet set, set it to the last component of `path`
        let name = if let Some(name) = self.name {
            name
        } else {
            util::last_component(&self.path)?
        };

        // set `private = 1` in `info` if the torrent is private
        let mut extra_info_fields = self.extra_info_fields;
        if self.is_private {
            extra_info_fields
                .get_or_insert_with(HashMap::new)
                .insert("private".to_string(), BencodeElem::Integer(1));
        }

        // delegate the actual file reading to other methods
        let canonicalized_path = self.path.canonicalize()?;
        if self.path.metadata()?.is_dir() {
            let (length, files, pieces) = Self::read_dir(canonicalized_path, self.piece_length)?;

            Ok(Torrent {
                announce: self.announce,
                announce_list: self.announce_list,
                length,
                files: Some(files),
                name,
                piece_length: self.piece_length,
                pieces,
                extra_fields: self.extra_fields,
                extra_info_fields,
            })
        } else {
            let (length, pieces) = Self::read_file(canonicalized_path, self.piece_length)?;

            Ok(Torrent {
                announce: self.announce,
                announce_list: self.announce_list,
                length,
                files: None,
                name,
                piece_length: self.piece_length,
                pieces,
                extra_fields: self.extra_fields,
                extra_info_fields,
            })
        }
    }

    /// Set the `announce` field of the `Torrent` to be built.
    ///
    /// Calling this method multiple times will simply override previous settings.
    ///
    /// The caller has to ensure that `announce` is valid, as this method
    /// does not validate its value. If `announce`
    /// turns out to be invalid, calling [`build()`] later will fail.
    ///
    /// [`build()`]: #method.build
    pub fn set_announce(self, announce: String) -> TorrentBuilder {
        TorrentBuilder { announce, ..self }
    }

    /// Set the `announce_list` field of the `Torrent` to be built.
    ///
    /// Calling this method multiple times will simply override previous settings.
    ///
    /// The caller has to ensure that `announce_list` is valid, as
    /// this method does not validate its value. If `announce_list`
    /// turns out to be invalid, calling [`build()`] later will fail.
    ///
    /// [`build()`]: #method.build
    pub fn set_announce_list(self, announce_list: AnnounceList) -> TorrentBuilder {
        TorrentBuilder {
            announce_list: Some(announce_list),
            ..self
        }
    }

    /// Set the `name` field of the `Torrent` to be built.
    ///
    /// Calling this method multiple times will simply override previous settings.
    ///
    /// The caller has to ensure that `name` is valid, as
    /// this method does not validate its value. If `name`
    /// turns out to be invalid, calling [`build()`] later will fail.
    ///
    /// [`build()`]: #method.build
    pub fn set_name(self, name: String) -> TorrentBuilder {
        TorrentBuilder {
            name: Some(name),
            ..self
        }
    }

    /// Set the path to the file(s) from which the `Torrent` will be built.
    ///
    /// Calling this method multiple times will simply override previous settings.
    ///
    /// The caller has to ensure that `path` is valid, as
    /// this method does not validate its value. If `path`
    /// turns out to be invalid, calling [`build()`] later will fail.
    ///
    /// # Notes
    /// - `path` must be absolute.
    ///
    /// - Using a `path` containing hidden components will make `build()` return `Err`.
    /// This is \*nix-specific. Hidden components are those that start with `.`.
    ///
    /// - Paths with components exactly matching `..` are invalid.
    ///
    /// [`build()`]: #method.build
    pub fn set_path<P>(self, path: P) -> TorrentBuilder
    where
        P: AsRef<Path>,
    {
        TorrentBuilder {
            path: path.as_ref().to_path_buf(),
            ..self
        }
    }

    /// Set the `piece_length` field of the `Torrent` to be built.
    ///
    /// Calling this method multiple times will simply override previous settings.
    ///
    /// The caller has to ensure that `piece_length` is valid, as
    /// this method does not validate its value. If `piece_length`
    /// turns out to be invalid, calling [`build()`] later will fail.
    ///
    /// NOTE: **A valid `piece_length` is larger than `0` AND is a power of `2`.**
    ///
    /// [`build()`]: #method.build
    pub fn set_piece_length(self, piece_length: Integer) -> TorrentBuilder {
        TorrentBuilder {
            piece_length,
            ..self
        }
    }

    /// Add an extra field to `Torrent` (i.e. to the root dictionary).
    ///
    /// Calling this method multiple times with the same key will
    /// simply override previous settings.
    ///
    /// The caller has to ensure that `key` and `val` are valid, as
    /// this method does not validate their values. If they
    /// turn out to be invalid, calling [`build()`] later will fail.
    ///
    /// [`build()`]: #method.build
    pub fn add_extra_field(self, key: String, val: BencodeElem) -> TorrentBuilder {
        let mut extra_fields = self.extra_fields;
        extra_fields
            .get_or_insert_with(HashMap::new)
            .insert(key, val);

        TorrentBuilder {
            extra_fields,
            ..self
        }
    }

    /// Add an extra `info` field to `Torrent` (i.e. to the `info` dictionary).
    ///
    /// Calling this method multiple times with the same key will
    /// simply override previous settings.
    ///
    /// The caller has to ensure that `key` and `val` are valid, as
    /// this method does not validate their values. If they
    /// turn out to be invalid, calling [`build()`] later will fail.
    ///
    /// [`build()`]: #method.build
    pub fn add_extra_info_field(self, key: String, val: BencodeElem) -> TorrentBuilder {
        let mut extra_info_fields = self.extra_info_fields;
        extra_info_fields
            .get_or_insert_with(HashMap::new)
            .insert(key, val);

        TorrentBuilder {
            extra_info_fields,
            ..self
        }
    }

    /// Make the `Torrent` private or public, as defined in [BEP 27].
    ///
    /// Calling this method multiple times will simply override previous settings.
    ///
    /// [BEP 27]: http://bittorrent.org/beps/bep_0027.html
    pub fn set_privacy(self, is_private: bool) -> TorrentBuilder {
        TorrentBuilder { is_private, ..self }
    }

    fn validate_announce(&self) -> Result<()> {
        if self.announce.is_empty() {
            Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Borrowed("TorrentBuilder has `announce` but its length is 0."),
            ))
        } else {
            Ok(())
        }
    }

    fn validate_announce_list(&self) -> Result<()> {
        if let Some(ref announce_list) = self.announce_list {
            if announce_list.is_empty() {
                Err(Error::new(
                    ErrorKind::TorrentBuilderFailure,
                    Cow::Borrowed("TorrentBuilder has `announce_list` but it's empty."),
                ))
            } else {
                for tier in announce_list {
                    if tier.is_empty() {
                        return Err(Error::new(
                            ErrorKind::TorrentBuilderFailure,
                            Cow::Borrowed(
                                "TorrentBuilder has `announce_list` but \
                                 one of its tiers is empty.",
                            ),
                        ));
                    } else {
                        for url in tier {
                            if url.is_empty() {
                                return Err(Error::new(
                                    ErrorKind::TorrentBuilderFailure,
                                    Cow::Borrowed(
                                        "TorrentBuilder has `announce_list` but \
                                         one of its tiers contains a 0-length url.",
                                    ),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn validate_name(&self) -> Result<()> {
        if let Some(ref name) = self.name {
            if name.is_empty() {
                Err(Error::new(
                    ErrorKind::TorrentBuilderFailure,
                    Cow::Borrowed("TorrentBuilder has `name` but its length is 0."),
                ))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn validate_path(&self) -> Result<()> {
        // detect path components exactly matching ".."
        // also detect hidden component
        for component in self.path.components() {
            match component {
                Component::ParentDir => {
                    return Err(Error::new(
                        ErrorKind::TorrentBuilderFailure,
                        Cow::Owned(format!(
                            "Root path [{}] contains components \
                             exactly matching \"..\".",
                            self.path.display()
                        )),
                    ));
                }
                Component::Normal(s) => {
                    if s.to_string_lossy().starts_with('.') {
                        return Err(Error::new(
                            ErrorKind::TorrentBuilderFailure,
                            Cow::Owned(format!(
                                "Root path [{}] contains hidden components.",
                                self.path.display()
                            )),
                        ));
                    }
                }
                _ => (),
            }
        }

        if !self.path.is_absolute() {
            return Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Borrowed("TorrentBuilder has `path` but it is not absolute."),
            ));
        }

        if self.path.exists() {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Borrowed("TorrentBuilder has `path` but it does not point to anything."),
            ))
        }
    }

    fn validate_piece_length(&self) -> Result<()> {
        if self.piece_length <= 0 {
            Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Borrowed("TorrentBuilder has `piece_length` <= 0."),
            ))
        } else if (self.piece_length & (self.piece_length - 1)) != 0 {
            // bit trick to check if a number is a power of 2
            // found at: https://stackoverflow.com/a/600306
            Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Borrowed("TorrentBuilder has `piece_length` that is not a power of 2."),
            ))
        } else {
            Ok(())
        }
    }

    fn validate_extra_fields(&self) -> Result<()> {
        if let Some(ref extra_fields) = self.extra_fields {
            if extra_fields.is_empty() {
                panic!("TorrentBuilder has `extra_fields` but it's empty.")
            } else {
                for key in extra_fields.keys() {
                    if key.is_empty() {
                        return Err(Error::new(
                            ErrorKind::TorrentBuilderFailure,
                            Cow::Borrowed(
                                "TorrentBuilder has `extra_fields` \
                                 but it contains a 0-length key.",
                            ),
                        ));
                    }
                }
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn validate_extra_info_fields(&self) -> Result<()> {
        if let Some(ref extra_info_fields) = self.extra_info_fields {
            if extra_info_fields.is_empty() {
                panic!("TorrentBuilder has `extra_info_fields` but it's empty.")
            } else {
                for key in extra_info_fields.keys() {
                    if key.is_empty() {
                        return Err(Error::new(
                            ErrorKind::TorrentBuilderFailure,
                            Cow::Borrowed(
                                "TorrentBuilder has `extra_info_fields` \
                                 but it contains a 0-length key.",
                            ),
                        ));
                    }
                }
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn read_file<P>(path: P, piece_length: Integer) -> Result<(Integer, Vec<Piece>)>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let length = util::u64_to_usize(path.metadata()?.len())?;
        let piece_length = util::i64_to_usize(piece_length)?;

        // read file content + calculate pieces/hashs
        let mut file = BufReader::new(::std::fs::File::open(&path)?);
        let mut piece = Vec::with_capacity(piece_length);
        let mut pieces = Vec::with_capacity(length / piece_length + 1);
        let mut total_read = 0;

        let mut hasher = Sha1::new();
        loop {
            if total_read >= length {
                break;
            } else {
                total_read += file.by_ref()
                    .take(util::usize_to_u64(piece_length)?)
                    .read_to_end(&mut piece)?;
            }

            // @todo: is this vector pre-filling avoidable?
            let mut output = vec![0; PIECE_STRING_LENGTH];
            hasher.input(&piece);
            hasher.result(output.as_mut_slice());
            pieces.push(output);
            hasher.reset();
            piece.clear();
        }

        Ok((util::usize_to_i64(length)?, pieces))
    }

    fn read_dir<P>(path: P, piece_length: Integer) -> Result<(Integer, Vec<File>, Vec<Piece>)>
    where
        P: AsRef<Path>,
    {
        let piece_length = util::i64_to_usize(piece_length)?;
        let entries = util::list_dir(path)?;
        let total_length = entries.iter().fold(0, |acc, &(_, len)| acc + len);
        let mut files = Vec::with_capacity(entries.len());
        let mut pieces = Vec::with_capacity(total_length / piece_length + 1);

        let mut piece = Vec::new();
        let mut hasher = Sha1::new();
        for (path, length) in entries {
            let mut file = BufReader::new(::std::fs::File::open(&path)?);
            let mut file_remaining = length;

            loop {
                // calculate the # of bytes to read in this iteration
                let piece_filled = piece.len();
                let piece_reamining = piece_length - piece_filled;
                let to_read = if file_remaining < piece_reamining {
                    file_remaining
                } else {
                    piece_reamining
                };

                // read bytes
                // @todo: can we avoid allocating a new vec?
                let mut bytes = Vec::with_capacity(to_read);
                file.by_ref()
                    .take(util::usize_to_u64(to_read)?)
                    .read_to_end(&mut bytes)?;
                piece.extend(bytes);
                file_remaining -= to_read;

                // if piece is completely filled, hash it
                if piece.len() == piece_length {
                    // @todo: is this vector pre-filling avoidable?
                    let mut output = vec![0; PIECE_STRING_LENGTH];
                    hasher.input(&piece);
                    hasher.result(output.as_mut_slice());
                    pieces.push(output);
                    hasher.reset();
                    piece.clear();
                }

                // done with current file
                if file_remaining == 0 {
                    break;
                }
            }

            files.push(File {
                length: util::usize_to_i64(length)?,
                path: PathBuf::from(util::last_component(path)?),
                extra_fields: None,
            });
        }

        // if piece is empty then the total file size is divisible by the piece length
        // otherwise the last piece is partially filled and we have to hash it
        if !piece.is_empty() {
            // @todo: is this vector pre-filling avoidable?
            let mut output = vec![0; PIECE_STRING_LENGTH];
            hasher.input(&piece);
            hasher.result(output.as_mut_slice());
            pieces.push(output);
            hasher.reset();
            piece.clear();
        }

        Ok((util::usize_to_i64(total_length)?, files, pieces))
    }
}

#[cfg(test)]
mod torrent_builder_tests {
    // @note: `build()` is not tested here as it is
    // best left to integration tests (in `tests/`)
    //
    // `read_dir()` is also not tested here, as it is
    // implicitly tested with `build()`
    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn new_ok() {
        assert_eq!(
            TorrentBuilder::new("url".to_string(), "dir/", 42),
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );
    }

    #[test]
    fn set_announce_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.set_announce("url2".to_string());
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url2".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );

        let builder = builder.set_announce("url3".to_string());
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url3".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );
    }

    #[test]
    fn set_announce_list_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder =
            builder.set_announce_list(vec![vec!["url2".to_string()], vec!["url3".to_string()]]);
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                announce_list: Some(vec![vec!["url2".to_string()], vec!["url3".to_string()]]),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );

        let builder = builder.set_announce_list(vec![vec!["url2".to_string()]]);
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                announce_list: Some(vec![vec!["url2".to_string()]]),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );
    }

    #[test]
    fn set_name_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.set_name("sample".to_string());
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                name: Some("sample".to_string()),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );

        let builder = builder.set_name("sample2".to_string());
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                name: Some("sample2".to_string()),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );
    }

    #[test]
    fn set_path_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.set_path("dir2");
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir2"),
                piece_length: 42,
                ..Default::default()
            }
        );

        let builder = builder.set_path("dir3");
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir3"),
                piece_length: 42,
                ..Default::default()
            }
        );
    }

    #[test]
    fn set_piece_length_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.set_piece_length(256);
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 256,
                ..Default::default()
            }
        );

        let builder = builder.set_piece_length(512);
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 512,
                ..Default::default()
            }
        );
    }

    #[test]
    fn add_extra_field_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.add_extra_field("k1".to_string(), bencode_elem!("v1"));
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                extra_fields: Some(HashMap::from_iter(
                    vec![("k1".to_string(), bencode_elem!("v1"))].into_iter()
                )),
                ..Default::default()
            }
        );

        let builder = builder.add_extra_field("k2".to_string(), bencode_elem!("v2"));
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                extra_fields: Some(HashMap::from_iter(
                    vec![
                        ("k1".to_string(), bencode_elem!("v1")),
                        ("k2".to_string(), bencode_elem!("v2")),
                    ].into_iter()
                )),
                ..Default::default()
            }
        );
    }

    #[test]
    fn add_extra_info_field_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.add_extra_info_field("k1".to_string(), bencode_elem!("v1"));
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                extra_info_fields: Some(HashMap::from_iter(
                    vec![("k1".to_string(), bencode_elem!("v1"))].into_iter()
                )),
                ..Default::default()
            }
        );

        let builder = builder.add_extra_info_field("k2".to_string(), bencode_elem!("v2"));
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                extra_info_fields: Some(HashMap::from_iter(
                    vec![
                        ("k1".to_string(), bencode_elem!("v1")),
                        ("k2".to_string(), bencode_elem!("v2")),
                    ].into_iter()
                )),
                ..Default::default()
            }
        );
    }

    #[test]
    fn set_privacy_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        let builder = builder.set_privacy(true);
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                is_private: true,
                ..Default::default()
            }
        );

        let builder = builder.set_privacy(false);
        assert_eq!(
            builder,
            TorrentBuilder {
                announce: "url".to_string(),
                path: PathBuf::from("dir"),
                piece_length: 42,
                ..Default::default()
            }
        );
    }

    #[test]
    fn validate_announce_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        builder.validate_announce().unwrap();
        // validation methods should not modify builder
        assert_eq!(builder, TorrentBuilder::new("url".to_string(), "dir/", 42));
    }

    #[test]
    fn validate_announce_empty() {
        let builder = TorrentBuilder::new("".to_string(), "dir/", 42);

        match builder.validate_announce() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_announce_list_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42)
            .set_announce_list(vec![vec!["url2".to_string()]]);

        builder.validate_announce_list().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "dir/", 42)
                .set_announce_list(vec![vec!["url2".to_string()]])
        );
    }

    #[test]
    fn validate_announce_list_none() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        builder.validate_announce_list().unwrap();
        // validation methods should not modify builder
        assert_eq!(builder, TorrentBuilder::new("url".to_string(), "dir/", 42));
    }

    #[test]
    fn validate_announce_list_empty() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42).set_announce_list(vec![]);

        match builder.validate_announce_list() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_announce_list_empty_tier() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42)
            .set_announce_list(vec![vec!["url2".to_string()], vec![]]);

        match builder.validate_announce_list() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_announce_list_empty_url() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42)
            .set_announce_list(vec![vec!["url2".to_string()], vec!["".to_string()]]);

        match builder.validate_announce_list() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_name_ok() {
        let builder =
            TorrentBuilder::new("url".to_string(), "dir/", 42).set_name("sample".to_string());

        builder.validate_name().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "dir/", 42).set_name("sample".to_string())
        );
    }

    #[test]
    fn validate_name_none() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        builder.validate_name().unwrap();
        // validation methods should not modify builder
        assert_eq!(builder, TorrentBuilder::new("url".to_string(), "dir/", 42));
    }

    #[test]
    fn validate_name_empty() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42).set_name("".to_string());

        match builder.validate_name() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_path_ok() {
        let mut path = PathBuf::from(".").canonicalize().unwrap();
        path.push("target");
        let builder = TorrentBuilder::new("url".to_string(), &path, 42);

        builder.validate_path().unwrap();
        // validation methods should not modify builder
        assert_eq!(builder, TorrentBuilder::new("url".to_string(), path, 42));
    }

    #[test]
    fn validate_path_does_not_exist() {
        let mut path = PathBuf::from(".").canonicalize().unwrap();
        path.push("dir");
        let builder = TorrentBuilder::new("url".to_string(), path, 42);

        match builder.validate_path() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_path_has_invalid_component() {
        let mut path = PathBuf::from(".").canonicalize().unwrap();
        path.push("target/..");
        let builder = TorrentBuilder::new("url".to_string(), path, 42);

        match builder.validate_path() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_path_has_hidden_component() {
        let mut path = PathBuf::from(".").canonicalize().unwrap();
        path.push("tests/files/.hidden");
        let builder = TorrentBuilder::new("url".to_string(), path, 42);

        match builder.validate_path() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_path_not_absolute() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42);

        match builder.validate_path() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_piece_length_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 1024);

        builder.validate_piece_length().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "target/", 1024),
        );
    }

    #[test]
    fn validate_piece_length_not_positive() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", -1024);

        match builder.validate_piece_length() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_piece_length_not_power_of_two() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 1023);

        match builder.validate_piece_length() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_extra_fields_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42)
            .add_extra_field("k1".to_string(), bencode_elem!("v1"));

        builder.validate_extra_fields().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "target/", 42)
                .add_extra_field("k1".to_string(), bencode_elem!("v1")),
        );
    }

    #[test]
    fn validate_extra_fields_none() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42);

        builder.validate_extra_fields().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "target/", 42),
        );
    }

    #[test]
    fn validate_extra_fields_empty_key() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42)
            .add_extra_field("".to_string(), bencode_elem!("v1"));

        match builder.validate_extra_fields() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn validate_extra_info_fields_ok() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42)
            .add_extra_info_field("k1".to_string(), bencode_elem!("v1"));

        builder.validate_extra_info_fields().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "target/", 42)
                .add_extra_info_field("k1".to_string(), bencode_elem!("v1")),
        );
    }

    #[test]
    fn validate_extra_info_fields_none() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42);

        builder.validate_extra_info_fields().unwrap();
        // validation methods should not modify builder
        assert_eq!(
            builder,
            TorrentBuilder::new("url".to_string(), "target/", 42),
        );
    }

    #[test]
    fn validate_extra_info_fields_empty_key() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42)
            .add_extra_info_field("".to_string(), bencode_elem!("v1"));

        match builder.validate_extra_info_fields() {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }

    #[test]
    fn read_file_ok() {
        // byte_sequence contains 256 bytes ranging from 0x0 to 0xff
        let (length, pieces) = TorrentBuilder::read_file("tests/files/byte_sequence", 64).unwrap();
        assert_eq!(length, 256);
        assert_eq!(
            pieces,
            vec![
                vec![
                    198, 19, 141, 81, 79, 250, 33, 53, 191, 206, 14, 208, 184, 250, 198, 86, 105,
                    145, 126, 199,
                ],
                vec![
                    8, 244, 44, 162, 89, 207, 18, 29, 46, 169, 205, 139, 108, 91, 36, 200, 109,
                    115, 61, 183,
                ],
                vec![
                    156, 122, 162, 177, 31, 39, 9, 152, 166, 59, 27, 23, 149, 207, 243, 137, 10,
                    78, 181, 111,
                ],
                vec![
                    185, 161, 57, 156, 18, 128, 41, 140, 193, 70, 116, 118, 156, 255, 135, 160,
                    167, 133, 230, 171,
                ],
            ]
        );
    }
}
