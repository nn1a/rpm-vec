use crate::error::{Result, RpmSearchError};
use crate::repomd::model::{RpmDependency, RpmPackage};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::BufRead;

/// Tracks which dependency section we're currently inside
#[derive(Debug, Clone, Copy, PartialEq)]
enum DepSection {
    None,
    Requires,
    Provides,
}

pub struct PrimaryXmlParser;

impl PrimaryXmlParser {
    /// Parse primary.xml (or primary.xml.gz) and extract package metadata
    pub fn parse<R: BufRead>(reader: R) -> Result<Vec<RpmPackage>> {
        let mut xml_reader = Reader::from_reader(reader);

        let mut packages = Vec::new();
        let mut buf = Vec::new();
        let mut current_package: Option<RpmPackage> = None;
        let mut current_text = String::new();
        let mut in_element = String::new();
        let mut dep_section = DepSection::None;

        loop {
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    in_element = name.clone();

                    match name.as_str() {
                        "package" => {
                            current_package = Some(RpmPackage {
                                name: String::new(),
                                epoch: None,
                                version: String::new(),
                                release: String::new(),
                                arch: String::new(),
                                summary: String::new(),
                                description: String::new(),
                                license: None,
                                vcs: None,
                                packager: None,
                                url: None,
                                location_href: None,
                                requires: Vec::new(),
                                provides: Vec::new(),
                                files: Vec::new(),
                            });
                        }
                        "name" => {
                            current_text.clear();
                        }
                        "version" => {
                            if let Some(pkg) = current_package.as_mut() {
                                for attr in e.attributes().flatten() {
                                    let key = String::from_utf8_lossy(attr.key.as_ref());
                                    let value = String::from_utf8_lossy(&attr.value);
                                    match key.as_ref() {
                                        "epoch" => {
                                            pkg.epoch = value.parse().ok();
                                        }
                                        "ver" => {
                                            pkg.version = value.to_string();
                                        }
                                        "rel" => {
                                            pkg.release = value.to_string();
                                        }
                                        "vcs" => {
                                            pkg.vcs = Some(value.to_string());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "arch" => {
                            current_text.clear();
                        }
                        "summary" => {
                            current_text.clear();
                        }
                        "description" => {
                            current_text.clear();
                        }
                        "rpm:license" => {
                            current_text.clear();
                        }
                        "location" => {
                            if let Some(pkg) = current_package.as_mut() {
                                for attr in e.attributes().flatten() {
                                    if attr.key.as_ref() == b"href" {
                                        let value = String::from_utf8_lossy(&attr.value);
                                        pkg.location_href = Some(value.to_string());
                                    }
                                }
                            }
                        }
                        "rpm:requires" => {
                            dep_section = DepSection::Requires;
                        }
                        "rpm:provides" => {
                            dep_section = DepSection::Provides;
                        }
                        "rpm:entry" => {
                            let mut dep_name = String::new();
                            let mut dep_flags = None;
                            let mut dep_epoch = None;
                            let mut dep_ver = None;
                            let mut dep_rel = None;

                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref());
                                let value = String::from_utf8_lossy(&attr.value);
                                match key.as_ref() {
                                    "name" => dep_name = value.to_string(),
                                    "flags" => dep_flags = Some(value.to_string()),
                                    "epoch" => dep_epoch = Some(value.to_string()),
                                    "ver" => dep_ver = Some(value.to_string()),
                                    "rel" => dep_rel = Some(value.to_string()),
                                    _ => {}
                                }
                            }

                            if !dep_name.is_empty() {
                                let dep = RpmDependency {
                                    name: dep_name,
                                    flags: dep_flags,
                                    epoch: dep_epoch,
                                    version: dep_ver,
                                    release: dep_rel,
                                };

                                if let Some(pkg) = current_package.as_mut() {
                                    match dep_section {
                                        DepSection::Provides => pkg.provides.push(dep),
                                        _ => pkg.requires.push(dep),
                                    }
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
                    match name.as_ref() {
                        "package" => {
                            if let Some(pkg) = current_package.take() {
                                packages.push(pkg);
                            }
                        }
                        "name" => {
                            if let Some(pkg) = current_package.as_mut() {
                                if in_element == "name" {
                                    pkg.name = current_text.clone();
                                }
                            }
                        }
                        "arch" => {
                            if let Some(pkg) = current_package.as_mut() {
                                pkg.arch = current_text.clone();
                            }
                        }
                        "summary" => {
                            if let Some(pkg) = current_package.as_mut() {
                                pkg.summary = current_text.clone();
                            }
                        }
                        "description" => {
                            if let Some(pkg) = current_package.as_mut() {
                                pkg.description = current_text.clone();
                            }
                        }
                        "rpm:license" => {
                            if let Some(pkg) = current_package.as_mut() {
                                if !current_text.is_empty() {
                                    pkg.license = Some(current_text.clone());
                                }
                            }
                        }
                        "rpm:requires" | "rpm:provides" => {
                            dep_section = DepSection::None;
                        }
                        _ => {}
                    }
                    current_text.clear();
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(RpmSearchError::XmlParse(format!(
                        "XML parsing error: {}",
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
    fn test_parse_simple_package() {
        let xml = r#"<?xml version="1.0"?>
        <metadata xmlns="http://linux.duke.edu/metadata/common">
          <package>
            <name>test-package</name>
            <arch>x86_64</arch>
            <version epoch="0" ver="1.0.0" rel="1"/>
            <location href="x86_64/test-package-1.0.0-1.x86_64.rpm"/>
            <summary>Test package</summary>
            <description>A test package for unit testing</description>
          </package>
        </metadata>"#;

        let result = PrimaryXmlParser::parse(xml.as_bytes());
        assert!(result.is_ok());
        let packages = result.unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "test-package");
        assert_eq!(packages[0].version, "1.0.0");
        assert_eq!(
            packages[0].location_href.as_deref(),
            Some("x86_64/test-package-1.0.0-1.x86_64.rpm")
        );
    }

    #[test]
    fn test_parse_requires_and_provides() {
        let xml = r#"<?xml version="1.0"?>
        <metadata xmlns="http://linux.duke.edu/metadata/common"
                  xmlns:rpm="http://linux.duke.edu/metadata/rpm">
          <package>
            <name>openssl</name>
            <arch>x86_64</arch>
            <version epoch="1" ver="3.0.0" rel="1.el9"/>
            <summary>Cryptography toolkit</summary>
            <description>OpenSSL library</description>
            <rpm:provides>
              <rpm:entry name="libssl.so.3()(64bit)"/>
              <rpm:entry name="openssl" flags="EQ" ver="3.0.0" rel="1.el9" epoch="1"/>
            </rpm:provides>
            <rpm:requires>
              <rpm:entry name="glibc" flags="GE" ver="2.34"/>
              <rpm:entry name="libcrypto.so.3()(64bit)"/>
            </rpm:requires>
          </package>
        </metadata>"#;

        let packages = PrimaryXmlParser::parse(xml.as_bytes()).unwrap();
        assert_eq!(packages.len(), 1);
        let pkg = &packages[0];
        assert_eq!(pkg.name, "openssl");

        // Provides should be correctly classified
        assert_eq!(pkg.provides.len(), 2);
        assert_eq!(pkg.provides[0].name, "libssl.so.3()(64bit)");
        assert_eq!(pkg.provides[1].name, "openssl");
        assert_eq!(pkg.provides[1].flags.as_deref(), Some("EQ"));

        // Requires should be correctly classified
        assert_eq!(pkg.requires.len(), 2);
        assert_eq!(pkg.requires[0].name, "glibc");
        assert_eq!(pkg.requires[0].flags.as_deref(), Some("GE"));
        assert_eq!(pkg.requires[1].name, "libcrypto.so.3()(64bit)");
    }

    #[test]
    fn test_parse_license_and_vcs() {
        let xml = r#"<?xml version="1.0"?>
        <metadata xmlns="http://linux.duke.edu/metadata/common"
                  xmlns:rpm="http://linux.duke.edu/metadata/rpm">
          <package>
            <name>bash</name>
            <arch>x86_64</arch>
            <version epoch="0" ver="5.2.15" rel="3.el9" vcs="https://github.com/bminor/bash#devel"/>
            <summary>The GNU Bourne Again shell</summary>
            <description>The GNU Bourne Again shell</description>
            <rpm:license>GPLv3+</rpm:license>
          </package>
        </metadata>"#;

        let packages = PrimaryXmlParser::parse(xml.as_bytes()).unwrap();
        assert_eq!(packages.len(), 1);
        let pkg = &packages[0];
        assert_eq!(pkg.name, "bash");
        assert_eq!(pkg.license.as_deref(), Some("GPLv3+"));
        assert_eq!(
            pkg.vcs.as_deref(),
            Some("https://github.com/bminor/bash#devel")
        );
    }

    #[test]
    fn test_parse_no_license_no_vcs() {
        let xml = r#"<?xml version="1.0"?>
        <metadata xmlns="http://linux.duke.edu/metadata/common">
          <package>
            <name>minimal</name>
            <arch>noarch</arch>
            <version epoch="0" ver="1.0" rel="1"/>
            <summary>Minimal package</summary>
            <description>No license or vcs</description>
          </package>
        </metadata>"#;

        let packages = PrimaryXmlParser::parse(xml.as_bytes()).unwrap();
        let pkg = &packages[0];
        assert!(pkg.license.is_none());
        assert!(pkg.vcs.is_none());
    }
}
