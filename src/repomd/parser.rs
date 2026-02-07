use crate::error::{Result, RpmSearchError};
use crate::repomd::model::{RpmDependency, RpmPackage};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::BufRead;

pub struct PrimaryXmlParser;

impl PrimaryXmlParser {
    /// Parse primary.xml (or primary.xml.gz) and extract package metadata
    pub fn parse<R: BufRead>(reader: R) -> Result<Vec<RpmPackage>> {
        let mut xml_reader = Reader::from_reader(reader);
        // trim_text removed in quick-xml 0.39

        let mut packages = Vec::new();
        let mut buf = Vec::new();
        let mut current_package: Option<RpmPackage> = None;
        let mut current_text = String::new();
        let mut in_element = String::new();

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
                                packager: None,
                                url: None,
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
                        "rpm:entry" => {
                            // Parse dependency entry
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
                                    // Determine if this is requires or provides based on parent
                                    // This is a simplified approach - in real parsing we'd track parent elements
                                    pkg.requires.push(dep);
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
    }
}
