use dpedal_config::{
    ComputerInput, Config, DPedalControl, DpedalInput, KeyboardInput, MAX_COMPUTER_INPUTS,
    MAX_DPEDAL_INPUTS, MAX_MAPPINGS, MAX_NICKNAME_LEN, MAX_PIN_REMAPPINGS, MAX_PROFILES,
    MappingMode, Meta, MouseInput,
};
use kdl::{KdlDocument, KdlNode};
use kdl_config::{
    KdlConfig, KdlConfigFinalize, Parsed,
    error::{ParseDiagnostic, ParseError},
};
use kdl_config_derive::{KdlConfig, KdlConfigFinalize};
use miette::{IntoDiagnostic, NamedSource};
use std::{path::PathBuf, str::FromStr};

pub fn load(path: Option<PathBuf>) -> miette::Result<Config> {
    let input = load_source(path)?;
    // TODO: upstream a way to tell KDL parser what the filename is.
    let kdl: KdlDocument = input.inner().parse()?;
    let (profile, error): (Parsed<ConfigKdl>, ParseError) = kdl_config::parse(input, kdl);

    // TODO: extra diagnostics here.

    if !error.diagnostics.is_empty() {
        return Err(error.into());
    }

    Ok(profile.value.finalize())
}

fn load_source(path: Option<PathBuf>) -> miette::Result<NamedSource<String>> {
    let path = if let Some(path) = path {
        path
    } else if let Ok(cargo_manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        PathBuf::from(cargo_manifest_dir)
            .parent()
            .unwrap()
            .join("config.kdl")
    } else {
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("config.kdl")
    };
    let filename = path.file_name().unwrap().to_str().unwrap();
    let text = std::fs::read_to_string(&path)
        .into_diagnostic()
        .map_err(|e| e.context(format!("Failed to load config file at {path:?}")))?;
    Ok(NamedSource::new(filename, text))
}

#[derive(KdlConfig, Default, Debug)]
pub struct ConfigKdl {
    pub version: Parsed<u32>,
    pub nickname: Parsed<heapless::String<MAX_NICKNAME_LEN>>,
    pub device: Parsed<DeviceKdl>,
    pub color: Parsed<u32>,
    pub profiles: Parsed<heapless::Vec<Parsed<ProfileKdl>, MAX_PROFILES>>,
    // TODO: add validation: no duplicate pins (including default values), valid pin range
    pub pin_remappings: Parsed<heapless::Vec<Parsed<PinRemappingKdl>, MAX_PIN_REMAPPINGS>>,
}

impl KdlConfigFinalize for ConfigKdl {
    type FinalizeType = Config;

    fn finalize(&self) -> Self::FinalizeType {
        Config {
            meta: Meta {
                version: self.version.value.finalize(),
                nickname: self.nickname.value.finalize(),
                device: self.device.value.finalize(),
                color: self.color.value.finalize(),
                pin_remappings: self.pin_remappings.value.finalize(),
            },
            profiles: self.profiles.value.finalize(),
        }
    }
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::PinRemapping"]
pub struct PinRemappingKdl {
    pub input: Parsed<DpedalInputKdl>,
    pub pin: Parsed<u32>,
}

// TODO: add derive side validation that Parsed is used everywhere.
#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::Profile"]
pub struct ProfileKdl {
    pub mappings: Parsed<heapless::Vec<Parsed<MappingKdl>, MAX_MAPPINGS>>,
}

#[derive(Default, Debug)]
pub struct MappingKdl {
    pub input_set: heapless::Vec<dpedal_config::DpedalInput, MAX_DPEDAL_INPUTS>,
    pub mode: MappingMode,
    pub output_sequence: heapless::Vec<dpedal_config::ComputerInput, MAX_COMPUTER_INPUTS>,
}

impl KdlConfigFinalize for MappingKdl {
    type FinalizeType = dpedal_config::Mapping;

    fn finalize(&self) -> Self::FinalizeType {
        Self::FinalizeType {
            input_set: self.input_set.clone(),
            mode: self.mode,
            output_sequence: self.output_sequence.clone(),
        }
    }
}

impl KdlConfig for MappingKdl {
    fn parse_as_node(
        source: NamedSource<String>,
        node: &KdlNode,
        diagnostics: &mut Vec<kdl_config::error::ParseDiagnostic>,
    ) -> Parsed<Self>
    where
        Self: Sized,
    {
        let entries = node.entries();
        let span = node.span();

        // Find the ":" separator between mode and inputs
        let Some(colon_idx) = entries.iter().position(|e| is_token(e, ":")) else {
            diagnostics.push(
                ParseDiagnostic::new(source.clone(), span)
                    .message("Mapping needs format `mode : inputs -> outputs`"),
            );
            return Parsed::invalid(span);
        };

        // Find the "->" separator between inputs and outputs (must come after ":")
        let after_colon = &entries[colon_idx + 1..];
        let after_colon_span = match (after_colon.first(), after_colon.last()) {
            (Some(first), Some(last)) => {
                let start = first.span().offset();
                let end = last.span().offset() + last.span().len();
                miette::SourceSpan::new(start.into(), end - start)
            }
            _ => span,
        };
        let Some(arrow_offset) = after_colon.iter().position(|e| is_token(e, "->")) else {
            diagnostics.push(
                ParseDiagnostic::new(source.clone(), after_colon_span)
                    .message("Mapping needs `->` separator between inputs and outputs"),
            );
            return Parsed::invalid(span);
        };
        let arrow_idx = colon_idx + 1 + arrow_offset;

        let mode_entries = &entries[..colon_idx];
        let input_entries = &entries[colon_idx + 1..arrow_idx];
        let output_entries = &entries[arrow_idx + 1..];

        let Some(mode) = parse_mode(source.clone(), mode_entries, span, diagnostics) else {
            return Parsed::invalid(span);
        };

        // Parse inputs separated by "+"
        let mut input_set = heapless::Vec::new();
        for group in split_by_plus(input_entries) {
            let Some(entry) = group.first() else {
                diagnostics.push(
                    ParseDiagnostic::new(source.clone(), span)
                        .message("Empty input group, check for duplicate or leading/trailing `+`"),
                );
                return Parsed::invalid(span);
            };
            match parse_dpedal_input(source.clone(), entry, diagnostics) {
                Some(input) => {
                    if input_set.push(input).is_err() {
                        diagnostics.push(
                            ParseDiagnostic::new(source.clone(), span)
                                .message(format!("Too many inputs, max is {MAX_DPEDAL_INPUTS}")),
                        );
                        return Parsed::invalid(span);
                    }
                }
                None => return Parsed::invalid(span),
            }
        }

        // Parse outputs separated by "+"
        let mut output_sequence = heapless::Vec::new();
        for group in split_by_plus(output_entries) {
            if group.is_empty() {
                diagnostics
                    .push(ParseDiagnostic::new(source.clone(), span).message(
                        "Empty output group, check for duplicate or leading/trailing `+`",
                    ));
                return Parsed::invalid(span);
            }
            match parse_computer_input(source.clone(), group, diagnostics) {
                Some(output) => {
                    if output_sequence.push(output).is_err() {
                        diagnostics
                            .push(ParseDiagnostic::new(source.clone(), span).message(format!(
                                "Too many outputs, max is {MAX_COMPUTER_INPUTS}"
                            )));
                        return Parsed::invalid(span);
                    }
                }
                None => return Parsed::invalid(span),
            }
        }

        Parsed {
            value: MappingKdl {
                input_set,
                mode,
                output_sequence,
            },
            full_span: span,
            name_span: span,
            valid: true,
        }
    }
}

fn is_token(entry: &kdl::KdlEntry, token: &str) -> bool {
    matches!(entry.value(), kdl::KdlValue::String(s) if s == token)
}

/// Split a slice of KDL entries into groups at each "+" token entry.
fn split_by_plus(entries: &[kdl::KdlEntry]) -> Vec<&[kdl::KdlEntry]> {
    let mut groups = vec![];
    let mut start = 0;
    for (i, entry) in entries.iter().enumerate() {
        if is_token(entry, "+") {
            groups.push(&entries[start..i]);
            start = i + 1;
        }
    }
    groups.push(&entries[start..]);
    groups
}

// bit silly, but the enum parsing logic shared with the web app expects the integer as a string,
// maybe we can clean this up later
fn entry_integer_as_string(entry: &kdl::KdlEntry) -> Option<String> {
    match entry.value() {
        kdl::KdlValue::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

fn kebab_to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut upper = true;
    for char in s.chars() {
        if upper {
            result.push(char.to_ascii_uppercase());
            upper = false;
        } else if char == '-' {
            upper = true;
        } else {
            result.push(char);
        }
    }
    result
}

fn parse_mode(
    source: NamedSource<String>,
    entries: &[kdl::KdlEntry],
    span: miette::SourceSpan,
    diagnostics: &mut Vec<ParseDiagnostic>,
) -> Option<MappingMode> {
    let Some(name_entry) = entries.first() else {
        diagnostics
            .push(ParseDiagnostic::new(source, span).message("Mapping is missing mode before `:`"));
        return None;
    };

    let name = match name_entry.value() {
        kdl::KdlValue::String(s) => s.as_str(),
        value => {
            diagnostics.push(
                ParseDiagnostic::new(source, name_entry.span())
                    .message(format!("Expected mode string but got {value:?}")),
            );
            return None;
        }
    };

    let num_str = entries
        .get(1)
        .and_then(entry_integer_as_string)
        .unwrap_or_default();

    let pascal = kebab_to_pascal_case(name);
    match MappingMode::from_string(&pascal, &num_str) {
        Some(mode) => Some(mode),
        None => {
            diagnostics.push(
                ParseDiagnostic::new(source, name_entry.span())
                    .message(format!("Unknown mode {name:?}")),
            );
            None
        }
    }
}

fn parse_dpedal_input(
    source: NamedSource<String>,
    entry: &kdl::KdlEntry,
    diagnostics: &mut Vec<ParseDiagnostic>,
) -> Option<DpedalInput> {
    let value = match entry.value() {
        kdl::KdlValue::String(value) => value.as_str(),
        value => {
            diagnostics.push(
                ParseDiagnostic::new(source, entry.span())
                    .message(format!("Expected a string but got {value:?}")),
            );
            return None;
        }
    };
    match DpedalInput::from_string_kebab(value) {
        Some(input) => Some(input),
        None => {
            diagnostics.push(
                ParseDiagnostic::new(source, entry.span())
                    .message(format!("Unknown input {value:?}")),
            );
            None
        }
    }
}

fn parse_computer_input(
    source: NamedSource<String>,
    entries: &[kdl::KdlEntry],
    diagnostics: &mut Vec<ParseDiagnostic>,
) -> Option<ComputerInput> {
    let name_entry = entries.first().expect("caller ensures non-empty");

    let name = match name_entry.value() {
        kdl::KdlValue::String(s) => s.as_str(),
        value => {
            diagnostics.push(
                ParseDiagnostic::new(source, name_entry.span())
                    .message(format!("Expected a string but got {value:?}")),
            );
            return None;
        }
    };

    let num_str = entries
        .get(1)
        .and_then(entry_integer_as_string)
        .unwrap_or_default();

    if let Some(rest) = name.strip_prefix("mouse-") {
        match MouseInput::from_string(rest, &num_str) {
            Some(input) => Some(ComputerInput::Mouse(input)),
            None => {
                diagnostics.push(
                    ParseDiagnostic::new(source, name_entry.span())
                        .message(format!("Unknown mouse output {name:?}")),
                );
                None
            }
        }
    } else if let Some(rest) = name.strip_prefix("keyboard-") {
        match keyboard_from_string_kebab(rest) {
            Some(input) => Some(ComputerInput::Keyboard(input)),
            None => {
                diagnostics.push(
                    ParseDiagnostic::new(source, name_entry.span())
                        .message(format!("Unknown keyboard output {name:?}")),
                );
                None
            }
        }
    } else {
        let pascal = kebab_to_pascal_case(name);
        match DPedalControl::from_string(&pascal, &num_str) {
            Some(ctrl) => Some(ComputerInput::Control(ctrl)),
            None => {
                diagnostics.push(
                    ParseDiagnostic::new(source, name_entry.span())
                        .message(format!("Unknown output {name:?}")),
                );
                None
            }
        }
    }
}

pub fn keyboard_from_string_kebab(s: &str) -> Option<KeyboardInput> {
    KeyboardInput::from_str(&kebab_to_pascal_case(s)).ok()
}

#[test]
fn test_keyboard_from_string_kebab() {
    assert_eq!(
        keyboard_from_string_kebab("page-up").unwrap(),
        KeyboardInput::PageUp
    );
    assert_eq!(keyboard_from_string_kebab("a").unwrap(), KeyboardInput::A);
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::DpedalInput"]
pub enum DpedalInputKdl {
    #[default]
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    ButtonLeft,
    ButtonRight,
}

#[derive(KdlConfig, KdlConfigFinalize, Default, Debug)]
#[kdl_config_finalize_into = "dpedal_config::Device"]
pub enum DeviceKdl {
    #[default]
    DpedalV3,
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use dpedal_config::{
        ComputerInput, Config, DPedalControl, DpedalInput, KeyboardInput, MappingMode, Meta,
        MouseInput,
    };
    use dpedal_config::{Device, Mapping, PinRemapping, Profile};
    use miette::{GraphicalReportHandler, GraphicalTheme};

    use crate::config::load;

    #[test]
    fn test_example_config_loads() {
        load(None).unwrap();
    }

    fn fmt_report(diag: miette::Error) -> String {
        let mut out = String::new();
        GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
            .without_syntax_highlighting()
            .with_width(80)
            .render_report(&mut out, diag.as_ref())
            .unwrap();
        out
    }

    #[test]
    fn test_parse_config_bad_nickname() {
        let err = load(Some(PathBuf::from("src/test-configs/bad-nickname.kdl"))).unwrap_err();
        let expected = r#"
  × Failed to parse configuration

Error: 
  × Expected type String but was Integer
   ╭─[bad-nickname.kdl:3:1]
 2 │ device dpedal-v3
 3 │ nickname 5
   · ─────┬────
   ·      ╰── here
 4 │ color 0xFF0000
   ╰────
"#;
        pretty_assertions::assert_eq!(fmt_report(err).trim(), expected.trim());
    }

    #[test]
    fn test_parse_config_bad_mappings() {
        let err = load(Some(PathBuf::from("src/test-configs/bad-mappings.kdl"))).unwrap_err();
        let expected = r#"
  × Failed to parse configuration

Error: 
  × Unknown mode "on-pressa"
   ╭─[bad-mappings.kdl:8:13]
 7 │         mappings {
 8 │           - on-pressa : dpad-up -> mouse-scroll-up 20
   ·             ────┬────
   ·                 ╰── here
 9 │           - on-press : dpad-down + -> mouse-scroll-down 20
   ╰────

Error: 
  × Empty input group, check for duplicate or leading/trailing `+`
    ╭─[bad-mappings.kdl:9:11]
  8 │           - on-pressa : dpad-up -> mouse-scroll-up 20
  9 │           - on-press : dpad-down + -> mouse-scroll-down 20
    ·           ────────────────────────┬───────────────────────
    ·                                   ╰── here
 10 │           - on-press : dpad-left mouse-scroll-left 20
    ╰────

Error: 
  × Mapping needs `->` separator between inputs and outputs
    ╭─[bad-mappings.kdl:10:24]
  9 │           - on-press : dpad-down + -> mouse-scroll-down 20
 10 │           - on-press : dpad-left mouse-scroll-left 20
    ·                        ───────────────┬──────────────
    ·                                       ╰── here
 11 │           - on-press : dpad-right -> mouse-scroll-right
    ╰────

Error: 
  × Unknown mouse output "mouse-scroll-right"
    ╭─[bad-mappings.kdl:11:38]
 10 │           - on-press : dpad-left mouse-scroll-left 20
 11 │           - on-press : dpad-right -> mouse-scroll-right
    ·                                      ─────────┬────────
    ·                                               ╰── here
 12 │           - on-press : dpad-diagonal -> keyboard-a
    ╰────

Error: 
  × Unknown input "dpad-diagonal"
    ╭─[bad-mappings.kdl:12:24]
 11 │           - on-press : dpad-right -> mouse-scroll-right
 12 │           - on-press : dpad-diagonal -> keyboard-a
    ·                        ──────┬──────
    ·                              ╰── here
 13 │           - on-press dpad-diagonal -> keyboard-a
    ╰────

Error: 
  × Mapping needs format `mode : inputs -> outputs`
    ╭─[bad-mappings.kdl:13:11]
 12 │           - on-press : dpad-diagonal -> keyboard-a
 13 │           - on-press dpad-diagonal -> keyboard-a
    ·           ───────────────────┬──────────────────
    ·                              ╰── here
 14 │           - on-press :
    ╰────

Error: 
  × Mapping needs `->` separator between inputs and outputs
    ╭─[bad-mappings.kdl:14:11]
 13 │           - on-press dpad-diagonal -> keyboard-a
 14 │           - on-press :
    ·           ──────┬─────
    ·                 ╰── here
 15 │         }
    ╰────
"#;
        pretty_assertions::assert_eq!(fmt_report(err).trim(), expected.trim());
    }

    #[test]
    fn test_parse_config_success() {
        let config = load(Some(PathBuf::from("src/test-configs/config.kdl"))).unwrap();
        assert_eq!(
            config,
            Config {
                meta: Meta {
                    version: 0,
                    nickname: heapless::String::try_from("My DPedal").unwrap(),
                    device: Device::DpedalV3,
                    color: 0xFF0000,
                    pin_remappings: heapless::Vec::from_iter([
                        PinRemapping {
                            input: DpedalInput::ButtonLeft,
                            pin: 3
                        },
                        PinRemapping {
                            input: DpedalInput::ButtonRight,
                            pin: 20
                        }
                    ])
                },
                profiles: heapless::Vec::from_iter([Profile {
                    mappings: heapless::Vec::from_iter([
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadUp]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollUp(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadDown]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollDown(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadLeft]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollLeft(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::DpadRight]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Mouse(
                                MouseInput::ScrollRight(20),
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::ButtonLeft]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Keyboard(
                                KeyboardInput::PageUp,
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([DpedalInput::ButtonRight]),
                            mode: MappingMode::OnPress,
                            output_sequence: heapless::Vec::from_iter([ComputerInput::Keyboard(
                                KeyboardInput::PageDown,
                            )]),
                        },
                        Mapping {
                            input_set: heapless::Vec::from_iter([
                                DpedalInput::ButtonLeft,
                                DpedalInput::ButtonRight
                            ]),
                            mode: MappingMode::OnHold { hold_ms: 50 },
                            output_sequence: heapless::Vec::from_iter([
                                ComputerInput::Control(DPedalControl::AfterMillisHold(50)),
                                ComputerInput::Control(DPedalControl::SetProfile(2))
                            ]),
                        },
                    ]),
                }]),
            }
        );
    }
}
