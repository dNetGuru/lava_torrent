use std::io::{BufReader, Read};
use std::path::Component;
use conv::ValueFrom;
use crypto::sha1::Sha1;
use crypto::digest::Digest;
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
    /// - Using a symbolic link as `path` will make `build()` return `Err`.
    ///
    /// - Using a `path` containing hidden components will also make `build()` return `Err`.
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
            Self::last_component(&self.path)?
        };

        // set `private = 1` in `info` if the torrent is private
        let mut extra_info_fields = self.extra_info_fields;
        if self.is_private {
            extra_info_fields
                .get_or_insert_with(HashMap::new)
                .insert("private".to_string(), BencodeElem::Integer(1));
        }

        // delegate the actual file reading to other methods
        let file_type = self.path.symlink_metadata()?.file_type();
        let canonicalized_path = self.path.canonicalize()?;
        if file_type.is_dir() {
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
        } else if file_type.is_file() {
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
        } else {
            Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Owned(format!(
                    "Root path [{}] points to a symbolic link.",
                    self.path.display()
                )),
            ))
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
    /// - Using a symbolic link as `path` will make `build()` return `Err`.
    ///
    /// - Using a `path` containing hidden components will also make `build()` return `Err`.
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
        // convert u64 => usize
        // @todo: switch to `usize::try_from()` when it's stable
        let length = match usize::value_from(path.metadata()?.len()) {
            Ok(n) => n,
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::IOError,
                    Cow::Owned(format!(
                        "Length of [{}] does not fit into usize.",
                        path.to_string_lossy()
                    )),
                ));
            }
        };
        // convert i64 => usize
        // @todo: switch to `usize::try_from(len)` when it's stable
        let piece_length = match usize::value_from(piece_length) {
            Ok(n) => n,
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::IOError,
                    Cow::Owned(format!(
                        "piece_length [{}] does not fit into usize.",
                        piece_length
                    )),
                ));
            }
        };

        // read file content
        let mut file = BufReader::new(::std::fs::File::open(&path)?);
        let mut bytes = Vec::with_capacity(length);
        let bytes_read = file.read_to_end(&mut bytes)?;

        // calculate pieces/hashs
        let mut pieces = Vec::with_capacity(length / piece_length + 1);
        if bytes_read != length {
            return Err(Error::new(
                ErrorKind::IOError,
                Cow::Borrowed("# of bytes read from file != file size."),
            ));
        } else {
            let mut hasher = Sha1::new();
            for chunk in bytes.chunks(piece_length) {
                // @todo: is this vector pre-filling avoidable?
                let mut output = vec![0; PIECE_STRING_LENGTH];
                hasher.input(chunk);
                hasher.result(output.as_mut_slice());
                pieces.push(output);
                hasher.reset();
            }
        }

        // convert usize => i64
        // @todo: switch to `i64::try_from(len)` when it's stable
        let length = match i64::value_from(length) {
            Ok(n) => n,
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::IOError,
                    Cow::Owned(format!(
                        "Length of [{}] does not fit into i64.",
                        path.to_string_lossy()
                    )),
                ));
            }
        };
        Ok((length, pieces))
    }

    fn read_dir<P>(path: P, piece_length: Integer) -> Result<(Integer, Vec<File>, Vec<Piece>)>
    where
        P: AsRef<Path>,
    {
        let mut entries = Self::list_dir(path)?;
        let mut total_length = 0;
        let mut files = Vec::new();
        let mut all_pieces = Vec::new();

        entries.sort();
        for entry in entries {
            let (length, pieces) = Self::read_file(&entry, piece_length)?;

            files.push(File {
                length,
                path: PathBuf::from(Self::last_component(entry)?),
                extra_fields: None,
            });

            total_length += length;
            all_pieces.extend(pieces);
        }

        Ok((total_length, files, all_pieces))
    }

    // this method is recursive, i.e. entries in subdirectories
    // are also returned
    //
    // symbolic links and *nix hidden files/dirs are ignored
    fn list_dir<P>(path: P) -> Result<Vec<PathBuf>>
    where
        P: AsRef<Path>,
    {
        let mut entries = Vec::new();

        for entry in path.as_ref().read_dir()? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;

            if Self::last_component(&path)?.starts_with('.') {
                continue;
            } // hidden files/dirs are ignored

            if metadata.is_dir() {
                entries.extend(Self::list_dir(path)?);
            } else if metadata.is_file() {
                entries.push(path);
            } // symbolic links are ignored
        }

        Ok(entries)
    }

    fn last_component<P>(path: P) -> Result<String>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        match path.file_name() {
            Some(s) => Ok(s.to_string_lossy().into_owned()),
            None => Err(Error::new(
                ErrorKind::TorrentBuilderFailure,
                Cow::Owned(format!("[{}] ends in \"..\".", path.display())),
            )),
        }
    }
}

#[cfg(test)]
mod torrent_builder_tests {
    // @note: `build()` is not tested here as it is
    // best left to integration tests (in `tests/`)
    //
    // `read_file()` and `read_dir()` are
    // also not tested here, as they are implicitly
    // tested with `build()`
    //
    // also note that conversion failures in `read_file()`
    // due to overflow are not tested at the moment, as
    // there is no straightforward way to test them
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

        match builder.validate_announce() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(builder, TorrentBuilder::new("url".to_string(), "dir/", 42)),
            Err(_) => assert!(false),
        }
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

        match builder.validate_announce_list() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "dir/", 42)
                    .set_announce_list(vec![vec!["url2".to_string()]])
            ),
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn validate_announce_list_none() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        match builder.validate_announce_list() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(builder, TorrentBuilder::new("url".to_string(), "dir/", 42)),
            Err(_) => assert!(false),
        }
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

        match builder.validate_name() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "dir/", 42).set_name("sample".to_string())
            ),
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn validate_name_none() {
        let builder = TorrentBuilder::new("url".to_string(), "dir/", 42);

        match builder.validate_name() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(builder, TorrentBuilder::new("url".to_string(), "dir/", 42)),
            Err(_) => assert!(false),
        }
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

        match builder.validate_path() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(builder, TorrentBuilder::new("url".to_string(), path, 42),),
            Err(_) => assert!(false),
        }
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

        match builder.validate_piece_length() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "target/", 1024),
            ),
            Err(_) => assert!(false),
        }
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

        match builder.validate_extra_fields() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "target/", 42)
                    .add_extra_field("k1".to_string(), bencode_elem!("v1")),
            ),
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn validate_extra_fields_none() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42);

        match builder.validate_extra_fields() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "target/", 42),
            ),
            Err(_) => assert!(false),
        }
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

        match builder.validate_extra_info_fields() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "target/", 42)
                    .add_extra_info_field("k1".to_string(), bencode_elem!("v1")),
            ),
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn validate_extra_info_fields_none() {
        let builder = TorrentBuilder::new("url".to_string(), "target/", 42);

        match builder.validate_extra_info_fields() {
            // validation methods should not modify builder
            Ok(_) => assert_eq!(
                builder,
                TorrentBuilder::new("url".to_string(), "target/", 42),
            ),
            Err(_) => assert!(false),
        }
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
    fn list_dir_ok() {
        let mut entries = TorrentBuilder::list_dir("tests/files").unwrap();
        entries.sort();

        assert_eq!(
            entries,
            vec![
                PathBuf::from("tests/files/tails-amd64-3.6.1.torrent"),
                PathBuf::from("tests/files/ubuntu-16.04.4-desktop-amd64.iso.torrent"),
                // no [.hidden] and [symlink]
            ]
        );
    }

    #[test]
    fn list_dir_with_subdir() {
        let mut entries = TorrentBuilder::list_dir("src/torrent").unwrap();
        entries.sort();

        assert_eq!(
            entries,
            vec![
                PathBuf::from("src/torrent/mod.rs"),
                PathBuf::from("src/torrent/v1/build.rs"),
                PathBuf::from("src/torrent/v1/mod.rs"),
                PathBuf::from("src/torrent/v1/read.rs"),
                PathBuf::from("src/torrent/v1/write.rs"),
            ]
        );
    }

    #[test]
    fn last_component_ok() {
        assert_eq!(
            TorrentBuilder::last_component("/root/dir/file.ext").unwrap(),
            "file.ext".to_string()
        );
    }

    #[test]
    fn last_component_ok_2() {
        assert_eq!(
            TorrentBuilder::last_component("/root/dir/dir2").unwrap(),
            "dir2".to_string()
        );
    }

    #[test]
    fn last_component_err() {
        match TorrentBuilder::last_component("/root/dir/..") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), ErrorKind::TorrentBuilderFailure),
        }
    }
}