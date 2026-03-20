//! `JUnit` XML output parser.
//!
//! Parses `JUnit` XML format (used by pytest, phpunit, maven surefire) into
//! [`TestResult`] values. Handles both `<testsuites>` and bare `<testsuite>`
//! as root elements.

use quick_xml::Reader;
use quick_xml::events::Event;
use t3str_domain_types::{T3strError, TestResult, TestStatus};

use super::seconds_to_ms;

/// Create a [`T3strError::ParseFailed`] for `JUnit` XML parsing.
fn parse_err(reason: impl std::fmt::Display) -> T3strError {
    T3strError::ParseFailed {
        format: String::from("JUnit XML"),
        reason: reason.to_string(),
    }
}

/// Parse `JUnit` XML content into test results.
///
/// Accepts XML with either `<testsuites>` or `<testsuite>` as the root element.
/// Each `<testcase>` element becomes a [`TestResult`].
///
/// # Errors
///
/// Returns [`T3strError::ParseFailed`] if the XML is malformed.
pub fn parse(xml: &str) -> super::ParseResult {
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut reader = Reader::from_str(xml);
    let mut results = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"testcase" => {
                let result = parse_testcase(&mut reader, e)?;
                results.push(result);
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"testcase" => {
                // Self-closing <testcase ... /> — always passed, no children.
                let result = build_testcase_from_attrs(e, TestStatus::Passed, None)?;
                results.push(result);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(parse_err(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(results)
}

/// Parse a `<testcase>` element and its children until the closing `</testcase>`.
fn parse_testcase(
    reader: &mut Reader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
) -> Result<TestResult, T3strError> {
    let mut status = TestStatus::Passed;
    let mut message: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let (child_status, child_msg) = handle_child_element(reader, e)?;
                if let Some(s) = child_status {
                    status = s;
                }
                if child_msg.is_some() {
                    message = child_msg;
                }
            }
            Ok(Event::Empty(ref e)) => {
                let (child_status, child_msg) = handle_empty_child(e)?;
                if let Some(s) = child_status {
                    status = s;
                }
                if child_msg.is_some() {
                    message = child_msg;
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"testcase" => break,
            Ok(Event::Eof) => {
                return Err(parse_err("unexpected EOF inside <testcase>"));
            }
            Err(e) => return Err(parse_err(e)),
            _ => {}
        }
        buf.clear();
    }

    build_testcase_from_attrs(start, status, message)
}

/// Map an element tag name to a [`TestStatus`], if it represents a status child.
fn tag_to_status(tag: &[u8]) -> Option<TestStatus> {
    match tag {
        b"failure" => Some(TestStatus::Failed),
        b"error" => Some(TestStatus::Error),
        b"skipped" => Some(TestStatus::Skipped),
        _ => None,
    }
}

/// Determine status and message from a child element with content
/// (e.g. `<failure>...</failure>`).
fn handle_child_element(
    reader: &mut Reader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
) -> super::ChildOutcome {
    let tag_name = start.name();
    let tag_bytes = tag_name.as_ref().to_vec();
    let status = tag_to_status(&tag_bytes);

    let msg: Option<String> = if status.is_some() {
        extract_message_attr(start)?
    } else {
        None
    };

    // Read until the matching end tag, discarding content.
    let mut depth: u32 = 1;
    let mut inner_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut inner_buf) {
            Ok(Event::Start(_)) => {
                depth = depth.saturating_add(1);
            }
            Ok(Event::End(ref e)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 && e.name().as_ref() == tag_bytes.as_slice() {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(parse_err(e)),
            _ => {}
        }
        inner_buf.clear();
    }

    Ok((status, msg))
}

/// Determine status and message from a self-closing child element
/// (e.g. `<skipped message="..."/>`).
fn handle_empty_child(start: &quick_xml::events::BytesStart<'_>) -> super::ChildOutcome {
    let tag_name = start.name();
    let status = tag_to_status(tag_name.as_ref());

    let msg: Option<String> = if status.is_some() {
        extract_message_attr(start)?
    } else {
        None
    };

    Ok((status, msg))
}

/// Extract the `message` attribute from an element.
fn extract_message_attr(element: &quick_xml::events::BytesStart<'_>) -> super::OptStringResult {
    for attr_result in element.attributes() {
        let attr = attr_result.map_err(parse_err)?;
        if attr.key.as_ref() == b"message" {
            let value: std::borrow::Cow<'_, str> = attr.unescape_value().map_err(parse_err)?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

/// Build a [`TestResult`] from the attributes of a `<testcase>` element.
fn build_testcase_from_attrs(
    element: &quick_xml::events::BytesStart<'_>,
    status: TestStatus,
    message: Option<String>,
) -> Result<TestResult, T3strError> {
    let mut classname = String::new();
    let mut name = String::new();
    let mut time_str: Option<String> = None;

    for attr_result in element.attributes() {
        let attr = attr_result.map_err(parse_err)?;
        let value: std::borrow::Cow<'_, str> = attr.unescape_value().map_err(parse_err)?;
        match attr.key.as_ref() {
            b"classname" => classname = value.into_owned(),
            b"name" => name = value.into_owned(),
            b"time" => time_str = Some(value.into_owned()),
            _ => {}
        }
    }

    let full_name = if classname.is_empty() {
        name
    } else {
        let mut joined = classname;
        joined.push_str("::");
        joined.push_str(&name);
        joined
    };

    let duration_ms = time_str.and_then(|s| s.parse::<f64>().ok().and_then(seconds_to_ms));

    Ok(TestResult {
        name: full_name,
        status,
        duration_ms,
        message,
        file: None,
    })
}

#[cfg(test)]
#[path = "junit_xml_tests.rs"]
mod tests;
