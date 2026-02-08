use crate::error::{Result, RpmSearchError};
use crate::repomd::model::{FilelistsPackage, RpmFileEntry, RpmFileType};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::BufRead;

pub struct FilelistsXmlParser;

impl FilelistsXmlParser {
    /// Parse filelists.xml and extract per-package file lists
    pub fn parse<R: BufRead>(reader: R) -> Result<Vec<FilelistsPackage>> {
        let mut xml_reader = Reader::from_reader(reader);

        let mut packages = Vec::new();
        let mut buf = Vec::new();
        let mut current_package: Option<FilelistsPackage> = None;
        let mut current_text = String::new();
        let mut current_file_type = RpmFileType::File;

        loop {
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    match name.as_str() {
                        "package" => {
                            let mut pkg_name = String::new();
                            let mut arch = String::new();

                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref());
                                let value = String::from_utf8_lossy(&attr.value);
                                match key.as_ref() {
                                    "name" => pkg_name = value.to_string(),
                                    "arch" => arch = value.to_string(),
                                    _ => {}
                                }
                            }

                            current_package = Some(FilelistsPackage {
                                name: pkg_name,
                                arch,
                                epoch: None,
                                version: String::new(),
                                release: String::new(),
                                files: Vec::new(),
                            });
                        }
                        "version" => {
                            if let Some(pkg) = current_package.as_mut() {
                                for attr in e.attributes().flatten() {
                                    let key = String::from_utf8_lossy(attr.key.as_ref());
                                    let value = String::from_utf8_lossy(&attr.value);
                                    match key.as_ref() {
                                        "epoch" => pkg.epoch = value.parse().ok(),
                                        "ver" => pkg.version = value.to_string(),
                                        "rel" => pkg.release = value.to_string(),
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "file" => {
                            current_text.clear();
                            current_file_type = RpmFileType::File;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref());
                                if &*key == "type" {
                                    let value = String::from_utf8_lossy(&attr.value);
                                    current_file_type = match &*value {
                                        "dir" => RpmFileType::Dir,
                                        "ghost" => RpmFileType::Ghost,
                                        _ => RpmFileType::File,
                                    };
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    current_text = xml_reader
                        .decoder()
                        .decode(e.as_ref())
                        .unwrap_or_default()
                        .to_string();
                }
                Ok(Event::End(e)) => {
                    let e_name = e.name();
                    let name = String::from_utf8_lossy(e_name.as_ref());
                    match &*name {
                        "package" => {
                            if let Some(pkg) = current_package.take() {
                                packages.push(pkg);
                            }
                        }
                        "file" => {
                            if let Some(pkg) = current_package.as_mut() {
                                if !current_text.is_empty() {
                                    pkg.files.push(RpmFileEntry {
                                        path: current_text.clone(),
                                        file_type: current_file_type,
                                    });
                                }
                            }
                            current_text.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(RpmSearchError::XmlParse(format!(
                        "Filelists XML parsing error: {}",
                        e
                    )))
                }
                _ => {}
            }
            buf.clear();
        }

        Ok(packages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filelists_basic() {
        let xml = r#"<?xml version="1.0"?>
        <filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="1">
          <package pkgid="abc123" name="bash" arch="x86_64">
            <version epoch="0" ver="5.2" rel="1.el9"/>
            <file>/usr/bin/bash</file>
            <file type="dir">/etc/bash</file>
            <file type="ghost">/var/log/bash.log</file>
          </package>
        </filelists>"#;

        let packages = FilelistsXmlParser::parse(xml.as_bytes()).unwrap();
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name, "bash");
        assert_eq!(pkg.arch, "x86_64");
        assert_eq!(pkg.epoch, Some(0));
        assert_eq!(pkg.version, "5.2");
        assert_eq!(pkg.release, "1.el9");
        assert_eq!(pkg.files.len(), 3);

        assert_eq!(pkg.files[0].path, "/usr/bin/bash");
        assert_eq!(pkg.files[0].file_type, RpmFileType::File);

        assert_eq!(pkg.files[1].path, "/etc/bash");
        assert_eq!(pkg.files[1].file_type, RpmFileType::Dir);

        assert_eq!(pkg.files[2].path, "/var/log/bash.log");
        assert_eq!(pkg.files[2].file_type, RpmFileType::Ghost);
    }

    #[test]
    fn test_parse_filelists_multiple_packages() {
        let xml = r#"<?xml version="1.0"?>
        <filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="2">
          <package pkgid="aaa" name="pkg-a" arch="x86_64">
            <version epoch="0" ver="1.0" rel="1"/>
            <file>/usr/bin/a</file>
          </package>
          <package pkgid="bbb" name="pkg-b" arch="noarch">
            <version epoch="1" ver="2.0" rel="3"/>
            <file>/usr/lib/b.so</file>
            <file>/usr/lib/b.so.1</file>
          </package>
        </filelists>"#;

        let packages = FilelistsXmlParser::parse(xml.as_bytes()).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "pkg-a");
        assert_eq!(packages[0].files.len(), 1);
        assert_eq!(packages[1].name, "pkg-b");
        assert_eq!(packages[1].epoch, Some(1));
        assert_eq!(packages[1].files.len(), 2);
    }

    #[test]
    fn test_parse_filelists_empty() {
        let xml = r#"<?xml version="1.0"?>
        <filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="0">
        </filelists>"#;

        let packages = FilelistsXmlParser::parse(xml.as_bytes()).unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn test_parse_filelists_no_type_defaults_to_file() {
        let xml = r#"<?xml version="1.0"?>
        <filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="1">
          <package pkgid="abc" name="test" arch="x86_64">
            <version epoch="0" ver="1.0" rel="1"/>
            <file>/usr/bin/test</file>
          </package>
        </filelists>"#;

        let packages = FilelistsXmlParser::parse(xml.as_bytes()).unwrap();
        assert_eq!(packages[0].files[0].file_type, RpmFileType::File);
    }
}
