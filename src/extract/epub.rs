use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

use quick_xml::{events::BytesStart, events::Event, name::QName, Reader};
use zip::ZipArchive;

use super::{extract_markup_text, normalize_extracted_document_text, MAX_EXPANDED_DOCUMENT_BYTES};
use crate::{ContextForgeError, Result};

#[derive(Debug)]
struct ManifestItem {
    href: String,
    xhtml: bool,
    navigation: bool,
}

pub(super) fn extract_epub_text(path: &Path) -> Result<String> {
    extract_epub(path).map_err(|reason| ContextForgeError::ExtractEpub {
        path: path.to_path_buf(),
        reason,
    })
}

fn extract_epub(path: &Path) -> std::result::Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let container = read_entry(&mut archive, "META-INF/container.xml", None)?;
    let package_path = parse_package_path(&container)?;
    let package = read_entry(&mut archive, &package_path, None)?;
    let (manifest, spine) = parse_package(&package)?;
    let content_paths = content_paths(&package_path, &manifest, &spine)?;
    let mut expanded_bytes = 0_u64;
    let mut chapters = Vec::new();

    for content_path in content_paths {
        let markup = read_entry(&mut archive, &content_path, Some(&mut expanded_bytes))?;
        let text = extract_markup_text(&markup);
        if !text.is_empty() {
            chapters.push(text);
        }
    }

    if chapters.is_empty() {
        return Err("EPUB contains no readable XHTML body text".to_string());
    }

    Ok(normalize_extracted_document_text(&chapters.join("\n\n")))
}

fn read_entry(
    archive: &mut ZipArchive<fs::File>,
    name: &str,
    expanded_bytes: Option<&mut u64>,
) -> std::result::Result<String, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|error| format!("cannot read EPUB entry `{name}`: {error}"))?;

    if let Some(total) = expanded_bytes {
        let next_total = total.saturating_add(entry.size());
        if next_total > MAX_EXPANDED_DOCUMENT_BYTES {
            return Err(format!(
                "expanded EPUB text exceeds {} MiB",
                MAX_EXPANDED_DOCUMENT_BYTES / 1024 / 1024
            ));
        }
        *total = next_total;
    }

    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .map_err(|error| format!("cannot decode EPUB entry `{name}` as UTF-8: {error}"))?;
    Ok(content)
}

fn parse_package_path(container: &str) -> std::result::Result<String, String> {
    let mut reader = Reader::from_str(container);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(event) | Event::Empty(event)) if name_is(event.name(), b"rootfile") => {
                if let Some(path) = attribute_value(&reader, &event, b"full-path")? {
                    return normalize_archive_path("", &path);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("invalid EPUB container.xml: {error}")),
            _ => {}
        }
    }

    Err("EPUB container does not name an OPF package".to_string())
}

fn parse_package(
    package: &str,
) -> std::result::Result<(BTreeMap<String, ManifestItem>, Vec<String>), String> {
    let mut reader = Reader::from_str(package);
    reader.config_mut().trim_text(true);
    let mut manifest = BTreeMap::new();
    let mut spine = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event) | Event::Empty(event)) if name_is(event.name(), b"item") => {
                let Some(id) = attribute_value(&reader, &event, b"id")? else {
                    continue;
                };
                let Some(href) = attribute_value(&reader, &event, b"href")? else {
                    continue;
                };
                let media_type =
                    attribute_value(&reader, &event, b"media-type")?.unwrap_or_default();
                let properties =
                    attribute_value(&reader, &event, b"properties")?.unwrap_or_default();
                manifest.insert(
                    id,
                    ManifestItem {
                        href,
                        xhtml: matches!(media_type.as_str(), "application/xhtml+xml" | "text/html"),
                        navigation: properties.split_whitespace().any(|value| value == "nav"),
                    },
                );
            }
            Ok(Event::Start(event) | Event::Empty(event)) if name_is(event.name(), b"itemref") => {
                if let Some(idref) = attribute_value(&reader, &event, b"idref")? {
                    spine.push(idref);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("invalid EPUB package document: {error}")),
            _ => {}
        }
    }

    Ok((manifest, spine))
}

fn content_paths(
    package_path: &str,
    manifest: &BTreeMap<String, ManifestItem>,
    spine: &[String],
) -> std::result::Result<Vec<String>, String> {
    let package_directory = package_path
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let mut hrefs = spine
        .iter()
        .filter_map(|id| manifest.get(id))
        .filter(|item| item.xhtml && !item.navigation)
        .map(|item| item.href.as_str())
        .collect::<Vec<_>>();

    if hrefs.is_empty() {
        hrefs = manifest
            .values()
            .filter(|item| item.xhtml && !item.navigation)
            .map(|item| item.href.as_str())
            .collect();
        hrefs.sort_unstable();
    }

    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for href in hrefs {
        let path = normalize_archive_path(package_directory, href)?;
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    if paths.is_empty() {
        return Err("EPUB package contains no XHTML content entries".to_string());
    }
    Ok(paths)
}

fn normalize_archive_path(base: &str, href: &str) -> std::result::Result<String, String> {
    let href = href.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_decode(href)?;
    let mut segments = base
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    for segment in decoded.replace('\\', "/").split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(format!("EPUB entry escapes archive root: {href}"));
                }
            }
            value => segments.push(value.to_string()),
        }
    }
    Ok(segments.join("/"))
}

fn percent_decode(value: &str) -> std::result::Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(format!("invalid percent encoding in EPUB path: {value}"));
            }
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(format!("invalid percent encoding in EPUB path: {value}"));
            };
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| format!("invalid UTF-8 EPUB path: {error}"))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn attribute_value(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &[u8],
) -> std::result::Result<Option<String>, String> {
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| error.to_string())?;
        if name_is(attribute.key, name) {
            return attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                .map(|value| Some(value.into_owned()))
                .map_err(|error| error.to_string());
        }
    }
    Ok(None)
}

fn name_is(actual: QName<'_>, expected: &[u8]) -> bool {
    actual
        .as_ref()
        .rsplit(|byte| *byte == b':')
        .next()
        .is_some_and(|name| name == expected)
}
